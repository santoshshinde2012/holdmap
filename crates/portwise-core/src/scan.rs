//! Snapshot engine: sockets + processes + containers + projects → grouped [`PortEntry`] rows,
//! plus the filter/query language shared by all surfaces.

use crate::docker::PublishedPort;
use crate::model::*;
use crate::process::ProcessTable;
use crate::project::{detect_framework, ProjectDetector};
use crate::provider::{
    ContainerProvider, DockerContainers, ProcessProvider, SocketProvider, StaticContainers,
    StaticProcesses, StaticSockets, SystemProcesses, SystemSockets,
};
use crate::safety::{DefaultProtectionPolicy, Protection, ProtectionPolicy};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;

/// What to include in a scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanOptions {
    /// Include non-listening sockets (ESTABLISHED, TIME_WAIT, …).
    pub all_states: bool,
    /// Query container runtimes (Docker/Podman/OrbStack/Colima) for published ports.
    pub docker: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            all_states: false,
            docker: true,
        }
    }
}

/// Everything learned in one scan; reused by explain/plan so they see a consistent world.
#[derive(Debug, Clone)]
pub struct Scan {
    /// The user-facing snapshot.
    pub snapshot: Snapshot,
    /// Process table used for the snapshot.
    pub table: ProcessTable,
    /// Container-published ports.
    pub published: Vec<PublishedPort>,
    /// Raw sockets as returned by the provider.
    pub raw: Vec<RawSocket>,
}

/// Run a full scan of the machine with the system providers and default policy.
pub fn scan(opts: &ScanOptions) -> Result<Scan, std::io::Error> {
    Scanner::system().scan(opts)
}

/// Builds a [`Scan`] from injected providers and a protection policy.
#[derive(Clone)]
pub struct Scanner {
    sockets: Arc<dyn SocketProvider>,
    processes: Arc<dyn ProcessProvider>,
    containers: Arc<dyn ContainerProvider>,
    policy: Arc<dyn ProtectionPolicy>,
    platform: String,
}

impl std::fmt::Debug for Scanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scanner")
            .field("platform", &self.platform)
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl Scanner {
    /// The live machine: OS sockets, sysinfo processes, Docker-compatible runtimes.
    pub fn system() -> Self {
        Self::new(
            Arc::new(SystemSockets),
            SystemProcesses::shared(),
            Arc::new(DockerContainers),
        )
    }

    /// A scanner over the given socket, process and container providers.
    pub fn new(
        sockets: Arc<dyn SocketProvider>,
        processes: Arc<dyn ProcessProvider>,
        containers: Arc<dyn ContainerProvider>,
    ) -> Self {
        Self {
            sockets,
            processes,
            containers,
            policy: Arc::new(DefaultProtectionPolicy),
            platform: crate::sys::platform_name().to_string(),
        }
    }

    /// A scanner over fixed data (tests, fixtures, remote snapshots).
    pub fn fixed(raw: Vec<RawSocket>, table: ProcessTable, published: Vec<PublishedPort>) -> Self {
        Self::new(
            Arc::new(StaticSockets(raw)),
            Arc::new(StaticProcesses(table)),
            Arc::new(StaticContainers(published)),
        )
        .with_platform("fixture")
    }

    /// Use a custom protection policy.
    pub fn with_policy(mut self, policy: Arc<dyn ProtectionPolicy>) -> Self {
        self.policy = policy;
        self
    }

    /// Override the platform name reported in snapshots.
    pub fn with_platform(mut self, platform: impl Into<String>) -> Self {
        self.platform = platform.into();
        self
    }

    /// The protection policy in use.
    pub fn policy(&self) -> Arc<dyn ProtectionPolicy> {
        self.policy.clone()
    }

    /// Take a snapshot.
    pub fn scan(&self, opts: &ScanOptions) -> Result<Scan, std::io::Error> {
        let started = Instant::now();
        let mut warnings = Vec::new();
        // Sockets, processes and containers are independent: collect them in parallel.
        let (raw, table, containers) = std::thread::scope(|s| {
            let containers = opts.docker.then(|| s.spawn(|| self.containers.published()));
            let table = s.spawn(|| self.processes.processes());
            let raw = self.sockets.sockets();
            (
                raw,
                table.join().unwrap_or_default(),
                containers.map(|h| h.join()),
            )
        });
        let raw = raw?;
        let (published, docker_available) = match containers {
            Some(Ok(r)) => r,
            Some(Err(_)) => {
                warnings.push("container runtime query failed".to_string());
                (Vec::new(), false)
            }
            None => (Vec::new(), false),
        };
        let (entries, hidden) = build_entries_with(
            &raw,
            &table,
            &published,
            opts.all_states,
            self.policy.as_ref(),
        );
        let snapshot = Snapshot {
            entries,
            hidden_sockets: hidden,
            platform: self.platform.clone(),
            taken_at_ms: crate::util::now_ms(),
            scan_ms: started.elapsed().as_millis() as u64,
            docker_available,
            warnings,
        };
        Ok(Scan {
            snapshot,
            table,
            published,
            raw,
        })
    }
}

fn exposure(addrs: &[IpAddr]) -> Exposure {
    if addrs.iter().any(|a| a.is_unspecified()) {
        Exposure::AllInterfaces
    } else if addrs.iter().all(is_loopback) {
        Exposure::Loopback
    } else {
        Exposure::Specific
    }
}

fn is_loopback(a: &IpAddr) -> bool {
    match a {
        IpAddr::V4(v) => v.is_loopback(),
        IpAddr::V6(v) => v.is_loopback() || v.to_ipv4_mapped().is_some_and(|m| m.is_loopback()),
    }
}

fn fmt_addr(a: &IpAddr) -> String {
    match a {
        IpAddr::V6(v) => match v.to_ipv4_mapped() {
            Some(m) => m.to_string(),
            None => v.to_string(),
        },
        IpAddr::V4(v) => v.to_string(),
    }
}

/// Names of processes that forward container ports on the host.
pub fn is_container_forwarder(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let n = n.strip_suffix(".exe").unwrap_or(&n);
    matches!(
        n,
        "docker-proxy"
            | "com.docker.backend"
            | "com.docker.vpnkit"
            | "vpnkit"
            | "vpnkit-bridge"
            | "docker desktop"
            | "orbstack helper"
            | "orbstack"
            | "rootlessport"
            | "gvproxy"
            | "slirp4netns"
            | "pasta"
            | "limactl"
            | "ssh"
            | "rancher-desktop"
            | "wslrelay"
            | "com.docker.dev-envs"
            | "rootlesskit"
    ) || n.starts_with("com.docker.")
}

/// Pick the "root" of a set of PIDs sharing a socket (the one whose parent isn't in the set).
fn primary_pid(pids: &[u32], table: &ProcessTable) -> Option<u32> {
    let mut v: Vec<u32> = pids.to_vec();
    v.sort_unstable();
    v.iter()
        .copied()
        .find(|p| {
            table
                .get(*p)
                .and_then(|i| i.ppid)
                .is_none_or(|pp| !v.contains(&pp))
        })
        .or_else(|| v.first().copied())
}

type Key = (Protocol, u16, Option<u32>, SocketState, Option<String>);

/// Group raw sockets into rows with the default protection policy.
pub fn build_entries(
    raw: &[RawSocket],
    table: &ProcessTable,
    published: &[PublishedPort],
    all_states: bool,
) -> (Vec<PortEntry>, usize) {
    build_entries_with(raw, table, published, all_states, &DefaultProtectionPolicy)
}

/// Group raw sockets into rows. Returns (entries, hidden listening sockets).
pub fn build_entries_with(
    raw: &[RawSocket],
    table: &ProcessTable,
    published: &[PublishedPort],
    all_states: bool,
    policy: &dyn ProtectionPolicy,
) -> (Vec<PortEntry>, usize) {
    let mut groups: BTreeMap<Key, (Vec<&RawSocket>, Vec<u32>)> = BTreeMap::new();
    let mut hidden = 0;
    for s in raw {
        // Local port 0 means the socket isn't bound to a port yet (macOS lists unbound UDP
        // sockets that way): nothing can connect to it and there's no port to free.
        if s.local_port == 0 {
            continue;
        }
        if !all_states && !s.state.is_listening() {
            continue;
        }
        if s.state.is_listening() && s.pids.is_empty() {
            hidden += 1;
        }
        let pid = primary_pid(&s.pids, table);
        let remote = if s.state.is_listening() {
            None
        } else {
            s.remote_addr.map(|a| match s.remote_port {
                Some(p) if a.is_ipv6() => format!("[{}]:{p}", fmt_addr(&a)),
                Some(p) => format!("{}:{p}", fmt_addr(&a)),
                None => fmt_addr(&a),
            })
        };
        let g = groups
            .entry((s.protocol, s.local_port, pid, s.state, remote))
            .or_default();
        g.0.push(s);
        for p in &s.pids {
            if !g.1.contains(p) {
                g.1.push(*p);
            }
        }
    }

    let mut detector = ProjectDetector::new();
    let mut entries = Vec::with_capacity(groups.len());
    for ((protocol, port, pid, state, remote), (socks, mut pids)) in groups {
        pids.sort_unstable();
        let mut addrs: Vec<IpAddr> = socks.iter().map(|s| s.local_addr).collect();
        addrs.sort();
        addrs.dedup();
        let mut families: Vec<Family> = socks.iter().map(|s| s.family).collect();
        families.sort();
        families.dedup();
        let uid = socks.iter().find_map(|s| s.uid);
        let process = pid.and_then(|p| table.get(p)).cloned();
        let project = process
            .as_ref()
            .and_then(|p| p.cwd.as_ref())
            .and_then(|cwd| detector.detect(cwd));
        let mut framework = process
            .as_ref()
            .and_then(|p| detect_framework(p, project.as_ref()));
        let proto_str = match protocol {
            Protocol::Tcp => "tcp",
            Protocol::Udp => "udp",
        };
        let forwarder = process
            .as_ref()
            .is_none_or(|p| is_container_forwarder(&p.name));
        let container = if state.is_listening() && forwarder {
            published
                .iter()
                .find(|pp| pp.host_port == port && pp.protocol == proto_str)
                .map(|pp| pp.container.clone())
        } else {
            None
        };
        if container.is_some() {
            framework = Some(Framework {
                name: "Container".into(),
                category: FrameworkCategory::Container,
            });
        }
        let user = process
            .as_ref()
            .and_then(|p| p.user.clone())
            .or_else(|| uid.and_then(|u| table.user_name(u)));
        let is_mine = match (pid, uid) {
            (Some(p), _) if table.get(p).is_some() => table.is_mine(p),
            (_, Some(u)) => table.current_uid() == Some(u),
            _ => false,
        };
        let protected = process
            .as_ref()
            .is_some_and(|p| policy.protection(p, table) != Protection::None);
        let is_dev = container.is_some()
            || framework.as_ref().is_some_and(|f| f.category.is_dev())
            || (project.is_some()
                && !framework.as_ref().is_some_and(|f| {
                    matches!(
                        f.category,
                        FrameworkCategory::App | FrameworkCategory::System
                    )
                }));
        let tunnel = process
            .as_ref()
            .filter(|_| state.is_listening())
            .and_then(|p| crate::tunnel::detect_tunnel(p, Some(port)));
        if tunnel.is_some() && framework.is_none() {
            framework = Some(Framework {
                name: "Port forward".into(),
                category: FrameworkCategory::Tool,
            });
        }
        let label = make_label(
            process.as_ref(),
            project.as_ref().map(|p| &p.info),
            framework.as_ref(),
            container.as_ref(),
            uid,
            user.as_deref(),
        );
        let id = format!(
            "{proto_str}:{port}:{}:{}{}",
            pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
            state.as_str(),
            remote.as_ref().map(|r| format!(":{r}")).unwrap_or_default()
        );
        entries.push(PortEntry {
            id,
            port,
            protocol,
            state,
            exposure: exposure(&addrs),
            addresses: addrs
                .iter()
                .map(fmt_addr)
                .collect::<Vec<_>>()
                .into_iter()
                .fold(Vec::new(), |mut v, a| {
                    if !v.contains(&a) {
                        v.push(a);
                    }
                    v
                }),
            families,
            remote,
            pid,
            pids,
            uid,
            user,
            process,
            project: project.map(|p| p.info),
            framework,
            container,
            label,
            is_dev,
            is_mine,
            protected,
            tunnel,
        });
    }

    // Published container ports with no visible host socket (e.g. userland-proxy disabled,
    // nftables DNAT, or a VM-based runtime): still "busy" from the user's point of view.
    for pp in published {
        let proto = if pp.protocol == "udp" {
            Protocol::Udp
        } else {
            Protocol::Tcp
        };
        if entries
            .iter()
            .any(|e| e.port == pp.host_port && e.protocol == proto && e.state.is_listening())
        {
            continue;
        }
        let addr = pp.host_ip.clone().unwrap_or_else(|| "0.0.0.0".into());
        let exposure = match addr.parse::<IpAddr>() {
            Ok(a) => exposure(&[a]),
            Err(_) => Exposure::AllInterfaces,
        };
        entries.push(PortEntry {
            id: format!(
                "{}:{}:ctr:{}",
                pp.protocol,
                pp.host_port,
                &pp.container.id[..pp.container.id.len().min(12)]
            ),
            port: pp.host_port,
            protocol: proto,
            state: if proto == Protocol::Udp {
                SocketState::Bound
            } else {
                SocketState::Listen
            },
            addresses: vec![addr],
            families: vec![Family::V4],
            remote: None,
            exposure,
            pid: None,
            pids: vec![],
            uid: None,
            user: None,
            process: None,
            project: None,
            framework: Some(Framework {
                name: "Container".into(),
                category: FrameworkCategory::Container,
            }),
            label: make_label(None, None, None, Some(&pp.container), None, None),
            container: Some(pp.container.clone()),
            is_dev: true,
            is_mine: true,
            protected: false,
            tunnel: None,
        });
    }
    entries.sort_by(|a, b| {
        (a.port, a.protocol, !a.state.is_listening(), a.pid).cmp(&(
            b.port,
            b.protocol,
            !b.state.is_listening(),
            b.pid,
        ))
    });
    (entries, hidden)
}

fn make_label(
    process: Option<&ProcessInfo>,
    project: Option<&ProjectInfo>,
    framework: Option<&Framework>,
    container: Option<&ContainerInfo>,
    uid: Option<u32>,
    user: Option<&str>,
) -> String {
    if let Some(c) = container {
        let svc = match (&c.compose_project, &c.compose_service) {
            (Some(p), Some(s)) => format!(" ({p}/{s})"),
            _ => String::new(),
        };
        return format!("{} · {}{svc} · {}", c.runtime, c.name, c.image);
    }
    let fw = framework.map(|f| f.name.as_str());
    match (fw, project) {
        (Some(f), Some(p)) => format!("{f} · {}", p.name),
        (Some(f), None) => f.to_string(),
        (None, Some(p)) => format!(
            "{} · {}",
            process.map(|p| p.name.as_str()).unwrap_or("?"),
            p.name
        ),
        (None, None) => match process {
            Some(p) => p.name.clone(),
            None => match (user, uid) {
                (Some(u), _) => format!("hidden (owned by {u})"),
                (None, Some(u)) => format!("hidden (uid {u})"),
                _ => "hidden (another user / root)".into(),
            },
        },
    }
}

/// Filter + query shared by CLI, TUI, GUI and MCP.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    /// Free-text query: words, `:3000` / `3000`, `3000-3999`, `proto:udp`, `user:me`.
    pub query: String,
    /// Only this protocol.
    pub protocol: Option<Protocol>,
    /// Only listening (TCP) / bound (UDP) sockets.
    pub listening_only: bool,
    /// Only likely development servers.
    pub dev_only: bool,
    /// Only processes owned by the current user.
    pub mine_only: bool,
    /// Only ports reachable from the network.
    pub exposed_only: bool,
    /// Only ports in this inclusive range.
    pub range: Option<(u16, u16)>,
}

impl Filter {
    /// True when `e` passes every condition.
    pub fn matches(&self, e: &PortEntry) -> bool {
        if self.listening_only && !e.state.is_listening() {
            return false;
        }
        if self.protocol.is_some_and(|p| p != e.protocol) {
            return false;
        }
        if self.dev_only && !e.is_dev {
            return false;
        }
        if self.mine_only && !e.is_mine {
            return false;
        }
        if self.exposed_only && e.exposure != Exposure::AllInterfaces {
            return false;
        }
        if let Some((lo, hi)) = self.range {
            if e.port < lo || e.port > hi {
                return false;
            }
        }
        self.query
            .split_whitespace()
            .all(|tok| token_matches(tok, e))
    }

    /// The entries that pass the filter.
    pub fn apply<'a>(&self, entries: &'a [PortEntry]) -> Vec<&'a PortEntry> {
        entries.iter().filter(|e| self.matches(e)).collect()
    }
}

/// Parse "3000-3999" into a range.
pub fn parse_range(s: &str) -> Option<(u16, u16)> {
    let (a, b) = s.split_once('-')?;
    let (a, b): (u16, u16) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    Some((a.min(b), a.max(b)))
}

fn token_matches(tok: &str, e: &PortEntry) -> bool {
    let t = tok.to_ascii_lowercase();
    if let Some(p) = t.strip_prefix(':') {
        return p.parse::<u16>().is_ok_and(|p| p == e.port)
            || parse_range(p).is_some_and(|(a, b)| (a..=b).contains(&e.port));
    }
    if let Ok(p) = t.parse::<u16>() {
        // A bare number matches the port exactly, or the PID.
        return p == e.port || e.pid == Some(p as u32);
    }
    if let Some((a, b)) = parse_range(&t) {
        return (a..=b).contains(&e.port);
    }
    if let Some(v) = t.strip_prefix("proto:") {
        return e.protocol.to_string().eq_ignore_ascii_case(v);
    }
    if let Some(v) = t.strip_prefix("pid:") {
        return v
            .parse::<u32>()
            .is_ok_and(|p| e.pids.contains(&p) || e.pid == Some(p));
    }
    if let Some(v) = t.strip_prefix("user:") {
        return (v == "me" && e.is_mine)
            || e.user.as_deref().is_some_and(|u| u.eq_ignore_ascii_case(v));
    }
    let mut hay = format!("{} {} {}", e.label, e.state, e.protocol).to_ascii_lowercase();
    if let Some(p) = &e.process {
        hay.push(' ');
        hay.push_str(&p.command().to_ascii_lowercase());
        hay.push(' ');
        hay.push_str(&p.name.to_ascii_lowercase());
    }
    if let Some(p) = &e.project {
        hay.push(' ');
        hay.push_str(&p.root.to_string_lossy().to_ascii_lowercase());
        if let Some(b) = &p.git_branch {
            hay.push(' ');
            hay.push_str(&b.to_ascii_lowercase());
        }
    }
    if let Some(u) = &e.user {
        hay.push(' ');
        hay.push_str(&u.to_ascii_lowercase());
    }
    for a in &e.addresses {
        hay.push(' ');
        hay.push_str(a);
    }
    hay.contains(&t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::{proc, table};

    fn sock(
        proto: Protocol,
        fam: Family,
        addr: &str,
        port: u16,
        state: SocketState,
        pids: &[u32],
    ) -> RawSocket {
        RawSocket {
            protocol: proto,
            family: fam,
            local_addr: addr.parse().unwrap(),
            local_port: port,
            remote_addr: None,
            remote_port: None,
            state,
            uid: Some(1000),
            inode: Some(1),
            pids: pids.to_vec(),
        }
    }

    #[test]
    fn skips_sockets_without_a_port() {
        let t = table(
            vec![proc(1, 0, "init", &[]), proc(40, 1, "sharingd", &[])],
            1,
        );
        let raw = vec![
            sock(
                Protocol::Udp,
                Family::V4,
                "0.0.0.0",
                0,
                SocketState::Bound,
                &[40],
            ),
            sock(
                Protocol::Udp,
                Family::V6,
                "::",
                0,
                SocketState::Bound,
                &[40],
            ),
            sock(
                Protocol::Udp,
                Family::V6,
                "::",
                56327,
                SocketState::Bound,
                &[40],
            ),
        ];
        for all_states in [false, true] {
            let (entries, hidden) = build_entries(&raw, &t, &[], all_states);
            assert_eq!(hidden, 0);
            let ports: Vec<u16> = entries.iter().map(|e| e.port).collect();
            assert_eq!(ports, [56327]);
        }
    }

    #[test]
    fn groups_dual_stack_and_labels() {
        let t = table(
            vec![
                proc(1, 0, "init", &[]),
                proc(20, 1, "node", &["node", "node_modules/.bin/vite"]),
                proc(
                    30,
                    1,
                    "docker-proxy",
                    &["docker-proxy", "-host-port", "5432"],
                ),
            ],
            1,
        );
        let raw = vec![
            sock(
                Protocol::Tcp,
                Family::V4,
                "127.0.0.1",
                5173,
                SocketState::Listen,
                &[20],
            ),
            sock(
                Protocol::Tcp,
                Family::V6,
                "::1",
                5173,
                SocketState::Listen,
                &[20],
            ),
            sock(
                Protocol::Tcp,
                Family::V4,
                "0.0.0.0",
                5432,
                SocketState::Listen,
                &[30],
            ),
            sock(
                Protocol::Tcp,
                Family::V4,
                "0.0.0.0",
                22,
                SocketState::Listen,
                &[],
            ),
            sock(
                Protocol::Tcp,
                Family::V4,
                "127.0.0.1",
                40000,
                SocketState::Established,
                &[20],
            ),
        ];
        let body = include_bytes!("../tests/fixtures/docker_containers.json");
        let rt = crate::docker::Runtime {
            endpoint: crate::docker::Endpoint::Unix("/x".into()),
            label: "Docker".into(),
        };
        let published = crate::docker::parse_containers(body, &rt).unwrap();
        let (entries, hidden) = build_entries(&raw, &t, &published, false);
        assert_eq!(hidden, 1);
        // 22 (hidden), 5173 (grouped), 5432 (container), 6380 (container, no host socket)
        let ports: Vec<u16> = entries.iter().map(|e| e.port).collect();
        assert_eq!(ports, vec![22, 5173, 5432, 6380]);
        let vite = &entries[1];
        assert_eq!(vite.addresses, vec!["127.0.0.1", "::1"]);
        assert_eq!(vite.families, vec![Family::V4, Family::V6]);
        assert_eq!(vite.exposure, Exposure::Loopback);
        assert_eq!(vite.label, "Vite");
        assert!(vite.is_dev);
        let pg = &entries[2];
        assert_eq!(pg.container.as_ref().unwrap().name, "shop-db-1");
        assert_eq!(pg.exposure, Exposure::AllInterfaces);
        assert!(pg.label.contains("shop-db-1"));
        assert!(entries[0].label.starts_with("hidden"));
        assert!(entries[3].pid.is_none() && entries[3].container.is_some());
        let hidden_rows: Vec<u16> = entries
            .iter()
            .filter(|e| e.is_hidden())
            .map(|e| e.port)
            .collect();
        assert_eq!(hidden_rows, [22]);

        let (all, _) = build_entries(&raw, &t, &[], true);
        assert!(all.iter().any(|e| e.state == SocketState::Established));
    }

    #[test]
    fn primary_pid_is_parent() {
        let t = table(
            vec![
                proc(10, 1, "nginx", &[]),
                proc(11, 10, "nginx", &[]),
                proc(12, 10, "nginx", &[]),
            ],
            1,
        );
        assert_eq!(primary_pid(&[12, 11, 10], &t), Some(10));
        assert_eq!(primary_pid(&[], &t), None);
    }

    #[test]
    fn filter_queries() {
        let t = table(vec![proc(20, 1, "node", &["node", "vite"])], 1);
        let raw = vec![
            sock(
                Protocol::Tcp,
                Family::V4,
                "0.0.0.0",
                5173,
                SocketState::Listen,
                &[20],
            ),
            sock(
                Protocol::Udp,
                Family::V4,
                "0.0.0.0",
                5353,
                SocketState::Bound,
                &[],
            ),
        ];
        let (e, _) = build_entries(&raw, &t, &[], false);
        let q = |s: &str| {
            Filter {
                query: s.into(),
                ..Default::default()
            }
            .apply(&e)
            .len()
        };
        assert_eq!(q(""), 2);
        assert_eq!(q("vite"), 1);
        assert_eq!(q(":5173"), 1);
        assert_eq!(q("5000-5200"), 1);
        assert_eq!(q("proto:udp"), 1);
        assert_eq!(q("pid:20"), 1);
        assert_eq!(q("vite proto:udp"), 0);
        assert_eq!(
            Filter {
                protocol: Some(Protocol::Tcp),
                ..Default::default()
            }
            .apply(&e)
            .len(),
            1
        );
        assert_eq!(
            Filter {
                exposed_only: true,
                ..Default::default()
            }
            .apply(&e)
            .len(),
            2
        );
        assert_eq!(
            Filter {
                dev_only: true,
                ..Default::default()
            }
            .apply(&e)
            .len(),
            1
        );
        assert_eq!(parse_range("9000-8000"), Some((8000, 9000)));
    }
}

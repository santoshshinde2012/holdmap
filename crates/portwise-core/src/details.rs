//! The extra facts behind a port that are too costly or noisy for every row: who is connected,
//! the process tree, how long it has run, and what its bind address means for safety.
//!
//! Built on demand for one port (`portwise inspect`, MCP `explain_port`, the app's details
//! pane) from a scan that includes connected sockets (`ScanOptions { all_states: true }`).

use crate::model::{Exposure, FrameworkCategory, PortEntry, Protocol, SocketState};
use crate::process::ProcessTable;
use crate::scan::Scan;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::IpAddr;

/// How many peers / descendants are listed before the rest are summarized.
const MAX_PEERS: usize = 8;
const MAX_CHILDREN: usize = 40;

/// Everything extra about one listener.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortDetails {
    /// The [`PortEntry::id`] this describes.
    pub id: String,
    /// Port number.
    pub port: u16,
    /// Process start time (Unix seconds), when known.
    pub started_at: Option<u64>,
    /// Seconds since it started.
    pub uptime_secs: Option<u64>,
    /// Connections to this port right now.
    pub connections: Connections,
    /// The owning process with its parents and children.
    pub tree: Option<ProcessTree>,
    /// What the bind address means, in plain words.
    pub bind_risk: BindRisk,
}

/// Open connections to a listening port.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Connections {
    /// Every connection on this port (any state).
    pub total: usize,
    /// Established connections.
    pub established: usize,
    /// Count per TCP state, e.g. `established: 3, time_wait: 1`.
    pub by_state: BTreeMap<String, usize>,
    /// Who is connected, busiest first.
    pub peers: Vec<Peer>,
    /// Peers not listed in `peers`.
    pub more_peers: usize,
}

/// One remote endpoint (or local client process) connected to the port.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Peer {
    /// Remote address (`192.168.1.20`, `127.0.0.1`).
    pub address: String,
    /// Connections from it.
    pub connections: usize,
    /// The peer is on this machine.
    pub local: bool,
    /// For local peers: the client process, when visible (`Google Chrome`, `psql`).
    pub process: Option<String>,
    /// Its PID.
    pub pid: Option<u32>,
}

/// One process in a [`ProcessTree`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TreeProcess {
    /// PID.
    pub pid: u32,
    /// Process name.
    pub name: String,
    /// Command line, secrets hidden.
    pub command: String,
    /// Depth below the listening process (children) or above it (ancestors, 1 = parent).
    pub depth: usize,
    /// Resident memory.
    pub memory_bytes: u64,
    /// CPU percent of one core.
    pub cpu_percent: f32,
}

/// The listening process in context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessTree {
    /// Parents, outermost first (`launchd` … `zsh`, `npm`).
    pub ancestors: Vec<TreeProcess>,
    /// The process holding the socket.
    pub process: TreeProcess,
    /// Children and their children, in tree order.
    pub children: Vec<TreeProcess>,
    /// Descendants not listed.
    pub more_children: usize,
}

/// How exposed a bind address is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Only this machine can connect.
    Low,
    /// Some other machines can connect.
    Medium,
    /// Anyone on the network can connect to something that usually has no login.
    High,
}

/// The bind address explained for people who don't think in `0.0.0.0`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindRisk {
    /// Low / medium / high.
    pub level: RiskLevel,
    /// One line: "Only this computer can connect".
    pub title: String,
    /// Why it matters.
    pub explanation: String,
    /// What to do about it, when anything.
    pub fix: Option<String>,
}

/// Details for every listener on `port` (one per owner / protocol).
pub fn for_port(scan: &Scan, port: u16) -> Vec<PortDetails> {
    scan.snapshot
        .entries
        .iter()
        .filter(|e| e.port == port && e.state.is_listening())
        .map(|e| for_entry(scan, e))
        .collect()
}

/// Details for one listener.
pub fn for_entry(scan: &Scan, e: &PortEntry) -> PortDetails {
    let now = crate::util::now_ms() / 1000;
    let started = e.process.as_ref().map(|p| p.start_time).filter(|t| *t > 0);
    PortDetails {
        id: e.id.clone(),
        port: e.port,
        started_at: started,
        uptime_secs: started.map(|t| now.saturating_sub(t)),
        connections: connections(scan, e),
        tree: e.pid.and_then(|pid| tree(&scan.table, pid)),
        bind_risk: bind_risk(e),
    }
}

fn is_loopback(a: &IpAddr) -> bool {
    match a {
        IpAddr::V4(v) => v.is_loopback(),
        IpAddr::V6(v) => v.is_loopback() || v.to_ipv4_mapped().is_some_and(|m| m.is_loopback()),
    }
}

fn state_key(s: SocketState) -> String {
    format!("{s:?}")
        .chars()
        .enumerate()
        .flat_map(|(i, c)| {
            let lower = c.to_ascii_lowercase();
            let sep = (i > 0 && c.is_ascii_uppercase()).then_some('_');
            sep.into_iter().chain(std::iter::once(lower))
        })
        .collect()
}

/// Connections whose local end is this port (the server side), grouped by peer.
pub fn connections(scan: &Scan, e: &PortEntry) -> Connections {
    if e.protocol != Protocol::Tcp {
        return Connections::default();
    }
    let mut c = Connections::default();
    let mut peers: BTreeMap<(String, Option<u32>), Peer> = BTreeMap::new();
    for s in &scan.raw {
        if s.protocol != Protocol::Tcp || s.local_port != e.port || s.state.is_listening() {
            continue;
        }
        // Only sockets of this listener's owner (another process may use the same port number
        // as a client-side ephemeral port on a different address).
        if !e.pids.is_empty() && !s.pids.is_empty() && !s.pids.iter().any(|p| e.pids.contains(p)) {
            continue;
        }
        let (Some(addr), Some(rport)) = (s.remote_addr, s.remote_port) else {
            continue;
        };
        c.total += 1;
        if s.state == SocketState::Established {
            c.established += 1;
        }
        *c.by_state.entry(state_key(s.state)).or_default() += 1;
        let local = is_loopback(&addr);
        // On this machine the client's own socket is in the table too: name the process.
        let client = local
            .then(|| {
                scan.raw.iter().find(|o| {
                    o.protocol == Protocol::Tcp
                        && o.local_port == rport
                        && o.remote_port == Some(e.port)
                        && o.remote_addr.is_some_and(|a| is_loopback(&a))
                })
            })
            .flatten()
            .and_then(|o| o.pids.first().copied());
        let name = client
            .and_then(|p| scan.table.get(p))
            .map(|p| p.name.clone());
        let key = (
            name.clone().unwrap_or_else(|| addr.to_string()),
            client.filter(|_| name.is_some()),
        );
        peers
            .entry(key)
            .or_insert_with(|| Peer {
                address: addr.to_string(),
                connections: 0,
                local,
                process: name,
                pid: client,
            })
            .connections += 1;
    }
    let mut list: Vec<Peer> = peers.into_values().collect();
    list.sort_by(|a, b| {
        b.connections
            .cmp(&a.connections)
            .then_with(|| a.address.cmp(&b.address))
    });
    c.more_peers = list.len().saturating_sub(MAX_PEERS);
    list.truncate(MAX_PEERS);
    c.peers = list;
    c
}

fn node(t: &ProcessTable, pid: u32, depth: usize) -> Option<TreeProcess> {
    let p = t.get(pid)?;
    Some(TreeProcess {
        pid,
        name: p.name.clone(),
        command: p.command(),
        depth,
        memory_bytes: p.memory_bytes,
        cpu_percent: p.cpu_percent,
    })
}

/// The process with its ancestors and descendants.
pub fn tree(t: &ProcessTable, pid: u32) -> Option<ProcessTree> {
    let process = node(t, pid, 0)?;
    let up = t.ancestors(pid);
    let n = up.len();
    let ancestors = up
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(i, p)| node(t, *p, i + 1))
        .collect::<Vec<_>>();
    debug_assert!(ancestors.len() <= n);
    // Depth-first so children sit under their parent.
    let mut children = Vec::new();
    let mut total = 0usize;
    let mut stack: Vec<(u32, usize)> = t.children(pid).iter().rev().map(|c| (*c, 1)).collect();
    let mut seen = std::collections::HashSet::from([pid]);
    while let Some((c, d)) = stack.pop() {
        if !seen.insert(c) {
            continue;
        }
        total += 1;
        if children.len() < MAX_CHILDREN {
            if let Some(n) = node(t, c, d) {
                children.push(n);
            }
        }
        stack.extend(t.children(c).iter().rev().map(|g| (*g, d + 1)));
    }
    Some(ProcessTree {
        ancestors,
        process,
        more_children: total.saturating_sub(children.len()),
        children,
    })
}

fn is_private(a: &IpAddr) -> bool {
    match a {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_private() || v.is_link_local() || (o[0] == 100 && (64..128).contains(&o[1]))
        }
        IpAddr::V6(v) => {
            let s = v.segments()[0];
            (s & 0xfe00) == 0xfc00 || (s & 0xffc0) == 0xfe80
        }
    }
}

/// The bind address in plain words, weighted by what is listening.
pub fn bind_risk(e: &PortEntry) -> BindRisk {
    let what = e
        .framework
        .as_ref()
        .map(|f| f.category)
        .unwrap_or(if e.container.is_some() {
            FrameworkCategory::Container
        } else {
            FrameworkCategory::App
        });
    let data = matches!(
        what,
        FrameworkCategory::Database | FrameworkCategory::Cache | FrameworkCategory::Queue
    );
    let dev = e.is_dev || matches!(what, FrameworkCategory::DevServer | FrameworkCategory::Tool);
    let local_fix = if e.container.is_some() {
        format!(
            "Publish it on loopback only: `-p 127.0.0.1:{0}:{1}` (compose: `\"127.0.0.1:{0}:{1}\"`).",
            e.port,
            e.container.as_ref().map(|c| c.private_port).unwrap_or(e.port)
        )
    } else if data {
        "Bind it to 127.0.0.1 (e.g. `bind 127.0.0.1` / `listen_addresses = 'localhost'`), or set a strong password.".to_string()
    } else {
        "Start it with `--host 127.0.0.1` (or `HOST=127.0.0.1`) unless you're testing from another device.".to_string()
    };
    match e.exposure {
        Exposure::Loopback => BindRisk {
            level: RiskLevel::Low,
            title: "Only this computer can connect".into(),
            explanation: format!(
                "It listens on {} (loopback), so other devices on your network can't reach it.",
                e.addresses.join(", ")
            ),
            fix: None,
        },
        Exposure::AllInterfaces => {
            let (level, why) = if data {
                (
                    RiskLevel::High,
                    "Databases and caches often accept connections without a password in development, so anyone on the same Wi-Fi could read or change your data.",
                )
            } else if dev {
                (
                    RiskLevel::High,
                    "Dev servers rarely ask for a login and can expose source files, debug pages and admin routes to anyone on the same Wi-Fi (a café, an office, a conference).",
                )
            } else {
                (
                    RiskLevel::Medium,
                    "Other devices on your network can connect. That's expected for things like file sharing, and a risk for anything without a login.",
                )
            };
            BindRisk {
                level,
                title: "Anyone on your network can connect".into(),
                explanation: format!(
                    "It listens on {} (every network interface). {why}",
                    e.addresses.join(", ")
                ),
                fix: Some(local_fix),
            }
        }
        Exposure::Specific => {
            let addrs: Vec<IpAddr> = e.addresses.iter().filter_map(|a| a.parse().ok()).collect();
            let private = !addrs.is_empty() && addrs.iter().all(is_private);
            BindRisk {
                level: if private {
                    RiskLevel::Medium
                } else {
                    RiskLevel::High
                },
                title: if private {
                    "Devices on one network can connect".into()
                } else {
                    "Reachable on a public address".into()
                },
                explanation: format!(
                    "It listens on {} only. {}",
                    e.addresses.join(", "),
                    if private {
                        "Devices on that network (home/office LAN or VPN) can reach it; the rest of the internet can't."
                    } else {
                        "That address can be reached from outside your network unless a firewall blocks it."
                    }
                ),
                fix: Some(local_fix),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Family, Framework, RawSocket};
    use crate::process::tests::{proc, table};

    fn sock(local: u16, remote: Option<(&str, u16)>, state: SocketState, pid: u32) -> RawSocket {
        RawSocket {
            protocol: Protocol::Tcp,
            family: Family::V4,
            local_addr: "127.0.0.1".parse().unwrap(),
            local_port: local,
            remote_addr: remote.map(|r| r.0.parse().unwrap()),
            remote_port: remote.map(|r| r.1),
            state,
            uid: None,
            inode: None,
            pids: vec![pid],
        }
    }

    fn entry(exposure: Exposure, cat: Option<FrameworkCategory>) -> PortEntry {
        let s = sample_scan();
        let mut e = s.snapshot.entries[0].clone();
        e.exposure = exposure;
        e.addresses = vec![match exposure {
            Exposure::Loopback => "127.0.0.1",
            Exposure::AllInterfaces => "0.0.0.0",
            Exposure::Specific => "192.168.1.5",
        }
        .into()];
        e.framework = cat.map(|category| Framework {
            name: "x".into(),
            category,
        });
        e.is_dev = false;
        e
    }

    fn sample_scan() -> Scan {
        let t = table(
            vec![
                proc(1, 0, "launchd", &["launchd"]),
                proc(10, 1, "zsh", &["-zsh"]),
                proc(20, 10, "npm", &["npm", "run", "dev"]),
                proc(30, 20, "node", &["node", "server.js", "--token=abc"]),
                proc(31, 30, "esbuild", &["esbuild", "--service"]),
                proc(40, 1, "Google Chrome", &["chrome"]),
            ],
            999,
        );
        let raw = vec![
            sock(3000, None, SocketState::Listen, 30),
            sock(
                3000,
                Some(("127.0.0.1", 51000)),
                SocketState::Established,
                30,
            ),
            sock(
                51000,
                Some(("127.0.0.1", 3000)),
                SocketState::Established,
                40,
            ),
            sock(
                3000,
                Some(("127.0.0.1", 51001)),
                SocketState::Established,
                30,
            ),
            sock(
                51001,
                Some(("127.0.0.1", 3000)),
                SocketState::Established,
                40,
            ),
            sock(
                3000,
                Some(("192.168.1.20", 60000)),
                SocketState::TimeWait,
                30,
            ),
        ];
        crate::scan::Scanner::fixed(raw, t, Vec::new())
            .scan(&crate::scan::ScanOptions {
                all_states: true,
                docker: false,
            })
            .unwrap()
    }

    #[test]
    fn connections_name_local_clients_and_count_states() {
        let s = sample_scan();
        let d = for_port(&s, 3000);
        assert_eq!(d.len(), 1);
        let c = &d[0].connections;
        assert_eq!((c.total, c.established), (3, 2));
        assert_eq!(c.by_state.get("time_wait"), Some(&1));
        assert_eq!(c.peers[0].process.as_deref(), Some("Google Chrome"));
        assert_eq!(c.peers[0].connections, 2);
        assert!(c.peers[0].local);
        assert_eq!(c.peers[1].address, "192.168.1.20");
        assert!(!c.peers[1].local);
    }

    #[test]
    fn tree_shows_parents_outermost_first_and_children() {
        let s = sample_scan();
        let t = tree(&s.table, 30).unwrap();
        let names: Vec<&str> = t.ancestors.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["launchd", "zsh", "npm"]);
        assert_eq!(t.ancestors[2].depth, 1, "npm is the parent");
        assert_eq!(t.process.command, "node server.js --token=••••");
        assert_eq!(t.children.len(), 1);
        assert_eq!(t.children[0].name, "esbuild");
    }

    #[test]
    fn bind_risk_reads_plainly() {
        assert_eq!(
            bind_risk(&entry(Exposure::Loopback, None)).level,
            RiskLevel::Low
        );
        let db = bind_risk(&entry(
            Exposure::AllInterfaces,
            Some(FrameworkCategory::Database),
        ));
        assert_eq!(db.level, RiskLevel::High);
        assert!(db.explanation.contains("password"));
        assert!(db.fix.unwrap().contains("127.0.0.1"));
        let app = bind_risk(&entry(
            Exposure::AllInterfaces,
            Some(FrameworkCategory::App),
        ));
        assert_eq!(app.level, RiskLevel::Medium);
        let lan = bind_risk(&entry(Exposure::Specific, None));
        assert_eq!(lan.level, RiskLevel::Medium);
        assert_eq!(state_key(SocketState::TimeWait), "time_wait");
    }
}

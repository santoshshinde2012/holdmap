//! AI coding agents on this machine and their footprint.
//!
//! Built from one [`Scan`] like the [topology](crate::topology): each process is matched against
//! the [`catalog`]; the topmost process of a product is an agent, and everything started under
//! it (helpers, shells, dev servers, MCP servers) belongs to it, up to the next agent. From
//! that the [`AgentsBuilder`] derives:
//!
//! * **folders**: working directories of the agent and its processes, plus recent projects
//!   from the [`recent`] sources that are safe to read;
//! * **ports**: listening sockets held by the agent or anything it started;
//! * **links**: live TCP connections, grouped by local service or remote host (by IP only);
//! * **access**: account, sandbox and approval hints, network exposure and macOS privacy areas
//!   (see [`access`]), each marked observed, inferred or unknown.
//!
//! Nothing here reads chats, settings that may hold tokens, or files in privacy-protected
//! folders.

pub mod access;
pub mod catalog;
mod model;
pub mod recent;

pub use catalog::{identify, AgentKind, AgentProduct, CATALOG};
pub use model::*;
pub use recent::{HomeRecent, NoRecent, RecentProjects};

use crate::model::{PortEntry, ProcessInfo, ProjectInfo, Protocol, SocketState};
use crate::process::ProcessTable;
use crate::project::ProjectDetector;
use crate::scan::Scan;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// Processes listed per agent.
pub const MAX_PROCESSES: usize = 40;
/// Folders listed per agent (working directories), before recent projects.
pub const MAX_FOLDERS: usize = 12;
/// Link groups listed per agent.
pub const MAX_LINKS: usize = 16;

/// Builds an [`AgentsReport`] from a scan.
#[derive(Debug)]
pub struct AgentsBuilder<'a> {
    scan: &'a Scan,
    recent: &'a dyn RecentProjects,
    home: Option<PathBuf>,
    detect_projects: bool,
    project_budget: std::time::Duration,
}

impl<'a> AgentsBuilder<'a> {
    /// A builder over `scan` without recent-project sources, for the current user's home.
    pub fn new(scan: &'a Scan) -> Self {
        static NONE: NoRecent = NoRecent;
        Self {
            scan,
            recent: &NONE,
            home: std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from),
            detect_projects: true,
            project_budget: crate::project::scan_budget(),
        }
    }

    /// Where recent projects come from.
    pub fn with_recent(mut self, recent: &'a dyn RecentProjects) -> Self {
        self.recent = recent;
        self
    }

    /// The home directory used for privacy areas and project detection.
    pub fn with_home(mut self, home: Option<PathBuf>) -> Self {
        self.home = home;
        self
    }

    /// Whether to look for a project (manifest, git) in working directories that no port
    /// entry already explains. Never done inside privacy-protected folders.
    pub fn with_project_detection(mut self, on: bool) -> Self {
        self.detect_projects = on;
        self
    }

    /// How long to wait, in total, for project lookups that aren't cached yet (default: the
    /// scan budget). Lookups still running are left empty and show up on a later build.
    pub fn with_project_budget(mut self, budget: std::time::Duration) -> Self {
        self.project_budget = budget;
        self
    }

    fn table(&self) -> &ProcessTable {
        &self.scan.table
    }

    /// Agent roots and, for every process under one, the root it belongs to.
    fn assign(&self) -> (Vec<(u32, &'static AgentProduct)>, HashMap<u32, u32>) {
        let t = self.table();
        let matched: HashMap<u32, &'static AgentProduct> = t
            .iter()
            .filter_map(|p| identify(p).map(|x| (p.pid, x)))
            .collect();
        // The topmost process of a product is the agent; the same product below it is a helper.
        let mut roots: Vec<(u32, &'static AgentProduct)> = matched
            .iter()
            .filter(|(pid, prod)| {
                !t.ancestors(**pid)
                    .iter()
                    .any(|a| matched.get(a).is_some_and(|x| x.id == prod.id))
            })
            .map(|(pid, prod)| (*pid, *prod))
            .collect();
        roots.sort_by_key(|(pid, _)| *pid);
        // An editor, desktop app or agent host is one agent even when a helper was re-parented
        // away from its main process: fold extra roots of the same product and user into one.
        let mut alias: HashMap<u32, u32> = HashMap::new();
        let mut primary: HashMap<(&str, Option<u32>), u32> = HashMap::new();
        let is_helper = |pid: u32| {
            t.get(pid)
                .is_some_and(|p| catalog::norm(&p.name).contains("helper"))
        };
        let mut ordered = roots.clone();
        ordered.sort_by_key(|(pid, _)| (is_helper(*pid), *pid));
        for (pid, prod) in &ordered {
            if !matches!(
                prod.kind,
                AgentKind::Ide | AgentKind::Desktop | AgentKind::Host
            ) {
                continue;
            }
            let key = (prod.id, t.get(*pid).and_then(|p| p.uid));
            match primary.get(&key) {
                Some(main) => {
                    alias.insert(*pid, *main);
                }
                None => {
                    primary.insert(key, *pid);
                }
            }
        }
        roots.retain(|(pid, _)| !alias.contains_key(pid));
        let root_set: HashSet<u32> = roots.iter().map(|(p, _)| *p).collect();
        let mut owner: HashMap<u32, u32> = HashMap::new();
        for p in t.iter() {
            let chain = std::iter::once(p.pid).chain(t.ancestors(p.pid));
            for a in chain {
                let a = alias.get(&a).copied().unwrap_or(a);
                if root_set.contains(&a) {
                    owner.insert(p.pid, a);
                    break;
                }
            }
        }
        (roots, owner)
    }

    /// Build the report.
    pub fn build(&self) -> AgentsReport {
        let t = self.table();
        let platform = self.scan.snapshot.platform.as_str();
        let (roots, owner) = self.assign();
        let mut members: HashMap<u32, Vec<u32>> = HashMap::new();
        for (pid, root) in &owner {
            members.entry(*root).or_default().push(*pid);
        }
        let known_projects: Vec<&ProjectInfo> = self
            .scan
            .snapshot
            .entries
            .iter()
            .filter_map(|e| e.project.as_ref())
            .collect();
        let known_paths = self.known_paths();
        let mut detector = ProjectDetector::with_cache(
            crate::project::ProjectCache::global(),
            self.home.clone(),
            self.project_budget,
        );
        if self.detect_projects {
            // Start every lookup at once so one slow folder doesn't use up the budget.
            let home = self.home.as_deref();
            detector.prefetch(
                owner
                    .keys()
                    .filter_map(|p| t.get(*p)?.cwd.as_deref())
                    .filter(|c| c.parent().is_some())
                    .filter(|c| access::privacy_area(c, home, platform).is_none()),
            );
        }
        let local_addrs: HashSet<IpAddr> =
            self.scan.raw.iter().map(|s| norm(s.local_addr)).collect();

        let mut agents: Vec<Agent> = roots
            .iter()
            .filter_map(|(pid, product)| {
                let root = t.get(*pid)?;
                let mut pids = members.remove(pid).unwrap_or_default();
                pids.sort_unstable();
                let parent = t
                    .ancestors(*pid)
                    .into_iter()
                    .find_map(|a| owner.get(&a).copied())
                    .map(|r| format!("agent:{r}"));
                Some(self.agent(
                    root,
                    product,
                    &pids,
                    parent,
                    &known_projects,
                    &known_paths,
                    &mut detector,
                    &local_addrs,
                    platform,
                ))
            })
            .collect();
        agents.sort_by(|a, b| a.name.cmp(&b.name).then(a.pid.cmp(&b.pid)));

        let mut limits = vec![
            "Folders are working directories and the agents' recent-project lists; open files aren't collected.".to_string(),
            "Remote hosts are shown by IP address; portwise doesn't look names up.".to_string(),
            "Chats, settings, tokens and credentials are never read.".to_string(),
        ];
        if platform == "macos" {
            limits.push("macOS privacy grants (TCC) can't be read without Full Disk Access, and folders it guards aren't inspected.".into());
        }
        if self.scan.snapshot.hidden_sockets > 0 {
            limits.push(format!(
                "{} listening sockets belong to processes portwise can't see (other users); they can't be attributed.",
                self.scan.snapshot.hidden_sockets
            ));
        }
        AgentsReport {
            agents,
            platform: platform.to_string(),
            taken_at_ms: self.scan.snapshot.taken_at_ms,
            limits,
        }
    }

    /// Paths seen on the machine: working directories, project roots and their parents.
    fn known_paths(&self) -> HashSet<PathBuf> {
        let mut out = HashSet::new();
        let mut add = |p: &Path| {
            for a in p.ancestors().take(6) {
                if a.parent().is_none() {
                    break;
                }
                out.insert(a.to_path_buf());
            }
        };
        for p in self.table().iter() {
            if let Some(c) = &p.cwd {
                add(c);
            }
        }
        for e in &self.scan.snapshot.entries {
            if let Some(pr) = &e.project {
                add(&pr.root);
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn agent(
        &self,
        root: &ProcessInfo,
        product: &AgentProduct,
        pids: &[u32],
        parent: Option<String>,
        known_projects: &[&ProjectInfo],
        known_paths: &HashSet<PathBuf>,
        detector: &mut ProjectDetector,
        local_addrs: &HashSet<IpAddr>,
        platform: &str,
    ) -> Agent {
        let t = self.table();
        let set: HashSet<u32> = pids.iter().copied().collect();
        let role = |p: &ProcessInfo| {
            if p.pid == root.pid {
                ProcessRole::Agent
            } else if identify(p).is_some_and(|x| x.id == product.id) {
                ProcessRole::Helper
            } else {
                ProcessRole::Child
            }
        };
        let procs: Vec<&ProcessInfo> = pids.iter().filter_map(|p| t.get(*p)).collect();
        let mut processes: Vec<AgentProcess> = procs
            .iter()
            .map(|p| AgentProcess {
                pid: p.pid,
                ppid: p.ppid,
                name: p.name.clone(),
                command: p.command(),
                role: role(p),
                cwd: p.cwd.clone(),
                memory_bytes: p.memory_bytes,
                cpu_percent: p.cpu_percent,
            })
            .collect();
        processes.sort_by_key(|p| (p.role as u8, p.pid));
        let more_processes = processes.len().saturating_sub(MAX_PROCESSES);
        let memory_bytes = procs.iter().map(|p| p.memory_bytes).sum();
        let cpu_percent = procs.iter().map(|p| p.cpu_percent).sum();
        processes.truncate(MAX_PROCESSES);

        let mut folders = self.folders(root, &procs, known_projects, detector, platform);
        let listed: HashSet<PathBuf> = folders.iter().map(|f| f.path.clone()).collect();
        for r in self.recent.recent(product.id, known_paths) {
            if listed.contains(&r.path) {
                continue;
            }
            folders.push(AgentFolder {
                label: folder_label(&r.path, self.home.as_deref()),
                privacy_area: access::privacy_area(&r.path, self.home.as_deref(), platform)
                    .map(str::to_string),
                project: None,
                source: FolderSource::Recent,
                evidence: r.evidence,
                pids: Vec::new(),
                note: Some(r.note),
                path: r.path,
            });
        }

        let helper_pids: HashSet<u32> = procs
            .iter()
            .filter(|p| role(p) != ProcessRole::Child)
            .map(|p| p.pid)
            .collect();
        let ports = self.ports(&set, &helper_pids);
        let (links, more_links) = self.links(&set, &ports, local_addrs);
        let remote_hosts = links
            .iter()
            .filter(|l| l.kind == LinkKind::Remote)
            .map(|l| {
                l.address
                    .rsplit_once(':')
                    .map_or(l.address.as_str(), |(h, _)| h)
            })
            .collect::<HashSet<_>>()
            .len();
        let ancestors: Vec<&ProcessInfo> = t
            .ancestors(root.pid)
            .iter()
            .filter_map(|a| t.get(*a))
            .collect();
        let mine = t.is_mine(root.pid);
        let facts = vec![
            access::user(root, mine),
            access::sandbox(product.id, root, &ancestors, &procs),
            access::approvals(product.id, root),
            access::network(&ports, remote_hosts),
            access::privacy(platform, &folders),
        ];
        Agent {
            id: format!("agent:{}", root.pid),
            product: product.id.to_string(),
            name: product.name.to_string(),
            vendor: product.vendor.to_string(),
            kind: product.kind,
            pid: root.pid,
            process_name: root.name.clone(),
            command: root.command(),
            started_at: root.start_time,
            parent,
            memory_bytes,
            cpu_percent,
            processes,
            more_processes,
            folders,
            ports,
            links,
            more_links,
            access: AgentAccess {
                user: root.user.clone(),
                uid: root.uid,
                root: root.uid == Some(0),
                mine,
                facts,
            },
        }
    }

    fn folders(
        &self,
        root: &ProcessInfo,
        procs: &[&ProcessInfo],
        known_projects: &[&ProjectInfo],
        detector: &mut ProjectDetector,
        platform: &str,
    ) -> Vec<AgentFolder> {
        // Working directory → processes there, agent's own first.
        let mut at: BTreeMap<PathBuf, Vec<u32>> = BTreeMap::new();
        for p in procs {
            let Some(cwd) = &p.cwd else { continue };
            if cwd.parent().is_none() || cwd.to_string_lossy().contains(".app/Contents") {
                continue; // "/" and app bundles say nothing about the work
            }
            at.entry(cwd.clone()).or_default().push(p.pid);
        }
        let mut list: Vec<(PathBuf, Vec<u32>)> = at.into_iter().collect();
        list.sort_by_key(|(path, pids)| {
            (
                root.cwd.as_ref() != Some(path),
                std::cmp::Reverse(pids.len()),
                path.clone(),
            )
        });
        list.truncate(MAX_FOLDERS);
        let home = self.home.as_deref();
        list.into_iter()
            .map(|(path, pids)| {
                let area = access::privacy_area(&path, home, platform);
                let known = known_projects
                    .iter()
                    .filter(|p| path.starts_with(&p.root))
                    .max_by_key(|p| p.root.components().count())
                    .map(|p| (*p).clone());
                let (project, note) = match (known, area) {
                    (Some(p), _) => (Some(p), None),
                    (None, Some(a)) => (
                        None,
                        Some(format!("Not inspected: inside {a}, which macOS guards.")),
                    ),
                    (None, None) if self.detect_projects => {
                        (detector.detect(&path).map(|d| d.info), None)
                    }
                    (None, None) => (None, None),
                };
                AgentFolder {
                    label: project
                        .as_ref()
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| folder_label(&path, home)),
                    source: if root.cwd.as_ref() == Some(&path) {
                        FolderSource::Agent
                    } else {
                        FolderSource::Child
                    },
                    evidence: Evidence::Observed,
                    privacy_area: area.map(str::to_string),
                    project,
                    pids,
                    note,
                    path,
                }
            })
            .collect()
    }

    fn ports(&self, set: &HashSet<u32>, helpers: &HashSet<u32>) -> Vec<AgentPort> {
        let holder = |e: &PortEntry| {
            e.pid
                .into_iter()
                .chain(e.pids.iter().copied())
                .find(|p| set.contains(p))
        };
        let mut out: Vec<AgentPort> = self
            .scan
            .snapshot
            .entries
            .iter()
            .filter(|e| e.state.is_listening())
            .filter_map(|e| {
                let pid = holder(e)?;
                let role = if helpers.contains(&pid) {
                    PortRole::Agent
                } else if e.is_dev {
                    PortRole::DevServer
                } else {
                    PortRole::Service
                };
                Some(AgentPort {
                    entry_id: e.id.clone(),
                    port: e.port,
                    protocol: e.protocol,
                    exposure: e.exposure,
                    pid: Some(pid),
                    process: e.process.as_ref().map(|p| p.name.clone()),
                    label: e.label.clone(),
                    role,
                    project: e.project.as_ref().map(|p| p.name.clone()),
                    framework: e.framework.as_ref().map(|f| f.name.clone()),
                })
            })
            .collect();
        out.sort_by_key(|p| (p.role as u8, p.port, p.protocol as u8));
        out
    }

    fn links(
        &self,
        set: &HashSet<u32>,
        ports: &[AgentPort],
        local_addrs: &HashSet<IpAddr>,
    ) -> (Vec<AgentLink>, usize) {
        let own_ports: HashSet<u16> = ports
            .iter()
            .filter(|p| p.protocol == Protocol::Tcp)
            .map(|p| p.port)
            .collect();
        let listeners: Vec<&PortEntry> = self
            .scan
            .snapshot
            .entries
            .iter()
            .filter(|e| e.protocol == Protocol::Tcp && e.state.is_listening())
            .collect();
        let is_local = |a: IpAddr| {
            let a = norm(a);
            a.is_loopback() || a.is_unspecified() || local_addrs.contains(&a)
        };
        let mut groups: BTreeMap<String, AgentLink> = BTreeMap::new();
        for s in &self.scan.raw {
            if s.protocol != Protocol::Tcp || s.state != SocketState::Established {
                continue;
            }
            let (Some(raddr), Some(rport)) = (s.remote_addr, s.remote_port) else {
                continue;
            };
            if !s.pids.iter().any(|p| set.contains(p)) {
                continue;
            }
            // The accepted side of a connection to one of the agent's own listeners.
            if own_ports.contains(&s.local_port) {
                continue;
            }
            let link = if is_local(raddr) {
                let target = listeners.iter().find(|e| e.port == rport).copied();
                if target.is_some_and(|e| {
                    e.pid
                        .into_iter()
                        .chain(e.pids.iter().copied())
                        .any(|p| set.contains(&p))
                }) {
                    continue; // between the agent's own processes
                }
                match target {
                    Some(e) => AgentLink {
                        id: format!("local:{}", e.id),
                        kind: LinkKind::Local,
                        label: e.label.clone(),
                        address: fmt_endpoint(raddr, rport),
                        port: rport,
                        connections: 0,
                        entry_id: Some(e.id.clone()),
                        process: e.process.as_ref().map(|p| p.name.clone()),
                        pid: e.pid,
                        service: access::service_hint(rport).map(str::to_string),
                    },
                    None => AgentLink {
                        id: format!("local::{rport}"),
                        kind: LinkKind::Local,
                        label: format!("localhost:{rport}"),
                        address: fmt_endpoint(raddr, rport),
                        port: rport,
                        connections: 0,
                        entry_id: None,
                        process: None,
                        pid: None,
                        service: access::service_hint(rport).map(str::to_string),
                    },
                }
            } else {
                let address = fmt_endpoint(raddr, rport);
                AgentLink {
                    id: format!("remote:{address}"),
                    kind: LinkKind::Remote,
                    label: address.clone(),
                    address,
                    port: rport,
                    connections: 0,
                    entry_id: None,
                    process: None,
                    pid: None,
                    service: access::service_hint(rport).map(str::to_string),
                }
            };
            groups.entry(link.id.clone()).or_insert(link).connections += 1;
        }
        let mut out: Vec<AgentLink> = groups.into_values().collect();
        out.sort_by(|a, b| {
            (a.kind as u8)
                .cmp(&(b.kind as u8))
                .then(b.connections.cmp(&a.connections))
                .then(a.id.cmp(&b.id))
        });
        let more = out.len().saturating_sub(MAX_LINKS);
        out.truncate(MAX_LINKS);
        (out, more)
    }
}

fn norm(a: IpAddr) -> IpAddr {
    match a {
        IpAddr::V6(v) => v.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(v)),
        v4 => v4,
    }
}

fn fmt_endpoint(a: IpAddr, port: u16) -> String {
    match norm(a) {
        IpAddr::V6(v) => format!("[{v}]:{port}"),
        IpAddr::V4(v) => format!("{v}:{port}"),
    }
}

/// A folder's display name: its own name, or `~` for the home directory.
fn folder_label(path: &Path, home: Option<&Path>) -> String {
    if home == Some(path) {
        return "~".into();
    }
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests;

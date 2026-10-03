//! [`TopologyBuilder`]: listening entries + established sockets → service graph.

use super::cluster::{supervisor_tool, ClusterRegistry, NodeFacts};
use super::model::*;
use crate::engine::is_launcher;
use crate::model::{PortEntry, ProjectInfo, Protocol, RawSocket, SocketState};
use crate::process::ProcessTable;
use crate::project::ProjectDetector;
use crate::safety::{DefaultProtectionPolicy, ProtectionPolicy};
use crate::scan::{is_container_forwarder, Scan};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::IpAddr;

/// Builds a [`Graph`] from a [`Scan`]. Pure: reads only the scan (plus project files on disk for
/// client processes), so it is fully testable with fixtures.
pub struct TopologyBuilder<'a> {
    scan: &'a Scan,
    policy: &'a dyn ProtectionPolicy,
    clusters: ClusterRegistry,
    external: bool,
}

/// Key of the listener a connection targets.
#[derive(Clone)]
struct Listener {
    addr: IpAddr,
    node: String,
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

impl<'a> TopologyBuilder<'a> {
    /// A builder over `scan` with the default policy and cluster detectors.
    pub fn new(scan: &'a Scan) -> Self {
        static DEFAULT: DefaultProtectionPolicy = DefaultProtectionPolicy;
        Self {
            scan,
            policy: &DEFAULT,
            clusters: ClusterRegistry::default(),
            external: true,
        }
    }

    /// Use a custom protection policy.
    pub fn with_policy(mut self, policy: &'a dyn ProtectionPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Use a custom cluster registry.
    pub fn with_clusters(mut self, clusters: ClusterRegistry) -> Self {
        self.clusters = clusters;
        self
    }

    /// Include the collapsed "external hosts" node and its edges (default: true).
    pub fn with_external(mut self, external: bool) -> Self {
        self.external = external;
        self
    }

    fn table(&self) -> &'a ProcessTable {
        &self.scan.table
    }

    /// Service identity: climb launchers (npm → sh -c → node) like the stop engine does, but stop
    /// below supervisors so `turbo`/`concurrently` children stay separate services.
    pub fn service_root(&self, pid: u32) -> u32 {
        let t = self.table();
        let mut root = pid;
        let mut child = t.get(pid);
        for anc in t.ancestors(pid) {
            let Some(a) = t.get(anc) else { break };
            if anc <= 1
                || t.is_self_or_ancestor(anc)
                || self.policy.protection(a, t).is_protected()
                || supervisor_tool(a).is_some()
                || (t.is_mine(pid) && !t.is_mine(anc))
            {
                break;
            }
            let reloader = child.is_some_and(|c| !c.cmdline.is_empty() && c.cmdline == a.cmdline);
            if !(is_launcher(a) || reloader) {
                break;
            }
            root = anc;
            child = Some(a);
        }
        root
    }

    fn node_id_for_entry(&self, e: &PortEntry) -> String {
        if let Some(c) = &e.container {
            return format!("ctr:{}", &c.id[..c.id.len().min(12)]);
        }
        match e.pid {
            Some(pid) => format!("svc:{}", self.service_root(pid)),
            None => format!("port:{}:{}", e.protocol, e.port),
        }
    }

    fn tree_pids(&self, root: u32) -> Vec<u32> {
        let mut v = vec![root];
        v.extend(self.table().descendants(root));
        v
    }

    fn usage(&self, pids: &[u32]) -> (f32, u64) {
        pids.iter()
            .filter_map(|p| self.table().get(*p))
            .fold((0.0, 0), |(c, m), p| {
                (c + p.cpu_percent, m + p.memory_bytes)
            })
    }

    fn new_node_from_entry(&self, id: &str, e: &PortEntry) -> Node {
        let kind = if e.container.is_some() {
            NodeKind::Container
        } else if e.process.is_none() {
            NodeKind::Hidden
        } else {
            NodeKind::Service
        };
        let root_pid = e.pid.map(|p| self.service_root(p));
        // A protected host (IDE server, terminal) "owns" every process below it; don't adopt them.
        let pids = match root_pid {
            Some(r) if !e.protected => self.tree_pids(r),
            Some(r) => {
                let mut v = vec![r];
                v.extend(e.pids.iter().copied().filter(|p| *p != r));
                v
            }
            None => Vec::new(),
        };
        let (cpu, mem) = self.usage(&pids);
        let label = if let Some(c) = &e.container {
            c.compose_service.clone().unwrap_or_else(|| c.name.clone())
        } else if let Some(p) = &e.project {
            p.name.clone()
        } else if let Some(f) = &e.framework {
            f.name.clone()
        } else if let Some(p) = &e.process {
            p.name.clone()
        } else {
            e.label.clone()
        };
        let subtitle = e
            .framework
            .as_ref()
            .map(|f| f.name.clone())
            .or_else(|| e.process.as_ref().map(|p| p.name.clone()))
            .filter(|s| *s != label);
        Node {
            id: id.to_string(),
            kind,
            label,
            subtitle,
            root_pid,
            pids,
            ports: Vec::new(),
            framework: e.framework.clone(),
            project: e.project.as_ref().map(|p| p.name.clone()),
            project_root: e.project.as_ref().map(|p| p.root.clone()),
            container: e.container.clone(),
            tunnel: e.tunnel.clone(),
            cluster: None,
            cpu_percent: cpu,
            memory_bytes: mem,
            is_dev: e.is_dev,
            protected: e.protected,
        }
    }

    fn client_node(
        &self,
        root: u32,
        detector: &mut ProjectDetector,
    ) -> Option<(Node, Option<ProjectInfo>)> {
        let t = self.table();
        let p = t.get(root)?;
        let project = p.cwd.as_ref().and_then(|c| detector.detect(c));
        let framework = crate::project::detect_framework(p, project.as_ref());
        let pids = self.tree_pids(root);
        let (cpu, mem) = self.usage(&pids);
        let label = project
            .as_ref()
            .map(|d| d.info.name.clone())
            .unwrap_or_else(|| p.name.clone());
        let subtitle = framework
            .as_ref()
            .map(|f| f.name.clone())
            .or_else(|| Some(p.name.clone()))
            .filter(|s| *s != label);
        let protected = self.policy.protection(p, t).is_protected();
        let is_dev = project.is_some() && !protected
            || framework.as_ref().is_some_and(|f| f.category.is_dev());
        Some((
            Node {
                id: format!("svc:{root}"),
                kind: NodeKind::Client,
                label,
                subtitle,
                root_pid: Some(root),
                pids,
                ports: Vec::new(),
                framework,
                project: project.as_ref().map(|d| d.info.name.clone()),
                project_root: project.as_ref().map(|d| d.info.root.clone()),
                container: None,
                tunnel: crate::tunnel::detect_tunnel(p, None),
                cluster: None,
                cpu_percent: cpu,
                memory_bytes: mem,
                is_dev,
                protected,
            },
            project.map(|d| d.info),
        ))
    }

    /// Build the graph.
    pub fn build(&self) -> Graph {
        let snap = &self.scan.snapshot;
        let mut nodes: BTreeMap<String, Node> = BTreeMap::new();
        let mut projects: HashMap<String, ProjectInfo> = HashMap::new();
        let mut listeners: HashMap<u16, Vec<Listener>> = HashMap::new();

        // 1. Services: every TCP/UDP listener, grouped by service root / container.
        for e in snap.entries.iter().filter(|e| e.state.is_listening()) {
            let id = self.node_id_for_entry(e);
            let node = nodes
                .entry(id.clone())
                .or_insert_with(|| self.new_node_from_entry(&id, e));
            if !node
                .ports
                .iter()
                .any(|p| p.port == e.port && p.protocol == e.protocol)
            {
                node.ports.push(NodePort {
                    port: e.port,
                    protocol: e.protocol,
                    exposure: e.exposure,
                    entry_id: e.id.clone(),
                });
            }
            node.is_dev |= e.is_dev;
            if node.tunnel.is_none() {
                node.tunnel = e.tunnel.clone();
            }
            if let Some(p) = &e.project {
                projects.entry(id.clone()).or_insert_with(|| p.clone());
            }
            if e.protocol == Protocol::Tcp {
                for a in &e.addresses {
                    if let Ok(addr) = a.parse::<IpAddr>() {
                        listeners.entry(e.port).or_default().push(Listener {
                            addr: norm(addr),
                            node: id.clone(),
                        });
                    }
                }
            }
        }

        // 2. Connections.
        let local_addrs: HashSet<IpAddr> =
            self.scan.raw.iter().map(|s| norm(s.local_addr)).collect();
        let is_local = |a: IpAddr| {
            let a = norm(a);
            a.is_loopback() || a.is_unspecified() || local_addrs.contains(&a)
        };
        let find_listener = |addr: IpAddr, port: u16| -> Option<String> {
            let addr = norm(addr);
            let ls = listeners.get(&port)?;
            ls.iter()
                .find(|l| l.addr == addr)
                .or_else(|| ls.iter().find(|l| l.addr.is_unspecified()))
                .or_else(|| {
                    // 127.0.0.1 vs ::1 / any loopback alias.
                    ls.iter()
                        .find(|l| l.addr.is_loopback() && addr.is_loopback())
                })
                .map(|l| l.node.clone())
        };
        let listen_ports_of: HashMap<String, HashSet<u16>> = nodes
            .values()
            .map(|n| (n.id.clone(), n.ports.iter().map(|p| p.port).collect()))
            .collect();

        // Every process inside a service tree belongs to that service (workers, children).
        let pid_owner: HashMap<u32, String> = nodes
            .values()
            .filter(|n| !n.protected)
            .flat_map(|n| n.pids.iter().map(move |p| (*p, n.id.clone())))
            .collect();
        let mut detector = ProjectDetector::new();
        let mut edges: BTreeMap<(String, String, u16, EdgeKind), Edge> = BTreeMap::new();
        let mut add_edge =
            |from: String, to: String, port: u16, kind: EdgeKind, remote: Option<String>| {
                let e = edges
                    .entry((from.clone(), to.clone(), port, kind))
                    .or_insert_with(|| Edge {
                        id: format!("{from}->{to}:{port}"),
                        from,
                        to,
                        kind,
                        port,
                        connections: 0,
                        remotes: Vec::new(),
                    });
                e.connections += 1;
                if let Some(r) = remote {
                    if e.remotes.len() < 8 && !e.remotes.contains(&r) {
                        e.remotes.push(r);
                    }
                }
            };

        let established = self.scan.raw.iter().filter(|s| {
            s.protocol == Protocol::Tcp
                && s.state == SocketState::Established
                && s.remote_addr.is_some()
        });
        for s in established {
            let (Some(raddr), Some(rport)) = (s.remote_addr, s.remote_port) else {
                continue;
            };
            let Some(owner) = self.owner_node(s, &pid_owner) else {
                continue;
            };
            if is_local(raddr) {
                // Client side of a local connection: its remote end is one of our listeners.
                let Some(server) = find_listener(raddr, rport) else {
                    continue;
                };
                if server == owner {
                    continue;
                }
                // The accepted (server) side of a connection: local port is our listener port.
                if listen_ports_of
                    .get(&owner)
                    .is_some_and(|ps| ps.contains(&s.local_port))
                    && find_listener(s.local_addr, s.local_port).as_deref() == Some(owner.as_str())
                {
                    continue;
                }
                if !nodes.contains_key(&owner) {
                    let Some(root) = owner.strip_prefix("svc:").and_then(|r| r.parse().ok()) else {
                        continue;
                    };
                    let Some((n, proj)) = self.client_node(root, &mut detector) else {
                        continue;
                    };
                    if let Some(p) = proj {
                        projects.insert(owner.clone(), p);
                    }
                    nodes.insert(owner.clone(), n);
                }
                add_edge(owner, server, rport, EdgeKind::Local, None);
            } else if self.external && nodes.contains_key(&owner) {
                let inbound = listen_ports_of
                    .get(&owner)
                    .is_some_and(|ps| ps.contains(&s.local_port));
                if inbound {
                    add_edge(
                        EXTERNAL_ID.into(),
                        owner,
                        s.local_port,
                        EdgeKind::Inbound,
                        Some(norm(raddr).to_string()),
                    );
                } else {
                    add_edge(
                        owner,
                        EXTERNAL_ID.into(),
                        rport,
                        EdgeKind::Outbound,
                        Some(fmt_endpoint(raddr, rport)),
                    );
                }
            }
        }
        if edges.values().any(|e| e.kind != EdgeKind::Local) {
            nodes.insert(
                EXTERNAL_ID.into(),
                Node {
                    id: EXTERNAL_ID.into(),
                    kind: NodeKind::External,
                    label: "External hosts".into(),
                    subtitle: Some("internet / LAN".into()),
                    root_pid: None,
                    pids: vec![],
                    ports: vec![],
                    framework: None,
                    project: None,
                    project_root: None,
                    container: None,
                    tunnel: None,
                    cluster: None,
                    cpu_percent: 0.0,
                    memory_bytes: 0,
                    is_dev: false,
                    protected: false,
                },
            );
        }

        // 3. Clusters.
        let mut node_list: Vec<Node> = nodes.into_values().collect();
        node_list.sort_by(|a, b| {
            let pa = a.ports.first().map(|p| p.port).unwrap_or(u16::MAX);
            let pb = b.ports.first().map(|p| p.port).unwrap_or(u16::MAX);
            (a.kind == NodeKind::External, pa, &a.id).cmp(&(
                b.kind == NodeKind::External,
                pb,
                &b.id,
            ))
        });
        let (assigned, clusters) = {
            let facts: Vec<NodeFacts> = node_list
                .iter()
                .filter(|n| n.kind != NodeKind::External)
                .map(|n| NodeFacts {
                    node: n,
                    project: projects.get(&n.id),
                    table: self.table(),
                    policy: self.policy,
                })
                .collect();
            let (a, c) = self.clusters.assign(&facts);
            let ids: Vec<String> = facts.iter().map(|f| f.node.id.clone()).collect();
            (ids.into_iter().zip(a).collect::<HashMap<_, _>>(), c)
        };
        for n in &mut node_list {
            n.cluster = assigned.get(&n.id).cloned().flatten();
        }
        let mut g = Graph {
            nodes: node_list,
            edges: edges.into_values().collect(),
            clusters,
            stats: GraphStats::default(),
            taken_at_ms: snap.taken_at_ms,
        };
        g.restat();
        g
    }

    /// The node that owns a connected socket (by its holding process), if any.
    fn owner_node(&self, s: &RawSocket, pid_owner: &HashMap<u32, String>) -> Option<String> {
        let pid = *s.pids.iter().min()?;
        let p = self.table().get(pid)?;
        // Port forwarders' own upstream connections (docker-proxy → container) are plumbing.
        if is_container_forwarder(&p.name) && crate::tunnel::detect_tunnel(p, None).is_none() {
            return None;
        }
        if let Some(n) = pid_owner.get(&pid) {
            return Some(n.clone());
        }
        if let Some(n) = self
            .table()
            .ancestors(pid)
            .iter()
            .find_map(|a| pid_owner.get(a))
        {
            return Some(n.clone());
        }
        Some(format!("svc:{}", self.service_root(pid)))
    }
}

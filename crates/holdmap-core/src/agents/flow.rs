//! A bounded, payload-free projection of process ancestry and TCP socket relationships.
//!
//! A process signature can suggest an MCP server, but neither ancestry nor sockets prove a
//! tool invocation or stdio transport. TCP edges are peer relationships, not traffic direction.

use super::{Agent, AgentsReport, DefaultToolClassifier, Evidence, ToolClassifier, ToolKind};
use crate::{PortEntry, Protocol, RawSocket, Scan, SocketState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::IpAddr;

/// Maximum nodes exposed by one flow snapshot.
pub const MAX_FLOW_NODES: usize = 256;
/// Maximum relationships exposed by one flow snapshot.
pub const MAX_FLOW_EDGES: usize = 512;
const MAX_SOCKET_HOLDERS: usize = 32;

/// What a flow node represents; tool nodes are processes, not individual invocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFlowNodeKind {
    /// A recognised agent root process.
    Agent,
    /// A child tool process, with separate classification evidence.
    Tool,
    /// Another visible process, including product helpers.
    Process,
    /// A local listening endpoint.
    Listener,
    /// A peer outside this machine.
    Remote,
    /// A local peer whose process identity is unavailable or shared.
    Unknown,
}

/// One process or socket endpoint, without command lines, prompts or working directories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFlowNode {
    /// Process IDs include their start token so PID reuse produces a different identity.
    pub id: String,
    /// Node purpose.
    pub kind: AgentFlowNodeKind,
    /// Printable, redacted display label.
    pub label: String,
    /// Owning agent's existing report ID, when attributable.
    pub agent_id: Option<String>,
    /// Visible process ID, if this node represents one process.
    pub pid: Option<u32>,
    /// Process start token; absent when its identity could not be captured.
    pub start_token: Option<u64>,
    /// Listening row ID, when this is a local listener.
    pub entry_id: Option<String>,
    /// Numeric endpoint, with no DNS lookup.
    pub address: Option<String>,
    /// Endpoint port.
    pub port: Option<u16>,
    /// Endpoint transport.
    pub protocol: Option<Protocol>,
    /// Child process classification, never proof of a tool call.
    pub tool_kind: Option<ToolKind>,
    /// Evidence for identity or classification.
    pub evidence: Evidence,
}

/// What a relationship establishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFlowEdgeKind {
    /// OS parent process → child process, with no inferred stdio messages.
    Parent,
    /// An observed process holding a local listener.
    OwnsListener,
    /// Established TCP peers; this says nothing about payloads or message direction.
    Tcp,
}

/// A relationship with its evidence and actual captured socket holders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFlowEdge {
    /// Stable relationship ID, including process identities through its node IDs.
    pub id: String,
    /// Origin node; TCP edges identify the captured local socket side.
    pub from: String,
    /// Child, listener or TCP peer node.
    pub to: String,
    /// Relationship type.
    pub kind: AgentFlowEdgeKind,
    /// Observed relationship, or an inferred listener match.
    pub evidence: Evidence,
    /// Distinct established sockets; zero for process and listener ownership relationships.
    pub connections: usize,
    /// Exact PIDs holding the captured TCP socket, empty when unavailable.
    pub source_pids: Vec<u32>,
}

/// A separate flow contract; the stable [`AgentsReport`] remains unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFlowReport {
    /// Source scan timestamp, in Unix epoch milliseconds.
    pub taken_at_ms: u64,
    /// Bounded process and endpoint nodes.
    pub nodes: Vec<AgentFlowNode>,
    /// Bounded relationships; both endpoints always exist in `nodes`.
    pub edges: Vec<AgentFlowEdge>,
    /// Collection gaps and interpretation limits.
    pub limits: Vec<String>,
    /// Distinct candidate nodes excluded by the display bound.
    pub omitted_nodes: usize,
    /// Distinct relationships excluded by the display or socket-holder bounds.
    pub omitted_edges: usize,
}

/// Project a consistent scan and its agent attribution without probes or filesystem reads.
pub fn flow(scan: &Scan, agents: &AgentsReport) -> AgentFlowReport {
    let mut projection = Projection::new(scan, agents);
    let mut owned: Vec<u32> = projection.owners.keys().copied().collect();
    owned.sort_unstable();
    // Roots get space before a large child process list fills the bounded projection.
    for agent in &agents.agents {
        projection.process(agent.pid);
    }
    for pid in owned {
        let Some(child) = projection.process(pid) else {
            continue;
        };
        if let Some(parent_pid) = scan.table.get(pid).and_then(|p| p.ppid) {
            if let Some(parent) = projection.process(parent_pid) {
                projection.edge(
                    parent,
                    child,
                    AgentFlowEdgeKind::Parent,
                    Evidence::Observed,
                    vec![],
                    false,
                );
            }
        }
    }
    for entry in scan
        .snapshot
        .entries
        .iter()
        .filter(|e| e.state.is_listening())
    {
        let pids = holders(entry);
        if pids.iter().any(|p| projection.owners.contains_key(p)) {
            projection.listener(entry);
        }
    }

    let listeners: Vec<&PortEntry> = scan
        .snapshot
        .entries
        .iter()
        .filter(|e| e.protocol == Protocol::Tcp && e.state.is_listening())
        .collect();
    let local: HashSet<IpAddr> = scan
        .raw
        .iter()
        .map(|s| super::network::norm(s.local_addr))
        .collect();
    let mut sockets: Vec<&RawSocket> = scan
        .raw
        .iter()
        .filter(|s| {
            s.protocol == Protocol::Tcp
                && s.state == SocketState::Established
                && s.remote_addr.is_some()
                && s.remote_port.is_some()
        })
        .collect();
    // Prefer the client side when both sides are captured. This counts a connection once
    // and retains its source process, including connections within the same agent.
    sockets.sort_by_key(|s| {
        let target = find_listener(
            &listeners,
            s.remote_addr.expect("remote address"),
            s.remote_port.expect("remote port"),
        );
        (target.is_none(), connection_key(s), sorted_pids(&s.pids))
    });
    let mut seen = HashSet::new();
    let mut oversized = 0;
    for socket in sockets {
        let remote_addr = socket.remote_addr.expect("remote address");
        let remote_port = socket.remote_port.expect("remote port");
        let target = find_listener(&listeners, remote_addr, remote_port);
        let source_pids = sorted_pids(&socket.pids);
        let own_local = find_listener(&listeners, socket.local_addr, socket.local_port)
            .is_some_and(|entry| {
                holders(entry).iter().any(|p| {
                    projection.owners.contains_key(p)
                        && (source_pids.is_empty() || source_pids.contains(p))
                })
            });
        let targets = matching_listeners(&listeners, remote_addr, remote_port);
        let own_target = targets.iter().any(|entry| {
            holders(entry)
                .iter()
                .any(|p| projection.owners.contains_key(p))
        });
        if !own_local
            && !own_target
            && !source_pids
                .iter()
                .any(|p| projection.owners.contains_key(p))
        {
            continue;
        }
        if !seen.insert(connection_key(socket)) {
            continue;
        }
        if source_pids.len() > MAX_SOCKET_HOLDERS {
            oversized += 1;
            projection
                .omitted_edges
                .insert(format!("shared:{}", connection_key(socket)));
            continue;
        }
        let from = projection.socket_owner(socket, &source_pids);
        let (to, evidence) = match target {
            Some(_) if targets.len() > 1 => (
                projection.peer(super::network::norm(remote_addr), remote_port, true),
                Evidence::Inferred,
            ),
            Some(entry) => (projection.listener(entry), Evidence::Observed),
            None => {
                let address = super::network::norm(remote_addr);
                let local_peer =
                    address.is_loopback() || address.is_unspecified() || local.contains(&address);
                (
                    projection.peer(address, remote_port, local_peer),
                    Evidence::Observed,
                )
            }
        };
        projection.edge(
            from,
            to,
            AgentFlowEdgeKind::Tcp,
            evidence,
            source_pids,
            true,
        );
    }
    let mut limits = vec![
        "Process ancestry does not establish a stdio transport or a tool invocation.".into(),
        "TCP relationships count established sockets, not requests, payload direction or traffic bytes. UDP traffic is not mapped.".into(),
        "MCP process classifications are inferred; calls need an attached activity observer.".into(),
    ];
    if projection.unknown_identities > 0 {
        limits.push("Some process start tokens are unavailable; those identities cannot be correlated across snapshots.".into());
    }
    if projection.unknown_peers > 0 {
        limits.push(
            "Some local peers have no visible process owner; no agent attribution is guessed."
                .into(),
        );
    }
    if oversized > 0 {
        limits.push(format!(
            "{oversized} shared sockets exceeded the 32-holder bound and were omitted."
        ));
    }
    if !projection.omitted_nodes.is_empty() || !projection.omitted_edges.is_empty() {
        limits.push(format!("Flow display is limited to {MAX_FLOW_NODES} nodes and {MAX_FLOW_EDGES} relationships; omitted counts describe this snapshot."));
    }
    if scan.snapshot.hidden_sockets > 0 {
        limits.push(format!(
            "{} listener rows have unavailable process ownership.",
            scan.snapshot.hidden_sockets
        ));
    }
    limits.extend(scan.snapshot.warnings.iter().take(8).map(|s| label(s)));
    if scan.snapshot.platform == "macos" {
        limits.push(
            "macOS can omit sockets of inaccessible processes; this is a partial snapshot.".into(),
        );
    }
    if scan.snapshot.taken_at_ms != agents.taken_at_ms {
        limits.push("Agent attribution and socket collection have different timestamps; refresh them together.".into());
    }
    AgentFlowReport {
        taken_at_ms: scan.snapshot.taken_at_ms,
        nodes: projection.nodes.into_values().collect(),
        edges: projection.edges.into_values().collect(),
        limits,
        omitted_nodes: projection.omitted_nodes.len(),
        omitted_edges: projection.omitted_edges.len(),
    }
}

struct Projection<'a> {
    scan: &'a Scan,
    owners: HashMap<u32, &'a Agent>,
    nodes: BTreeMap<String, AgentFlowNode>,
    edges: BTreeMap<String, AgentFlowEdge>,
    omitted_nodes: HashSet<String>,
    omitted_edges: HashSet<String>,
    unknown_identities: usize,
    unknown_peers: usize,
}

impl<'a> Projection<'a> {
    fn new(scan: &'a Scan, agents: &'a AgentsReport) -> Self {
        let owners = agents
            .agents
            .iter()
            .flat_map(|a| {
                a.process_ids
                    .iter()
                    .copied()
                    .chain(std::iter::once(a.pid))
                    .map(move |pid| (pid, a))
            })
            .collect();
        Self {
            scan,
            owners,
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            omitted_nodes: HashSet::new(),
            omitted_edges: HashSet::new(),
            unknown_identities: 0,
            unknown_peers: 0,
        }
    }

    // Return the candidate identity even when hidden by the bound, so omitted relationships
    // are counted too. `edge` refuses relationships with undisplayed endpoints.
    fn node(&mut self, node: AgentFlowNode) -> String {
        if self.nodes.contains_key(&node.id) {
            return node.id;
        }
        if self.nodes.len() >= MAX_FLOW_NODES {
            self.omitted_nodes.insert(node.id.clone());
            return node.id;
        }
        let id = node.id.clone();
        self.nodes.insert(id.clone(), node);
        id
    }

    fn process(&mut self, pid: u32) -> Option<String> {
        let process = self.scan.table.get(pid)?;
        let token = (process.start_token != 0).then_some(process.start_token);
        let id = token.map_or_else(
            || format!("process:{pid}:unknown:{}", self.scan.snapshot.taken_at_ms),
            |token| format!("process:{pid}:{token}"),
        );
        if self.nodes.contains_key(&id) {
            return Some(id);
        }
        let agent = self.owners.get(&pid).copied();
        let root = agent.is_some_and(|a| a.pid == pid);
        let helper = agent
            .is_some_and(|a| super::catalog::identify(process).is_some_and(|p| p.id == a.product));
        let classification = agent
            .filter(|_| !root && !helper)
            .map(|a| DefaultToolClassifier.classify(process, &a.ports));
        if token.is_none() {
            self.unknown_identities += 1;
        }
        Some(self.node(AgentFlowNode {
            id,
            kind: if root {
                AgentFlowNodeKind::Agent
            } else if classification.is_some() {
                AgentFlowNodeKind::Tool
            } else {
                AgentFlowNodeKind::Process
            },
            label: label(&if root {
                agent.expect("root agent").name.clone()
            } else {
                classification
                    .as_ref()
                    .map_or_else(|| process.name.clone(), |c| c.name.clone())
            }),
            agent_id: agent.map(|a| a.id.clone()),
            pid: Some(pid),
            start_token: token,
            entry_id: None,
            address: None,
            port: None,
            protocol: None,
            tool_kind: classification.as_ref().map(|c| c.kind),
            evidence: if token.is_none() {
                Evidence::Unknown
            } else {
                classification.map_or(Evidence::Observed, |c| c.evidence)
            },
        }))
    }

    fn listener(&mut self, entry: &PortEntry) -> String {
        let pids = holders(entry);
        let primary = entry.pid.or_else(|| pids.first().copied());
        let token = primary
            .and_then(|pid| self.scan.table.get(pid))
            .map(|p| p.start_token)
            .filter(|token| *token != 0);
        let identity = token.map_or_else(
            || format!("unknown:{}", self.scan.snapshot.taken_at_ms),
            |token| token.to_string(),
        );
        let id = format!("listener:{}:{}:{identity}", entry.id, primary.unwrap_or(0));
        if self.nodes.contains_key(&id) {
            return id;
        }
        let agent_ids: BTreeSet<_> = pids
            .iter()
            .filter_map(|p| self.owners.get(p).map(|a| a.id.clone()))
            .collect();
        let agent_id =
            (agent_ids.len() == 1).then(|| agent_ids.iter().next().expect("one agent").clone());
        let node = self.node(AgentFlowNode {
            id,
            kind: AgentFlowNodeKind::Listener,
            label: label(&format!(
                "{} · {}/{}",
                entry.label, entry.port, entry.protocol
            )),
            agent_id,
            pid: primary,
            start_token: token,
            entry_id: Some(entry.id.clone()),
            address: entry
                .addresses
                .iter()
                .find_map(|address| address.parse::<IpAddr>().ok())
                .map(|address| super::network::norm(address).to_string()),
            port: Some(entry.port),
            protocol: Some(entry.protocol),
            tool_kind: None,
            evidence: if primary.is_none() {
                Evidence::Unknown
            } else if agent_ids.len() > 1 {
                Evidence::Inferred
            } else {
                Evidence::Observed
            },
        });
        for pid in pids {
            if let Some(owner) = self.process(pid) {
                self.edge(
                    owner,
                    node.clone(),
                    AgentFlowEdgeKind::OwnsListener,
                    Evidence::Observed,
                    vec![],
                    false,
                );
            }
        }
        node
    }

    fn peer(&mut self, address: IpAddr, port: u16, local: bool) -> String {
        let address = endpoint(address, port);
        if local {
            self.unknown_peers += 1;
        }
        self.node(AgentFlowNode {
            id: format!("{}:{address}", if local { "unknown" } else { "remote" }),
            kind: if local {
                AgentFlowNodeKind::Unknown
            } else {
                AgentFlowNodeKind::Remote
            },
            label: if local {
                format!("{address} · owner unknown")
            } else {
                address.clone()
            },
            address: Some(address),
            port: Some(port),
            protocol: Some(Protocol::Tcp),
            agent_id: None,
            pid: None,
            start_token: None,
            entry_id: None,
            tool_kind: None,
            evidence: if local {
                Evidence::Unknown
            } else {
                Evidence::Observed
            },
        })
    }

    fn socket_owner(&mut self, socket: &RawSocket, pids: &[u32]) -> String {
        if let [pid] = pids {
            if let Some(node) = self.process(*pid) {
                return node;
            }
        }
        self.unknown_peers += 1;
        let identity = pids
            .iter()
            .map(|pid| {
                format!(
                    "{pid}:{}",
                    self.scan.table.get(*pid).map_or(0, |p| p.start_token)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        self.node(AgentFlowNode {
            id: format!(
                "socket-owner:{}:{identity}:{}",
                endpoint(socket.local_addr, socket.local_port),
                self.scan.snapshot.taken_at_ms
            ),
            kind: AgentFlowNodeKind::Unknown,
            label: if pids.is_empty() {
                "Socket owner unavailable".into()
            } else {
                format!("Shared or unavailable socket owner · {} PIDs", pids.len())
            },
            agent_id: None,
            pid: None,
            start_token: None,
            entry_id: None,
            address: Some(endpoint(socket.local_addr, socket.local_port)),
            port: Some(socket.local_port),
            protocol: Some(Protocol::Tcp),
            tool_kind: None,
            evidence: Evidence::Unknown,
        })
    }

    fn edge(
        &mut self,
        from: String,
        to: String,
        kind: AgentFlowEdgeKind,
        evidence: Evidence,
        pids: Vec<u32>,
        connection: bool,
    ) {
        if from == to {
            return;
        }
        let id = format!("{kind:?}:{from}->{to}");
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            self.omitted_edges.insert(id);
            return;
        }
        if let Some(edge) = self.edges.get_mut(&id) {
            if connection {
                edge.connections += 1;
            }
            let mut sources: BTreeSet<u32> = edge.source_pids.iter().copied().collect();
            sources.extend(pids);
            edge.source_pids = sources.into_iter().collect();
            if evidence == Evidence::Inferred {
                edge.evidence = evidence;
            }
            return;
        }
        if self.edges.len() >= MAX_FLOW_EDGES {
            self.omitted_edges.insert(id);
            return;
        }
        self.edges.insert(
            id.clone(),
            AgentFlowEdge {
                id,
                from,
                to,
                kind,
                evidence,
                connections: usize::from(connection),
                source_pids: pids,
            },
        );
    }
}

fn holders(entry: &PortEntry) -> Vec<u32> {
    sorted_pids(
        &entry
            .pid
            .into_iter()
            .chain(entry.pids.iter().copied())
            .collect::<Vec<_>>(),
    )
}

fn sorted_pids(pids: &[u32]) -> Vec<u32> {
    let mut pids = pids.to_vec();
    pids.sort_unstable();
    pids.dedup();
    pids
}

fn label(value: &str) -> String {
    crate::util::printable(&crate::redact::line(value))
        .chars()
        .filter(|character| !matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .take(160)
        .collect()
}

fn endpoint(address: IpAddr, port: u16) -> String {
    match super::network::norm(address) {
        IpAddr::V4(v) => format!("{v}:{port}"),
        IpAddr::V6(v) => format!("[{v}]:{port}"),
    }
}

fn connection_key(socket: &RawSocket) -> String {
    let mut ends = [
        endpoint(socket.local_addr, socket.local_port),
        endpoint(
            socket.remote_addr.expect("TCP peer"),
            socket.remote_port.expect("TCP peer port"),
        ),
    ];
    ends.sort();
    format!("{}|{}", ends[0], ends[1])
}

fn rank(entry: &PortEntry, address: IpAddr) -> Option<u8> {
    let address = super::network::norm(address);
    entry
        .addresses
        .iter()
        .filter_map(|s| s.parse::<IpAddr>().ok())
        .filter_map(|bound| {
            let bound = super::network::norm(bound);
            if bound == address {
                Some(0)
            } else if bound.is_unspecified() && bound.is_ipv4() == address.is_ipv4() {
                Some(1)
            } else if bound.is_unspecified() && bound.is_ipv6() && address.is_ipv4() {
                Some(2)
            } else {
                None
            }
        })
        .min()
}

fn find_listener<'a>(
    listeners: &[&'a PortEntry],
    address: IpAddr,
    port: u16,
) -> Option<&'a PortEntry> {
    listeners
        .iter()
        .filter(|entry| entry.port == port)
        .filter_map(|entry| rank(entry, address).map(|rank| (rank, *entry)))
        .min_by(|(a_rank, a), (b_rank, b)| a_rank.cmp(b_rank).then(a.id.cmp(&b.id)))
        .map(|(_, entry)| entry)
}

fn matching_listeners<'a>(
    listeners: &[&'a PortEntry],
    address: IpAddr,
    port: u16,
) -> Vec<&'a PortEntry> {
    let best = listeners
        .iter()
        .filter(|entry| entry.port == port)
        .filter_map(|entry| rank(entry, address))
        .min();
    let Some(best) = best else { return vec![] };
    listeners
        .iter()
        .filter(|entry| entry.port == port && rank(entry, address) == Some(best))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{AgentsBuilder, NoRecent};
    use crate::model::{Family, ProcessInfo, Snapshot};
    use crate::process::tests::{proc, table};
    use crate::scan::build_entries;

    fn socket(pid: Option<u32>, local_port: u16, peer: Option<(&str, u16)>) -> RawSocket {
        RawSocket {
            protocol: Protocol::Tcp,
            family: Family::V4,
            local_addr: "127.0.0.1".parse().unwrap(),
            local_port,
            remote_addr: peer.map(|(address, _)| address.parse().unwrap()),
            remote_port: peer.map(|(_, port)| port),
            state: if peer.is_some() {
                SocketState::Established
            } else {
                SocketState::Listen
            },
            uid: Some(1000),
            inode: None,
            pids: pid.into_iter().collect(),
        }
    }

    fn scan(processes: Vec<ProcessInfo>, raw: Vec<RawSocket>) -> Scan {
        let table = table(processes, 999);
        let (entries, hidden_sockets) = build_entries(&raw, &table, &[], false);
        Scan {
            table,
            raw,
            published: vec![],
            snapshot: Snapshot {
                entries,
                hidden_sockets,
                platform: "test".into(),
                taken_at_ms: 7,
                scan_ms: 0,
                docker_available: false,
                warnings: vec![],
            },
        }
    }

    fn project(scan: &Scan) -> AgentFlowReport {
        let agents = AgentsBuilder::new(scan)
            .with_project_detection(false)
            .with_recent(&NoRecent)
            .build();
        flow(scan, &agents)
    }

    fn processes() -> Vec<ProcessInfo> {
        vec![
            proc(100, 1, "claude", &["claude", "--api-key=sk-private-secret"]),
            proc(
                101,
                100,
                "node",
                &["node", "mcp-server-filesystem.js", "private-chat-text"],
            ),
            proc(102, 101, "node", &["node", "server.js"]),
            proc(200, 1, "codex", &["codex"]),
            proc(201, 200, "node", &["node", "server.js"]),
        ]
    }

    fn pid_node(report: &AgentFlowReport, pid: u32) -> &AgentFlowNode {
        report
            .nodes
            .iter()
            .find(|node| node.pid == Some(pid) && node.entry_id.is_none())
            .unwrap()
    }

    #[test]
    fn ancestry_and_internal_tcp_preserve_source_pid_and_classification_evidence() {
        let report = project(&scan(
            processes(),
            vec![
                socket(Some(102), 3000, None),
                socket(Some(101), 52000, Some(("127.0.0.1", 3000))),
                socket(Some(102), 3000, Some(("127.0.0.1", 52000))),
            ],
        ));
        let agent = pid_node(&report, 100);
        let tool = pid_node(&report, 101);
        let service = pid_node(&report, 102);
        assert_eq!(tool.tool_kind, Some(ToolKind::McpServer));
        assert_eq!(tool.evidence, Evidence::Inferred);
        assert!(report
            .edges
            .iter()
            .any(|e| e.kind == AgentFlowEdgeKind::Parent
                && e.from == agent.id
                && e.to == tool.id
                && e.evidence == Evidence::Observed));
        assert!(report
            .edges
            .iter()
            .any(|e| e.kind == AgentFlowEdgeKind::Parent
                && e.from == tool.id
                && e.to == service.id));
        let tcp: Vec<_> = report
            .edges
            .iter()
            .filter(|e| e.kind == AgentFlowEdgeKind::Tcp)
            .collect();
        assert_eq!(tcp.len(), 1);
        assert_eq!(tcp[0].from, tool.id);
        assert_eq!(tcp[0].source_pids, [101]);
        assert_eq!(
            tcp[0].connections, 1,
            "both captured halves are one connection"
        );
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.contains("sk-private-secret"));
        assert!(!serialized.contains("private-chat-text"));
        assert!(!serialized.contains("cmdline"));
    }

    #[test]
    fn connections_between_agents_keep_both_attributions() {
        let report = project(&scan(
            processes(),
            vec![
                socket(Some(201), 4000, None),
                socket(Some(101), 52000, Some(("127.0.0.1", 4000))),
            ],
        ));
        let tcp = report
            .edges
            .iter()
            .find(|e| e.kind == AgentFlowEdgeKind::Tcp)
            .unwrap();
        let target = report.nodes.iter().find(|n| n.id == tcp.to).unwrap();
        assert_eq!(
            pid_node(&report, 101).agent_id.as_deref(),
            Some("agent:100")
        );
        assert_eq!(target.agent_id.as_deref(), Some("agent:200"));
        assert_eq!(target.port, Some(4000));
        assert_eq!(tcp.source_pids, [101]);
    }

    #[test]
    fn remote_endpoint_and_duplicate_samples_are_deduplicated() {
        let first = socket(Some(101), 52000, Some(("203.0.113.10", 443)));
        let report = project(&scan(
            processes(),
            vec![
                first.clone(),
                first,
                socket(Some(101), 52001, Some(("203.0.113.10", 443))),
            ],
        ));
        assert_eq!(
            report
                .nodes
                .iter()
                .filter(|n| n.kind == AgentFlowNodeKind::Remote)
                .count(),
            1
        );
        let tcp = report
            .edges
            .iter()
            .find(|e| e.kind == AgentFlowEdgeKind::Tcp)
            .unwrap();
        assert_eq!(tcp.connections, 2);
        assert_eq!(tcp.source_pids, [101]);
        assert_eq!(
            report.nodes.iter().find(|n| n.id == tcp.to).unwrap().label,
            "203.0.113.10:443"
        );
    }

    #[test]
    fn process_and_listener_identities_change_when_pid_is_reused() {
        let before = project(&scan(processes(), vec![socket(Some(102), 3000, None)]));
        let mut reused = processes();
        reused
            .iter_mut()
            .find(|p| p.pid == 102)
            .unwrap()
            .start_token += 1;
        let after = project(&scan(reused, vec![socket(Some(102), 3000, None)]));
        assert_ne!(pid_node(&before, 102).id, pid_node(&after, 102).id);
        assert_ne!(
            before
                .nodes
                .iter()
                .find(|n| n.entry_id.is_some())
                .unwrap()
                .id,
            after
                .nodes
                .iter()
                .find(|n| n.entry_id.is_some())
                .unwrap()
                .id
        );
    }

    #[test]
    fn missing_socket_owner_is_explicit_and_does_not_guess_an_agent() {
        let report = project(&scan(
            processes(),
            vec![
                socket(Some(102), 3000, None),
                socket(None, 52000, Some(("127.0.0.1", 3000))),
            ],
        ));
        let tcp = report
            .edges
            .iter()
            .find(|e| e.kind == AgentFlowEdgeKind::Tcp)
            .unwrap();
        let source = report.nodes.iter().find(|n| n.id == tcp.from).unwrap();
        assert_eq!(source.kind, AgentFlowNodeKind::Unknown);
        assert_eq!(source.evidence, Evidence::Unknown);
        assert!(source.agent_id.is_none());
        assert!(tcp.source_pids.is_empty());
    }

    #[test]
    fn unknown_process_identity_is_snapshot_scoped_and_labels_are_terminal_safe() {
        let mut processes = processes();
        let child = processes.iter_mut().find(|p| p.pid == 102).unwrap();
        child.start_token = 0;
        child.name = "service\u{1b}[31m\napi_key=private-value".into();
        child.cwd = Some("/private/working-folder".into());
        let mut current = scan(processes, vec![]);
        let before = project(&current);
        current.snapshot.taken_at_ms += 1;
        let after = project(&current);
        assert_ne!(pid_node(&before, 102).id, pid_node(&after, 102).id);
        assert_eq!(pid_node(&before, 102).evidence, Evidence::Unknown);
        let serialized = serde_json::to_string(&after).unwrap();
        assert!(!serialized.contains("private-value"));
        assert!(!serialized.contains("working-folder"));
        assert!(!pid_node(&after, 102).label.contains('\u{1b}'));
        assert!(!pid_node(&after, 102).label.contains('\n'));
    }

    #[test]
    fn labels_remove_direction_controls_without_removing_language_characters() {
        let mut processes = processes();
        processes
            .iter_mut()
            .find(|process| process.pid == 102)
            .unwrap()
            .name = "service\u{202e}live\u{2066}مرحبا\u{2069}".into();
        let mut current = scan(processes, vec![socket(Some(102), 3000, None)]);
        current.snapshot.warnings = vec!["warning\u{200f}\u{061c}".into()];
        let report = project(&current);
        assert_eq!(pid_node(&report, 102).label, "serviceliveمرحبا");
        assert!(report.limits.iter().any(|limit| limit == "warning"));
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.chars().any(|character| matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')));
    }

    #[test]
    fn unrelated_clients_are_not_attributed_only_by_a_reused_local_port() {
        let mut processes = processes();
        processes.push(proc(500, 1, "other", &["other"]));
        let report = project(&scan(
            processes,
            vec![
                socket(Some(102), 3000, None),
                socket(Some(500), 3000, Some(("203.0.113.10", 443))),
            ],
        ));
        assert!(!report
            .edges
            .iter()
            .any(|e| e.kind == AgentFlowEdgeKind::Tcp));
    }

    #[test]
    fn ambiguous_local_listeners_do_not_guess_the_destination_agent() {
        let mut current = scan(
            processes(),
            vec![
                socket(Some(102), 3000, None),
                socket(Some(101), 52000, Some(("127.0.0.1", 3000))),
            ],
        );
        let mut second = current.snapshot.entries[0].clone();
        second.id = "second-listener".into();
        second.pid = Some(201);
        second.pids = vec![201];
        current.snapshot.entries.push(second);
        let report = project(&current);
        let tcp = report
            .edges
            .iter()
            .find(|e| e.kind == AgentFlowEdgeKind::Tcp)
            .unwrap();
        let target = report.nodes.iter().find(|n| n.id == tcp.to).unwrap();
        assert_eq!(target.kind, AgentFlowNodeKind::Unknown);
        assert_eq!(target.evidence, Evidence::Unknown);
        assert_eq!(tcp.evidence, Evidence::Inferred);
        assert!(target.agent_id.is_none());
        assert_eq!(
            report
                .nodes
                .iter()
                .filter(|n| n.kind == AgentFlowNodeKind::Listener)
                .count(),
            2
        );
    }

    #[test]
    fn exact_bind_precedes_wildcard_and_udp_is_not_a_tcp_relationship() {
        let mut current = scan(
            processes(),
            vec![
                socket(Some(102), 3000, None),
                socket(Some(101), 52000, Some(("127.0.0.1", 3000))),
            ],
        );
        let mut wildcard = current.snapshot.entries[0].clone();
        wildcard.id = "wildcard-listener".into();
        wildcard.addresses = vec!["0.0.0.0".into()];
        wildcard.pid = Some(201);
        wildcard.pids = vec![201];
        current.snapshot.entries.push(wildcard);
        let mut udp = socket(Some(101), 52001, Some(("203.0.113.10", 3000)));
        udp.protocol = Protocol::Udp;
        current.raw.push(udp);
        let report = project(&current);
        let tcp: Vec<_> = report
            .edges
            .iter()
            .filter(|e| e.kind == AgentFlowEdgeKind::Tcp)
            .collect();
        assert_eq!(tcp.len(), 1);
        assert_eq!(tcp[0].evidence, Evidence::Observed);
        assert_eq!(
            report.nodes.iter().find(|n| n.id == tcp[0].to).unwrap().pid,
            Some(102)
        );
        assert!(!report
            .nodes
            .iter()
            .any(|n| n.kind == AgentFlowNodeKind::Remote));
    }

    #[test]
    fn node_bound_keeps_roots_and_never_leaves_dangling_edges() {
        let mut processes = processes();
        processes.extend((1000..1400).map(|pid| proc(pid, 100, "child", &["child"])));
        let report = project(&scan(processes, vec![]));
        assert_eq!(report.nodes.len(), MAX_FLOW_NODES);
        assert!(report.omitted_nodes > 0);
        assert!(
            report.omitted_edges > 0,
            "relationships hidden with their nodes are counted"
        );
        assert_eq!(pid_node(&report, 100).kind, AgentFlowNodeKind::Agent);
        assert_eq!(pid_node(&report, 200).kind, AgentFlowNodeKind::Agent);
        assert!(report
            .edges
            .iter()
            .all(|e| report.nodes.iter().any(|n| n.id == e.from)
                && report.nodes.iter().any(|n| n.id == e.to)));
    }

    #[test]
    fn edge_and_shared_socket_bounds_are_reported() {
        let mut processes = vec![proc(100, 1, "claude", &["claude"])];
        processes.extend((1000..1040).map(|pid| proc(pid, 100, "child", &["child"])));
        let mut raw: Vec<_> = (1000..1020)
            .map(|pid| socket(Some(pid), pid as u16 + 2000, None))
            .collect();
        for source in 1000..1040 {
            for target in 1000..1020 {
                raw.push(socket(
                    Some(source),
                    20000 + ((source - 1000) * 20 + target - 1000) as u16,
                    Some(("127.0.0.1", target as u16 + 2000)),
                ));
            }
        }
        let mut shared = socket(None, 60000, Some(("127.0.0.1", 3000)));
        shared.pids = (1000..1040).collect();
        raw.push(shared);
        let report = project(&scan(processes, raw));
        assert_eq!(report.edges.len(), MAX_FLOW_EDGES);
        assert!(report.omitted_edges > 0);
        assert!(report.nodes.len() <= MAX_FLOW_NODES);
        assert!(report.limits.iter().any(|s| s.contains("32-holder")));
    }
}

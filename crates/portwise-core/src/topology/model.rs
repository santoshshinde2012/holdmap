//! Graph data types shared by the builder, exporters and every frontend (serialized as JSON).

use crate::model::{ContainerInfo, Exposure, Framework, Protocol};
use crate::tunnel::TunnelInfo;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Id of the single node that stands for every host outside this machine.
pub const EXTERNAL_ID: &str = "external";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A process tree that listens on at least one port.
    Service,
    /// A container publishing ports.
    Container,
    /// A listener whose owning process isn't visible (another user / root).
    Hidden,
    /// A local process that only connects to local services (browser, CLI, worker).
    Client,
    /// Every remote host, collapsed into one node.
    External,
}

/// A port a node listens on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodePort {
    pub port: u16,
    pub protocol: Protocol,
    pub exposure: Exposure,
    /// [`PortEntry::id`](crate::model::PortEntry::id) of the listener row (for UI selection sync).
    pub entry_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Stable id: `svc:<root pid>`, `ctr:<container id>`, `port:<proto>:<port>` or `external`.
    pub id: String,
    pub kind: NodeKind,
    /// Main label (project / container / process name).
    pub label: String,
    /// Secondary label (framework or process name).
    pub subtitle: Option<String>,
    pub root_pid: Option<u32>,
    /// Every process in the service tree.
    pub pids: Vec<u32>,
    pub ports: Vec<NodePort>,
    pub framework: Option<Framework>,
    pub project: Option<String>,
    pub project_root: Option<PathBuf>,
    pub container: Option<ContainerInfo>,
    pub tunnel: Option<TunnelInfo>,
    /// Cluster id this node belongs to.
    pub cluster: Option<String>,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub is_dev: bool,
    pub protected: bool,
}

impl Node {
    pub fn listens(&self) -> bool {
        !self.ports.is_empty()
    }

    /// `web :3000` style display.
    pub fn display(&self) -> String {
        let ports: Vec<String> = self.ports.iter().map(|p| format!(":{}", p.port)).collect();
        if ports.is_empty() {
            self.label.clone()
        } else {
            format!("{} {}", self.label, ports.join(" "))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Local process → local listener. `from` depends on `to`.
    Local,
    /// Local process → remote host(s).
    Outbound,
    /// Remote host(s) → local listener.
    Inbound,
}

/// Established connections from one node to another, aggregated per target port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    /// `from->to:port`.
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    /// Target port (the listener port for local/inbound edges, the remote port for outbound).
    pub port: u16,
    /// Number of established connections.
    pub connections: usize,
    /// Remote endpoints (external edges only, at most 8).
    pub remotes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterKind {
    /// docker compose project (container labels).
    Compose,
    /// Kubernetes namespace (from `kubectl port-forward`).
    Kubernetes,
    /// A process supervisor / task runner parenting several services (pm2, turbo, concurrently…).
    Supervisor,
    /// Monorepo / workspace root (pnpm, turbo, nx, Cargo workspace, compose file…).
    Workspace,
    /// Git repository.
    Git,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cluster {
    /// `compose:<project>`, `k8s:<ns>`, `sup:<pid>`, `ws:<root>`, `git:<root>`.
    pub id: String,
    pub name: String,
    pub kind: ClusterKind,
    /// Detail such as the workspace kind (`pnpm`) or supervisor tool (`turbo`).
    pub detail: Option<String>,
    pub root: Option<PathBuf>,
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphStats {
    pub nodes: usize,
    pub edges: usize,
    pub clusters: usize,
    pub connections: usize,
}

/// The service mesh of this machine.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub clusters: Vec<Cluster>,
    pub stats: GraphStats,
    pub taken_at_ms: u64,
}

impl Graph {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn cluster(&self, id: &str) -> Option<&Cluster> {
        self.clusters.iter().find(|c| c.id == id)
    }

    /// Find a cluster by id or (case-insensitive) name.
    pub fn find_cluster(&self, query: &str) -> Option<&Cluster> {
        let q = query.strip_prefix("cluster:").unwrap_or(query);
        self.cluster(q)
            .or_else(|| {
                self.clusters
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(q))
            })
            .or_else(|| {
                self.clusters.iter().find(|c| {
                    c.id.split_once(':')
                        .is_some_and(|(_, rest)| rest.eq_ignore_ascii_case(q))
                })
            })
    }

    /// The node that listens on `port`.
    pub fn node_for_port(&self, port: u16) -> Option<&Node> {
        self.nodes
            .iter()
            .find(|n| n.ports.iter().any(|p| p.port == port))
    }

    /// The node owning a list entry.
    pub fn node_for_entry(&self, entry_id: &str) -> Option<&Node> {
        self.nodes
            .iter()
            .find(|n| n.ports.iter().any(|p| p.entry_id == entry_id))
    }

    /// Nodes with a local edge into `id` (they depend on it).
    pub fn dependents(&self, id: &str) -> Vec<&Node> {
        self.local_edges()
            .filter(|e| e.to == id)
            .filter_map(|e| self.node(&e.from))
            .fold(Vec::new(), |mut v, n| {
                if !v.iter().any(|x: &&Node| x.id == n.id) {
                    v.push(n);
                }
                v
            })
    }

    /// Nodes `id` connects to locally (its dependencies).
    pub fn dependencies(&self, id: &str) -> Vec<&Node> {
        self.local_edges()
            .filter(|e| e.from == id)
            .filter_map(|e| self.node(&e.to))
            .fold(Vec::new(), |mut v, n| {
                if !v.iter().any(|x: &&Node| x.id == n.id) {
                    v.push(n);
                }
                v
            })
    }

    pub fn local_edges(&self) -> impl Iterator<Item = &Edge> {
        self.edges.iter().filter(|e| e.kind == EdgeKind::Local)
    }

    /// Keep dev nodes (not protected), everything connected to them, and the clusters that still
    /// have members.
    pub fn retain_dev(&mut self) {
        let dev: std::collections::HashSet<String> = self
            .nodes
            .iter()
            .filter(|n| n.is_dev && !n.protected)
            .map(|n| n.id.clone())
            .collect();
        let keep: std::collections::HashSet<String> = self
            .nodes
            .iter()
            .filter(|n| {
                dev.contains(&n.id)
                    || self.edges.iter().any(|e| {
                        (e.from == n.id && dev.contains(&e.to))
                            || (e.to == n.id && dev.contains(&e.from))
                    })
            })
            .map(|n| n.id.clone())
            .collect();
        self.retain(|n| keep.contains(&n.id));
    }

    /// Keep only nodes matching `f`, dropping dangling edges and empty clusters.
    pub fn retain(&mut self, f: impl Fn(&Node) -> bool) {
        self.nodes.retain(|n| f(n));
        let ids: std::collections::HashSet<String> =
            self.nodes.iter().map(|n| n.id.clone()).collect();
        self.edges
            .retain(|e| ids.contains(&e.from) && ids.contains(&e.to));
        for c in &mut self.clusters {
            c.nodes.retain(|n| ids.contains(n));
        }
        self.clusters.retain(|c| !c.nodes.is_empty());
        let cluster_ids: std::collections::HashSet<String> =
            self.clusters.iter().map(|c| c.id.clone()).collect();
        for n in &mut self.nodes {
            if n.cluster.as_ref().is_some_and(|c| !cluster_ids.contains(c)) {
                n.cluster = None;
            }
        }
        self.restat();
    }

    pub(crate) fn restat(&mut self) {
        self.stats = GraphStats {
            nodes: self.nodes.len(),
            edges: self.edges.len(),
            clusters: self.clusters.len(),
            connections: self.edges.iter().map(|e| e.connections).sum(),
        };
    }
}

//! Graph tab state: the topology flattened into selectable rows (cluster headers + service
//! nodes), with selection that survives refreshes. Pure data — rendering lives in `graph_ui`.

use holdmap_core::topology::{ClusterKind, Graph, Node, EXTERNAL_ID};

/// One visual row of the graph tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphRow {
    /// Cluster header (id, title).
    Cluster { id: String, title: String },
    /// A node; `last` marks the final member of its group (for tree glyphs).
    Node {
        id: String,
        grouped: bool,
        last: bool,
    },
}

#[derive(Debug, Default)]
pub struct GraphView {
    pub graph: Graph,
    pub rows: Vec<GraphRow>,
    /// Index into `rows`; always points at a `Node` row when any exist.
    pub selected: usize,
    /// Show the collapsed "external hosts" node.
    pub show_external: bool,
}

pub fn cluster_title(kind: ClusterKind, name: &str, detail: Option<&str>) -> String {
    let k = match kind {
        ClusterKind::Compose => "compose",
        ClusterKind::Kubernetes => "k8s",
        ClusterKind::Supervisor => "supervisor",
        ClusterKind::Workspace => "workspace",
        ClusterKind::Git => "git",
    };
    match detail {
        Some(d) if !d.is_empty() => format!("{name}  ({k} · {d})"),
        _ => format!("{name}  ({k})"),
    }
}

impl GraphView {
    pub fn new() -> GraphView {
        GraphView {
            show_external: true,
            ..Default::default()
        }
    }

    /// Replace the graph, keeping the selection on the same node id when it still exists.
    pub fn set_graph(&mut self, graph: Graph) {
        let keep = self.selected_id().map(str::to_string);
        self.graph = graph;
        self.rows = self.flatten();
        self.selected = keep
            .and_then(|id| self.index_of(&id))
            .or_else(|| self.first_node())
            .unwrap_or(0);
    }

    fn flatten(&self) -> Vec<GraphRow> {
        let g = &self.graph;
        let visible = |n: &&Node| self.show_external || n.id != EXTERNAL_ID;
        let mut rows = Vec::new();
        for c in &g.clusters {
            let members: Vec<&Node> = c
                .nodes
                .iter()
                .filter_map(|id| g.node(id))
                .filter(visible)
                .collect();
            if members.is_empty() {
                continue;
            }
            rows.push(GraphRow::Cluster {
                id: c.id.clone(),
                title: cluster_title(c.kind, &c.name, c.detail.as_deref()),
            });
            let n = members.len();
            for (i, m) in members.into_iter().enumerate() {
                rows.push(GraphRow::Node {
                    id: m.id.clone(),
                    grouped: true,
                    last: i + 1 == n,
                });
            }
        }
        let loose: Vec<&Node> = g
            .nodes
            .iter()
            .filter(|n| n.cluster.is_none())
            .filter(visible)
            .collect();
        if !loose.is_empty() && !g.clusters.is_empty() {
            rows.push(GraphRow::Cluster {
                id: String::new(),
                title: "ungrouped".into(),
            });
        }
        let n = loose.len();
        for (i, m) in loose.into_iter().enumerate() {
            rows.push(GraphRow::Node {
                id: m.id.clone(),
                grouped: !g.clusters.is_empty(),
                last: i + 1 == n,
            });
        }
        rows
    }

    pub fn toggle_external(&mut self) {
        self.show_external = !self.show_external;
        let g = std::mem::take(&mut self.graph);
        self.set_graph(g);
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| matches!(r, GraphRow::Node { id: n, .. } if n == id))
    }

    fn first_node(&self) -> Option<usize> {
        self.rows
            .iter()
            .position(|r| matches!(r, GraphRow::Node { .. }))
    }

    pub fn selected_id(&self) -> Option<&str> {
        match self.rows.get(self.selected)? {
            GraphRow::Node { id, .. } => Some(id),
            GraphRow::Cluster { .. } => None,
        }
    }

    pub fn selected_node(&self) -> Option<&Node> {
        self.graph.node(self.selected_id()?)
    }

    /// Select the node that owns this port entry (sync from the Ports tab).
    pub fn select_entry(&mut self, entry_id: &str) {
        if let Some(i) = self
            .graph
            .node_for_entry(entry_id)
            .map(|n| n.id.clone())
            .and_then(|id| self.index_of(&id))
        {
            self.selected = i;
        }
    }

    /// Move by `delta` node rows, skipping cluster headers.
    pub fn move_by(&mut self, delta: isize) {
        let nodes: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, GraphRow::Node { .. }))
            .map(|(i, _)| i)
            .collect();
        if nodes.is_empty() {
            return;
        }
        let cur = nodes.iter().position(|&i| i == self.selected).unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, nodes.len() as isize - 1) as usize;
        self.selected = nodes[next];
    }

    /// Cluster name of the selected node (target for "stop cluster").
    pub fn selected_cluster(&self) -> Option<&str> {
        let cid = self.selected_node()?.cluster.as_deref()?;
        self.graph.cluster(cid).map(|c| c.name.as_str())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use holdmap_core::topology::{Cluster, Edge, EdgeKind, NodeKind, NodePort};
    use holdmap_core::{Exposure, Protocol};

    pub fn node(id: &str, port: Option<u16>, cluster: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind: if id == EXTERNAL_ID {
                NodeKind::External
            } else {
                NodeKind::Service
            },
            label: id.into(),
            subtitle: None,
            root_pid: None,
            pids: vec![],
            ports: port
                .map(|p| NodePort {
                    port: p,
                    protocol: Protocol::Tcp,
                    exposure: Exposure::Loopback,
                    entry_id: format!("tcp:{p}"),
                })
                .into_iter()
                .collect(),
            framework: None,
            project: None,
            project_root: None,
            container: None,
            tunnel: None,
            cluster: cluster.map(Into::into),
            cpu_percent: 0.0,
            memory_bytes: 0,
            is_dev: true,
            protected: false,
        }
    }

    pub fn sample() -> Graph {
        let edge = |from: &str, to: &str, port| Edge {
            id: format!("{from}->{to}"),
            from: from.into(),
            to: to.into(),
            kind: EdgeKind::Local,
            port,
            connections: 1,
            remotes: vec![],
        };
        Graph {
            nodes: vec![
                node("web", Some(3000), Some("c1")),
                node("api", Some(4000), Some("c1")),
                node("db", Some(5432), Some("c1")),
                node("docs", Some(5173), None),
                node(EXTERNAL_ID, None, None),
            ],
            edges: vec![edge("web", "api", 4000), edge("api", "db", 5432)],
            clusters: vec![Cluster {
                id: "c1".into(),
                name: "acme".into(),
                kind: ClusterKind::Workspace,
                detail: Some("pnpm".into()),
                root: None,
                nodes: vec!["web".into(), "api".into(), "db".into()],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn flattens_clusters_then_ungrouped_and_skips_headers() {
        let mut v = GraphView::new();
        v.set_graph(sample());
        assert_eq!(v.rows.len(), 1 + 3 + 1 + 2);
        assert_eq!(v.selected_id(), Some("web"));
        v.move_by(3);
        assert_eq!(v.selected_id(), Some("docs"), "header row skipped");
        v.move_by(10);
        assert_eq!(v.selected_id(), Some(EXTERNAL_ID));
        v.toggle_external();
        assert_eq!(v.rows.len(), 6);
        assert_eq!(
            v.selected_id(),
            Some("web"),
            "hidden node falls back to first"
        );
    }

    #[test]
    fn selection_survives_refresh_and_syncs_from_entries() {
        let mut v = GraphView::new();
        v.set_graph(sample());
        v.select_entry("tcp:5432");
        assert_eq!(v.selected_id(), Some("db"));
        assert_eq!(v.selected_cluster(), Some("acme"));
        v.set_graph(sample());
        assert_eq!(v.selected_id(), Some("db"));
        assert_eq!(
            v.graph
                .dependents("db")
                .iter()
                .map(|n| n.id.as_str())
                .collect::<Vec<_>>(),
            ["api"]
        );
    }
}

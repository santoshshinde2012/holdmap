//! Property tests for dependency ordering and the parsers fed by untrusted text.

use super::{stop_order, Edge, EdgeKind, Graph, Node};
use proptest::prelude::*;

fn node(id: usize) -> Node {
    serde_json::from_value(serde_json::json!({
        "id": format!("n{id}"), "kind": "service", "label": format!("n{id}"), "subtitle": null,
        "root_pid": id, "pids": [id], "ports": [], "framework": null, "project": null,
        "project_root": null, "container": null, "tunnel": null, "cluster": null,
        "cpu_percent": 0.0, "memory_bytes": 0, "is_dev": true, "protected": false
    }))
    .expect("node fixture")
}

fn graph(n: usize, edges: &[(usize, usize)]) -> Graph {
    Graph {
        nodes: (0..n).map(node).collect(),
        edges: edges
            .iter()
            .map(|&(a, b)| Edge {
                id: format!("n{a}->n{b}"),
                from: format!("n{a}"),
                to: format!("n{b}"),
                kind: EdgeKind::Local,
                port: 1000,
                connections: 1,
                remotes: vec![],
            })
            .collect(),
        ..Default::default()
    }
}

proptest! {
    /// For any DAG, every dependent is stopped before what it depends on, and nothing is lost.
    #[test]
    fn dag_order_respects_every_edge(n in 1usize..12, raw in prop::collection::vec((0usize..12, 0usize..12), 0..40)) {
        // Only i → j with i < j: guaranteed acyclic.
        let edges: Vec<(usize, usize)> = raw.into_iter().map(|(a, b)| (a % n, b % n)).filter(|(a, b)| a < b).collect();
        let g = graph(n, &edges);
        let ids: Vec<String> = (0..n).rev().map(|i| format!("n{i}")).collect();
        let o = stop_order(&g, &ids);
        prop_assert!(o.cyclic.is_empty());
        let mut sorted = o.order.clone();
        sorted.sort();
        let mut want = ids.clone();
        want.sort();
        prop_assert_eq!(sorted, want);
        let pos = |id: &str| o.order.iter().position(|x| x == id).unwrap();
        for (a, b) in edges {
            let (pa, pb) = (pos(&format!("n{a}")), pos(&format!("n{b}")));
            prop_assert!(pa < pb, "n{} must stop before n{}", a, b);
        }
    }

    /// With cycles, the order is still a deterministic permutation of the input.
    #[test]
    fn any_graph_yields_a_deterministic_permutation(n in 1usize..10, raw in prop::collection::vec((0usize..10, 0usize..10), 0..40)) {
        let edges: Vec<(usize, usize)> = raw.into_iter().map(|(a, b)| (a % n, b % n)).collect();
        let g = graph(n, &edges);
        let ids: Vec<String> = (0..n).map(|i| format!("n{i}")).collect();
        let a = stop_order(&g, &ids);
        let b = stop_order(&g, &ids);
        prop_assert_eq!(&a, &b);
        let mut sorted = a.order.clone();
        sorted.sort();
        let mut want = ids.clone();
        want.sort();
        prop_assert_eq!(sorted, want);
    }

    /// Remote `ss`/`ps` output is untrusted: the parsers must never panic.
    #[test]
    fn remote_parsers_never_panic(text in "(?s).{0,400}") {
        let _ = crate::remote::parse_ss(&text);
        let _ = crate::remote::parse_ps(&text);
    }

    #[test]
    fn ss_lines_round_trip_ports(port in 1u16..=65535, pid in 1u32..4_000_000) {
        let line = format!("tcp LISTEN 0 4096 127.0.0.1:{port} 0.0.0.0:* users:((\"node\",pid={pid},fd=20))\n");
        let socks = crate::remote::parse_ss(&line);
        prop_assert_eq!(socks.len(), 1);
        prop_assert_eq!(socks[0].local_port, port);
    }

    /// Stop targets come from users and agents: parsing never panics.
    #[test]
    fn target_parse_never_panics(s in "\\PC{0,40}") {
        let _ = crate::Target::parse(&s);
    }
}

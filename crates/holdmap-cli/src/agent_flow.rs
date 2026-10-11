//! Pure terminal presentation of observed process and socket relationships.

use holdmap_core::agents::flow::{AgentFlowEdgeKind, AgentFlowReport};
use holdmap_core::util::printable;

pub fn render(report: &AgentFlowReport) -> String {
    let mut output = format!(
        "Observed agent connections · {} nodes · {} relationships\n\
         Process ancestry and live TCP sockets; tool calls and payloads require recording.\n\n",
        report.nodes.len(),
        report.edges.len()
    );
    for edge in &report.edges {
        let Some(from) = report.nodes.iter().find(|node| node.id == edge.from) else {
            continue;
        };
        let Some(to) = report.nodes.iter().find(|node| node.id == edge.to) else {
            continue;
        };
        let relationship = match edge.kind {
            AgentFlowEdgeKind::Parent => "parent process".to_owned(),
            AgentFlowEdgeKind::OwnsListener => "owns listener".to_owned(),
            AgentFlowEdgeKind::Tcp => format!("connected · {} live sockets", edge.connections),
        };
        let label = |node: &holdmap_core::agents::flow::AgentFlowNode| {
            format!(
                "{}{}{}",
                printable(&node.label),
                node.pid
                    .map_or(String::new(), |pid| format!(" [pid {pid}]")),
                if node.evidence == holdmap_core::agents::Evidence::Inferred {
                    " (role inferred)"
                } else {
                    ""
                }
            )
        };
        output.push_str(&format!(
            "{}  — {relationship} —  {}\n",
            label(from),
            label(to)
        ));
    }
    if report.edges.is_empty() {
        output.push_str("No relationships visible in this scan.\n");
    }
    if report.omitted_nodes > 0 || report.omitted_edges > 0 {
        output.push_str(&format!(
            "\nDisplay limit: {} nodes and {} relationships omitted.\n",
            report.omitted_nodes, report.omitted_edges
        ));
    }
    for limitation in &report.limits {
        output.push_str(&format!("  · {}\n", printable(limitation)));
    }
    output
}

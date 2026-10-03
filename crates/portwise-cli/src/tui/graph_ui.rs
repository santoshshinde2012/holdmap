//! Graph tab rendering: a cluster tree on the left (with dependency highlighting for the
//! selected service) and the selected node's neighbourhood on the right.

use super::app::App;
use super::graph::GraphRow;
use super::theme;
use super::ui::{kv, section, ACCENT, MUTED};
use portwise_core::topology::{EdgeKind, Graph, Node, NodeKind};
use portwise_core::util::human_bytes;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, List, ListItem, ListState, Padding, Paragraph, Wrap};
use ratatui::Frame;
use std::collections::HashSet;

fn kind_color(n: &Node) -> Color {
    if n.protected {
        return MUTED;
    }
    match n.kind {
        NodeKind::Service if n.is_dev => Color::Green,
        NodeKind::Service => Color::Reset,
        NodeKind::Container => Color::Blue,
        NodeKind::Hidden => MUTED,
        NodeKind::Client => Color::Cyan,
        NodeKind::External => Color::Magenta,
    }
}

fn glyph(n: &Node) -> &'static str {
    match n.kind {
        NodeKind::Service => "●",
        NodeKind::Container => "▣",
        NodeKind::Hidden => "◌",
        NodeKind::Client => "○",
        NodeKind::External => "◍",
    }
}

/// Short "→ api, cache" summary of where a node connects.
fn outgoing(g: &Graph, id: &str) -> String {
    let deps: Vec<String> = g.dependencies(id).iter().map(|n| n.label.clone()).collect();
    if deps.is_empty() {
        String::new()
    } else {
        format!("  → {}", deps.join(", "))
    }
}

pub fn draw_graph(f: &mut Frame, app: &mut App, list: Rect, detail: Rect) {
    draw_tree(f, app, list);
    draw_node(f, app, detail);
}

fn draw_tree(f: &mut Frame, app: &mut App, area: Rect) {
    let v = &app.graph;
    let g = &v.graph;
    let sel = v.selected_id().unwrap_or_default().to_string();
    let deps: HashSet<String> = g.dependencies(&sel).iter().map(|n| n.id.clone()).collect();
    let users: HashSet<String> = g.dependents(&sel).iter().map(|n| n.id.clone()).collect();
    let items: Vec<ListItem> = v
        .rows
        .iter()
        .map(|r| match r {
            GraphRow::Cluster { title, .. } => ListItem::new(Line::from(vec![
                Span::styled("▾ ", Style::new().fg(ACCENT)),
                Span::styled(title.clone(), theme::title()),
            ])),
            GraphRow::Node { id, grouped, last } => {
                let Some(n) = g.node(id) else {
                    return ListItem::new("");
                };
                let branch = match (grouped, last) {
                    (false, _) => "",
                    (true, false) => "├─ ",
                    (true, true) => "└─ ",
                };
                let (mark, mark_style) = if deps.contains(id) {
                    ("◂ dep ", Style::new().fg(Color::Green))
                } else if users.contains(id) {
                    ("▸ uses", Style::new().fg(Color::Yellow))
                } else {
                    ("      ", Style::new())
                };
                let ports: Vec<String> = n.ports.iter().map(|p| format!(":{}", p.port)).collect();
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {branch}"), theme::muted()),
                    Span::styled(format!("{} ", glyph(n)), Style::new().fg(kind_color(n))),
                    Span::styled(n.label.clone(), Style::new().fg(kind_color(n)).bold()),
                    Span::raw(" "),
                    Span::styled(ports.join(" "), Style::new().fg(ACCENT)),
                    Span::styled(outgoing(g, id), theme::muted()),
                    Span::raw("  "),
                    Span::styled(mark, mark_style),
                ]))
            }
        })
        .collect();
    let s = &g.stats;
    let n = portwise_core::util::count;
    let title = format!(
        " Graph · {} · {} · {} ",
        n(s.nodes, "service", "services"),
        n(s.edges, "link", "links"),
        n(s.clusters, "cluster", "clusters")
    );
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::muted())
        .title(Span::styled(title, theme::heading()));
    if items.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "No dev services yet — press a to include everything.",
                theme::muted(),
            ))
            .block(block),
            area,
        );
        return;
    }
    let mut state = ListState::default().with_selected(Some(v.selected));
    f.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(
                Style::new()
                    .bg(Color::Indexed(236))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▌"),
        area,
        &mut state,
    );
}

fn draw_node(f: &mut Frame, app: &mut App, area: Rect) {
    let g = &app.graph.graph;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::muted())
        .padding(Padding::horizontal(1));
    let Some(n) = app.graph.selected_node() else {
        f.render_widget(
            Paragraph::new(Span::styled("Select a service.", theme::muted()))
                .block(block.title(" Service ")),
            area,
        );
        return;
    };
    let block = block.title(Span::styled(format!(" {} ", n.display()), theme::title()));
    let w = block.inner(area).width;
    let mut lines: Vec<Line> = Vec::new();
    if let Some(sub) = &n.subtitle {
        lines.push(Line::from(Span::styled(sub.clone(), theme::muted())));
    }
    let users = g.dependents(&n.id);
    let deps = g.dependencies(&n.id);
    // Neighbourhood diagram: users → [node] → deps.
    lines.push(Line::raw(""));
    let names = |v: &[&Node]| -> String {
        if v.is_empty() {
            "·".into()
        } else {
            v.iter()
                .map(|n| n.label.clone())
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    lines.push(Line::from(vec![
        Span::styled(names(&users), Style::new().fg(Color::Yellow)),
        Span::styled("  ──▶  ", theme::muted()),
        Span::styled(
            format!("[ {} ]", n.label),
            Style::new().fg(kind_color(n)).bold(),
        ),
        Span::styled("  ──▶  ", theme::muted()),
        Span::styled(names(&deps), Style::new().fg(Color::Green)),
    ]));
    lines.push(Line::raw(""));
    lines.push(section("service", w));
    let kind = format!("{:?}", n.kind).to_lowercase();
    lines.push(kv("kind", kind));
    if let Some(fw) = &n.framework {
        lines.push(kv("framework", fw.name.clone()));
    }
    if let Some(c) = n.cluster.as_deref().and_then(|c| g.cluster(c)) {
        lines.push(kv(
            "cluster",
            super::graph::cluster_title(c.kind, &c.name, c.detail.as_deref()),
        ));
    }
    if let Some(p) = &n.project {
        lines.push(kv("project", p.clone()));
    }
    if let Some(c) = &n.container {
        lines.push(kv("container", format!("{} ({})", c.name, c.image)));
    }
    if let Some(t) = &n.tunnel {
        lines.push(kv("tunnel", t.target.clone()));
    }
    if !n.pids.is_empty() {
        let pids: Vec<String> = n.pids.iter().take(6).map(|p| p.to_string()).collect();
        let more = n.pids.len().saturating_sub(6);
        lines.push(kv(
            "pids",
            if more > 0 {
                format!("{} +{more}", pids.join(" "))
            } else {
                pids.join(" ")
            },
        ));
        lines.push(kv(
            "usage",
            format!(
                "{:.1}% CPU · {}",
                n.cpu_percent,
                human_bytes(n.memory_bytes)
            ),
        ));
    }
    if n.protected {
        lines.push(kv("safety", "protected — portwise won't stop this"));
    }
    let edges_out: Vec<_> = g.edges.iter().filter(|e| e.from == n.id).collect();
    let edges_in: Vec<_> = g.edges.iter().filter(|e| e.to == n.id).collect();
    if !edges_out.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("depends on", w));
        for e in edges_out {
            let to = g.node(&e.to).map(|n| n.label.clone()).unwrap_or_default();
            let what = if e.kind == EdgeKind::Outbound {
                e.remotes.join(", ")
            } else {
                format!(":{}", e.port)
            };
            lines.push(Line::from(vec![
                Span::styled(" → ", Style::new().fg(Color::Green)),
                Span::styled(to, theme::heading()),
                Span::raw(format!(" {what}")),
                Span::styled(format!("  ×{}", e.connections), theme::muted()),
            ]));
        }
    }
    if !edges_in.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("used by", w));
        for e in edges_in {
            let from = g.node(&e.from).map(|n| n.label.clone()).unwrap_or_default();
            lines.push(Line::from(vec![
                Span::styled(" ← ", Style::new().fg(Color::Yellow)),
                Span::styled(from, theme::heading()),
                Span::raw(format!(" :{}", e.port)),
                Span::styled(format!("  ×{}", e.connections), theme::muted()),
            ]));
        }
        lines.push(Line::from(Span::styled(
            format!(
                " ! stopping {} breaks {} dependent{}",
                n.label,
                users.len(),
                if users.len() == 1 { "" } else { "s" }
            ),
            Style::new().fg(Color::Yellow),
        )));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

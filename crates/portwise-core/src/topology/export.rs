//! Graph exporters (Strategy): JSON, Graphviz DOT, Mermaid and a plain-text tree. Frontends pick
//! one by name via [`exporter`]; adding a format means adding one type.

use super::model::*;
use std::collections::BTreeMap;
use std::fmt::Write;

/// Renders a [`Graph`] as text.
pub trait GraphExporter {
    /// Format name (`json`, `dot`, `mermaid`, `tree`).
    fn name(&self) -> &'static str;
    /// Render the graph.
    fn export(&self, g: &Graph) -> String;
}

/// Look up an exporter by name.
pub fn exporter(name: &str) -> Option<Box<dyn GraphExporter>> {
    match name.to_ascii_lowercase().as_str() {
        "json" => Some(Box::new(JsonExporter)),
        "dot" | "graphviz" => Some(Box::new(DotExporter)),
        "mermaid" | "mmd" => Some(Box::new(MermaidExporter)),
        "tree" | "text" | "ascii" => Some(Box::new(TreeExporter::default())),
        _ => None,
    }
}

/// JSON (the same shape as `portwise graph --json`).
pub struct JsonExporter;
impl GraphExporter for JsonExporter {
    fn name(&self) -> &'static str {
        "json"
    }
    fn export(&self, g: &Graph) -> String {
        serde_json::to_string_pretty(g).unwrap_or_else(|_| "{}".into())
    }
}

fn node_caption(n: &Node) -> String {
    let mut s = n.label.clone();
    if let Some(sub) = &n.subtitle {
        let _ = write!(s, " ({sub})");
    }
    for p in &n.ports {
        let _ = write!(s, " :{}", p.port);
    }
    s
}

fn edge_caption(e: &Edge) -> String {
    let n = if e.connections == 1 {
        String::new()
    } else {
        format!(" ×{}", e.connections)
    };
    match e.kind {
        EdgeKind::Local | EdgeKind::Inbound => format!(":{}{n}", e.port),
        EdgeKind::Outbound => match e.remotes.as_slice() {
            [one] => format!("{one}{n}"),
            _ => format!(":{}{n}", e.port),
        },
    }
}

fn by_cluster(g: &Graph) -> (BTreeMap<&str, Vec<&Node>>, Vec<&Node>) {
    let mut m: BTreeMap<&str, Vec<&Node>> = BTreeMap::new();
    let mut loose = Vec::new();
    for n in &g.nodes {
        match &n.cluster {
            Some(c) => m.entry(c.as_str()).or_default().push(n),
            None => loose.push(n),
        }
    }
    (m, loose)
}

fn esc_dot(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Graphviz DOT.
pub struct DotExporter;
impl GraphExporter for DotExporter {
    fn name(&self) -> &'static str {
        "dot"
    }
    fn export(&self, g: &Graph) -> String {
        let mut o = String::from(
            "digraph portwise {\n  rankdir=LR;\n  node [shape=box, style=\"rounded,filled\", fillcolor=\"#f5f7fb\", fontname=\"Helvetica\"];\n  edge [fontname=\"Helvetica\", fontsize=10];\n",
        );
        let (clusters, loose) = by_cluster(g);
        for (i, c) in g.clusters.iter().enumerate() {
            let _ = writeln!(
                o,
                "  subgraph cluster_{i} {{\n    label=\"{} ({:?})\";\n    style=\"rounded,dashed\";",
                esc_dot(&c.name),
                c.kind
            );
            for n in clusters.get(c.id.as_str()).into_iter().flatten() {
                let _ = writeln!(
                    o,
                    "    \"{}\" [label=\"{}\"];",
                    esc_dot(&n.id),
                    esc_dot(&node_caption(n))
                );
            }
            o.push_str("  }\n");
        }
        for n in loose {
            let shape = if n.kind == NodeKind::External {
                ", shape=ellipse, fillcolor=\"#eef0f3\""
            } else {
                ""
            };
            let _ = writeln!(
                o,
                "  \"{}\" [label=\"{}\"{shape}];",
                esc_dot(&n.id),
                esc_dot(&node_caption(n))
            );
        }
        for e in &g.edges {
            let style = if e.kind == EdgeKind::Local {
                ""
            } else {
                ", style=dashed"
            };
            let _ = writeln!(
                o,
                "  \"{}\" -> \"{}\" [label=\"{}\"{style}];",
                esc_dot(&e.from),
                esc_dot(&e.to),
                esc_dot(&edge_caption(e))
            );
        }
        o.push_str("}\n");
        o
    }
}

fn mermaid_id(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn esc_mermaid(s: &str) -> String {
    s.replace('"', "#quot;")
}

/// Mermaid flowchart (pastes into GitHub comments).
pub struct MermaidExporter;
impl GraphExporter for MermaidExporter {
    fn name(&self) -> &'static str {
        "mermaid"
    }
    fn export(&self, g: &Graph) -> String {
        let mut o = String::from("flowchart LR\n");
        let (clusters, loose) = by_cluster(g);
        for c in &g.clusters {
            let _ = writeln!(
                o,
                "  subgraph {}[\"{}\"]",
                mermaid_id(&c.id),
                esc_mermaid(&c.name)
            );
            for n in clusters.get(c.id.as_str()).into_iter().flatten() {
                let _ = writeln!(
                    o,
                    "    {}[\"{}\"]",
                    mermaid_id(&n.id),
                    esc_mermaid(&node_caption(n))
                );
            }
            o.push_str("  end\n");
        }
        for n in loose {
            if n.kind == NodeKind::External {
                let _ = writeln!(
                    o,
                    "  {}((\"{}\"))",
                    mermaid_id(&n.id),
                    esc_mermaid(&n.label)
                );
            } else {
                let _ = writeln!(
                    o,
                    "  {}[\"{}\"]",
                    mermaid_id(&n.id),
                    esc_mermaid(&node_caption(n))
                );
            }
        }
        for e in &g.edges {
            let arrow = if e.kind == EdgeKind::Local {
                "-->"
            } else {
                "-.->"
            };
            let _ = writeln!(
                o,
                "  {} {arrow}|\"{}\"| {}",
                mermaid_id(&e.from),
                esc_mermaid(&edge_caption(e)),
                mermaid_id(&e.to)
            );
        }
        o
    }
}

/// Styling hooks for [`TreeExporter`] (frontends add colour without re-implementing layout).
pub trait TreeStyle {
    /// Style a cluster heading.
    fn heading(&self, s: &str) -> String {
        s.to_string()
    }
    /// Style a service label.
    fn label(&self, s: &str) -> String {
        s.to_string()
    }
    /// Style a port.
    fn port(&self, s: &str) -> String {
        s.to_string()
    }
    /// Style secondary text.
    fn dim(&self, s: &str) -> String {
        s.to_string()
    }
    /// Style a warning.
    fn warn(&self, s: &str) -> String {
        s.to_string()
    }
}

/// No styling.
pub struct PlainStyle;
impl TreeStyle for PlainStyle {}

/// Indented text tree: clusters → services → their dependencies.
pub struct TreeExporter<'s> {
    /// Use ASCII instead of box-drawing characters.
    pub ascii: bool,
    /// How to colour the output.
    pub style: &'s dyn TreeStyle,
}

impl Default for TreeExporter<'_> {
    fn default() -> Self {
        Self {
            ascii: false,
            style: &PlainStyle,
        }
    }
}

impl TreeExporter<'_> {
    /// An ASCII-only exporter.
    pub fn ascii() -> Self {
        Self {
            ascii: true,
            ..Default::default()
        }
    }

    fn glyphs(&self) -> [&'static str; 6] {
        if self.ascii {
            ["|-- ", "`-- ", "|   ", "    ", "->", "<-"]
        } else {
            ["├─ ", "└─ ", "│  ", "   ", "→", "←"]
        }
    }

    fn header(&self, n: &Node) -> String {
        let st = self.style;
        let mut s = st.label(&n.label);
        if let Some(sub) = &n.subtitle {
            let _ = write!(s, " {}", st.dim(&format!("({sub})")));
        }
        for p in &n.ports {
            let _ = write!(s, " {}", st.port(&format!(":{}", p.port)));
        }
        if let Some(pid) = n.root_pid {
            let _ = write!(s, "  {}", st.dim(&format!("pid {pid}")));
        }
        if n.kind == NodeKind::Client {
            let _ = write!(s, "  {}", st.dim("client"));
        }
        if let Some(t) = &n.tunnel {
            let _ = write!(s, "  {}", st.warn(&format!("[tunnel: {}]", t.target)));
        }
        s
    }

    fn node_lines(&self, g: &Graph, n: &Node, prefix: &str, out: &mut String) {
        let [tee, elbow, _, _, arrow, back] = self.glyphs();
        let st = self.style;
        let mut deps: Vec<String> = g
            .edges
            .iter()
            .filter(|e| e.from == n.id && e.kind != EdgeKind::Inbound)
            .map(|e| {
                let to = g
                    .node(&e.to)
                    .map(|t| t.label.clone())
                    .unwrap_or_else(|| e.to.clone());
                format!("{} {} {}", st.dim(arrow), to, st.port(&edge_caption(e)))
            })
            .collect();
        let inbound: usize = g
            .edges
            .iter()
            .filter(|e| e.to == n.id && e.kind == EdgeKind::Inbound)
            .map(|e| e.connections)
            .sum();
        if inbound > 0 {
            deps.push(st.warn(&format!(
                "{back} {}",
                crate::util::count(inbound, "external connection", "external connections")
            )));
        }
        for (i, d) in deps.iter().enumerate() {
            let last = i + 1 == deps.len();
            let _ = writeln!(out, "{prefix}{}{d}", st.dim(if last { elbow } else { tee }));
        }
    }

    fn members(&self, g: &Graph, nodes: &[&Node], out: &mut String) {
        let [tee, elbow, pipe, blank, _, _] = self.glyphs();
        for (i, n) in nodes.iter().enumerate() {
            let last = i + 1 == nodes.len();
            let _ = writeln!(
                out,
                "{}{}",
                self.style.dim(if last { elbow } else { tee }),
                self.header(n)
            );
            let p = if last { blank } else { pipe };
            self.node_lines(g, n, &format!("{}  ", self.style.dim(p)), out);
        }
    }
}

impl GraphExporter for TreeExporter<'_> {
    fn name(&self) -> &'static str {
        "tree"
    }
    fn export(&self, g: &Graph) -> String {
        let st = self.style;
        let mut o = String::new();
        let (clusters, loose) = by_cluster(g);
        for c in &g.clusters {
            let kind = format!("{:?}", c.kind).to_lowercase();
            let detail = c
                .detail
                .as_ref()
                .map(|d| format!(", {d}"))
                .unwrap_or_default();
            let _ = writeln!(
                o,
                "{} {}",
                st.heading(&c.name),
                st.dim(&format!("({kind}{detail})"))
            );
            let members = clusters.get(c.id.as_str()).cloned().unwrap_or_default();
            self.members(g, &members, &mut o);
            o.push('\n');
        }
        let loose: Vec<&Node> = loose
            .into_iter()
            .filter(|n| n.kind != NodeKind::External)
            .collect();
        if !loose.is_empty() {
            let _ = writeln!(o, "{}", st.heading("Ungrouped"));
            self.members(g, &loose, &mut o);
            o.push('\n');
        }
        let s = &g.stats;
        let plural = |n: usize, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
        let _ = writeln!(
            o,
            "{}",
            st.dim(&format!(
                "{} · {} · {} · {}",
                plural(
                    g.nodes
                        .iter()
                        .filter(|n| n.kind != NodeKind::External)
                        .count(),
                    "service"
                ),
                plural(s.edges, "link"),
                plural(s.connections, "connection"),
                plural(s.clusters, "cluster")
            ))
        );
        if self.ascii {
            o = o.replace('×', "x").replace('·', "-");
        }
        o
    }
}

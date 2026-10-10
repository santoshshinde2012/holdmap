//! `holdmap graph`: the service topology as a coloured tree, or JSON / DOT / Mermaid.

use crate::style::{self, paint, S};
use anyhow::{bail, Context, Result};
use holdmap_core::topology::{exporter, Graph, GraphExporter, TreeExporter, TreeStyle};
use holdmap_core::{Engine, ScanOptions};

#[derive(clap::Args, Debug, Default)]
pub struct GraphArgs {
    /// Include everything (system services, apps), not just dev services and their peers.
    #[arg(short, long)]
    pub all: bool,
    /// Only show one cluster (id or name).
    #[arg(short, long)]
    pub cluster: Option<String>,
    /// Hide the collapsed "external hosts" node.
    #[arg(long)]
    pub no_external: bool,
    /// Output format.
    #[arg(short, long, value_parser = ["tree", "json", "dot", "mermaid"], default_value = "tree")]
    pub format: String,
    /// Shorthand for --format json.
    #[arg(long, conflicts_with_all = ["dot", "mermaid"])]
    pub json: bool,
    /// Shorthand for --format dot (Graphviz).
    #[arg(long, conflicts_with = "mermaid")]
    pub dot: bool,
    /// Shorthand for --format mermaid.
    #[arg(long)]
    pub mermaid: bool,
    /// ASCII-only tree (no box-drawing characters).
    #[arg(long)]
    pub ascii: bool,
}

struct Ansi;
impl TreeStyle for Ansi {
    fn heading(&self, s: &str) -> String {
        paint(s, S::Bold)
    }
    fn label(&self, s: &str) -> String {
        paint(s, S::Bold)
    }
    fn port(&self, s: &str) -> String {
        // Same as the `list` table's port column and the TUI: accent + bold.
        paint(s, S::BoldCyan)
    }
    fn dim(&self, s: &str) -> String {
        style::dim(s)
    }
    fn warn(&self, s: &str) -> String {
        paint(s, S::Yellow)
    }
}

/// Build the graph the same way for every surface.
pub fn build(e: &Engine, all: bool, cluster: Option<&str>, external: bool) -> Result<Graph> {
    let mut g = e.topology();
    if !external {
        g.retain(|n| n.kind != holdmap_core::topology::NodeKind::External);
    }
    if let Some(q) = cluster {
        let Some(c) = g.find_cluster(q).cloned() else {
            let names: Vec<&str> = g.clusters.iter().map(|c| c.name.as_str()).collect();
            bail!(
                "no cluster named `{q}`{}",
                if names.is_empty() {
                    String::new()
                } else {
                    format!(" (known: {})", names.join(", "))
                }
            );
        };
        // The cluster plus its direct dependencies.
        let members: std::collections::HashSet<String> = c.nodes.iter().cloned().collect();
        let mut keep = members.clone();
        for e in g.edges.iter().filter(|e| members.contains(&e.from)) {
            keep.insert(e.to.clone());
        }
        g.retain(|n| keep.contains(&n.id));
    } else if !all {
        g.retain_dev();
    }
    Ok(g)
}

pub fn run(a: &GraphArgs, docker: bool) -> Result<u8> {
    let e = Engine::new(&ScanOptions {
        all_states: false,
        docker,
    })
    .context("failed to scan sockets")?;
    let g = build(&e, a.all, a.cluster.as_deref(), !a.no_external)?;
    let format = if a.json {
        "json"
    } else if a.dot {
        "dot"
    } else if a.mermaid {
        "mermaid"
    } else {
        a.format.as_str()
    };
    let text = if format == "tree" {
        if g.nodes.is_empty() {
            println!("{}", style::dim("No services found. Try --all."));
            return Ok(crate::exit::BUSY);
        }
        TreeExporter {
            ascii: a.ascii,
            style: &Ansi,
        }
        .export(&g)
    } else {
        exporter(format).expect("validated by clap").export(&g)
    };
    print!("{}", style::terminal_text(&text));
    if !text.ends_with('\n') {
        println!();
    }
    Ok(crate::exit::OK)
}

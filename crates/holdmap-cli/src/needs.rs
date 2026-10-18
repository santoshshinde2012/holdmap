//! `holdmap needs`: the short "needs you" list (exposed agents, root or wide access, risky
//! settings, unpinned MCP servers, open dev servers, analyzer findings). Read-only; the same
//! rules as the desktop's Home and the MCP `list_needs` tool (holdmap-core `attention`).

use crate::style::{self, dim, paint, S};
use anyhow::{Context, Result};
use clap::Args;
use holdmap_core::attention::{attention, AttentionItem, Urgency};
use holdmap_core::{Engine, ScanOptions};

#[derive(Args, Debug)]
pub struct NeedsArgs {
    /// Machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
    /// Exit with status 1 when any item is listed.
    #[arg(long)]
    pub fail: bool,
}

pub fn run(a: &NeedsArgs, docker: bool) -> Result<u8> {
    let e = Engine::new(&ScanOptions {
        all_states: false,
        docker,
    })
    .context("failed to scan processes")?;
    let report = e.agents_with_capabilities();
    let extra =
        holdmap_analyzers::needs::insight_items(&crate::analyzers::service().insights(), &report);
    let items = attention(Some(&e.snapshot().entries), Some(&report), extra);
    if a.json {
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else {
        print!("{}", style::terminal_text(&render(&items)));
    }
    Ok(if a.fail && !items.is_empty() {
        crate::exit::BUSY
    } else {
        crate::exit::OK
    })
}

pub fn render(items: &[AttentionItem]) -> String {
    if items.is_empty() {
        return format!("{} Nothing needs you right now.\n", style::ok_mark());
    }
    let mut out = String::new();
    for i in items {
        let mark = match i.severity {
            Urgency::High => paint("high  ", S::BoldRed),
            Urgency::Medium => paint("medium", S::BoldYellow),
        };
        out.push_str(&format!("{mark} {}\n       {}\n", i.title, dim(&i.detail)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_core::attention::{RuleId, Target};

    #[test]
    fn renders_items_and_the_empty_case() {
        assert!(render(&[]).contains("Nothing needs you"));
        let item = AttentionItem {
            id: "root:a".into(),
            rule: RuleId::Root,
            severity: Urgency::High,
            title: "Codex runs as root".into(),
            detail: "Everything it starts gets administrator rights.".into(),
            target: Target::Agents {
                agent_id: "a".into(),
            },
            stop: None,
        };
        let text = render(&[item]);
        assert!(text.contains("high") && text.contains("Codex runs as root"));
        assert!(text.contains("administrator rights"));
    }
}

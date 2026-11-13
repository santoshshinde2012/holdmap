//! Cursor (editor and `cursor-agent`).
//!
//! Sources (checked November 2026):
//! - <https://cursor.com/docs/context/rules> (`.cursor/rules` holds `.mdc` files and nests;
//!   AGENTS.md, nested AGENTS.md; User and Team rules live in settings or the dashboard)
//! - <https://cursor.com/docs/context/skills> (`.agents/skills`, `.cursor/skills`,
//!   `~/.agents/skills`, `~/.cursor/skills`, plus compatibility `.claude/skills`,
//!   `.codex/skills`, `~/.claude/skills`, `~/.codex/skills`)
//! - <https://cursor.com/docs/context/mcp> (`.cursor/mcp.json`, `~/.cursor/mcp.json`)
//! - <https://cursor.com/docs/agent/hooks> (`.cursor/hooks.json`, `~/.cursor/hooks.json`,
//!   enterprise hooks per OS)
//! - <https://cursor.com/docs/context/subagents> (`.cursor/agents`, `~/.cursor/agents`, and the
//!   `.claude/agents` / `.codex/agents` compatibility folders)
//! - <https://cursor.com/docs/cli/reference/permissions> (`~/.cursor/cli-config.json`)

use super::{per_os, Base as B, Form, LocationSpec as L, Reader as R};
use crate::agents::capabilities::{ConfigFormat, InstructionKind as I};
use crate::transparency::places::{PlaceKind as K, PlaceScope as S};

const COMPAT: &str = "Compatibility folder Cursor also reads.";

/// Cursor rows for `platform`.
pub(super) fn specs(platform: &str) -> Vec<L> {
    let mut rows = vec![
        L::new(K::Skills, S::User, B::Home, ".cursor/skills", Form::SkillRoot, "Skills").read(R::Skills),
        L::new(K::Skills, S::User, B::Home, ".agents/skills", Form::SkillRoot, "Skills").read(R::Skills),
        L::new(K::Skills, S::User, B::Home, ".claude/skills", Form::SkillRoot, "Claude skills")
            .read(R::Skills)
            .note(COMPAT),
        L::new(K::Skills, S::User, B::Home, ".codex/skills", Form::SkillRoot, "Codex skills")
            .read(R::Skills)
            .note(COMPAT),
        L::new(K::McpConfig, S::User, B::Home, ".cursor/mcp.json", Form::File, "mcp.json")
            .read(R::Mcp(ConfigFormat::JsonMcpServers)),
        L::new(K::Settings, S::User, B::Home, ".cursor/cli-config.json", Form::File, "cli-config.json")
            .read(R::Policy),
        L::new(K::Commands, S::User, B::Home, ".cursor/commands", Form::Named(&["md"]), "Commands")
            .read(R::Instructions(I::Command, Some("md"))),
        L::new(K::Subagents, S::User, B::Home, ".cursor/agents", Form::Named(&["md"]), "Subagents"),
        L::new(K::Hooks, S::User, B::Home, ".cursor/hooks.json", Form::File, "hooks.json"),
        L::new(K::Skills, S::Project, B::Project, ".cursor/skills", Form::SkillRoot, "Skills").read(R::Skills),
        L::new(K::Skills, S::Project, B::Project, ".agents/skills", Form::SkillRoot, "Skills").read(R::Skills),
        L::new(K::Skills, S::Project, B::Project, ".claude/skills", Form::SkillRoot, "Claude skills")
            .read(R::Skills)
            .note(COMPAT),
        L::new(K::Skills, S::Project, B::Project, ".codex/skills", Form::SkillRoot, "Codex skills")
            .read(R::Skills)
            .note(COMPAT),
        L::new(K::McpConfig, S::Project, B::Project, ".cursor/mcp.json", Form::File, "mcp.json")
            .read(R::Mcp(ConfigFormat::JsonMcpServers)),
        L::new(K::Instructions, S::Project, B::Project, "AGENTS.md", Form::File, "AGENTS.md")
            .read(R::Instructions(I::Memory, None)),
        L::new(
            K::Rules,
            S::Project,
            B::Project,
            ".cursor/rules",
            Form::Deep { loads: &["mdc"], ignored: &["md"] },
            "Project rules",
        )
        .read(R::Instructions(I::Rule, Some("mdc")))
        .note("Cursor loads .mdc rule files; plain .md files in this folder are ignored."),
        L::new(K::Instructions, S::Project, B::Project, ".cursorrules", Form::File, ".cursorrules")
            .note("Legacy single rules file; Cursor recommends .cursor/rules."),
        L::new(K::Commands, S::Project, B::Project, ".cursor/commands", Form::Named(&["md"]), "Commands")
            .read(R::Instructions(I::Command, Some("md"))),
        L::new(K::Subagents, S::Project, B::Project, ".cursor/agents", Form::Named(&["md"]), "Subagents"),
        L::new(K::Hooks, S::Project, B::Project, ".cursor/hooks.json", Form::File, "hooks.json"),
        L::new(K::Rules, S::Account, B::Home, "", Form::Account, "User and Team rules")
            .note("User rules live in Cursor Settings and Team rules in the dashboard; neither is a file holdmap can see."),
    ];
    if let Some(path) = per_os(
        platform,
        "/Library/Application Support/Cursor/hooks.json",
        "/etc/cursor/hooks.json",
        r"C:\ProgramData\Cursor\hooks.json",
    ) {
        rows.push(L::new(
            K::Hooks,
            S::Managed,
            B::System,
            path,
            Form::File,
            "Enterprise hooks.json",
        ));
    }
    rows
}

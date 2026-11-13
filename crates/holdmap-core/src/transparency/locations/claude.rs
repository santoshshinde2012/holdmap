//! Claude Code and Claude Desktop.
//!
//! Sources (checked November 2026):
//! - <https://code.claude.com/docs/en/memory> (CLAUDE.md, CLAUDE.local.md, rules, auto memory,
//!   managed CLAUDE.md per OS, the 200-line / 25 KB MEMORY.md load limit)
//! - <https://code.claude.com/docs/en/settings> (settings files, precedence, `CLAUDE_CONFIG_DIR`)
//! - <https://code.claude.com/docs/en/claude-directory> (skills, commands, agents, output styles,
//!   agent-memory, plugins, transcripts, history, file-history, paste-cache, plans,
//!   `.credentials.json`, `remote-settings.json`)
//! - managed-settings.json paths: <https://code.claude.com/docs/en/settings.md>

use super::{per_os, Base as B, Form, LocationSpec as L, Reader as R};
use crate::agents::capabilities::{ConfigFormat, InstructionKind as I};
use crate::transparency::places::{PlaceKind as K, PlaceScope as S};

const AGENTS_MD_NOTE: &str = "Claude Code reads AGENTS.md only when no CLAUDE.md, .claude/CLAUDE.md or CLAUDE.local.md is in this folder or above it (the default Project instructions setting).";
const AUTO_MEMORY_NOTE: &str = "Notes Claude writes for itself. The first 200 lines or 25 KB of MEMORY.md load every session. holdmap derives the folder name from the project path; autoMemoryDirectory or CLAUDE_CODE_DISABLE_AUTO_MEMORY in the agent's own settings can change this and are not visible here.";
const TRANSCRIPT_NOTE: &str = "Plaintext session transcripts. Claude Code deletes them after cleanupPeriodDays (30 days by default). holdmap shows only total size, file count and newest time.";

/// Claude Code rows for `platform`.
pub(super) fn code(platform: &str) -> Vec<L> {
    let mut rows = vec![
        // User layer, under ~/.claude (or CLAUDE_CONFIG_DIR).
        L::new(
            K::Skills,
            S::User,
            B::ClaudeHome,
            "skills",
            Form::SkillRoot,
            "Skills",
        )
        .read(R::Skills),
        L::new(
            K::McpConfig,
            S::User,
            B::ClaudeJsonDir,
            ".claude.json",
            Form::File,
            ".claude.json",
        )
        .read(R::Mcp(ConfigFormat::JsonMcpServers)),
        L::new(
            K::Instructions,
            S::User,
            B::ClaudeHome,
            "CLAUDE.md",
            Form::File,
            "CLAUDE.md",
        )
        .read(R::Instructions(I::Memory, None)),
        L::new(
            K::Rules,
            S::User,
            B::ClaudeHome,
            "rules",
            Form::Deep {
                loads: &["md"],
                ignored: &[],
            },
            "Rules",
        ),
        L::new(
            K::Settings,
            S::User,
            B::ClaudeHome,
            "settings.json",
            Form::File,
            "settings.json",
        )
        .read(R::Policy),
        L::new(
            K::Commands,
            S::User,
            B::ClaudeHome,
            "commands",
            Form::Named(&["md"]),
            "Commands",
        )
        .read(R::Instructions(I::Command, Some("md"))),
        L::new(
            K::Subagents,
            S::User,
            B::ClaudeHome,
            "agents",
            Form::Named(&["md"]),
            "Subagents",
        )
        .read(R::Instructions(I::Subagent, Some("md"))),
        L::new(
            K::Instructions,
            S::User,
            B::ClaudeHome,
            "output-styles",
            Form::Named(&["md"]),
            "Output styles",
        ),
        L::new(
            K::Extensions,
            S::User,
            B::ClaudeHome,
            "plugins",
            Form::Folder,
            "Plugins",
        )
        .note("Installed plugins and cloned marketplaces, managed by claude plugin commands."),
        L::new(
            K::AgentMemory,
            S::User,
            B::ClaudeHome,
            "projects/{project}/memory",
            Form::AutoMemory,
            "Auto memory",
        )
        .note(AUTO_MEMORY_NOTE),
        L::new(
            K::AgentMemory,
            S::User,
            B::ClaudeHome,
            "agent-memory",
            Form::Folder,
            "Subagent memory",
        ),
        L::new(
            K::SessionData,
            S::User,
            B::ClaudeHome,
            "projects",
            Form::SessionData,
            "Transcripts",
        )
        .note(TRANSCRIPT_NOTE),
        L::new(
            K::SessionData,
            S::User,
            B::ClaudeHome,
            "history.jsonl",
            Form::SessionData,
            "Prompt history",
        )
        .note("Prompts you typed, kept until you delete them."),
        L::new(
            K::SessionData,
            S::User,
            B::ClaudeHome,
            "file-history",
            Form::SessionData,
            "Edit checkpoints",
        ),
        L::new(
            K::SessionData,
            S::User,
            B::ClaudeHome,
            "paste-cache",
            Form::SessionData,
            "Large pastes",
        ),
        L::new(
            K::SessionData,
            S::User,
            B::ClaudeHome,
            "plans",
            Form::SessionData,
            "Plans",
        ),
        L::new(
            K::SecretStore,
            S::User,
            B::ClaudeHome,
            ".credentials.json",
            Form::SecretStore,
            "Login credentials",
        ),
        L::new(
            K::Settings,
            S::Managed,
            B::ClaudeHome,
            "remote-settings.json",
            Form::File,
            "Cached server-managed settings",
        ),
        // Project layer.
        L::new(
            K::Skills,
            S::Project,
            B::Project,
            ".claude/skills",
            Form::SkillRoot,
            "Skills",
        )
        .read(R::Skills),
        L::new(
            K::McpConfig,
            S::Project,
            B::Project,
            ".mcp.json",
            Form::File,
            ".mcp.json",
        )
        .read(R::Mcp(ConfigFormat::JsonMcpServers)),
        L::new(
            K::Instructions,
            S::Project,
            B::Project,
            "CLAUDE.md",
            Form::File,
            "CLAUDE.md",
        )
        .read(R::Instructions(I::Memory, None)),
        L::new(
            K::Instructions,
            S::Project,
            B::Project,
            ".claude/CLAUDE.md",
            Form::File,
            ".claude/CLAUDE.md",
        )
        .read(R::Instructions(I::Memory, None)),
        L::new(
            K::Instructions,
            S::Local,
            B::Project,
            "CLAUDE.local.md",
            Form::File,
            "CLAUDE.local.md",
        )
        .note("Personal project instructions, loaded after CLAUDE.md in the same folder."),
        L::new(
            K::Instructions,
            S::Project,
            B::Project,
            "AGENTS.md",
            Form::File,
            "AGENTS.md",
        )
        .note(AGENTS_MD_NOTE),
        L::new(
            K::Rules,
            S::Project,
            B::Project,
            ".claude/rules",
            Form::Deep {
                loads: &["md"],
                ignored: &[],
            },
            "Rules",
        ),
        L::new(
            K::Settings,
            S::Project,
            B::Project,
            ".claude/settings.json",
            Form::File,
            "settings.json",
        )
        .read(R::Policy),
        L::new(
            K::Settings,
            S::Local,
            B::Project,
            ".claude/settings.local.json",
            Form::File,
            "settings.local.json",
        )
        .read(R::Policy),
        L::new(
            K::Commands,
            S::Project,
            B::Project,
            ".claude/commands",
            Form::Named(&["md"]),
            "Commands",
        )
        .read(R::Instructions(I::Command, Some("md"))),
        L::new(
            K::Subagents,
            S::Project,
            B::Project,
            ".claude/agents",
            Form::Named(&["md"]),
            "Subagents",
        )
        .read(R::Instructions(I::Subagent, Some("md"))),
        L::new(
            K::Instructions,
            S::Project,
            B::Project,
            ".claude/output-styles",
            Form::Named(&["md"]),
            "Output styles",
        ),
        L::new(
            K::AgentMemory,
            S::Project,
            B::Project,
            ".claude/agent-memory",
            Form::Folder,
            "Subagent memory",
        ),
    ];
    if let Some(path) = per_os(
        platform,
        "/Library/Application Support/ClaudeCode/managed-settings.json",
        "/etc/claude-code/managed-settings.json",
        r"C:\Program Files\ClaudeCode\managed-settings.json",
    ) {
        rows.push(
            L::new(
                K::Settings,
                S::Managed,
                B::System,
                path,
                Form::File,
                "managed-settings.json",
            )
            .note("Organization policy. Nothing in user, project or local settings overrides it."),
        );
    }
    if let Some(path) = per_os(
        platform,
        "/Library/Application Support/ClaudeCode/CLAUDE.md",
        "/etc/claude-code/CLAUDE.md",
        r"C:\Program Files\ClaudeCode\CLAUDE.md",
    ) {
        rows.push(L::new(
            K::Instructions,
            S::Managed,
            B::System,
            path,
            Form::File,
            "Managed CLAUDE.md",
        ));
    }
    rows
}

/// Claude Desktop: its manual MCP file (macOS Application Support, Windows `%APPDATA%`).
/// Skills and connectors live in the account.
pub(super) fn desktop() -> Vec<L> {
    vec![L::new(
        K::McpConfig,
        S::User,
        B::AppData,
        "Claude/claude_desktop_config.json",
        Form::File,
        "claude_desktop_config.json",
    )
    .read(R::Mcp(ConfigFormat::JsonMcpServers))]
}

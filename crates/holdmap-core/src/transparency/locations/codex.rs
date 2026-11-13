//! Codex CLI and the Codex app.
//!
//! Sources (checked November 2026):
//! - <https://developers.openai.com/codex/skills> (`.agents/skills` from the working folder up
//!   to the repository root, `~/.agents/skills`, `/etc/codex/skills`)
//! - <https://developers.openai.com/codex/config-advanced> (`CODEX_HOME`, `config.toml`,
//!   `auth.json`, `history.jsonl`, profiles)
//! - <https://developers.openai.com/codex/guides/agents-md> (`~/.codex/AGENTS.md`, project
//!   AGENTS.md, AGENTS.override.md)
//! - <https://developers.openai.com/codex/hooks> (`~/.codex/hooks.json`, `.codex/hooks.json`)
//! - <https://developers.openai.com/codex/memories> (`~/.codex/memories`)
//! - <https://developers.openai.com/codex/enterprise/managed-configuration>
//!   (`/etc/codex/managed_config.toml`, `/etc/codex/requirements.toml`)
//!
//! Not added: `~/.codex/skills` is not in the Codex docs (Cursor reads it for compatibility, so
//! it appears under Cursor); `~/.codex/sessions` is not documented as a stable location.

use super::{per_os, Base as B, Form, LocationSpec as L, Reader as R};
use crate::agents::capabilities::{ConfigFormat, InstructionKind as I};
use crate::transparency::places::{PlaceKind as K, PlaceScope as S};

const LEGACY: &str = "Not in current Codex docs; Codex reads .agents/skills. Shown so an old folder is not mistaken for a loaded one.";

/// Codex rows for `platform`.
pub(super) fn specs(platform: &str) -> Vec<L> {
    let mut rows = vec![
        L::new(
            K::Skills,
            S::User,
            B::Home,
            ".agents/skills",
            Form::SkillRoot,
            "Skills",
        )
        .read(R::Skills),
        L::new(
            K::McpConfig,
            S::User,
            B::CodexHome,
            "config.toml",
            Form::File,
            "config.toml",
        )
        .read(R::Mcp(ConfigFormat::Toml)),
        L::new(
            K::Instructions,
            S::User,
            B::CodexHome,
            "AGENTS.md",
            Form::File,
            "AGENTS.md",
        )
        .read(R::Instructions(I::Memory, None)),
        L::new(
            K::Instructions,
            S::User,
            B::CodexHome,
            "AGENTS.override.md",
            Form::File,
            "AGENTS.override.md",
        )
        .note("When present, Codex uses it instead of AGENTS.md in the same folder."),
        L::new(
            K::Commands,
            S::User,
            B::CodexHome,
            "prompts",
            Form::Named(&["md"]),
            "Prompts",
        )
        .read(R::Instructions(I::Prompt, Some("md"))),
        L::new(
            K::Hooks,
            S::User,
            B::CodexHome,
            "hooks.json",
            Form::File,
            "hooks.json",
        ),
        L::new(
            K::AgentMemory,
            S::User,
            B::CodexHome,
            "memories",
            Form::Folder,
            "Memories",
        )
        .note("Notes Codex writes for itself when memories are turned on."),
        L::new(
            K::SessionData,
            S::User,
            B::CodexHome,
            "history.jsonl",
            Form::SessionData,
            "Prompt history",
        ),
        L::new(
            K::SecretStore,
            S::User,
            B::CodexHome,
            "auth.json",
            Form::SecretStore,
            "Login credentials",
        ),
        L::new(
            K::Skills,
            S::User,
            B::CodexHome,
            "skills",
            Form::SkillRoot,
            "Legacy skills",
        )
        .note(LEGACY),
        L::new(
            K::Skills,
            S::Project,
            B::Project,
            ".agents/skills",
            Form::SkillRoot,
            "Skills",
        )
        .read(R::Skills),
        L::new(
            K::McpConfig,
            S::Project,
            B::Project,
            ".codex/config.toml",
            Form::File,
            "config.toml",
        )
        .read(R::Mcp(ConfigFormat::Toml))
        .note("Codex loads project config only for trusted projects."),
        L::new(
            K::Instructions,
            S::Project,
            B::Project,
            "AGENTS.md",
            Form::File,
            "AGENTS.md",
        )
        .read(R::Instructions(I::Memory, None)),
        L::new(
            K::Instructions,
            S::Project,
            B::Project,
            "AGENTS.override.md",
            Form::File,
            "AGENTS.override.md",
        ),
        L::new(
            K::Hooks,
            S::Project,
            B::Project,
            ".codex/hooks.json",
            Form::File,
            "hooks.json",
        ),
        L::new(
            K::Skills,
            S::Project,
            B::Project,
            ".codex/skills",
            Form::SkillRoot,
            "Legacy skills",
        )
        .note(LEGACY),
    ];
    if let Some(path) = per_os(
        platform,
        "/etc/codex/managed_config.toml",
        "/etc/codex/managed_config.toml",
        "",
    ) {
        rows.push(
            L::new(
                K::Settings,
                S::Managed,
                B::System,
                path,
                Form::File,
                "managed_config.toml",
            )
            .note("Administrator defaults applied over user config."),
        );
    }
    if platform == "windows" {
        rows.push(
            L::new(
                K::Settings,
                S::Managed,
                B::CodexHome,
                "managed_config.toml",
                Form::File,
                "managed_config.toml",
            )
            .note("Administrator defaults applied over user config."),
        );
    }
    if let Some(path) = per_os(
        platform,
        "/etc/codex/requirements.toml",
        "/etc/codex/requirements.toml",
        "",
    ) {
        rows.push(
            L::new(
                K::Settings,
                S::Managed,
                B::System,
                path,
                Form::File,
                "requirements.toml",
            )
            .note("Administrator limits that user and project config cannot loosen."),
        );
    }
    if let Some(path) = per_os(platform, "/etc/codex/skills", "/etc/codex/skills", "") {
        rows.push(L::new(
            K::Skills,
            S::Managed,
            B::System,
            path,
            Form::SkillRoot,
            "Admin skills",
        ));
    }
    rows
}

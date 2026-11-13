//! Windsurf (Cascade, now documented under Devin).
//!
//! Sources (checked November 2026):
//! - <https://docs.windsurf.com/windsurf/cascade/memories> (rules: `.devin/rules`,
//!   `.devin/global_rules.md`, `~/.devin/rules`, `~/.devin/global_rules.md`, legacy
//!   `.windsurf/rules`, `.windsurfrules`)
//! - <https://docs.windsurf.com/windsurf/cascade/workflows> (`.devin/workflows`,
//!   `.windsurf/workflows`, `~/.codeium/windsurf/global_workflows`)
//! - <https://docs.windsurf.com/windsurf/cascade/skills> (`.devin/skills`, `.agents/skills`,
//!   `.windsurf/skills`, `~/.config/devin/skills`, `~/.agents/skills`)
//! - <https://docs.windsurf.com/windsurf/cascade/mcp> (`~/.config/devin/mcp_config.json`,
//!   `.devin/mcp_config.json`, legacy `~/.codeium/windsurf/mcp_config.json`)
//! - <https://docs.windsurf.com/windsurf/cascade/hooks> (`.devin/hooks.v1.json`)
//!
//! Not added: auto-generated memories (`~/.codeium/windsurf/memories`) are not in the current
//! docs, so they are not listed.

use super::{Base as B, Form, LocationSpec as L, Reader as R};
use crate::agents::capabilities::{ConfigFormat, InstructionKind as I};
use crate::transparency::places::{PlaceKind as K, PlaceScope as S};

const LEGACY: &str = "Older Windsurf location, still read for compatibility.";

/// Windsurf rows.
pub(super) fn specs() -> Vec<L> {
    vec![
        L::new(
            K::McpConfig,
            S::User,
            B::XdgConfig,
            "devin/mcp_config.json",
            Form::File,
            "mcp_config.json",
        )
        .read(R::Mcp(ConfigFormat::JsonMcpServers)),
        L::new(
            K::McpConfig,
            S::User,
            B::Home,
            ".codeium/windsurf/mcp_config.json",
            Form::File,
            "mcp_config.json",
        )
        .read(R::Mcp(ConfigFormat::JsonMcpServers))
        .note(LEGACY),
        L::new(
            K::Rules,
            S::User,
            B::Home,
            ".devin/rules",
            Form::Named(&["md"]),
            "Global rules",
        ),
        L::new(
            K::Rules,
            S::User,
            B::Home,
            ".devin/global_rules.md",
            Form::File,
            "global_rules.md",
        ),
        L::new(
            K::Commands,
            S::User,
            B::Home,
            ".codeium/windsurf/global_workflows",
            Form::Named(&["md"]),
            "Global workflows",
        ),
        L::new(
            K::Skills,
            S::User,
            B::XdgConfig,
            "devin/skills",
            Form::SkillRoot,
            "Skills",
        ),
        L::new(
            K::Skills,
            S::User,
            B::Home,
            ".agents/skills",
            Form::SkillRoot,
            "Skills",
        ),
        L::new(
            K::McpConfig,
            S::Project,
            B::Project,
            ".devin/mcp_config.json",
            Form::File,
            "mcp_config.json",
        )
        .read(R::Mcp(ConfigFormat::JsonMcpServers)),
        L::new(
            K::McpConfig,
            S::Local,
            B::Project,
            ".devin/mcp_config.local.json",
            Form::File,
            "mcp_config.local.json",
        ),
        L::new(
            K::Rules,
            S::Project,
            B::Project,
            ".devin/rules",
            Form::Named(&["md"]),
            "Rules",
        ),
        L::new(
            K::Rules,
            S::Project,
            B::Project,
            ".windsurf/rules",
            Form::Named(&["md"]),
            "Rules",
        )
        .read(R::Instructions(I::Rule, Some("md")))
        .note(LEGACY),
        L::new(
            K::Rules,
            S::Project,
            B::Project,
            ".windsurfrules",
            Form::File,
            ".windsurfrules",
        )
        .note(LEGACY),
        L::new(
            K::Commands,
            S::Project,
            B::Project,
            ".devin/workflows",
            Form::Named(&["md"]),
            "Workflows",
        ),
        L::new(
            K::Commands,
            S::Project,
            B::Project,
            ".windsurf/workflows",
            Form::Named(&["md"]),
            "Workflows",
        )
        .note(LEGACY),
        L::new(
            K::Skills,
            S::Project,
            B::Project,
            ".devin/skills",
            Form::SkillRoot,
            "Skills",
        ),
        L::new(
            K::Skills,
            S::Project,
            B::Project,
            ".agents/skills",
            Form::SkillRoot,
            "Skills",
        ),
        L::new(
            K::Skills,
            S::Project,
            B::Project,
            ".windsurf/skills",
            Form::SkillRoot,
            "Skills",
        )
        .note(LEGACY),
        L::new(
            K::Hooks,
            S::Project,
            B::Project,
            ".devin/hooks.v1.json",
            Form::File,
            "hooks.v1.json",
        ),
    ]
}

//! Cline.
//!
//! Sources (checked November 2026):
//! - <https://docs.cline.bot/customization/cline-rules> (workspace `.clinerules/` file or folder,
//!   `.cline/rules/`; global `~/Documents/Cline/Rules` and `~/.cline/rules`; AGENTS.md)
//! - <https://docs.cline.bot/customization/skills> (`.cline/skills`)
//! - <https://docs.cline.bot/mcp/configuring-mcp-servers> (CLI home
//!   `~/.cline/data/settings/cline_mcp_settings.json`)
//!
//! Not added: the global workflows folder could not be confirmed in current docs.

use super::{Base as B, Form, LocationSpec as L, Reader as R};
use crate::agents::capabilities::{ConfigFormat, InstructionKind as I};
use crate::transparency::places::{PlaceKind as K, PlaceScope as S};

/// Cline rows.
pub(super) fn specs() -> Vec<L> {
    vec![
        L::new(
            K::McpConfig,
            S::User,
            B::Home,
            ".cline/data/settings/cline_mcp_settings.json",
            Form::File,
            "cline_mcp_settings.json",
        )
        .read(R::Mcp(ConfigFormat::JsonMcpServers)),
        L::new(
            K::Skills,
            S::User,
            B::Home,
            ".cline/skills",
            Form::SkillRoot,
            "Skills",
        )
        .read(R::Skills),
        L::new(
            K::Rules,
            S::User,
            B::Home,
            ".cline/rules",
            Form::Named(&["md"]),
            "Global rules",
        ),
        L::new(
            K::Rules,
            S::User,
            B::Home,
            "Documents/Cline/Rules",
            Form::Named(&["md"]),
            "Global rules",
        ),
        L::new(
            K::Skills,
            S::Project,
            B::Project,
            ".cline/skills",
            Form::SkillRoot,
            "Skills",
        )
        .read(R::Skills),
        L::new(
            K::Rules,
            S::Project,
            B::Project,
            ".clinerules",
            Form::Named(&["md"]),
            ".clinerules",
        )
        .read(R::Instructions(I::Rule, Some("md"))),
        L::new(
            K::Rules,
            S::Project,
            B::Project,
            ".cline/rules",
            Form::Named(&["md"]),
            "Rules",
        ),
        L::new(
            K::Instructions,
            S::Project,
            B::Project,
            "AGENTS.md",
            Form::File,
            "AGENTS.md",
        ),
    ]
}

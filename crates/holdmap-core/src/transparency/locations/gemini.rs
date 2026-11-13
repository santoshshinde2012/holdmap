//! Gemini CLI.
//!
//! Sources (checked November 2026):
//! - <https://geminicli.com/docs/reference/configuration/> (user, project, system settings and
//!   system defaults per OS; `GEMINI_CLI_SYSTEM_SETTINGS_PATH`; `.env` search; precedence:
//!   system settings override everything, system defaults sit below user settings)
//! - <https://geminicli.com/docs/cli/gemini-md/> (`~/.gemini/GEMINI.md`, project GEMINI.md)
//! - <https://geminicli.com/docs/cli/custom-commands/> (`.toml` commands, user and project)
//! - <https://geminicli.com/docs/cli/skills/> (`~/.gemini/skills`, `~/.agents/skills`,
//!   `.gemini/skills`, `.agents/skills`)
//! - <https://geminicli.com/docs/extensions/> (`~/.gemini/extensions/<name>/gemini-extension.json`)
//!
//! Not added: `~/.gemini/tmp` (chat checkpoints) is not documented as a stable location.

use super::{per_os, Base as B, Form, LocationSpec as L, Reader as R};
use crate::agents::capabilities::{ConfigFormat, InstructionKind as I};
use crate::transparency::places::{PlaceKind as K, PlaceScope as S};

/// Gemini CLI rows for `platform`.
pub(super) fn specs(platform: &str) -> Vec<L> {
    let mut rows = vec![
        L::new(
            K::McpConfig,
            S::User,
            B::Home,
            ".gemini/settings.json",
            Form::File,
            "settings.json",
        )
        .read(R::Mcp(ConfigFormat::JsonMcpServers)),
        L::new(
            K::Instructions,
            S::User,
            B::Home,
            ".gemini/GEMINI.md",
            Form::File,
            "GEMINI.md",
        )
        .read(R::Instructions(I::Memory, None)),
        L::new(
            K::Commands,
            S::User,
            B::Home,
            ".gemini/commands",
            Form::Named(&["toml"]),
            "Commands",
        )
        .read(R::Instructions(I::Command, Some("toml"))),
        L::new(
            K::Skills,
            S::User,
            B::Home,
            ".gemini/skills",
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
            K::Extensions,
            S::User,
            B::Home,
            ".gemini/extensions",
            Form::Folder,
            "Extensions",
        )
        .note("Each extension folder can add MCP servers, commands and a context file."),
        L::new(
            K::SecretStore,
            S::User,
            B::Home,
            ".gemini/.env",
            Form::SecretStore,
            ".env",
        ),
        L::new(
            K::McpConfig,
            S::Project,
            B::Project,
            ".gemini/settings.json",
            Form::File,
            "settings.json",
        )
        .read(R::Mcp(ConfigFormat::JsonMcpServers)),
        L::new(
            K::Instructions,
            S::Project,
            B::Project,
            "GEMINI.md",
            Form::File,
            "GEMINI.md",
        )
        .read(R::Instructions(I::Memory, None)),
        L::new(
            K::Commands,
            S::Project,
            B::Project,
            ".gemini/commands",
            Form::Named(&["toml"]),
            "Commands",
        )
        .read(R::Instructions(I::Command, Some("toml"))),
        L::new(
            K::Skills,
            S::Project,
            B::Project,
            ".gemini/skills",
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
            K::SecretStore,
            S::Project,
            B::Project,
            ".gemini/.env",
            Form::SecretStore,
            ".env",
        ),
    ];
    if let Some(path) = per_os(
        platform,
        "/Library/Application Support/GeminiCli/settings.json",
        "/etc/gemini-cli/settings.json",
        r"C:\ProgramData\gemini-cli\settings.json",
    ) {
        rows.push(
            L::new(
                K::Settings,
                S::Managed,
                B::System,
                path,
                Form::File,
                "System settings",
            )
            .note(
                "Overrides user and project settings. GEMINI_CLI_SYSTEM_SETTINGS_PATH can move it.",
            ),
        );
    }
    if let Some(path) = per_os(
        platform,
        "/Library/Application Support/GeminiCli/system-defaults.json",
        "/etc/gemini-cli/system-defaults.json",
        r"C:\ProgramData\gemini-cli\system-defaults.json",
    ) {
        rows.push(
            L::new(
                K::Settings,
                S::SystemDefault,
                B::System,
                path,
                Form::File,
                "System defaults",
            )
            .note("Base values that user and project settings override."),
        );
    }
    rows
}

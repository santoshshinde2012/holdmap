//! GitHub Copilot in VS Code and Copilot CLI.
//!
//! Sources (checked November 2026):
//! - <https://code.visualstudio.com/docs/copilot/customization/custom-instructions>
//!   (`.github/copilot-instructions.md`, `.github/instructions/*.instructions.md`, AGENTS.md)
//! - <https://code.visualstudio.com/docs/copilot/customization/prompt-files> (`.github/prompts`)
//! - <https://code.visualstudio.com/docs/copilot/customization/mcp-servers> (`.vscode/mcp.json`)
//! - <https://docs.github.com/en/copilot/how-tos/copilot-cli> and
//!   <https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-config-dir-reference>
//!   (`~/.copilot` or `COPILOT_HOME`: `mcp-config.json`, `settings.json`,
//!   `copilot-instructions.md`, `instructions/`, `skills/`, `agents/`, `hooks/`,
//!   `installed-plugins/`, `session-state/`, `command-history-state/`, `logs/`,
//!   `mcp-oauth-config/`, `mcp-secrets/`; repo `.github/hooks/`, `.mcp.json`, `.github/mcp.json`)
//!
//! Not added: the VS Code user-profile `mcp.json` path varies by profile and is not a single
//! documented location; `.github/agents` and `.github/skills` were not confirmed.

use super::{Base as B, Form, LocationSpec as L, Reader as R};
use crate::agents::capabilities::{ConfigFormat, InstructionKind as I};
use crate::transparency::places::{PlaceKind as K, PlaceScope as S};

fn repo_instructions() -> Vec<L> {
    vec![
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
            K::Rules,
            S::Project,
            B::Project,
            ".github/copilot-instructions.md",
            Form::File,
            "copilot-instructions.md",
        )
        .read(R::Instructions(I::Rule, None)),
        L::new(
            K::Rules,
            S::Project,
            B::Project,
            ".github/instructions",
            Form::Named(&["instructions.md"]),
            "Instructions",
        )
        .read(R::Instructions(I::Rule, Some("instructions.md"))),
        L::new(
            K::Commands,
            S::Project,
            B::Project,
            ".github/prompts",
            Form::Named(&["prompt.md"]),
            "Prompts",
        )
        .read(R::Instructions(I::Prompt, Some("prompt.md"))),
    ]
}

/// Copilot in VS Code.
pub(super) fn vscode() -> Vec<L> {
    let mut rows = vec![
        L::new(
            K::McpConfig,
            S::Project,
            B::Project,
            ".vscode/mcp.json",
            Form::File,
            "mcp.json",
        )
        .read(R::Mcp(ConfigFormat::JsonServers)),
        L::new(
            K::McpConfig,
            S::Project,
            B::Project,
            ".mcp.json",
            Form::File,
            ".mcp.json",
        )
        .read(R::Mcp(ConfigFormat::JsonCopilot)),
    ];
    rows.extend(repo_instructions());
    rows
}

/// Copilot CLI.
pub(super) fn cli() -> Vec<L> {
    let mut rows = vec![
        L::new(
            K::McpConfig,
            S::User,
            B::CopilotHome,
            "mcp-config.json",
            Form::File,
            "mcp-config.json",
        )
        .read(R::Mcp(ConfigFormat::JsonCopilot)),
        L::new(
            K::Settings,
            S::User,
            B::CopilotHome,
            "settings.json",
            Form::File,
            "settings.json",
        ),
        L::new(
            K::Instructions,
            S::User,
            B::CopilotHome,
            "copilot-instructions.md",
            Form::File,
            "copilot-instructions.md",
        ),
        L::new(
            K::Rules,
            S::User,
            B::CopilotHome,
            "instructions",
            Form::Named(&["instructions.md"]),
            "Instructions",
        ),
        L::new(
            K::Skills,
            S::User,
            B::CopilotHome,
            "skills",
            Form::SkillRoot,
            "Skills",
        ),
        L::new(
            K::Subagents,
            S::User,
            B::CopilotHome,
            "agents",
            Form::Named(&["md"]),
            "Custom agents",
        ),
        L::new(
            K::Hooks,
            S::User,
            B::CopilotHome,
            "hooks",
            Form::Folder,
            "Hooks",
        ),
        L::new(
            K::Extensions,
            S::User,
            B::CopilotHome,
            "installed-plugins",
            Form::Folder,
            "Plugins",
        ),
        L::new(
            K::SessionData,
            S::User,
            B::CopilotHome,
            "session-state",
            Form::SessionData,
            "Sessions",
        ),
        L::new(
            K::SessionData,
            S::User,
            B::CopilotHome,
            "command-history-state",
            Form::SessionData,
            "Prompt history",
        ),
        L::new(
            K::SessionData,
            S::User,
            B::CopilotHome,
            "logs",
            Form::SessionData,
            "Logs",
        ),
        L::new(
            K::SecretStore,
            S::User,
            B::CopilotHome,
            "mcp-oauth-config",
            Form::SecretStore,
            "MCP sign-ins",
        ),
        L::new(
            K::SecretStore,
            S::User,
            B::CopilotHome,
            "mcp-secrets",
            Form::SecretStore,
            "MCP secrets",
        ),
        L::new(
            K::McpConfig,
            S::Project,
            B::Project,
            ".mcp.json",
            Form::File,
            ".mcp.json",
        )
        .read(R::Mcp(ConfigFormat::JsonCopilot)),
        L::new(
            K::McpConfig,
            S::Project,
            B::Project,
            ".github/mcp.json",
            Form::File,
            "mcp.json",
        )
        .read(R::Mcp(ConfigFormat::JsonCopilot)),
        L::new(
            K::Hooks,
            S::Project,
            B::Project,
            ".github/hooks",
            Form::Folder,
            "Hooks",
        ),
    ];
    rows.extend(repo_instructions());
    rows
}

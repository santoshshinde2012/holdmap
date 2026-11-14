//! Smaller tables for agents that keep a few documented files: Kiro, opencode, Amp, Goose,
//! Aider, Qwen Code, Crush and Zed. Only rows in the vendor's own MCP-servers JSON format get a
//! reader; the rest are listed by the census so you can see where each agent looks.
//!
//! Sources (checked November 2026):
//! - Kiro: <https://kiro.dev/docs/steering/> (`.kiro/steering/*.md`, `~/.kiro/steering/`,
//!   AGENTS.md), <https://kiro.dev/docs/mcp/configuration/> (`.kiro/settings/mcp.json` and
//!   `~/.kiro/settings/mcp.json`, `mcpServers`, merged with the workspace winning).
//! - opencode: <https://opencode.ai/docs/config/> (global `~/.config/opencode/opencode.json(c)`
//!   with `agents/`, `commands/`, `skills/`, `plugins/`; project `opencode.json(c)` and
//!   `.opencode/`; managed folder per OS that overrides everything),
//!   <https://opencode.ai/docs/rules/> (AGENTS.md, CLAUDE.md as a fallback).
//! - Amp: <https://ampcode.com/docs/customize/agents-md>, <https://ampcode.com/docs/customize/skills>,
//!   <https://ampcode.com/docs/cli/settings> (`~/.config/amp/settings.json(c)` and
//!   `.amp/settings.json(c)` with `amp.mcpServers`; AGENTS.md layers; skills and plugins).
//! - Goose: <https://github.com/block/goose/blob/main/documentation/docs/guides/config-files.md>
//!   (`config.yaml`, `permission.yaml`, `secrets.yaml`; Windows `%APPDATA%\Block\goose\config`)
//!   and `crates/goose/src/hints/load_hints.rs` (`.goosehints` and AGENTS.md, global and
//!   project, joined).
//! - Aider: <https://aider.chat/docs/config/aider_conf.html> (`.aider.conf.yml` in home, git root
//!   and current folder, later wins), <https://aider.chat/docs/config/options.html>
//!   (`.aider.model.settings.yml`, `.env`, chat and input history files).
//! - Qwen Code: <https://qwenlm.github.io/qwen-code-docs/en/users/configuration/settings/>
//!   (user, project, system settings and system defaults per OS, `mcpServers`),
//!   <https://qwenlm.github.io/qwen-code-docs/en/users/configuration/memory/> (QWEN.md layers).
//! - Crush: <https://github.com/charmbracelet/crush> README (`.crush.json`, `crush.json`,
//!   `.crushrc`, `crushrc` in the project; `~/.config/crush/`; state in `~/.local/share/crush`).
//! - Zed: <https://zed.dev/docs/ai/instructions> and <https://zed.dev/docs/ai/skills> (global
//!   `~/.config/zed/AGENTS.md`, the first matching project rules file, `.agents/skills`),
//!   <https://zed.dev/docs/ai/mcp> (`context_servers` in `settings.json`).
//!
//! Not added: Aider's CONVENTIONS.md (loaded only when passed with `--read`), Crush's precedence
//! between its project files (not documented), and opencode's, Amp's and Crush's Windows folders
//! (not documented separately). Roo Code and Continue run inside the VS Code extension host
//! with no process of their own, so holdmap can't tell when they run.

use super::{per_os, Base as B, Form, LocationSpec as L, Reader as R};
use crate::agents::capabilities::{ConfigFormat, InstructionKind as I};
use crate::transparency::places::{PlaceKind as K, PlaceScope as S};

const MD: &[&str] = &["md"];

/// Rows for products without a table of their own.
pub(super) fn specs(product: &str, platform: &str) -> Vec<L> {
    match product {
        "kiro" => kiro(),
        "opencode" => opencode(platform),
        "amp" => amp(platform),
        "goose" => goose(platform),
        "aider" => aider(),
        "qwen" => qwen(platform),
        "crush" => crush(platform),
        "zed" => zed(platform),
        _ => Vec::new(),
    }
}

fn memory(scope: S, base: B, relative: &'static str, label: &'static str) -> L {
    L::new(K::Instructions, scope, base, relative, Form::File, label)
        .read(R::Instructions(I::Memory, None))
}

fn file(kind: K, scope: S, base: B, relative: &'static str, label: &'static str) -> L {
    L::new(kind, scope, base, relative, Form::File, label)
}

fn skills(scope: S, base: B, relative: &'static str) -> L {
    L::new(K::Skills, scope, base, relative, Form::SkillRoot, "Skills")
}

fn kiro() -> Vec<L> {
    let mcp = |scope, base| {
        file(
            K::McpConfig,
            scope,
            base,
            ".kiro/settings/mcp.json",
            "mcp.json",
        )
        .read(R::Mcp(ConfigFormat::JsonMcpServers))
    };
    let steering = |scope, base| {
        L::new(
            K::Rules,
            scope,
            base,
            ".kiro/steering",
            Form::Named(MD),
            "Steering",
        )
        .read(R::Instructions(I::Rule, Some("md")))
    };
    vec![
        steering(S::User, B::Home),
        mcp(S::User, B::Home),
        steering(S::Project, B::Project),
        mcp(S::Project, B::Project)
            .note("Merged with the user file; a server named in both uses the workspace one."),
        memory(S::Project, B::Project, "AGENTS.md", "AGENTS.md"),
        L::new(
            K::Subagents,
            S::Project,
            B::Project,
            ".kiro/agents",
            Form::Folder,
            "Agents",
        ),
    ]
}

fn opencode(platform: &str) -> Vec<L> {
    let mut rows = vec![
        file(
            K::Settings,
            S::User,
            B::XdgConfig,
            "opencode/opencode.json",
            "opencode.json",
        ),
        file(
            K::Settings,
            S::User,
            B::XdgConfig,
            "opencode/opencode.jsonc",
            "opencode.jsonc",
        ),
        memory(S::User, B::XdgConfig, "opencode/AGENTS.md", "AGENTS.md"),
        L::new(
            K::Subagents,
            S::User,
            B::XdgConfig,
            "opencode/agents",
            Form::Named(MD),
            "Agents",
        ),
        L::new(
            K::Commands,
            S::User,
            B::XdgConfig,
            "opencode/commands",
            Form::Named(MD),
            "Commands",
        )
        .read(R::Instructions(I::Command, Some("md"))),
        skills(S::User, B::XdgConfig, "opencode/skills"),
        L::new(
            K::Plugin,
            S::User,
            B::XdgConfig,
            "opencode/plugins",
            Form::Folder,
            "Plugins",
        ),
        file(
            K::Settings,
            S::Project,
            B::Project,
            "opencode.json",
            "opencode.json",
        ),
        file(
            K::Settings,
            S::Project,
            B::Project,
            "opencode.jsonc",
            "opencode.jsonc",
        ),
        file(
            K::Settings,
            S::Project,
            B::Project,
            ".opencode/opencode.json",
            "opencode.json",
        ),
        file(
            K::Settings,
            S::Project,
            B::Project,
            ".opencode/opencode.jsonc",
            "opencode.jsonc",
        ),
        skills(S::Project, B::Project, ".opencode/skills"),
        memory(S::Project, B::Project, "AGENTS.md", "AGENTS.md"),
        file(
            K::Instructions,
            S::Project,
            B::Project,
            "CLAUDE.md",
            "CLAUDE.md",
        )
        .note("Read only when the project has no AGENTS.md."),
    ];
    if let Some(path) = per_os(
        platform,
        "/Library/Application Support/opencode",
        "/etc/opencode",
        r"C:\ProgramData\opencode",
    ) {
        rows.push(
            L::new(
                K::Settings,
                S::Managed,
                B::System,
                path,
                Form::Folder,
                "Managed config",
            )
            .note("Set by an administrator; overrides every other layer."),
        );
    }
    rows
}

fn amp(platform: &str) -> Vec<L> {
    let settings = |scope, base, relative, label| {
        file(K::Settings, scope, base, relative, label)
            .note("MCP servers are listed under amp.mcpServers.")
    };
    let mut rows = vec![
        settings(S::User, B::XdgConfig, "amp/settings.json", "settings.json"),
        settings(
            S::User,
            B::XdgConfig,
            "amp/settings.jsonc",
            "settings.jsonc",
        ),
        memory(S::User, B::XdgConfig, "amp/AGENTS.md", "AGENTS.md"),
        memory(S::User, B::XdgConfig, "AGENTS.md", "AGENTS.md"),
        skills(S::User, B::XdgConfig, "agents/skills"),
        skills(S::User, B::Home, ".agents/skills"),
        skills(S::User, B::XdgConfig, "amp/skills"),
        L::new(
            K::Plugin,
            S::User,
            B::XdgConfig,
            "amp/plugins",
            Form::Folder,
            "Plugins",
        ),
        settings(
            S::Project,
            B::Project,
            ".amp/settings.json",
            "settings.json",
        ),
        settings(
            S::Project,
            B::Project,
            ".amp/settings.jsonc",
            "settings.jsonc",
        ),
        memory(S::Project, B::Project, "AGENTS.md", "AGENTS.md"),
        skills(S::Project, B::Project, ".agents/skills"),
        L::new(
            K::Plugin,
            S::Project,
            B::Project,
            ".amp/plugins",
            Form::Folder,
            "Plugins",
        ),
    ];
    if let Some(path) = per_os(
        platform,
        "/Library/Application Support/ampcode/AGENTS.md",
        "/etc/ampcode/AGENTS.md",
        r"C:\ProgramData\ampcode\AGENTS.md",
    ) {
        rows.push(memory(S::Managed, B::System, path, "System AGENTS.md"));
    }
    rows
}

fn goose(platform: &str) -> Vec<L> {
    // Goose keeps its config folder in %APPDATA% on Windows and the XDG folder elsewhere.
    let (base, [config, permission, secrets, hints, agents]) = if platform == "windows" {
        (
            B::AppData,
            [
                "Block/goose/config/config.yaml",
                "Block/goose/config/permission.yaml",
                "Block/goose/config/secrets.yaml",
                "Block/goose/config/.goosehints",
                "Block/goose/config/AGENTS.md",
            ],
        )
    } else {
        (
            B::XdgConfig,
            [
                "goose/config.yaml",
                "goose/permission.yaml",
                "goose/secrets.yaml",
                "goose/.goosehints",
                "goose/AGENTS.md",
            ],
        )
    };
    vec![
        file(K::Settings, S::User, base, config, "config.yaml")
            .note("Extensions, which include MCP servers, are listed here."),
        file(K::Settings, S::User, base, permission, "permission.yaml"),
        L::new(
            K::SecretStore,
            S::User,
            base,
            secrets,
            Form::SecretStore,
            "secrets.yaml",
        ),
        memory(S::User, base, hints, ".goosehints"),
        memory(S::User, base, agents, "AGENTS.md"),
        memory(S::Project, B::Project, ".goosehints", ".goosehints"),
        memory(S::Project, B::Project, "AGENTS.md", "AGENTS.md"),
    ]
}

fn aider() -> Vec<L> {
    vec![
        file(
            K::Settings,
            S::User,
            B::Home,
            ".aider.conf.yml",
            ".aider.conf.yml",
        ),
        file(
            K::Settings,
            S::User,
            B::Home,
            ".aider.model.settings.yml",
            "Model settings",
        ),
        file(
            K::Settings,
            S::Project,
            B::Project,
            ".aider.conf.yml",
            ".aider.conf.yml",
        )
        .note("Read from home, the git root and the current folder; the later file wins."),
        file(
            K::Settings,
            S::Project,
            B::Project,
            ".aider.model.settings.yml",
            "Model settings",
        ),
        L::new(
            K::SecretStore,
            S::Project,
            B::Project,
            ".env",
            Form::SecretStore,
            ".env",
        ),
        L::new(
            K::SessionData,
            S::Project,
            B::Project,
            ".aider.chat.history.md",
            Form::SessionData,
            "Chat history",
        ),
        L::new(
            K::SessionData,
            S::Project,
            B::Project,
            ".aider.input.history",
            Form::SessionData,
            "Input history",
        ),
    ]
}

fn qwen(platform: &str) -> Vec<L> {
    let mcp = |scope, base| {
        file(
            K::McpConfig,
            scope,
            base,
            ".qwen/settings.json",
            "settings.json",
        )
        .read(R::Mcp(ConfigFormat::JsonMcpServers))
    };
    let mut rows = vec![
        mcp(S::User, B::Home),
        memory(S::User, B::Home, ".qwen/QWEN.md", "QWEN.md"),
        mcp(S::Project, B::Project),
        memory(S::Project, B::Project, "QWEN.md", "QWEN.md"),
        memory(S::Local, B::Project, ".qwen/QWEN.local.md", "QWEN.local.md"),
        memory(S::Project, B::Project, "AGENTS.md", "AGENTS.md"),
        skills(S::Project, B::Project, ".qwen/skills"),
    ];
    if let Some(path) = per_os(
        platform,
        "/Library/Application Support/QwenCode/settings.json",
        "/etc/qwen-code/settings.json",
        r"C:\ProgramData\qwen-code\settings.json",
    ) {
        rows.push(
            file(K::Settings, S::Managed, B::System, path, "System settings")
                .note("Overrides user and project settings."),
        );
    }
    if let Some(path) = per_os(
        platform,
        "/Library/Application Support/QwenCode/system-defaults.json",
        "/etc/qwen-code/system-defaults.json",
        r"C:\ProgramData\qwen-code\system-defaults.json",
    ) {
        rows.push(
            file(
                K::Settings,
                S::SystemDefault,
                B::System,
                path,
                "System defaults",
            )
            .note("Base values that user and project settings override."),
        );
    }
    rows
}

fn crush(platform: &str) -> Vec<L> {
    let mut rows = vec![
        file(
            K::Settings,
            S::User,
            B::XdgConfig,
            "crush/crushrc",
            "crushrc",
        ),
        file(
            K::Settings,
            S::User,
            B::XdgConfig,
            "crush/crush.json",
            "crush.json",
        ),
        file(K::Settings, S::Project, B::Project, ".crushrc", ".crushrc"),
        file(K::Settings, S::Project, B::Project, "crushrc", "crushrc"),
        file(
            K::Settings,
            S::Project,
            B::Project,
            ".crush.json",
            ".crush.json",
        ),
        file(
            K::Settings,
            S::Project,
            B::Project,
            "crush.json",
            "crush.json",
        ),
    ];
    if platform != "windows" {
        rows.push(L::new(
            K::SessionData,
            S::User,
            B::Home,
            ".local/share/crush",
            Form::SessionData,
            "App data",
        ));
    }
    rows
}

fn zed(platform: &str) -> Vec<L> {
    let (base, agents, settings) = if platform == "windows" {
        (B::AppData, "Zed/AGENTS.md", "Zed/settings.json")
    } else {
        (B::XdgConfig, "zed/AGENTS.md", "zed/settings.json")
    };
    let mut rows = vec![
        memory(S::User, base, agents, "AGENTS.md"),
        file(K::Settings, S::User, base, settings, "settings.json")
            .note("MCP servers are listed under context_servers."),
        skills(S::User, B::Home, ".agents/skills"),
        skills(S::Project, B::Project, ".agents/skills"),
    ];
    for name in [
        ".rules",
        ".cursorrules",
        ".windsurfrules",
        ".clinerules",
        ".github/copilot-instructions.md",
        "AGENT.md",
        "AGENTS.md",
        "CLAUDE.md",
        "GEMINI.md",
    ] {
        rows.push(
            file(K::Rules, S::Project, B::Project, name, name)
                .note("Zed uses only the first of its project rules files that it finds."),
        );
    }
    rows
}

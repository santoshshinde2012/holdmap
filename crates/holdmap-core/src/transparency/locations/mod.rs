//! One registry of documented agent locations, shared by the location census and the opt-in
//! configuration reader.
//!
//! Each product has a small table of [`LocationSpec`] rows, one file per vendor. A row says
//! what a path is for, which layer it belongs to, how layers of that kind combine, how the
//! census may look at it, and whether the configuration reader parses it. Every row cites the
//! vendor page it came from in a comment beside the table. Paths that could not be confirmed
//! in vendor documentation are left out on purpose; the per-product notes say so.

mod claude;
mod cline;
mod codex;
mod copilot;
mod cursor;
mod gemini;
mod more;
mod windsurf;

use super::places::{PlaceKind, PlaceScope};
use crate::agents::capabilities::{ConfigFormat, InstructionKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where a row's relative path starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// The account home (`~`).
    Home,
    /// Claude Code's config home: `~/.claude`, or `CLAUDE_CONFIG_DIR`.
    ClaudeHome,
    /// The folder that holds `.claude.json`: `~`, or `CLAUDE_CONFIG_DIR`.
    ClaudeJsonDir,
    /// Codex's home: `~/.codex`, or `CODEX_HOME`.
    CodexHome,
    /// Copilot CLI's home: `~/.copilot`, or `COPILOT_HOME`.
    CopilotHome,
    /// The XDG config folder: `~/.config`, or `XDG_CONFIG_HOME`.
    XdgConfig,
    /// macOS `~/Library/Application Support` or Windows `%APPDATA%`.
    AppData,
    /// An observed project or working folder.
    Project,
    /// An absolute, machine-wide path. `relative` holds the whole path for one OS.
    System,
}

/// How the census may look at a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// A single file (or a directory whose children are counted one level deep).
    File,
    /// A directory of `<name>/SKILL.md` folders.
    SkillRoot,
    /// A directory of files with these suffixes, listed one level deep.
    Named(&'static [&'static str]),
    /// A directory of files with these suffixes, walked a few levels deep (rules folders nest).
    /// Files with an `ignored` suffix are counted separately because the product skips them.
    Deep {
        /// Suffixes the product loads.
        loads: &'static [&'static str],
        /// Suffixes the product documents as ignored in this folder.
        ignored: &'static [&'static str],
    },
    /// A directory whose children are counted, without candidate names.
    Folder,
    /// Claude auto memory: a `MEMORY.md` index whose lines are counted against the load limit.
    AutoMemory,
    /// Transcripts, history and logs. Only total size, file count and newest time; never names.
    SessionData,
    /// Credentials and secret stores. Presence only; never listed, sized or opened.
    SecretStore,
    /// Kept in the vendor's account or app settings, not on disk. Nothing is stat'ed.
    Account,
}

/// Which configuration-reader pass parses a row, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    /// The census stats it; nothing parses it.
    None,
    /// MCP server aliases are projected from it.
    Mcp(ConfigFormat),
    /// Skill folders and their bounded frontmatter.
    Skills,
    /// Permission rules and allowlisted settings.
    Policy,
    /// Instruction file names and frontmatter descriptions (`None` extension = one file).
    Instructions(InstructionKind, Option<&'static str>),
}

/// How files of one kind in several layers add up, in plain words for every surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Combine {
    /// One value wins per key; lists inside usually merge.
    Override,
    /// Entries from every layer are combined; the same name in a closer layer wins.
    Merge,
    /// Every file is loaded and joined, the broadest first.
    Join,
    /// Kept by the agent for itself; not configuration that layers combine.
    NotConfig,
}

impl Combine {
    /// Short phrase for text surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Override => "override",
            Self::Merge => "merge",
            Self::Join => "join",
            Self::NotConfig => "not configuration",
        }
    }

    /// The documented behaviour for a kind of location.
    pub fn for_kind(kind: PlaceKind) -> Self {
        match kind {
            PlaceKind::Settings => Self::Override,
            PlaceKind::McpConfig
            | PlaceKind::Skills
            | PlaceKind::Commands
            | PlaceKind::Subagents
            | PlaceKind::Extensions
            | PlaceKind::Builtin
            | PlaceKind::Plugin => Self::Merge,
            PlaceKind::Instructions
            | PlaceKind::Rules
            | PlaceKind::Hooks
            | PlaceKind::AgentMemory => Self::Join,
            PlaceKind::SessionData | PlaceKind::SecretStore => Self::NotConfig,
        }
    }
}

/// One documented location for one product.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocationSpec {
    /// What it is for.
    pub kind: PlaceKind,
    /// Which layer it belongs to.
    pub scope: PlaceScope,
    /// Where `relative` starts.
    pub base: Base,
    /// Slash-separated path under `base` (an absolute path for [`Base::System`]).
    /// `{project}` is replaced by the project folder name Claude Code derives.
    pub relative: &'static str,
    /// How the census may look at it.
    pub form: Form,
    /// Which reader pass parses it.
    pub reader: Reader,
    /// Short human name ("CLAUDE.md", "Auto memory", "Transcripts").
    pub label: &'static str,
    /// One sentence of vendor-documented context, when the path alone could mislead.
    pub note: Option<&'static str>,
}

impl LocationSpec {
    /// A row with no reader and no note.
    pub const fn new(
        kind: PlaceKind,
        scope: PlaceScope,
        base: Base,
        relative: &'static str,
        form: Form,
        label: &'static str,
    ) -> Self {
        Self {
            kind,
            scope,
            base,
            relative,
            form,
            reader: Reader::None,
            label,
            note: None,
        }
    }

    /// The same row parsed by `reader`.
    pub const fn read(mut self, reader: Reader) -> Self {
        self.reader = reader;
        self
    }

    /// The same row with a documented caveat.
    pub const fn note(mut self, note: &'static str) -> Self {
        self.note = Some(note);
        self
    }

    /// Whether the row is under an observed project (expanded once per project).
    pub fn per_project(&self) -> bool {
        self.base == Base::Project || self.relative.contains("{project}")
    }
}

/// Folders that rows resolve against. Built from holdmap's own account, never from an agent
/// process's environment (which can hold secrets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Homes {
    /// The account home.
    pub home: PathBuf,
    /// `~/.claude` or `CLAUDE_CONFIG_DIR`.
    pub claude: PathBuf,
    /// `~` or `CLAUDE_CONFIG_DIR`, where `.claude.json` lives.
    pub claude_json_dir: PathBuf,
    /// `~/.codex` or `CODEX_HOME`.
    pub codex: PathBuf,
    /// `~/.copilot` or `COPILOT_HOME`.
    pub copilot: PathBuf,
    /// `~/.config` or `XDG_CONFIG_HOME`.
    pub xdg_config: PathBuf,
    /// macOS Application Support or Windows `%APPDATA%`; `None` elsewhere.
    pub app_data: Option<PathBuf>,
    /// Environment variables that moved a home, by name. Values are paths, already applied.
    pub overrides: Vec<String>,
}

impl Homes {
    /// The documented default folders under `home`.
    pub fn defaults(home: &Path, app_data: Option<PathBuf>) -> Self {
        Self {
            home: home.to_path_buf(),
            claude: home.join(".claude"),
            claude_json_dir: home.to_path_buf(),
            codex: home.join(".codex"),
            copilot: home.join(".copilot"),
            xdg_config: home.join(".config"),
            app_data,
            overrides: Vec::new(),
        }
    }

    /// Defaults, then any documented home override set for holdmap's own account.
    ///
    /// `lookup` reads one variable. Only absolute, control-free paths are accepted.
    pub fn with_overrides(
        home: &Path,
        app_data: Option<PathBuf>,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Self {
        let mut homes = Self::defaults(home, app_data);
        let take = |name: &str| {
            lookup(name)
                .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        if let Some(path) = take("CLAUDE_CONFIG_DIR") {
            homes.claude_json_dir = path.clone();
            homes.claude = path;
            homes.overrides.push("CLAUDE_CONFIG_DIR".into());
        }
        if let Some(path) = take("CODEX_HOME") {
            homes.codex = path;
            homes.overrides.push("CODEX_HOME".into());
        }
        if let Some(path) = take("COPILOT_HOME") {
            homes.copilot = path;
            homes.overrides.push("COPILOT_HOME".into());
        }
        if let Some(path) = take("XDG_CONFIG_HOME") {
            homes.xdg_config = path;
            homes.overrides.push("XDG_CONFIG_HOME".into());
        }
        homes
    }

    /// Which override moved this base, for a label such as "custom home from CODEX_HOME".
    pub fn override_for(&self, base: Base) -> Option<&'static str> {
        let name = match base {
            Base::ClaudeHome | Base::ClaudeJsonDir => "CLAUDE_CONFIG_DIR",
            Base::CodexHome => "CODEX_HOME",
            Base::CopilotHome => "COPILOT_HOME",
            Base::XdgConfig => "XDG_CONFIG_HOME",
            _ => return None,
        };
        self.overrides.iter().any(|set| set == name).then_some(name)
    }

    /// The folder a non-project, non-system base starts at.
    pub fn root(&self, base: Base) -> Option<&Path> {
        match base {
            Base::Home => Some(&self.home),
            Base::ClaudeHome => Some(&self.claude),
            Base::ClaudeJsonDir => Some(&self.claude_json_dir),
            Base::CodexHome => Some(&self.codex),
            Base::CopilotHome => Some(&self.copilot),
            Base::XdgConfig => Some(&self.xdg_config),
            Base::AppData => self.app_data.as_deref(),
            Base::Project | Base::System => None,
        }
    }
}

/// The folder name Claude Code uses under `~/.claude/projects/` for a project path: every
/// character other than an ASCII letter or digit becomes `-`.
pub fn claude_project_dir(project: &Path) -> String {
    project
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Resolve one row to an absolute path. `None` when the base is not available here.
pub fn resolve(spec: &LocationSpec, homes: &Homes, project: Option<&Path>) -> Option<PathBuf> {
    let start = match spec.base {
        Base::System => return Some(PathBuf::from(spec.relative)),
        Base::Project => project?.to_path_buf(),
        other => homes.root(other)?.to_path_buf(),
    };
    let mut path = start;
    for part in spec.relative.split('/').filter(|part| !part.is_empty()) {
        if part == "{project}" {
            path.push(claude_project_dir(project?));
        } else {
            path.push(part);
        }
    }
    Some(path)
}

/// Every documented location for `product` on `platform` (`macos`, `linux`, `windows`).
pub fn specs(product: &str, platform: &str) -> Vec<LocationSpec> {
    match product {
        "claude-code" => claude::code(platform),
        "claude-desktop" => claude::desktop(),
        "codex" | "codex-desktop" => codex::specs(platform),
        "cursor" | "cursor-agent" => cursor::specs(platform),
        "gemini" => gemini::specs(platform),
        "copilot" => copilot::vscode(),
        "copilot-cli" => copilot::cli(),
        "windsurf" => windsurf::specs(),
        "cline" => cline::specs(),
        other => more::specs(other, platform),
    }
}

/// Rows for this product that a reader pass parses, in table order.
pub fn readable(product: &str, platform: &str) -> Vec<LocationSpec> {
    specs(product, platform)
        .into_iter()
        .filter(|spec| spec.reader != Reader::None)
        .collect()
}

/// Pick the managed or system path for this OS from a `(macos, linux, windows)` triple.
pub(super) fn per_os(
    platform: &str,
    macos: &'static str,
    linux: &'static str,
    windows: &'static str,
) -> Option<&'static str> {
    match platform {
        "macos" => Some(macos),
        "linux" => Some(linux),
        "windows" => Some(windows),
        _ => None,
    }
    .filter(|path| !path.is_empty())
}

#[cfg(test)]
mod tests;

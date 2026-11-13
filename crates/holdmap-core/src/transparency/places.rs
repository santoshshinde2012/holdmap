//! Where an agent looks for skills, context, rules, MCP configuration and built-in tools.
//!
//! The census stats documented paths. It does not read file bodies, follow symbolic links,
//! or open conversation stores. A path that exists is a candidate the product documents,
//! not proof the running agent loaded it. Rank is the documented layer (lower is more
//! specific). holdmap does not decide which layer won.

use crate::agents::access::privacy_area;
use crate::agents::capabilities::builtin_tools;
use serde::{Deserialize, Serialize};
use std::fs::{self, Metadata};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

const MAX_ENTRIES: usize = 256;
const MAX_CANDIDATES: usize = 64;

/// What a documented location is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceKind {
    /// A directory of `SKILL.md` folders.
    Skills,
    /// A context file such as `CLAUDE.md`, `AGENTS.md` or `GEMINI.md`.
    AgentMemory,
    /// Rules or settings files. Bodies are not read.
    Rules,
    /// An MCP client configuration file. Values are not read.
    McpConfig,
    /// Commands, prompts or subagent files.
    Instructions,
    /// Tools compiled into the product. There is no filesystem path.
    Builtin,
    /// Plugin, marketplace or cloud installs, which this census does not walk.
    Plugin,
}

impl PlaceKind {
    /// Short label shared by text surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Skills => "skills",
            Self::AgentMemory => "agent memory",
            Self::Rules => "rules",
            Self::McpConfig => "MCP config",
            Self::Instructions => "instructions",
            Self::Builtin => "built-in",
            Self::Plugin => "plugin",
        }
    }
}

/// Which documented layer a location belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceScope {
    /// Under the account home.
    User,
    /// Under an observed project or working folder.
    Project,
    /// A local override beside the project settings.
    Local,
    /// A plugin or marketplace install.
    Plugin,
    /// Shipped inside the agent.
    Builtin,
}

impl PlaceScope {
    /// Short label shared by text surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Local => "local",
            Self::Plugin => "plugin",
            Self::Builtin => "built-in",
        }
    }
}

/// Whether the path could be statted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceStatus {
    /// The path exists and is not a symbolic link, or it is a link whose target was recorded
    /// without being followed.
    Present,
    /// The path is not there.
    Missing,
    /// The path could not be statted.
    Unreadable,
    /// The path was refused (privacy area, a link in a parent, or a non-absolute path).
    Skipped,
}

/// One child of a directory, metadata only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceEntry {
    /// File or directory name.
    pub name: String,
    /// `file`, `directory` or `symlink`.
    pub kind: String,
    /// Length of a regular file. Absent for directories, links and unreadable entries.
    pub bytes: Option<u64>,
    /// Modification time, Unix milliseconds, when the metadata included one.
    pub modified_unix_ms: Option<u64>,
}

/// One documented location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityPlace {
    /// Stable display id. Not an authorization handle.
    pub id: String,
    /// What the location is for.
    pub kind: PlaceKind,
    /// Documented layer.
    pub scope: PlaceScope,
    /// Lower rank is the more specific documented layer. Not a resolved winner.
    pub rank: u16,
    /// Absolute path holdmap considered. Empty for built-ins and the plugin note.
    pub path: String,
    /// `readlink` of `path` when the final component is a symbolic link. The target is not opened.
    pub symlink_target: Option<String>,
    /// Result of the stat.
    pub status: PlaceStatus,
    /// Why the status is not a full listing, when that needs explaining.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    /// Files and directories counted. Absent when the count would be incomplete.
    pub file_count: Option<u64>,
    /// Sum of regular-file lengths, one directory level, or the file's own length.
    pub byte_size: Option<u64>,
    /// Modification time of the path itself, Unix milliseconds.
    pub modified_unix_ms: Option<u64>,
    /// Names of skill folders or instruction files found here. Not their contents.
    pub candidates: Vec<String>,
    /// Child metadata, capped. Empty when the path was not listed.
    pub entries: Vec<PlaceEntry>,
    /// True when the path exists (or, for built-ins, the names are the documented tool list).
    /// Presence is not proof the agent loaded the file.
    pub present: bool,
}

#[derive(Clone, Copy)]
enum Form {
    File,
    SkillRoot,
    /// File-name suffixes, without the leading dot (`md`, `mdc`, `instructions.md`).
    NamedFiles(&'static [&'static str]),
}

struct Spec {
    kind: PlaceKind,
    scope: PlaceScope,
    rank: u16,
    relative: &'static str,
    form: Form,
}

/// Stat every documented location for `product`.
pub fn census(
    product: &str,
    home: &Path,
    projects: &[PathBuf],
    platform: &str,
    desktop_config: Option<&Path>,
) -> Vec<CapabilityPlace> {
    let mut places = Vec::new();
    if product == "claude-desktop" {
        let spec = spec(
            PlaceKind::McpConfig,
            PlaceScope::User,
            30,
            "Claude/claude_desktop_config.json",
            Form::File,
        );
        places.push(match desktop_config {
            Some(base) => stat_place(product, base, spec, platform),
            None => {
                let mut place = stat_place(product, home, spec, platform);
                place.status = PlaceStatus::Skipped;
                place.present = false;
                place.status_reason = Some(
                    "Claude Desktop's configuration directory is only documented for macOS and Windows.".into(),
                );
                place
            }
        });
    }
    for spec in user_specs(product) {
        places.push(stat_place(product, home, spec, platform));
    }
    for project in projects {
        for spec in project_specs(product) {
            places.push(stat_place(product, project, spec, platform));
        }
    }
    places.push(plugin_place(product));
    places.push(builtin_place(product));
    places
}

fn plugin_place(product: &str) -> CapabilityPlace {
    CapabilityPlace {
        id: format!("{product}:plugin"),
        kind: PlaceKind::Plugin,
        scope: PlaceScope::Plugin,
        rank: 40,
        path: String::new(),
        symlink_target: None,
        status: PlaceStatus::Skipped,
        status_reason: Some(
            "Plugin, marketplace and cloud skill installs are not a single documented directory this census stats. Account databases are not opened.".into(),
        ),
        file_count: None,
        byte_size: None,
        modified_unix_ms: None,
        candidates: Vec::new(),
        entries: Vec::new(),
        present: false,
    }
}

fn builtin_place(product: &str) -> CapabilityPlace {
    let names: Vec<String> = builtin_tools(product)
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let present = !names.is_empty();
    CapabilityPlace {
        id: format!("{product}:builtin"),
        kind: PlaceKind::Builtin,
        scope: PlaceScope::Builtin,
        rank: 50,
        path: String::new(),
        symlink_target: None,
        status: if present {
            PlaceStatus::Present
        } else {
            PlaceStatus::Skipped
        },
        status_reason: Some(
            "Built-in tools ship inside the agent. holdmap has no filesystem path for them and does not claim they are enabled.".into(),
        ),
        file_count: None,
        byte_size: None,
        modified_unix_ms: None,
        candidates: names,
        entries: Vec::new(),
        present,
    }
}

fn stat_place(product: &str, base: &Path, spec: Spec, platform: &str) -> CapabilityPlace {
    let path = base.join(spec.relative);
    let id = format!(
        "{product}:{}:{}:{}",
        spec.kind.label(),
        spec.scope.label(),
        spec.relative
    );
    let mut place = CapabilityPlace {
        id,
        kind: spec.kind,
        scope: spec.scope,
        rank: spec.rank,
        path: path.display().to_string(),
        symlink_target: None,
        status: PlaceStatus::Missing,
        status_reason: None,
        file_count: None,
        byte_size: None,
        modified_unix_ms: None,
        candidates: Vec::new(),
        entries: Vec::new(),
        present: false,
    };
    if let Some(reason) = refuse(&path, base, platform) {
        place.status = PlaceStatus::Skipped;
        place.status_reason = Some(reason);
        return place;
    }
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => place,
        Err(error) => {
            place.status = PlaceStatus::Unreadable;
            place.status_reason = Some(format!("Could not stat the path ({})", io_reason(&error)));
            place
        }
        Ok(metadata) if is_link(&metadata) => {
            place.present = true;
            place.status = PlaceStatus::Present;
            place.symlink_target = fs::read_link(&path)
                .ok()
                .map(|target| target.display().to_string());
            place.status_reason = Some(
                "The path is a symbolic link. holdmap records the link target and does not follow it, so size and children are unknown.".into(),
            );
            place
        }
        Ok(metadata) => fill_present(&mut place, &path, &metadata, spec.form),
    }
}

fn fill_present(
    place: &mut CapabilityPlace,
    path: &Path,
    metadata: &Metadata,
    form: Form,
) -> CapabilityPlace {
    place.present = true;
    place.status = PlaceStatus::Present;
    place.modified_unix_ms = modified(metadata);
    if metadata.is_file() {
        place.file_count = Some(1);
        place.byte_size = Some(metadata.len());
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            place.candidates.push(name.to_owned());
        }
        return place.clone();
    }
    if !metadata.is_dir() {
        place.status_reason = Some(
            "The path is not a regular file or directory, so its children were not listed.".into(),
        );
        return place.clone();
    }
    let Ok(read) = fs::read_dir(path) else {
        place.status = PlaceStatus::Unreadable;
        place.present = false;
        place.status_reason = Some("The directory could not be listed.".into());
        return place.clone();
    };
    let mut children: Vec<_> = read.take(MAX_ENTRIES + 1).collect();
    if children.len() > MAX_ENTRIES {
        place.status_reason = Some(format!(
            "The directory has more than {MAX_ENTRIES} entries. The count would be incomplete, so it is not reported."
        ));
        return place.clone();
    }
    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut sized = true;
    children.sort_by_key(|entry| {
        entry
            .as_ref()
            .ok()
            .map(|entry| entry.file_name())
            .unwrap_or_default()
    });
    for entry in children {
        let Ok(entry) = entry else {
            sized = false;
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !crate::agents::capabilities::safe_name(&name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            sized = false;
            place.entries.push(PlaceEntry {
                name,
                kind: "unreadable".into(),
                bytes: None,
                modified_unix_ms: None,
            });
            continue;
        };
        let kind = if is_link(&meta) {
            "symlink"
        } else if meta.is_dir() {
            "directory"
        } else if meta.is_file() {
            "file"
        } else {
            "other"
        };
        let file_bytes = (kind == "file").then_some(meta.len());
        if kind == "file" {
            files += 1;
            bytes = bytes.saturating_add(meta.len());
        } else if kind == "directory" {
            files += 1;
            // Skill folders keep their text one level down. Stat that file only; do not read it.
            if matches!(form, Form::SkillRoot) {
                if let Some(len) = regular_file_len(&entry.path().join("SKILL.md")) {
                    bytes = bytes.saturating_add(len);
                }
            }
        }
        if place.entries.len() < MAX_CANDIDATES {
            place.entries.push(PlaceEntry {
                name: name.clone(),
                kind: kind.into(),
                bytes: file_bytes,
                modified_unix_ms: modified(&meta),
            });
        }
        if place.candidates.len() < MAX_CANDIDATES && is_candidate(&entry.path(), kind, form) {
            place.candidates.push(candidate_name(&name, form));
        }
    }
    if sized {
        place.file_count = Some(files);
        place.byte_size = Some(bytes);
    } else {
        place.status_reason = Some(
            "Some directory entries could not be statted, so the count and byte size are unknown."
                .into(),
        );
    }
    place.clone()
}

fn is_candidate(path: &Path, kind: &str, form: Form) -> bool {
    match form {
        Form::File => kind == "file",
        Form::SkillRoot => {
            kind == "directory" && regular_file_len(&path.join("SKILL.md")).is_some()
        }
        Form::NamedFiles(suffixes) => {
            kind == "file"
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| suffixes.iter().any(|suffix| name_has_suffix(name, suffix)))
        }
    }
}

fn name_has_suffix(name: &str, suffix: &str) -> bool {
    name.ends_with(&format!(".{suffix}"))
}

fn candidate_name(name: &str, form: Form) -> String {
    match form {
        Form::NamedFiles(suffixes) => suffixes
            .iter()
            .find_map(|suffix| name.strip_suffix(&format!(".{suffix}")))
            .unwrap_or(name)
            .to_owned(),
        _ => name.to_owned(),
    }
}

fn regular_file_len(path: &Path) -> Option<u64> {
    let metadata = fs::symlink_metadata(path).ok()?;
    (!is_link(&metadata) && metadata.is_file()).then_some(metadata.len())
}

fn refuse(path: &Path, home: &Path, platform: &str) -> Option<String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Some("Relative and traversing paths are not inspected.".into());
    }
    if path
        .to_str()
        .is_none_or(|text| text.len() > 4096 || text.chars().any(char::is_control))
    {
        return Some("Non-Unicode, control-bearing and oversized paths are not inspected.".into());
    }
    if let Some(area) = privacy_area(path, Some(home), platform) {
        return Some(format!("Not inspected: inside {area}, which macOS guards."));
    }
    for ancestor in path.ancestors().skip(1) {
        if ancestor.parent().is_none() {
            break;
        }
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if is_link(&metadata) => {
                return Some(
                    "A parent of this path is a symbolic link. holdmap does not follow it.".into(),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(_) => return Some("A parent of this path could not be statted.".into()),
        }
    }
    None
}

fn modified(metadata: &Metadata) -> Option<u64> {
    metadata.modified().ok().and_then(|time| {
        time.duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_millis()).ok())
    })
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn io_reason(error: &std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => "access denied",
        std::io::ErrorKind::NotFound => "not found",
        _ => "stat failed",
    }
}

fn spec(kind: PlaceKind, scope: PlaceScope, rank: u16, relative: &'static str, form: Form) -> Spec {
    Spec {
        kind,
        scope,
        rank,
        relative,
        form,
    }
}

fn user_specs(product: &str) -> Vec<Spec> {
    use PlaceKind::*;
    use PlaceScope::User;
    let skill = |relative| spec(Skills, User, 30, relative, Form::SkillRoot);
    let file = |kind, relative| spec(kind, User, 30, relative, Form::File);
    match product {
        "codex" | "codex-desktop" => vec![
            skill(".agents/skills"),
            skill(".codex/skills"),
            file(McpConfig, ".codex/config.toml"),
            file(AgentMemory, ".codex/AGENTS.md"),
            spec(
                Instructions,
                User,
                30,
                ".codex/prompts",
                Form::NamedFiles(&["md"]),
            ),
        ],
        "claude-code" => vec![
            skill(".claude/skills"),
            file(McpConfig, ".claude.json"),
            file(AgentMemory, ".claude/CLAUDE.md"),
            file(Rules, ".claude/settings.json"),
            spec(
                Instructions,
                User,
                30,
                ".claude/commands",
                Form::NamedFiles(&["md"]),
            ),
            spec(
                Instructions,
                User,
                30,
                ".claude/agents",
                Form::NamedFiles(&["md"]),
            ),
        ],
        "cursor" | "cursor-agent" => vec![
            skill(".cursor/skills"),
            skill(".agents/skills"),
            skill(".claude/skills"),
            skill(".codex/skills"),
            file(McpConfig, ".cursor/mcp.json"),
            file(Rules, ".cursor/cli-config.json"),
            spec(
                Instructions,
                User,
                30,
                ".cursor/commands",
                Form::NamedFiles(&["md"]),
            ),
        ],
        "gemini" => vec![
            file(McpConfig, ".gemini/settings.json"),
            file(AgentMemory, ".gemini/GEMINI.md"),
            spec(
                Instructions,
                User,
                30,
                ".gemini/commands",
                Form::NamedFiles(&["toml"]),
            ),
        ],
        "copilot-cli" => vec![file(McpConfig, ".copilot/mcp-config.json")],
        "cline" => vec![
            skill(".cline/skills"),
            file(McpConfig, ".cline/data/settings/cline_mcp_settings.json"),
        ],
        "windsurf" => vec![file(McpConfig, ".codeium/windsurf/mcp_config.json")],
        _ => Vec::new(),
    }
}

fn project_specs(product: &str) -> Vec<Spec> {
    use PlaceKind::*;
    use PlaceScope::{Local, Project};
    let skill = |relative| spec(Skills, Project, 20, relative, Form::SkillRoot);
    let file = |kind, relative| spec(kind, Project, 20, relative, Form::File);
    match product {
        "codex" | "codex-desktop" => vec![
            skill(".agents/skills"),
            skill(".codex/skills"),
            file(McpConfig, ".codex/config.toml"),
            file(AgentMemory, "AGENTS.md"),
        ],
        "claude-code" => vec![
            skill(".claude/skills"),
            file(McpConfig, ".mcp.json"),
            file(AgentMemory, "CLAUDE.md"),
            file(AgentMemory, ".claude/CLAUDE.md"),
            file(Rules, ".claude/settings.json"),
            spec(Rules, Local, 10, ".claude/settings.local.json", Form::File),
            spec(
                Instructions,
                Project,
                20,
                ".claude/commands",
                Form::NamedFiles(&["md"]),
            ),
            spec(
                Instructions,
                Project,
                20,
                ".claude/agents",
                Form::NamedFiles(&["md"]),
            ),
        ],
        "cursor" | "cursor-agent" => vec![
            skill(".cursor/skills"),
            skill(".agents/skills"),
            skill(".claude/skills"),
            skill(".codex/skills"),
            file(McpConfig, ".cursor/mcp.json"),
            file(AgentMemory, "AGENTS.md"),
            spec(
                Rules,
                Project,
                20,
                ".cursor/rules",
                Form::NamedFiles(&["md", "mdc"]),
            ),
            spec(
                Instructions,
                Project,
                20,
                ".cursor/commands",
                Form::NamedFiles(&["md"]),
            ),
        ],
        "gemini" => vec![
            file(McpConfig, ".gemini/settings.json"),
            file(AgentMemory, "GEMINI.md"),
            spec(
                Instructions,
                Project,
                20,
                ".gemini/commands",
                Form::NamedFiles(&["toml"]),
            ),
        ],
        "copilot" => vec![
            file(McpConfig, ".vscode/mcp.json"),
            file(McpConfig, ".mcp.json"),
            file(AgentMemory, "AGENTS.md"),
            file(Rules, ".github/copilot-instructions.md"),
            spec(
                Rules,
                Project,
                20,
                ".github/instructions",
                Form::NamedFiles(&["instructions.md"]),
            ),
            spec(
                Instructions,
                Project,
                20,
                ".github/prompts",
                Form::NamedFiles(&["prompt.md"]),
            ),
        ],
        "copilot-cli" => vec![
            file(McpConfig, ".mcp.json"),
            file(McpConfig, ".github/mcp.json"),
            file(AgentMemory, "AGENTS.md"),
        ],
        "cline" => vec![
            skill(".cline/skills"),
            spec(Rules, Project, 20, ".clinerules", Form::NamedFiles(&["md"])),
        ],
        "windsurf" => vec![spec(
            Rules,
            Project,
            20,
            ".windsurf/rules",
            Form::NamedFiles(&["md"]),
        )],
        _ => Vec::new(),
    }
}

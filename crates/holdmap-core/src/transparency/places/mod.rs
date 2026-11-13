//! Where an agent looks for instructions, rules, settings, skills, MCP configuration, hooks,
//! its own memory, and session data.
//!
//! The census stats documented paths from the shared [`locations`](super::locations)
//! registry. It does not read file bodies, follow symbolic links, or open conversation or
//! credential stores. A path that exists is a candidate the product documents, not proof the
//! running agent loaded it. holdmap does not decide which layer won; each row says how layers
//! of its kind combine.

mod presence;
mod stat;

use super::locations::{self, Combine, Form, Homes, LocationSpec};
use crate::agents::capabilities::builtin_tools;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(crate) const MAX_ENTRIES: usize = 256;
pub(crate) const MAX_CANDIDATES: usize = 64;

/// What a documented location is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceKind {
    /// A directory of `SKILL.md` folders.
    Skills,
    /// Instruction files you write for the agent: `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`.
    Instructions,
    /// Rule files (Cursor `.mdc`, Claude `.claude/rules`, Windsurf and Cline rules).
    Rules,
    /// Settings and permission files. Values are not read by the census.
    Settings,
    /// An MCP client configuration file. Values are not read by the census.
    McpConfig,
    /// Slash commands, prompts and workflows.
    Commands,
    /// Subagent or custom agent definitions.
    Subagents,
    /// Hook definitions that run commands on agent events.
    Hooks,
    /// Extensions and installed plugins.
    Extensions,
    /// Notes the agent writes for itself, such as Claude auto memory.
    AgentMemory,
    /// Transcripts, prompt history and logs. Size, count and newest time only.
    SessionData,
    /// Credentials or secret stores. Presence only.
    SecretStore,
    /// Tools compiled into the product. There is no filesystem path.
    Builtin,
    /// Plugin, marketplace or cloud installs, which this census does not walk.
    Plugin,
}

impl PlaceKind {
    /// Every kind, in display order.
    pub const ALL: [Self; 14] = [
        Self::Instructions,
        Self::Rules,
        Self::Settings,
        Self::McpConfig,
        Self::Skills,
        Self::Commands,
        Self::Subagents,
        Self::Hooks,
        Self::Extensions,
        Self::AgentMemory,
        Self::SessionData,
        Self::SecretStore,
        Self::Builtin,
        Self::Plugin,
    ];

    /// Short label shared by text surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Skills => "skills",
            Self::Instructions => "instructions",
            Self::Rules => "rules",
            Self::Settings => "settings",
            Self::McpConfig => "MCP config",
            Self::Commands => "commands",
            Self::Subagents => "subagents",
            Self::Hooks => "hooks",
            Self::Extensions => "extensions",
            Self::AgentMemory => "agent memory",
            Self::SessionData => "session data",
            Self::SecretStore => "secrets",
            Self::Builtin => "built-in",
            Self::Plugin => "plugin",
        }
    }
}

/// Which documented layer a location belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceScope {
    /// Set by an administrator for the whole machine; wins over everything else.
    Managed,
    /// A personal override beside the project files.
    Local,
    /// Under an observed project or working folder.
    Project,
    /// Under the account home.
    User,
    /// Machine-wide defaults that user and project files override.
    SystemDefault,
    /// A plugin or marketplace install.
    Plugin,
    /// Kept in the vendor account or app settings, not on disk.
    Account,
    /// Shipped inside the agent.
    Builtin,
}

impl PlaceScope {
    /// Every scope, most specific first.
    pub const ALL: [Self; 8] = [
        Self::Managed,
        Self::Local,
        Self::Project,
        Self::User,
        Self::SystemDefault,
        Self::Plugin,
        Self::Account,
        Self::Builtin,
    ];

    /// Short label shared by text surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::Local => "local",
            Self::Project => "project",
            Self::User => "user",
            Self::SystemDefault => "system default",
            Self::Plugin => "plugin",
            Self::Account => "account",
            Self::Builtin => "built-in",
        }
    }

    /// Documented order, most specific first. Not a resolved winner.
    pub fn rank(self) -> u16 {
        match self {
            Self::Managed => 0,
            Self::Local => 10,
            Self::Project => 20,
            Self::User => 30,
            Self::SystemDefault => 35,
            Self::Plugin => 40,
            Self::Account => 45,
            Self::Builtin => 50,
        }
    }
}

/// Whether the path metadata could be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceStatus {
    /// The path exists and is not a symbolic link, or it is a link whose target was recorded
    /// without being followed.
    Present,
    /// The path is not there.
    Missing,
    /// The path metadata could not be read.
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
    /// Documented layer order, most specific first. Not a resolved winner.
    pub rank: u16,
    /// Short human name ("CLAUDE.md", "Auto memory", "Transcripts").
    #[serde(default)]
    pub label: String,
    /// How files of this kind in several layers combine.
    #[serde(default = "default_combine")]
    pub combine: Combine,
    /// Vendor-documented context, when the path alone could mislead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Environment variable that moved this location's home, when one was set for holdmap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_home: Option<String>,
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
    /// Files here the product documents as ignored (Cursor `.md` in `.cursor/rules`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignored_count: Option<u64>,
    /// Lines in an auto-memory index (`MEMORY.md`). Counted; the text is discarded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_count: Option<u64>,
    /// Lines the product loads from that index each session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_limit_lines: Option<u64>,
}

fn default_combine() -> Combine {
    Combine::Merge
}

impl CapabilityPlace {
    /// One text row: layer, rank, status, path, size and day. Not a file body.
    pub fn summary_line(&self) -> String {
        let status = match self.status {
            PlaceStatus::Present => "present",
            PlaceStatus::Missing => "missing",
            PlaceStatus::Unreadable => "unreadable",
            PlaceStatus::Skipped => "skipped",
        };
        if self.path.is_empty() {
            let where_ = self.kind.label();
            let detail = if self.kind == PlaceKind::Builtin && !self.candidates.is_empty() {
                let noun = if self.candidates.len() == 1 {
                    "documented tool"
                } else {
                    "documented tools"
                };
                format!(" · {} {noun}", self.candidates.len())
            } else {
                String::new()
            };
            return format!(
                "{} {} ({}) {status}: {where_}{detail}",
                self.scope.label(),
                self.kind.label(),
                self.combine.label()
            );
        }
        let where_ = &self.path;
        let link = self
            .symlink_target
            .as_ref()
            .map(|target| format!(" → {target}"))
            .unwrap_or_default();
        let link_mark = if self.symlink_target.is_some() {
            " symlink"
        } else {
            ""
        };
        if self.status == PlaceStatus::Missing {
            return format!(
                "{} {} ({}) {status}{link_mark}: {where_}{link} · not on disk",
                self.scope.label(),
                self.kind.label(),
                self.combine.label()
            );
        }
        if self.kind == PlaceKind::SecretStore {
            return format!(
                "{} {} {status}: {where_} · not opened",
                self.scope.label(),
                self.kind.label()
            );
        }
        let count = match self.file_count {
            Some(1) => "1 entry".to_owned(),
            Some(n) => format!("{n} entries"),
            None => "count unknown".into(),
        };
        let lines = match (self.line_count, self.load_limit_lines) {
            (Some(n), Some(limit)) if n > limit => {
                format!(" · MEMORY.md {n} lines, only the first {limit} load")
            }
            (Some(n), Some(limit)) => format!(" · MEMORY.md {n} of {limit} lines"),
            _ => String::new(),
        };
        let ignored = match self.ignored_count {
            Some(n) if n > 0 => format!(" · {n} ignored"),
            _ => String::new(),
        };
        let size = match self.byte_size {
            Some(n) => crate::util::human_bytes(n),
            None => "size unknown".into(),
        };
        let when = self
            .modified_unix_ms
            .map(utc_day)
            .unwrap_or_else(|| "modified time unknown".into());
        format!(
            "{} {} ({}) {status}{link_mark}: {where_}{link} · {count} · {size} · {when}{lines}{ignored}",
            self.scope.label(),
            self.kind.label(),
            self.combine.label()
        )
    }
}

/// Calendar day in UTC for a Unix millisecond timestamp.
pub(crate) fn utc_day(unix_ms: u64) -> String {
    let days = unix_ms / 1000 / 86_400;
    // Howard Hinnant's `civil_from_days`, shifted so day 0 is 1970-01-01.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    format!("{year:04}-{month:02}-{day:02}")
}

/// Stat every documented location for `product` from the shared registry.
pub fn census(
    product: &str,
    homes: &Homes,
    projects: &[PathBuf],
    platform: &str,
) -> Vec<CapabilityPlace> {
    let specs = locations::specs(product, platform);
    let mut places = Vec::new();
    let mut seen = HashSet::new();
    for spec in specs.iter().filter(|spec| !spec.per_project()) {
        push_unique(
            &mut places,
            &mut seen,
            place_for(product, spec, homes, None, platform),
        );
    }
    for project in projects {
        for spec in specs.iter().filter(|spec| spec.per_project()) {
            push_unique(
                &mut places,
                &mut seen,
                place_for(product, spec, homes, Some(project), platform),
            );
        }
    }
    places.push(plugin_place(product));
    places.push(builtin_place(product));
    places
}

fn push_unique(
    places: &mut Vec<CapabilityPlace>,
    seen: &mut HashSet<String>,
    place: CapabilityPlace,
) {
    if place.path.is_empty() || seen.insert(place.path.clone()) {
        places.push(place);
    }
}

/// An unstat'ed row for `spec`; the caller fills status and sizes.
pub(crate) fn blank(product: &str, spec: &LocationSpec, path: String) -> CapabilityPlace {
    let id_tail = if path.is_empty() {
        spec.label.to_owned()
    } else {
        path.clone()
    };
    CapabilityPlace {
        id: format!(
            "{product}:{}:{}:{id_tail}",
            spec.kind.label(),
            spec.scope.label()
        ),
        kind: spec.kind,
        scope: spec.scope,
        rank: spec.scope.rank(),
        label: spec.label.to_owned(),
        combine: Combine::for_kind(spec.kind),
        note: spec.note.map(str::to_owned),
        custom_home: None,
        path,
        symlink_target: None,
        status: PlaceStatus::Missing,
        status_reason: None,
        file_count: None,
        byte_size: None,
        modified_unix_ms: None,
        candidates: Vec::new(),
        entries: Vec::new(),
        present: false,
        ignored_count: None,
        line_count: None,
        load_limit_lines: None,
    }
}

fn place_for(
    product: &str,
    spec: &LocationSpec,
    homes: &Homes,
    project: Option<&Path>,
    platform: &str,
) -> CapabilityPlace {
    if spec.form == Form::Account {
        let mut place = blank(product, spec, String::new());
        place.status = PlaceStatus::Skipped;
        place.status_reason = spec.note.map(str::to_owned);
        return place;
    }
    let Some(path) = locations::resolve(spec, homes, project) else {
        let mut place = blank(product, spec, String::new());
        place.status = PlaceStatus::Skipped;
        place.status_reason =
            Some("This location is not documented for this operating system.".into());
        return place;
    };
    let mut place = stat::stat_place(product, spec, &path, &homes.home, platform);
    place.custom_home = homes.override_for(spec.base).map(str::to_owned);
    place
}

fn plugin_place(product: &str) -> CapabilityPlace {
    CapabilityPlace {
        id: format!("{product}:plugin"),
        kind: PlaceKind::Plugin,
        scope: PlaceScope::Plugin,
        rank: PlaceScope::Plugin.rank(),
        label: "Marketplace and cloud installs".into(),
        combine: Combine::Merge,
        note: None,
        custom_home: None,
        path: String::new(),
        symlink_target: None,
        status: PlaceStatus::Skipped,
        status_reason: Some(
            "Marketplace and cloud skill installs are not a single documented folder this census stats. Account databases are not opened.".into(),
        ),
        file_count: None,
        byte_size: None,
        modified_unix_ms: None,
        candidates: Vec::new(),
        entries: Vec::new(),
        present: false,
        ignored_count: None,
        line_count: None,
        load_limit_lines: None,
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
        rank: PlaceScope::Builtin.rank(),
        label: "Built-in tools".into(),
        combine: Combine::Merge,
        note: None,
        custom_home: None,
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
        ignored_count: None,
        line_count: None,
        load_limit_lines: None,
    }
}

#[cfg(test)]
mod census_tests;

#[cfg(test)]
mod tests {
    use super::utc_day;

    #[test]
    fn unix_epoch_and_a_later_day_format_in_utc() {
        assert_eq!(utc_day(0), "1970-01-01");
        assert_eq!(utc_day(1_700_000_000_000), "2023-11-14");
    }
}

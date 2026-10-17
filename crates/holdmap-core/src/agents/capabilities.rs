//! Opt-in, bounded inventory of configuration aliases and skill file locations.
//!
//! Exported: aliases, transports, runner kind with the package/image/executable name and
//! whether it is pinned, the remote host, environment/header *names*, permission rules,
//! a few enumerated settings, and instruction-file names with frontmatter descriptions.
//! Never exported: arguments, environment/header values, URL paths or queries, prompts,
//! skill or rule bodies, transcripts or credentials.
//! Inventory is not evidence that a client loaded a definition or invoked a capability.

use super::{access, Evidence};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Debug;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

mod builtin;
mod frontmatter;
mod instructions;
mod launch;
mod policy;
mod reader;
mod runtime;

pub use builtin::builtin_tools;
pub use instructions::{InstructionFile, InstructionKind};
pub use launch::{McpLaunch, McpRunner};
pub use policy::{ConfigSetting, RuleDecision, ToolRule};
pub use runtime::{match_running, McpRunState, McpRuntime};

/// Distinct agent contexts inspected per report; further contexts remain unknown.
pub const MAX_CAPABILITY_CONTEXTS: usize = 16;
/// Definitions of each kind retained per context.
pub const MAX_CAPABILITIES: usize = 64;
const MAX_PROJECTS: usize = 8;
const MAX_SOURCES: usize = 24;
const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_TOTAL_BYTES: usize = 1024 * 1024;
const MAX_ENTRIES: usize = 256;
const MAX_TIME: Duration = Duration::from_millis(100);

/// Configuration scope, not proof of applicability to this running instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityScope {
    /// Account-wide definition.
    User,
    /// Definition in an observed working folder or its known project root.
    Project,
    /// Private project definition in an account's configuration file.
    Local,
}

impl CapabilityScope {
    /// Short scope label shared by text adapters.
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Local => "local",
        }
    }
}

/// Declared transport; no connection or protocol negotiation is attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// A configured local command (the command itself is omitted).
    Stdio,
    /// HTTP or Streamable HTTP.
    Http,
    /// Server-sent events.
    Sse,
    /// Missing, unsupported or contradictory transport metadata.
    Unknown,
}

impl McpTransport {
    /// Short declared-transport label shared by text adapters.
    pub fn label(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "HTTP",
            Self::Sse => "SSE",
            Self::Unknown => "unknown",
        }
    }
}

/// One MCP definition, retaining only explicitly allowed metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerCapability {
    /// Stable display identity; not an authorization handle.
    pub id: String,
    /// The configuration's server alias, not a tool or server-reported identity.
    pub name: String,
    /// Where the definition is scoped.
    pub scope: CapabilityScope,
    /// The file from which the alias was projected.
    pub source_path: PathBuf,
    /// Observed project context for project/local definitions.
    pub project_path: Option<PathBuf>,
    /// Declared transport kind, without endpoints or command lines.
    pub transport: McpTransport,
    /// Explicit configured switch only; `None` means unknown, including client policy.
    pub enabled: Option<bool>,
    /// The alias was observed in configuration; its use is not established.
    pub evidence: Evidence,
    /// What a stdio definition starts.
    #[serde(default)]
    pub launch: Option<McpLaunch>,
    /// Host (and port) of a remote endpoint, without credentials, path or query.
    #[serde(default)]
    pub remote_host: Option<String>,
    /// Names of configured environment variables; values are never read out.
    #[serde(default)]
    pub env_names: Vec<String>,
    /// Names of configured HTTP headers; values are never read out.
    #[serde(default)]
    pub header_names: Vec<String>,
    /// Whether one of the agent's processes runs it right now.
    #[serde(default)]
    pub runtime: McpRuntime,
}

/// Configured launch details of one server, already reduced to exportable metadata.
struct ServerDetails {
    transport: McpTransport,
    enabled: Option<bool>,
    launch: Option<McpLaunch>,
    remote_host: Option<String>,
    env_names: Vec<String>,
    header_names: Vec<String>,
}

/// A skill file found in a supported directory; only bounded frontmatter metadata is retained.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillCapability {
    /// Stable display identity; not an authorization handle.
    pub id: String,
    /// Containing folder name, without parsing or validating the skill instructions.
    pub name: String,
    /// Optional author-declared name, separate from the folder identity.
    #[serde(default)]
    pub declared_name: Option<String>,
    /// Optional sanitized one-line purpose, at most 300 characters. Never instructions.
    #[serde(default)]
    pub description: Option<String>,
    /// Account or observed project scope.
    pub scope: CapabilityScope,
    /// Exact `SKILL.md` location.
    pub source_path: PathBuf,
    /// Observed project context, when present.
    pub project_path: Option<PathBuf>,
    /// File presence was observed; activation and enablement remain unknown.
    pub evidence: Evidence,
}

/// Optional capability inventory attached to an observed agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentCapabilities {
    /// Whether supported locations were considered. An empty list alone proves no absence.
    pub inspected: bool,
    /// Configured MCP aliases, separate from running child tools.
    pub mcp_servers: Vec<McpServerCapability>,
    /// Skill file locations, separate from recorded activity.
    pub skills: Vec<SkillCapability>,
    /// Instruction files: rules, commands, prompts, subagents and memory files.
    pub instructions: Vec<InstructionFile>,
    /// Configured tool permission rules (allow/ask/deny).
    pub tool_rules: Vec<ToolRule>,
    /// Allowlisted safety settings with documented values.
    pub settings: Vec<ConfigSetting>,
    /// Documented built-in tools for the product; not observed.
    pub builtin_tools: Vec<String>,
    /// Additional observed definitions omitted by the display bound, not an unknown total.
    pub more_mcp_servers: usize,
    /// Additional observed skill files omitted by the display bound.
    pub more_skills: usize,
    /// Coverage limits; these never contain file contents or parser errors.
    pub limits: Vec<String>,
}

impl Default for AgentCapabilities {
    fn default() -> Self {
        Self {
            inspected: false,
            mcp_servers: Vec::new(),
            skills: Vec::new(),
            instructions: Vec::new(),
            tool_rules: Vec::new(),
            settings: Vec::new(),
            builtin_tools: Vec::new(),
            more_mcp_servers: 0,
            more_skills: 0,
            limits: vec!["Capability configuration and skill locations were not inspected.".into()],
        }
    }
}

impl AgentCapabilities {
    pub(super) fn limit(&mut self, message: &str) {
        if self.limits.len() < 12 && !self.limits.iter().any(|limit| limit == message) {
            self.limits.push(message.into());
        }
    }
}

/// Injected inventory source. Builders call it only for confirmed current-account agents.
pub trait CapabilityDiscovery: Send + Sync + Debug {
    /// Inspect only these observed working folders/project roots, never recent history.
    fn discover(&self, product: &str, projects: &[PathBuf], platform: &str) -> AgentCapabilities;
}

/// Default: no configuration-bearing files or skill directories are inspected.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoCapabilities;

impl CapabilityDiscovery for NoCapabilities {
    fn discover(&self, _: &str, _: &[PathBuf], _: &str) -> AgentCapabilities {
        AgentCapabilities::default()
    }
}

/// Read-only inventory under a specified home, enabled explicitly by a caller.
#[derive(Debug, Clone)]
pub struct HomeCapabilities {
    home: PathBuf,
    app_data: Option<PathBuf>,
}

impl HomeCapabilities {
    /// Use only the default documented locations under this home (isolatable in tests).
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            app_data: None,
        }
    }

    /// Explicit Windows roaming application-data location, useful for isolated fixtures.
    pub fn with_app_data(home: PathBuf, app_data: PathBuf) -> Self {
        Self {
            home,
            app_data: Some(app_data),
        }
    }

    /// Default locations for the current account. Custom client config homes are not followed.
    pub fn current() -> Option<Self> {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(|home| {
                let mut provider = Self::new(home.into());
                if cfg!(windows) {
                    provider.app_data = std::env::var_os("APPDATA").map(PathBuf::from);
                }
                provider
            })
    }
}

struct Inventory<'a> {
    product: &'a str,
    home: &'a Path,
    platform: &'a str,
    out: AgentCapabilities,
    start: Instant,
    sources: usize,
    bytes: usize,
    entries: usize,
    seen_sources: HashSet<PathBuf>,
    seen_skills: HashSet<PathBuf>,
    projects: Vec<PathBuf>,
}

struct ReadBytes {
    bytes: Vec<u8>,
    // Filling the read limit leaves EOF unknown, including a smaller total-byte remainder.
    at_eof: bool,
}

impl CapabilityDiscovery for HomeCapabilities {
    fn discover(&self, product: &str, projects: &[PathBuf], platform: &str) -> AgentCapabilities {
        if !supported_product(product) {
            let mut out = AgentCapabilities::default();
            out.limit("Capability inventory supports documented default locations for Codex, Claude Code, Claude Desktop, Cursor, Windsurf, Gemini CLI, GitHub Copilot, Copilot CLI and Cline.");
            return out;
        }
        if product == "claude-desktop" && !matches!(platform, "macos" | "windows") {
            let mut out = AgentCapabilities::default();
            out.limit("Claude Desktop manual MCP configuration is inspected only at documented macOS and Windows locations.");
            return out;
        }
        let mut inventory = Inventory {
            product,
            home: &self.home,
            platform,
            out: AgentCapabilities {
                inspected: true,
                limits: vec![
                    "Configuration and skill file presence do not prove loading, approval, connection or invocation; precedence and effective client policy are not resolved.".into(),
                    "Only default locations and observed working folders/project roots are considered; custom homes, parent-folder inheritance, managed settings, plugins and cloud skills are not inspected.".into(),
                    "Skill identity comes from folders; only optional declared name/description from bounded YAML frontmatter are retained. Instructions, validity and enablement are not collected. Skill directories are shallow, with one extra category/.system level only.".into(),
                ],
                ..Default::default()
            },
            start: Instant::now(),
            sources: 0,
            bytes: 0,
            entries: 0,
            seen_sources: HashSet::new(),
            seen_skills: HashSet::new(),
            projects: Vec::new(),
        };
        match product {
            "claude-desktop" => {
                inventory.out.limit("Claude Desktop inventory covers only its manual MCP JSON; extensions, connectors and skills are not inspected.");
            }
            "cursor" | "cursor-agent" => {
                inventory.out.limit("Untyped Cursor remote URLs can use HTTP or SSE; their transport is not established without connection negotiation.");
            }
            "windsurf" => {
                inventory.out.limit("Windsurf inventory covers only legacy ~/.codeium/windsurf/mcp_config.json; newer Devin/Cascade homes, project MCP files and skills are not inspected. Untyped remote endpoints do not establish HTTP versus SSE.");
            }
            "gemini" => {
                inventory.out.limit("Gemini CLI inventory covers mcpServers in default ~/.gemini/settings.json and observed-project .gemini/settings.json; custom homes, system settings, enablement policy/state and skill directories are not inspected. Untyped URLs leave HTTP versus SSE unknown.");
            }
            "copilot" => {
                inventory.out.limit("VS Code Copilot inventory covers observed-project .vscode/mcp.json (servers) and .mcp.json; user/profile config, enablement state and skill directories are not inspected.");
            }
            "copilot-cli" => {
                inventory.out.limit("Copilot CLI inventory covers default ~/.copilot/mcp-config.json and observed-project .mcp.json/.github/mcp.json; custom homes, enablement settings and skill directories are not inspected. Folder trust, inheritance and precedence are not resolved.");
            }
            "cline" => {
                inventory.out.limit("Cline inventory covers default ~/.cline/data/settings/cline_mcp_settings.json and shallow .cline/skills directories; custom homes, .agents/.clinerules/.claude compatibility roots and editor globalStorage are not followed.");
            }
            _ => {}
        }
        if product == "claude-desktop" {
            let base = if platform == "macos" {
                self.home.join("Library/Application Support")
            } else {
                self.app_data
                    .clone()
                    .unwrap_or_else(|| self.home.join("AppData/Roaming"))
            };
            inventory.config(
                &base.join("Claude/claude_desktop_config.json"),
                CapabilityScope::User,
                None,
                ConfigFormat::JsonMcpServers,
            );
            return inventory.out;
        }
        let mut seen = HashSet::new();
        for project in projects {
            if !seen.insert(project.clone()) {
                continue;
            }
            if seen.len() > MAX_PROJECTS {
                inventory.out.limit("Project inventory is limited to eight observed folders/project roots per agent.");
                break;
            }
            if project == &self.home || project.parent().is_none() {
                continue;
            }
            inventory.projects.push(project.clone());
        }
        // Collect MCP definitions before skill metadata can exhaust the shared read budget.
        inventory.location(&self.home, CapabilityScope::User, None);
        let projects = inventory.projects.clone();
        for project in &projects {
            inventory.location(project, CapabilityScope::Project, Some(project));
        }
        inventory.skill_location(&self.home, CapabilityScope::User, None);
        for project in &projects {
            inventory.skill_location(project, CapabilityScope::Project, Some(project));
        }
        inventory.policy_location(&self.home, CapabilityScope::User);
        inventory.instruction_location(&self.home, CapabilityScope::User);
        for project in &projects {
            inventory.policy_location(project, CapabilityScope::Project);
            inventory.instruction_location(project, CapabilityScope::Project);
        }
        inventory.out.builtin_tools = builtin_tools(product)
            .iter()
            .map(|tool| (*tool).to_owned())
            .collect();
        inventory
            .out
            .mcp_servers
            .sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        inventory
            .out
            .skills
            .sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        inventory.out
    }
}

impl Inventory<'_> {
    fn bounded(&mut self) -> bool {
        let permitted = self.sources < MAX_SOURCES
            && self.bytes < MAX_TOTAL_BYTES
            && self.entries < MAX_ENTRIES
            && self.start.elapsed() < MAX_TIME;
        if !permitted {
            self.out.limit("Inventory stopped at its file, byte, directory-entry or time bound; additional definitions may exist.");
        }
        permitted
    }

    fn safe_path(&mut self, path: &Path) -> bool {
        if path
            .to_str()
            .is_none_or(|path| path.len() > 4096 || path.chars().any(char::is_control))
        {
            self.out.limit(
                "Non-Unicode, control-bearing and oversized source paths are not inspected.",
            );
            return false;
        }
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
            || access::privacy_area(path, Some(self.home), self.platform).is_some()
        {
            self.out.limit(
                "Relative, traversing and macOS privacy-protected locations are not inspected.",
            );
            return false;
        }
        // Reject symlinks/reparse points in every component, including parent directories.
        // No canonicalization may silently redirect discovery into a protected location.
        for ancestor in path.ancestors() {
            if self.start.elapsed() >= MAX_TIME {
                self.out.limit(
                    "Inventory stopped at its time bound; additional definitions may exist.",
                );
                return false;
            }
            match std::fs::symlink_metadata(ancestor) {
                Ok(metadata) if is_link(&metadata) => {
                    self.out
                        .limit("Symbolic links and reparse points are not followed.");
                    return false;
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
                Err(_) => {
                    self.out
                        .limit("Some inventory locations could not be inspected.");
                    return false;
                }
            }
        }
        true
    }

    fn location(&mut self, base: &Path, scope: CapabilityScope, project: Option<&Path>) {
        if !self.bounded() || !self.safe_path(base) {
            return;
        }
        match self.product {
            "codex" | "codex-desktop" => self.config(
                &base.join(".codex/config.toml"),
                scope,
                project,
                ConfigFormat::Toml,
            ),
            "claude-code" if scope == CapabilityScope::User => {
                self.config(
                    &base.join(".claude.json"),
                    scope,
                    project,
                    ConfigFormat::JsonMcpServers,
                );
            }
            "claude-code" => self.config(
                &base.join(".mcp.json"),
                scope,
                project,
                ConfigFormat::JsonMcpServers,
            ),
            "cursor" | "cursor-agent" => self.config(
                &base.join(".cursor/mcp.json"),
                scope,
                project,
                ConfigFormat::JsonMcpServers,
            ),
            "windsurf" if scope == CapabilityScope::User => self.config(
                &base.join(".codeium/windsurf/mcp_config.json"),
                scope,
                project,
                ConfigFormat::JsonMcpServers,
            ),
            "gemini" => self.config(
                &base.join(".gemini/settings.json"),
                scope,
                project,
                ConfigFormat::JsonMcpServers,
            ),
            "copilot" if scope == CapabilityScope::Project => {
                self.config(
                    &base.join(".vscode/mcp.json"),
                    scope,
                    project,
                    ConfigFormat::JsonServers,
                );
                self.config(
                    &base.join(".mcp.json"),
                    scope,
                    project,
                    ConfigFormat::JsonCopilot,
                );
            }
            "copilot-cli" if scope == CapabilityScope::User => self.config(
                &base.join(".copilot/mcp-config.json"),
                scope,
                project,
                ConfigFormat::JsonCopilot,
            ),
            "copilot-cli" => {
                for file in [".mcp.json", ".github/mcp.json"] {
                    self.config(&base.join(file), scope, project, ConfigFormat::JsonCopilot);
                }
            }
            "cline" if scope == CapabilityScope::User => self.config(
                &base.join(".cline/data/settings/cline_mcp_settings.json"),
                scope,
                project,
                ConfigFormat::JsonMcpServers,
            ),
            _ => {}
        }
    }

    fn skill_location(&mut self, base: &Path, scope: CapabilityScope, project: Option<&Path>) {
        if !self.bounded() || !self.safe_path(base) {
            return;
        }
        let directories: &[&str] = match self.product {
            "codex" | "codex-desktop" => &[".agents/skills", ".codex/skills"],
            "claude-code" => &[".claude/skills"],
            "cursor" | "cursor-agent" => &[
                ".cursor/skills",
                ".agents/skills",
                ".claude/skills",
                ".codex/skills",
            ],
            "cline" => &[".cline/skills"],
            _ => &[],
        };
        for directory in directories {
            self.skills(&base.join(directory), scope, project, 0);
        }
    }

    fn read_bytes(&mut self, path: &Path, maximum: usize, complete: bool) -> Option<ReadBytes> {
        if !self.bounded() || !self.safe_path(path) {
            return None;
        }
        self.sources += 1;
        let Ok(mut file) = reader::open(path) else {
            self.out
                .limit("Some inventory locations could not be inspected.");
            return None;
        };
        let Ok(metadata) = file.metadata() else {
            self.out
                .limit("Some inventory locations could not be inspected.");
            return None;
        };
        if !metadata.is_file() || is_link(&metadata) || complete && metadata.len() > maximum as u64
        {
            self.out
                .limit("Nonregular or oversized configuration files are not inspected.");
            return None;
        }
        let remaining = maximum.min(MAX_TOTAL_BYTES.saturating_sub(self.bytes));
        if complete && metadata.len() > remaining as u64 {
            self.out
                .limit("Some configuration files exceeded the total read bound.");
            return None;
        }
        let mut bytes = Vec::new();
        let result = file.by_ref().take(remaining as u64).read_to_end(&mut bytes);
        self.bytes = self.bytes.saturating_add(bytes.len());
        if result.is_err()
            || reader::verify(&file, path).is_err()
            || complete
                && (bytes.len() as u64 != metadata.len()
                    || !file
                        .metadata()
                        .is_ok_and(|after| after.len() == metadata.len()))
        {
            self.out
                .limit("Some configuration files exceeded the read bound or could not be read.");
            return None;
        }
        Some(ReadBytes {
            at_eof: bytes.len() < remaining,
            bytes,
        })
    }

    fn read(&mut self, path: &Path) -> Option<String> {
        let contents = self.read_bytes(path, MAX_FILE_BYTES, true)?;
        match String::from_utf8(contents.bytes) {
            Ok(text) => Some(text),
            Err(_) => {
                self.out.limit(
                    "Some configuration files could not be parsed; their contents are omitted.",
                );
                None
            }
        }
    }

    fn read_json(&mut self, path: &Path) -> Option<serde_json::Value> {
        let text = self.read(path)?;
        // VS Code accepts JSONC (including trailing commas); Gemini's settings loader
        // strips comments but still rejects trailing commas. Other formats stay strict.
        let value = if matches!(self.product, "copilot" | "gemini") {
            jsonc_bytes(&text, self.product == "copilot")
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        } else {
            serde_json::from_str(&text).ok()
        };
        match value {
            Some(value) => Some(value),
            None => {
                self.out.limit(
                    "Some configuration files could not be parsed; their contents are omitted.",
                );
                None
            }
        }
    }

    fn config(
        &mut self,
        path: &Path,
        scope: CapabilityScope,
        project: Option<&Path>,
        format: ConfigFormat,
    ) {
        if !self.seen_sources.insert(path.to_owned()) {
            return;
        }
        match format {
            ConfigFormat::Toml => {
                let Some(text) = self.read(path) else { return };
                let Ok(value) = text.parse::<toml::Table>() else {
                    self.out.limit(
                        "Some configuration files could not be parsed; their contents are omitted.",
                    );
                    return;
                };
                self.toml_policy(&value, scope, path);
                if let Some(servers) = value.get("mcp_servers").and_then(toml::Value::as_table) {
                    for (name, entry) in servers {
                        if !self.bounded() {
                            break;
                        }
                        self.entries += 1;
                        let Some(entry) = entry.as_table() else {
                            continue;
                        };
                        let command = entry
                            .get("command")
                            .and_then(toml::Value::as_str)
                            .is_some_and(|value| !value.is_empty());
                        let url = entry
                            .get("url")
                            .and_then(toml::Value::as_str)
                            .is_some_and(|value| !value.is_empty());
                        let transport = match (command, url) {
                            (true, false) => McpTransport::Stdio,
                            (false, true) => McpTransport::Http,
                            _ => McpTransport::Unknown,
                        };
                        let enabled = entry.get("enabled").and_then(toml::Value::as_bool);
                        let details = ServerDetails {
                            transport,
                            enabled,
                            ..toml_details(entry)
                        };
                        self.server(name, path, scope, project, details);
                    }
                }
            }
            ConfigFormat::JsonMcpServers
            | ConfigFormat::JsonServers
            | ConfigFormat::JsonCopilot => {
                let Some(value) = self.read_json(path) else {
                    return;
                };
                let key = match format {
                    ConfigFormat::JsonServers => "servers",
                    _ => "mcpServers",
                };
                let servers = if matches!(format, ConfigFormat::JsonCopilot)
                    && self.product == "copilot-cli"
                    && scope == CapabilityScope::Project
                    && value.get(key).is_none()
                {
                    // Copilot CLI documents both wrapped and bare project server maps.
                    value.as_object()
                } else {
                    value.get(key).and_then(serde_json::Value::as_object)
                };
                if let Some(servers) = servers {
                    self.json_servers(servers, path, scope, project, format);
                }
                if self.product == "gemini" {
                    self.json_policy(&value, scope, path);
                }
                // Parse the account file once and project only exact observed project aliases.
                // No unrelated project records or credential values survive this projection.
                if matches!(format, ConfigFormat::JsonMcpServers)
                    && self.product == "claude-code"
                    && scope == CapabilityScope::User
                {
                    for project in self.projects.clone() {
                        if !self.bounded() || !self.safe_path(&project) {
                            continue;
                        }
                        let Some(key) = project.to_str() else {
                            continue;
                        };
                        if let Some(servers) = value
                            .get("projects")
                            .and_then(|projects| projects.get(key))
                            .and_then(|project| project.get("mcpServers"))
                            .and_then(serde_json::Value::as_object)
                        {
                            self.json_servers(
                                servers,
                                path,
                                CapabilityScope::Local,
                                Some(&project),
                                format,
                            );
                        }
                    }
                }
            }
        }
    }

    fn json_servers(
        &mut self,
        servers: &serde_json::Map<String, serde_json::Value>,
        path: &Path,
        scope: CapabilityScope,
        project: Option<&Path>,
        format: ConfigFormat,
    ) {
        for (name, entry) in servers {
            if !self.bounded() {
                break;
            }
            self.entries += 1;
            let Some(entry) = entry.as_object() else {
                continue;
            };
            let transport = json_transport(self.product, format, entry);
            let explicit = entry.get("enabled").and_then(serde_json::Value::as_bool);
            let inverse = entry
                .get("disabled")
                .and_then(serde_json::Value::as_bool)
                .map(|disabled| !disabled);
            let enabled = match self.product {
                // These clients use separate settings or policy; their server
                // definitions do not establish an enabled/disabled state.
                "gemini" | "copilot" | "copilot-cli" => None,
                "cline" => inverse,
                _ => match (explicit, inverse) {
                    (Some(enabled), Some(inverse)) if enabled == inverse => Some(enabled),
                    (Some(_), Some(_)) => None,
                    (enabled, disabled) => enabled.or(disabled),
                },
            };
            let details = ServerDetails {
                transport,
                enabled,
                ..json_details(entry)
            };
            self.server(name, path, scope, project, details);
        }
    }

    fn server(
        &mut self,
        name: &str,
        path: &Path,
        scope: CapabilityScope,
        project: Option<&Path>,
        details: ServerDetails,
    ) {
        let (transport, enabled) = (details.transport, details.enabled);
        if !safe_name(name) {
            self.out.limit(
                "Aliases and folder names with unsupported or recognized sensitive text are omitted.",
            );
            return;
        }
        if self.out.mcp_servers.len() == MAX_CAPABILITIES {
            self.out.more_mcp_servers += 1;
            self.out.limit(
                "MCP and skill lists retain at most 64 entries of each kind per agent context.",
            );
            return;
        }
        self.out.mcp_servers.push(McpServerCapability {
            id: identity(self.product, "mcp", scope, path, project, name),
            name: name.into(),
            scope,
            source_path: path.into(),
            project_path: project.map(Path::to_owned),
            transport,
            enabled,
            evidence: Evidence::Observed,
            launch: details.launch.filter(|_| transport == McpTransport::Stdio),
            remote_host: details.remote_host.filter(|_| {
                matches!(
                    transport,
                    McpTransport::Http | McpTransport::Sse | McpTransport::Unknown
                )
            }),
            env_names: details.env_names,
            header_names: details.header_names,
            runtime: McpRuntime::default(),
        });
    }

    fn skills(
        &mut self,
        directory: &Path,
        scope: CapabilityScope,
        project: Option<&Path>,
        depth: usize,
    ) {
        if !self.bounded() || !self.safe_path(directory) {
            return;
        }
        self.sources += 1;
        let Ok(entries) = std::fs::read_dir(directory) else {
            self.out
                .limit("Some inventory locations could not be inspected.");
            return;
        };
        let remaining = MAX_ENTRIES.saturating_sub(self.entries);
        let mut entries: Vec<_> = entries.take(remaining + 1).flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            if !self.bounded() {
                break;
            }
            self.entries += 1;
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                self.out
                    .limit("Symbolic links and reparse points are not followed.");
                continue;
            }
            if !kind.is_dir() {
                continue;
            }
            let directory = entry.path();
            let file = directory.join("SKILL.md");
            if self.safe_path(&file)
                && std::fs::symlink_metadata(&file)
                    .is_ok_and(|metadata| metadata.is_file() && !is_link(&metadata))
            {
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if !safe_name(&name) {
                    self.out.limit("Aliases and folder names with unsupported or recognized sensitive text are omitted.");
                    continue;
                }
                if !self.seen_skills.insert(file.clone()) {
                    continue;
                }
                if self.out.skills.len() == MAX_CAPABILITIES {
                    self.out.more_skills += 1;
                    self.out.limit("MCP and skill lists retain at most 64 entries of each kind per agent context.");
                    continue;
                }
                let metadata = self
                    .read_bytes(&file, frontmatter::MAX_HEADER_BYTES, false)
                    .and_then(|prefix| frontmatter::parse(&prefix.bytes, prefix.at_eof));
                if metadata.is_none() {
                    self.out.limit("Some skill frontmatter metadata was unavailable, unsupported, unsafe or exceeded its bounds.");
                }
                let (declared_name, description) = metadata.unwrap_or_default();
                self.out.skills.push(SkillCapability {
                    id: identity(self.product, "skill", scope, &file, project, &name),
                    name,
                    declared_name,
                    description,
                    scope,
                    source_path: file,
                    project_path: project.map(Path::to_owned),
                    evidence: Evidence::Observed,
                });
            } else if depth == 0
                && (matches!(self.product, "cursor" | "cursor-agent")
                    || entry.file_name() == ".system")
            {
                self.skills(&directory, scope, project, depth + 1);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ConfigFormat {
    Toml,
    JsonMcpServers,
    JsonServers,
    JsonCopilot,
}

fn json_details(entry: &serde_json::Map<String, serde_json::Value>) -> ServerDetails {
    let text = |key: &str| entry.get(key).and_then(serde_json::Value::as_str);
    let args: Vec<&str> = entry
        .get("args")
        .and_then(serde_json::Value::as_array)
        .map(|args| args.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();
    let keys = |key: &str| -> Vec<&str> {
        entry
            .get(key)
            .and_then(serde_json::Value::as_object)
            .map(|map| map.keys().map(String::as_str).collect())
            .unwrap_or_default()
    };
    ServerDetails {
        transport: McpTransport::Unknown,
        enabled: None,
        launch: text("command").map(|command| launch::stdio_launch(command, &args)),
        remote_host: ["url", "httpUrl", "serverUrl"]
            .iter()
            .find_map(|key| text(key))
            .and_then(launch::remote_host),
        env_names: launch::env_names(keys("env")),
        header_names: launch::header_names(keys("headers")),
    }
}

fn toml_details(entry: &toml::Table) -> ServerDetails {
    let args: Vec<&str> = entry
        .get("args")
        .and_then(toml::Value::as_array)
        .map(|args| args.iter().filter_map(toml::Value::as_str).collect())
        .unwrap_or_default();
    let keys = |key: &str| -> Vec<&str> {
        entry
            .get(key)
            .and_then(toml::Value::as_table)
            .map(|table| table.keys().map(String::as_str).collect())
            .unwrap_or_default()
    };
    let forwarded: Vec<&str> = entry
        .get("env_vars")
        .and_then(toml::Value::as_array)
        .map(|names| names.iter().filter_map(toml::Value::as_str).collect())
        .unwrap_or_default();
    ServerDetails {
        transport: McpTransport::Unknown,
        enabled: None,
        launch: entry
            .get("command")
            .and_then(toml::Value::as_str)
            .map(|command| launch::stdio_launch(command, &args)),
        remote_host: entry
            .get("url")
            .and_then(toml::Value::as_str)
            .and_then(launch::remote_host),
        env_names: launch::env_names(keys("env").into_iter().chain(forwarded)),
        header_names: launch::header_names(
            keys("http_headers")
                .into_iter()
                .chain(keys("env_http_headers")),
        ),
    }
}

fn json_transport(
    product: &str,
    format: ConfigFormat,
    entry: &serde_json::Map<String, serde_json::Value>,
) -> McpTransport {
    let command = nonempty_key(entry, "command");
    let url = nonempty_key(entry, "url");
    let http_url = nonempty_key(entry, "httpUrl");
    let server_url = nonempty_key(entry, "serverUrl");
    // A field from a different client's schema, a malformed endpoint or overlapping
    // launch methods cannot establish a transport for this particular definition.
    if ["command", "url", "httpUrl", "serverUrl"]
        .iter()
        .any(|key| entry.contains_key(*key) && !nonempty_key(entry, key))
        || http_url && product != "gemini"
        || server_url && product != "windsurf"
        || [command, url, http_url, server_url]
            .into_iter()
            .filter(|present| *present)
            .count()
            != 1
    {
        return McpTransport::Unknown;
    }
    let kind = match entry.get("type") {
        Some(serde_json::Value::String(kind)) => Some(kind.as_str()),
        None => None,
        Some(_) => return McpTransport::Unknown,
    };
    if product == "gemini" {
        // Older Gemini releases used url for SSE; current ones negotiate HTTP/SSE.
        // Only the explicit type or the dedicated HTTP key establishes the kind.
        return match (kind, command, url, http_url) {
            (None, true, _, _) => McpTransport::Stdio,
            (None, _, _, true) => McpTransport::Http,
            (Some("http"), _, true, _) => McpTransport::Http,
            (Some("sse"), _, true, _) => McpTransport::Sse,
            _ => McpTransport::Unknown,
        };
    }
    if product == "windsurf" && !command {
        // Its documented URL fields also serve legacy SSE endpoints. Without
        // connection negotiation, file presence alone cannot distinguish them.
        return McpTransport::Unknown;
    }
    match kind {
        Some("stdio") | None if command => McpTransport::Stdio,
        Some("local") if command && matches!(format, ConfigFormat::JsonCopilot) => {
            McpTransport::Stdio
        }
        Some("http" | "streamable-http") if url => McpTransport::Http,
        Some("streamableHttp") if url && product == "cline" => McpTransport::Http,
        Some("sse") if url => McpTransport::Sse,
        None if url && product == "cline" => McpTransport::Sse,
        _ => McpTransport::Unknown,
    }
}

/// Mask JSON comments and optional trailing commas without changing strings or
/// concatenating tokens. The ordinary bounded JSON parser still validates grammar.
fn jsonc_bytes(text: &str, trailing_commas: bool) -> Option<Vec<u8>> {
    if text.len() > MAX_FILE_BYTES {
        return None;
    }
    let mut bytes = text.as_bytes().to_vec();
    let mut index = 0;
    let mut string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            index += 1;
        } else if byte == b'"' {
            string = true;
            index += 1;
        } else if bytes.get(index..index + 2) == Some(b"//") {
            while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                bytes[index] = b' ';
                index += 1;
            }
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            bytes[index..index + 2].fill(b' ');
            index += 2;
            loop {
                if bytes.get(index..index + 2) == Some(b"*/") {
                    bytes[index..index + 2].fill(b' ');
                    index += 2;
                    break;
                }
                let byte = bytes.get_mut(index)?; // Unterminated comments stay invalid.
                if !matches!(*byte, b'\n' | b'\r') {
                    *byte = b' ';
                }
                index += 1;
            }
        } else {
            index += 1;
        }
    }
    if trailing_commas {
        string = false;
        escaped = false;
        for index in 0..bytes.len() {
            let byte = bytes[index];
            if string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    string = false;
                }
            } else if byte == b'"' {
                string = true;
            } else if byte == b','
                && bytes[..index]
                    .iter()
                    .rfind(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
                    .is_some_and(|byte| !matches!(byte, b'[' | b'{' | b',' | b':'))
                && bytes[index + 1..]
                    .iter()
                    .find(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
                    .is_some_and(|byte| matches!(byte, b'}' | b']'))
            {
                bytes[index] = b' ';
            }
        }
    }
    Some(bytes)
}

fn supported_product(product: &str) -> bool {
    matches!(
        product,
        "codex"
            | "codex-desktop"
            | "claude-code"
            | "claude-desktop"
            | "cursor"
            | "cursor-agent"
            | "windsurf"
            | "gemini"
            | "copilot"
            | "copilot-cli"
            | "cline"
    )
}

fn nonempty_key(entry: &serde_json::Map<String, serde_json::Value>, key: &str) -> bool {
    entry
        .get(key)
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !value.is_empty())
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.trim() == name
        && name
            .chars()
            .all(|character| character.is_alphanumeric() || "-_. ".contains(character))
        && crate::redact::line(name) == name
        && !["sk-", "ghp_", "github_pat_", "xoxb-", "xoxp-"]
            .iter()
            .any(|prefix| name.to_ascii_lowercase().contains(prefix))
}

fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn identity(
    product: &str,
    kind: &str,
    scope: CapabilityScope,
    source: &Path,
    project: Option<&Path>,
    name: &str,
) -> String {
    // Exact tuple encoding avoids ambiguous delimiters and randomized or colliding hashes.
    serde_json::json!([
        product,
        kind,
        scope,
        source.to_string_lossy(),
        project.map(Path::to_string_lossy),
        name
    ])
    .to_string()
}

#[cfg(test)]
mod tests;

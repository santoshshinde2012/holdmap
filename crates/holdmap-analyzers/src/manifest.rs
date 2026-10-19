//! `holdmap-analyzer.toml`: what an analyzer is, how it is started, what it may read and send,
//! and how its output maps to findings. Validation is strict and closed: unknown keys, unknown
//! placeholders, shells and anything v1 doesn't support are rejected.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// The only manifest schema this version reads.
pub const SCHEMA: u32 = 1;
/// Placeholder for the subject's staged folder.
pub const SUBJECT_DIR: &str = "{subject.dir}";
/// Placeholder for the file the analyzer writes its report to.
pub const OUTPUT: &str = "{output}";
/// Placeholder for an empty file holdmap creates for the run: a portable `/dev/null` for
/// options like `--config` that must not read settings from the scanned folder.
pub const EMPTY_FILE: &str = "{empty.file}";
const MAX_TIMEOUT_SECS: u64 = 600;
const MAX_OUTPUT_MB: u64 = 32;
/// Tools one MCP analyzer may call.
pub const MAX_TOOLS: usize = 8;
const SHELLS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "ksh",
    "csh",
    "tcsh",
    "cmd",
    "cmd.exe",
    "powershell",
    "powershell.exe",
    "pwsh",
    "pwsh.exe",
    "env",
    "sudo",
    "doas",
    "osascript",
];

/// A parsed, validated manifest plus the hash of its exact bytes (the approval pin).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzerManifest {
    /// Schema version.
    pub schema: u32,
    /// Reverse-DNS id, e.g. `nvidia.skillspector`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Who publishes the analyzer.
    pub publisher: String,
    /// Project page (`https`), or empty when unknown.
    #[serde(default)]
    pub homepage: String,
    /// SPDX licence.
    pub license: String,
    /// One-line purpose.
    pub summary: String,
    /// How the user installs it (shown, never run).
    #[serde(default)]
    pub install: Option<String>,
    /// How it is started.
    pub runner: RunnerSpec,
    /// What it may read and send.
    pub permissions: Permissions,
    /// How its output maps to findings.
    pub output: OutputSpec,
    /// SHA-256 of the manifest bytes; filled in by [`AnalyzerManifest::parse`].
    #[serde(skip)]
    pub sha256: String,
}

/// How an analyzer runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerKind {
    /// Part of holdmap.
    Builtin,
    /// A local command-line program.
    Cli,
    /// An MCP server the user registered: a local command started over stdio only for a run
    /// (then stopped), or a Streamable HTTP endpoint. Its chosen tools are called per skill.
    Mcp,
}

/// How holdmap reaches an MCP analyzer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// holdmap starts `command` for the run, sandboxed, and talks over stdin/stdout.
    #[default]
    Stdio,
    /// holdmap connects to a server that is already running at `url` (Streamable HTTP).
    Http,
}

/// One MCP tool an analyzer calls once per subject, with its argument mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSpec {
    /// Tool name as the server lists it.
    pub name: String,
    /// Arguments: constants (strings, numbers, booleans, lists) and templates using the
    /// placeholders in [`crate::mcp_map::Placeholder`].
    #[serde(default)]
    pub args: std::collections::BTreeMap<String, serde_json::Value>,
}

/// What one run covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerSubject {
    /// One run per skill folder.
    Skill,
    /// One run for all packages.
    Packages,
}

/// How an analyzer is started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerSpec {
    /// Runner kind.
    pub kind: RunnerKind,
    /// Executable name (looked up on PATH) or absolute path. Never a shell.
    #[serde(default)]
    pub command: Option<String>,
    /// Arguments; only [`SUBJECT_DIR`], [`OUTPUT`] and [`EMPTY_FILE`] are substituted.
    #[serde(default)]
    pub args: Vec<String>,
    /// What one run covers.
    pub per_subject: PerSubject,
    /// Arguments that print the version.
    #[serde(default)]
    pub version_args: Vec<String>,
    /// Text before the version in that output, e.g. `SkillSpector v`.
    #[serde(default)]
    pub version_prefix: Option<String>,
    /// Kill the run after this many seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Stop reading output beyond this size.
    #[serde(default = "default_output_mb")]
    pub max_output_mb: u64,
    /// Exit codes that mean the report is valid.
    #[serde(default = "default_exit_ok")]
    pub exit_ok: Vec<i32>,
    /// Exit codes that, with no report written, mean the program found nothing it could check
    /// (OSV-Scanner exits 128 when a folder declares no packages): a clean run, not a failure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exit_no_findings: Vec<i32>,
    /// MCP analyzers: `stdio` (default) or `http`.
    #[serde(default)]
    pub transport: Transport,
    /// MCP analyzers over HTTP: the Streamable HTTP endpoint.
    #[serde(default)]
    pub url: Option<String>,
    /// MCP analyzers over HTTP: the declared secret sent as `Authorization: Bearer`.
    #[serde(default)]
    pub auth_secret: Option<String>,
    /// MCP analyzers: the tools called once per skill, in order. Empty until mapped.
    #[serde(default)]
    pub tools: Vec<ToolSpec>,
    /// Older single-tool form; folded into `tools` when parsed.
    #[serde(default, skip_serializing)]
    pub tool: Option<String>,
    /// Older single-tool form; folded into `tools` when parsed.
    #[serde(default, skip_serializing)]
    pub tool_args: std::collections::BTreeMap<String, String>,
}

fn default_timeout() -> u64 {
    120
}
fn default_output_mb() -> u64 {
    8
}
fn default_exit_ok() -> Vec<i32> {
    vec![0]
}

/// What an analyzer may touch. Shown verbatim on the permissions sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    /// Readable inputs: only [`SUBJECT_DIR`] (a staged copy) in this version.
    #[serde(default)]
    pub read: Vec<String>,
    /// Hosts it contacts. Empty means it should not use the network.
    #[serde(default)]
    pub network: Vec<String>,
    /// Why it needs the network.
    #[serde(default)]
    pub network_reason: Option<String>,
    /// Plain-language list of what leaves the machine.
    #[serde(default)]
    pub sends: Vec<String>,
    /// Whether it starts an MCP server. Required (and only allowed) for MCP analyzers.
    #[serde(default)]
    pub spawns_mcp_servers: bool,
    /// Environment variable names of API keys it may receive from the OS keychain, e.g.
    /// `NVIDIA_API_KEY`. Only names are declared; values never appear in a manifest.
    #[serde(default)]
    pub secrets: Vec<String>,
}

/// Output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    /// Recognise SARIF or holdmap JSON; else apply the field mapping when `items` is set;
    /// else keep a short excerpt of the text as one detail.
    Auto,
    /// Arbitrary JSON mapped by field paths.
    JsonMap,
    /// SARIF 2.1.0.
    Sarif,
    /// holdmap's own result schema.
    HoldmapJson,
    /// OSV responses (built-in only).
    Osv,
}

/// A detail read from the report root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetailSpec {
    /// Label.
    pub label: String,
    /// Dotted path from the root.
    pub path: String,
    /// Appended to the value, e.g. ` / 100`.
    #[serde(default)]
    pub suffix: Option<String>,
}

/// How output maps to findings. Only the named fields are ever read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSpec {
    /// Format.
    pub format: OutputFormat,
    /// Path to the finding list, e.g. `$.issues[*]` (json-map).
    #[serde(default)]
    pub items: Option<String>,
    /// Field paths inside one item (json-map).
    #[serde(default)]
    pub rule_id: Option<String>,
    /// Category field.
    #[serde(default)]
    pub category: Option<String>,
    /// Severity field.
    #[serde(default)]
    pub severity: Option<String>,
    /// Title field.
    #[serde(default)]
    pub title: Option<String>,
    /// Fix field.
    #[serde(default)]
    pub fix: Option<String>,
    /// File field.
    #[serde(default)]
    pub file: Option<String>,
    /// Line field.
    #[serde(default)]
    pub line: Option<String>,
    /// The producer's stable fingerprint field.
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// Reference-URL field (string or list).
    #[serde(default)]
    pub refs: Option<String>,
    /// Root path of a `complete|partial|failed` status.
    #[serde(default)]
    pub status: Option<String>,
    /// Root path of a 0–100 coverage number.
    #[serde(default)]
    pub coverage_percent: Option<String>,
    /// Details from the root.
    #[serde(default)]
    pub details: Vec<DetailSpec>,
}

/// Why a manifest was rejected.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    /// Not valid TOML or an unknown/missing key.
    #[error("manifest is not valid: {0}")]
    Syntax(String),
    /// Valid TOML that breaks a rule.
    #[error("manifest rejected: {0}")]
    Rule(String),
}

impl AnalyzerManifest {
    /// Parse and validate a third-party manifest. Built-in runners are refused.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let manifest = Self::parse_unchecked(text)?;
        if manifest.runner.kind == RunnerKind::Builtin {
            return Err(rule("only holdmap itself can declare a built-in analyzer"));
        }
        manifest.validate()?;
        Ok(manifest)
    }

    /// Parse a manifest that ships inside holdmap (built-ins allowed).
    pub(crate) fn parse_bundled(text: &str) -> Result<Self, ManifestError> {
        let manifest = Self::parse_unchecked(text)?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn parse_unchecked(text: &str) -> Result<Self, ManifestError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            schema: u32,
            id: String,
            name: String,
            publisher: String,
            #[serde(default)]
            homepage: String,
            license: String,
            summary: String,
            #[serde(default)]
            install: Option<String>,
            runner: RunnerSpec,
            permissions: Permissions,
            output: OutputSpec,
        }
        let mut raw: Raw =
            toml::from_str(text).map_err(|e| ManifestError::Syntax(e.message().into()))?;
        if let Some(tool) = raw.runner.tool.take() {
            if !raw.runner.tools.is_empty() {
                return Err(rule("use either tool or [[runner.tools]], not both"));
            }
            let args = std::mem::take(&mut raw.runner.tool_args)
                .into_iter()
                .map(|(k, v)| (k, serde_json::Value::String(v)))
                .collect();
            raw.runner.tools.push(ToolSpec { name: tool, args });
        } else if !raw.runner.tool_args.is_empty() {
            return Err(rule("tool_args needs tool"));
        }
        Ok(Self {
            schema: raw.schema,
            id: raw.id,
            name: raw.name,
            publisher: raw.publisher,
            homepage: raw.homepage,
            license: raw.license,
            summary: raw.summary,
            install: raw.install,
            runner: raw.runner,
            permissions: raw.permissions,
            output: raw.output,
            sha256: crate::finding::hex(&Sha256::digest(text.as_bytes())),
        })
    }

    fn validate(&self) -> Result<(), ManifestError> {
        if self.schema != SCHEMA {
            return Err(rule(&format!(
                "schema {} is not supported (expected {SCHEMA})",
                self.schema
            )));
        }
        if !valid_id(&self.id) {
            return Err(rule(
                "id must be reverse-DNS: lowercase letters, digits, dots and dashes",
            ));
        }
        for (field, value, max) in [
            ("name", &self.name, 60),
            ("publisher", &self.publisher, 60),
            ("summary", &self.summary, 200),
            ("license", &self.license, 40),
        ] {
            if value.trim().is_empty() || value.chars().count() > max {
                return Err(rule(&format!("{field} must be 1–{max} characters")));
            }
        }
        if !self.homepage.is_empty() && !self.homepage.starts_with("https://") {
            return Err(rule("homepage must be an https URL"));
        }
        self.validate_runner()?;
        self.validate_permissions()?;
        self.validate_output()
    }

    fn validate_runner(&self) -> Result<(), ManifestError> {
        let r = &self.runner;
        match r.kind {
            RunnerKind::Builtin => return Ok(()),
            RunnerKind::Mcp => {
                self.validate_mcp()?;
                if r.transport == Transport::Http {
                    return self.validate_limits();
                }
            }
            RunnerKind::Cli => {
                if !r.tools.is_empty()
                    || r.url.is_some()
                    || r.auth_secret.is_some()
                    || r.transport != Transport::Stdio
                {
                    return Err(rule(
                        "tools, transport, url and auth_secret are for MCP analyzers only",
                    ));
                }
            }
        }
        let command = r.command.as_deref().unwrap_or("");
        if command.is_empty()
            || command
                .chars()
                .any(|c| c.is_whitespace() || "|&;<>$`'\"(){}*?!".contains(c))
        {
            return Err(rule(
                "command must be a single executable name or absolute path",
            ));
        }
        let base = command
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(command)
            .to_ascii_lowercase();
        if SHELLS.contains(&base.as_str()) {
            return Err(rule("command must not be a shell or launcher"));
        }
        // The version probe runs outside a run, so it gets no placeholders but these two.
        let run = r.args.iter().map(|a| (a, a.replace(EMPTY_FILE, "")));
        let version = r.version_args.iter().map(|a| (a, a.clone()));
        for (arg, rest) in run.chain(version) {
            let rest = rest.replace(SUBJECT_DIR, "").replace(OUTPUT, "");
            if rest.contains('{') || rest.contains('}') {
                return Err(rule(&format!("unknown placeholder in argument {arg:?}")));
            }
        }
        if r.kind == RunnerKind::Cli
            && (r.per_subject != PerSubject::Skill
                || !r.args.iter().any(|a| a.contains(SUBJECT_DIR)))
        {
            return Err(rule(
                "a CLI analyzer runs once per skill and must pass {subject.dir}",
            ));
        }
        self.validate_limits()
    }

    fn validate_limits(&self) -> Result<(), ManifestError> {
        let r = &self.runner;
        if r.timeout_secs == 0 || r.timeout_secs > MAX_TIMEOUT_SECS {
            return Err(rule(&format!("timeout_secs must be 1–{MAX_TIMEOUT_SECS}")));
        }
        if r.max_output_mb == 0 || r.max_output_mb > MAX_OUTPUT_MB {
            return Err(rule(&format!("max_output_mb must be 1–{MAX_OUTPUT_MB}")));
        }
        if r.exit_ok.is_empty() {
            return Err(rule("exit_ok must list at least one exit code"));
        }
        if !r.exit_no_findings.is_empty() && r.kind != RunnerKind::Cli {
            return Err(rule("exit_no_findings is for command-line analyzers"));
        }
        if r.exit_no_findings.len() > 8 || r.exit_no_findings.iter().any(|c| r.exit_ok.contains(c))
        {
            return Err(rule(
                "exit_no_findings lists up to 8 exit codes, none of them also in exit_ok",
            ));
        }
        Ok(())
    }

    fn validate_mcp(&self) -> Result<(), ManifestError> {
        let r = &self.runner;
        if r.per_subject != PerSubject::Skill {
            return Err(rule("an MCP analyzer runs once per skill"));
        }
        let remote = match r.transport {
            Transport::Stdio => {
                if r.url.is_some() || r.auth_secret.is_some() {
                    return Err(rule("url and auth_secret are for transport = \"http\""));
                }
                if let Some(arg) = r.args.iter().find(|a| a.contains('{') || a.contains('}')) {
                    return Err(rule(&format!(
                        "MCP server arguments take no placeholders (found {arg:?}); map inputs in [[runner.tools]]"
                    )));
                }
                if !self.permissions.spawns_mcp_servers {
                    return Err(rule(
                        "an MCP analyzer started from a command starts a server: set permissions.spawns_mcp_servers = true",
                    ));
                }
                false
            }
            Transport::Http => self.validate_http()?,
        };
        if r.tools.len() > MAX_TOOLS {
            return Err(rule(&format!(
                "an MCP analyzer may call at most {MAX_TOOLS} tools"
            )));
        }
        let mut names: Vec<&str> = Vec::new();
        for t in &r.tools {
            if !valid_tool_name(&t.name) {
                return Err(rule("tool names must be 1–128 letters, digits, _ - or ."));
            }
            if names.contains(&t.name.as_str()) {
                return Err(rule(&format!("tool {:?} is listed twice", t.name)));
            }
            names.push(&t.name);
            if t.args.len() > crate::mcp_map::MAX_ARGS {
                return Err(rule(&format!(
                    "tool {:?} may have at most {} arguments",
                    t.name,
                    crate::mcp_map::MAX_ARGS
                )));
            }
            for (key, value) in &t.args {
                if key.is_empty() || key.len() > 64 {
                    return Err(rule(&format!(
                        "tool {:?}: argument names are 1–64 characters",
                        t.name
                    )));
                }
                crate::mcp_map::check_template(value, remote)
                    .map_err(|why| rule(&format!("tool {:?} argument {key:?}: {why}", t.name)))?;
            }
        }
        Ok(())
    }

    fn validate_http(&self) -> Result<bool, ManifestError> {
        let r = &self.runner;
        if r.command.is_some() || !r.args.is_empty() || !r.version_args.is_empty() {
            return Err(rule(
                "an HTTP MCP analyzer connects to url; command and args are for stdio",
            ));
        }
        if self.permissions.spawns_mcp_servers {
            return Err(rule(
                "an HTTP MCP analyzer doesn't start a server: set spawns_mcp_servers = false",
            ));
        }
        let url = r.url.as_deref().unwrap_or("");
        let endpoint = Endpoint::parse(url).ok_or_else(|| {
            rule("url must be an http(s) URL without credentials, like http://127.0.0.1:8080/mcp")
        })?;
        let remote = !endpoint.loopback;
        if remote {
            if !endpoint.https {
                return Err(rule("a server on another machine must use https"));
            }
            if !self.permissions.network.iter().any(|h| h == &endpoint.host) {
                return Err(rule(&format!(
                    "a server on another machine must be listed in permissions.network ({})",
                    endpoint.host
                )));
            }
        }
        if let Some(name) = &r.auth_secret {
            if !self.permissions.secrets.contains(name) {
                return Err(rule("auth_secret must be one of permissions.secrets"));
            }
        }
        if self
            .permissions
            .secrets
            .iter()
            .any(|n| Some(n) != r.auth_secret.as_ref())
        {
            return Err(rule(
                "an HTTP MCP analyzer gets no environment; its only secret is auth_secret",
            ));
        }
        Ok(remote)
    }

    /// The HTTP endpoint, for HTTP MCP analyzers.
    pub fn endpoint(&self) -> Option<Endpoint> {
        (self.runner.kind == RunnerKind::Mcp && self.runner.transport == Transport::Http)
            .then(|| self.runner.url.as_deref().and_then(Endpoint::parse))
            .flatten()
    }

    /// Whether this is an HTTP MCP analyzer on another machine (metadata-only, opt-in).
    pub fn is_remote(&self) -> bool {
        self.endpoint().is_some_and(|e| !e.loopback)
    }

    fn validate_permissions(&self) -> Result<(), ManifestError> {
        let p = &self.permissions;
        if p.spawns_mcp_servers && self.runner.kind != RunnerKind::Mcp {
            return Err(rule("only MCP analyzers may start an MCP server"));
        }
        if p.read.iter().any(|r| r != SUBJECT_DIR) {
            return Err(rule("read may only list {subject.dir}"));
        }
        if let Some(host) = p.network.iter().find(|h| !valid_host(h)) {
            return Err(rule(&format!(
                "network entry {host:?} must be a plain host name"
            )));
        }
        if !p.network.is_empty() && p.network_reason.as_deref().unwrap_or("").trim().is_empty() {
            return Err(rule("network_reason is required when network is not empty"));
        }
        if p.secrets.len() > crate::secrets::MAX_SECRETS {
            return Err(rule(&format!(
                "secrets may list at most {} names",
                crate::secrets::MAX_SECRETS
            )));
        }
        if let Some(name) = p.secrets.iter().find(|n| !crate::secrets::valid_name(n)) {
            return Err(rule(&format!(
                "secret {name:?} must be an upper-case environment variable name that holdmap doesn't set itself"
            )));
        }
        let mut names = p.secrets.clone();
        names.sort();
        names.dedup();
        if names.len() != p.secrets.len() {
            return Err(rule("secrets must not repeat a name"));
        }
        if !p.network.is_empty() && p.sends.is_empty() {
            return Err(rule(
                "sends must say what leaves the machine when network is not empty",
            ));
        }
        Ok(())
    }

    fn validate_output(&self) -> Result<(), ManifestError> {
        let o = &self.output;
        match o.format {
            OutputFormat::Auto if o.items.is_some() => {
                let items = o.items.as_deref().unwrap_or("");
                if !items.starts_with("$.") || !items.ends_with("[*]") || o.rule_id.is_none() {
                    return Err(rule(
                        "a field mapping needs items like \"$.issues[*]\" and rule_id",
                    ));
                }
            }
            OutputFormat::JsonMap => {
                let items = o.items.as_deref().unwrap_or("");
                if !items.starts_with("$.") || !items.ends_with("[*]") {
                    return Err(rule("json-map needs items like \"$.issues[*]\""));
                }
                if o.rule_id.is_none() || o.severity.is_none() {
                    return Err(rule("json-map needs rule_id and severity"));
                }
            }
            OutputFormat::Osv if self.runner.kind != RunnerKind::Builtin => {
                return Err(rule("the osv format is built-in only"))
            }
            _ => {}
        }
        Ok(())
    }

    /// Whether the analyzer may use the network at all.
    pub fn uses_network(&self) -> bool {
        !self.permissions.network.is_empty()
    }
}

/// A parsed MCP endpoint URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// `https`.
    pub https: bool,
    /// Host without port or brackets, lower-case.
    pub host: String,
    /// Whether the host is this machine (`localhost`, `127.0.0.0/8`, `::1`).
    pub loopback: bool,
}

impl Endpoint {
    /// Parse `http(s)://host[:port][/path]`; credentials, fragments and spaces are refused.
    pub fn parse(url: &str) -> Option<Self> {
        if url.len() > 512 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return None;
        }
        let (https, rest) = if let Some(r) = url.strip_prefix("https://") {
            (true, r)
        } else {
            (false, url.strip_prefix("http://")?)
        };
        if url.contains('#') {
            return None;
        }
        let authority = rest.split(['/', '?']).next().unwrap_or("");
        if authority.is_empty() || authority.contains('@') {
            return None;
        }
        let host = if let Some(v6) = authority.strip_prefix('[') {
            let (h, port) = v6.split_once(']')?;
            if !(port.is_empty() || port.strip_prefix(':').is_some_and(valid_port)) {
                return None;
            }
            h.to_ascii_lowercase()
        } else {
            let (h, port) = authority.split_once(':').unwrap_or((authority, ""));
            if !(port.is_empty() && !authority.ends_with(':') || valid_port(port)) {
                return None;
            }
            h.to_ascii_lowercase()
        };
        if host.is_empty()
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
        {
            return None;
        }
        let loopback = host == "localhost"
            || host == "::1"
            || host
                .parse::<std::net::Ipv4Addr>()
                .is_ok_and(|ip| ip.is_loopback());
        Some(Self {
            https,
            host,
            loopback,
        })
    }
}

fn valid_port(p: &str) -> bool {
    !p.is_empty() && p.len() <= 5 && p.parse::<u16>().is_ok_and(|n| n > 0)
}

fn valid_tool_name(name: &str) -> bool {
    (1..=128).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn rule(message: &str) -> ManifestError {
    ManifestError::Rule(message.into())
}

fn valid_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    id.len() <= 64
        && parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

fn valid_host(host: &str) -> bool {
    host.len() <= 253
        && host.contains('.')
        && host
            .split('.')
            .all(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKILLSPECTOR: &str = include_str!("../manifests/nvidia.skillspector.toml");

    fn with(find: &str, replace: &str) -> Result<AnalyzerManifest, ManifestError> {
        assert!(SKILLSPECTOR.contains(find), "{find}");
        AnalyzerManifest::parse(&SKILLSPECTOR.replacen(find, replace, 1))
    }

    #[test]
    fn the_bundled_skillspector_manifest_is_valid_and_pinned_by_hash() {
        let m = AnalyzerManifest::parse(SKILLSPECTOR).unwrap();
        assert_eq!(m.id, "nvidia.skillspector");
        assert!(
            m.runner.args.contains(&"--no-llm".to_string()),
            "static by default"
        );
        assert_eq!(m.permissions.network, vec!["api.osv.dev"]);
        assert_eq!(m.sha256.len(), 64);
        let changed = AnalyzerManifest::parse(&SKILLSPECTOR.replace("120", "121")).unwrap();
        assert_ne!(m.sha256, changed.sha256);
    }

    #[test]
    fn exit_no_findings_codes_are_few_and_never_also_valid_report_codes() {
        let ok = with(
            "exit_ok = [0, 1]",
            "exit_ok = [0, 1]\nexit_no_findings = [128]",
        )
        .unwrap();
        assert_eq!(ok.runner.exit_no_findings, vec![128]);
        let overlap = with(
            "exit_ok = [0, 1]",
            "exit_ok = [0, 1]\nexit_no_findings = [1]",
        );
        assert!(overlap
            .unwrap_err()
            .to_string()
            .contains("exit_no_findings"));
        let many = with(
            "exit_ok = [0, 1]",
            "exit_ok = [0, 1]\nexit_no_findings = [2, 3, 4, 5, 6, 7, 8, 9, 10]",
        );
        assert!(many.is_err());
    }

    #[test]
    fn shells_launchers_and_metacharacters_are_rejected() {
        for cmd in ["bash", "/bin/sh", "pwsh.exe", "env", "sudo"] {
            let err = with(
                "command = \"skillspector\"",
                &format!("command = \"{cmd}\""),
            )
            .unwrap_err();
            assert!(err.to_string().contains("shell"), "{cmd}: {err}");
        }
        assert!(with("command = \"skillspector\"", "command = \"a;b\"").is_err());
        assert!(with("command = \"skillspector\"", "command = \"a b\"").is_err());
    }

    #[test]
    fn only_known_placeholders_are_allowed() {
        let err = with("\"--no-llm\"", "\"{env.HOME}\"").unwrap_err();
        assert!(err.to_string().contains("placeholder"), "{err}");
        // {empty.file} is for run arguments; the version probe has no run folder.
        assert!(with("\"--no-llm\"", "\"--config\", \"{empty.file}\"").is_ok());
        let err = with("version_args = [", "version_args = [\"{empty.file}\", ").unwrap_err();
        assert!(err.to_string().contains("placeholder"), "{err}");
    }

    #[test]
    fn unknown_keys_and_v1_unsupported_features_are_rejected() {
        assert!(matches!(
            with("license = ", "secret = 1\nlicense = "),
            Err(ManifestError::Syntax(_))
        ));
        assert!(with("kind = \"cli\"", "kind = \"mcp\"")
            .unwrap_err()
            .to_string()
            .contains("MCP"));
        assert!(with("kind = \"cli\"", "kind = \"builtin\"").is_err());
        assert!(with("spawns_mcp_servers = false", "spawns_mcp_servers = true").is_err());
        assert!(with("read = [\"{subject.dir}\"]", "read = [\"~/.ssh\"]").is_err());
        assert!(with(
            "network = [\"api.osv.dev\"]",
            "network = [\"https://x.dev/path\"]"
        )
        .is_err());
        assert!(with("schema = 1", "schema = 2").is_err());
        assert!(with("timeout_secs = 120", "timeout_secs = 0").is_err());
        assert!(with("max_output_mb = 8", "max_output_mb = 999").is_err());
    }

    #[test]
    fn network_needs_a_reason_and_a_sends_list() {
        let no_reason = SKILLSPECTOR
            .lines()
            .filter(|l| !l.starts_with("network_reason"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(AnalyzerManifest::parse(&no_reason).is_err());
        assert!(with(
            "sends = [\"Package names and versions found in the skill go to api.osv.dev.\"]",
            "sends = []"
        )
        .is_err());
    }

    #[test]
    fn ids_must_be_reverse_dns() {
        assert!(valid_id("nvidia.skillspector"));
        assert!(!valid_id("SkillSpector"));
        assert!(!valid_id("a..b"));
        assert!(!valid_id("x"));
    }

    #[test]
    fn secrets_are_declared_by_name_only() {
        let m = with(
            "spawns_mcp_servers = false",
            "spawns_mcp_servers = false\nsecrets = [\"NVIDIA_API_KEY\"]",
        )
        .unwrap();
        assert_eq!(m.permissions.secrets, ["NVIDIA_API_KEY"]);
        for bad in [
            "\"PATH\"",
            "\"lower\"",
            "\"LD_PRELOAD\"",
            "\"A\", \"A\"",
            "\"A\",\"B\",\"C\",\"D\",\"E\"",
        ] {
            let err = with(
                "spawns_mcp_servers = false",
                &format!("spawns_mcp_servers = false\nsecrets = [{bad}]"),
            )
            .unwrap_err();
            assert!(err.to_string().contains("secret"), "{bad}: {err}");
        }
    }
}

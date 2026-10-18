//! The MCP side of [`AnalyzerService`]: registering any MCP server as an analyzer (from a
//! command, a URL or one of your agents' configured servers), listing and pinning its tools,
//! suggesting and saving the input mapping, previewing the exact calls, and approval rules
//! (tools chosen, listing current, arguments fit, remote servers opted in).

use super::{AnalyzerService, AnalyzerView, ServiceError, ToolCallPreview};
use crate::consent::{Grant, ToolSlot};
use crate::input::AnalyzerInput;
use crate::manifest::{AnalyzerManifest, ManifestError, RunnerKind, ToolSpec, Transport};
use crate::mcp_map::{self, OutputMapping, Placeholder, SubjectData, Suggestion};
use crate::mcp_runner::endpoint_label;
use crate::mcp_tools::{self, ServerInfo, ToolSnapshot};
use crate::runner::{RunContext, RunError};
use holdmap_core::agents::{AgentsReport, McpRunner as Launcher, McpTransport};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Subjects shown in the call preview.
const PREVIEW_SUBJECTS: usize = 3;

/// MCP details on an analyzer view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpView {
    /// `stdio` or `http`.
    pub transport: Transport,
    /// The command line or URL.
    pub endpoint: String,
    /// A server on another machine (metadata only, opt-in).
    pub remote: bool,
    /// The user allowed the remote server when approving.
    pub remote_opt_in: bool,
    /// Chosen tools, in call order.
    pub tools: Vec<String>,
    /// The latest tool listing, if any.
    pub listing: Option<ToolListing>,
    /// No tools chosen yet.
    pub needs_tools: bool,
}

/// A tool listing, in brief.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolListing {
    /// Tools listed.
    pub count: usize,
    /// Server identity.
    pub server: Option<ServerInfo>,
    /// When.
    pub listed_at_ms: u64,
    /// First 12 hex digits of the pin.
    pub pin: String,
    /// Tools with poisoning flags (plus the server's instructions, counted as one).
    pub flagged: usize,
    /// It is the listing the user approved.
    pub approved: bool,
}

/// Everything the tool picker shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolReport {
    /// The listing.
    pub snapshot: ToolSnapshot,
    /// Poisoning flags by tool name.
    pub flags: BTreeMap<String, Vec<String>>,
    /// Flags for the server's instructions.
    pub server_flags: Vec<String>,
    /// A suggested mapping for every tool.
    pub suggestions: Vec<Suggestion>,
    /// What changed since the approved listing.
    pub changes: Vec<String>,
    /// It is the approved listing.
    pub approved: bool,
    /// The tools currently chosen, with their mapping.
    pub chosen: Vec<ToolSpec>,
    /// Placeholders this analyzer may use (remote servers get metadata only).
    pub placeholders: Vec<PlaceholderInfo>,
    /// The saved output mapping (empty: auto-detect, else a short detail).
    pub output: OutputMapping,
    /// Output mappings suggested from tools' output schemas, by tool name.
    pub output_suggestions: BTreeMap<String, OutputMapping>,
}

/// One placeholder offered in the mapping editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceholderInfo {
    /// Token, e.g. `{subject.dir}`.
    pub token: String,
    /// Plain label.
    pub label: String,
    /// Whether it expands to a list.
    pub list: bool,
}

/// What the user fills in to register an MCP analyzer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpDraft {
    /// Reverse-DNS id, e.g. `local.my-scanner`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// One line about it.
    #[serde(default)]
    pub summary: Option<String>,
    /// `stdio` (start a command) or `http` (connect to a URL).
    #[serde(default)]
    pub transport: Transport,
    /// Program (stdio).
    #[serde(default)]
    pub command: Option<String>,
    /// Its arguments (stdio).
    #[serde(default)]
    pub args: Vec<String>,
    /// Streamable HTTP endpoint (http).
    #[serde(default)]
    pub url: Option<String>,
    /// API key names it may receive from the keychain (stdio: environment variables).
    #[serde(default)]
    pub secrets: Vec<String>,
    /// The key sent as a bearer token (http).
    #[serde(default)]
    pub auth_secret: Option<String>,
    /// Hosts a stdio server contacts.
    #[serde(default)]
    pub network: Vec<String>,
    /// Why.
    #[serde(default)]
    pub network_reason: Option<String>,
    /// Time limit for one run.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// One MCP server from your agents' configuration, offered as a starting point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfiguredServer {
    /// Stable id of the configured entry.
    pub id: String,
    /// Agent product.
    pub agent: String,
    /// Configured name.
    pub name: String,
    /// `stdio`, `HTTP`, `SSE` or `unknown`.
    pub transport: String,
    /// A ready draft, when holdmap knows enough to start or reach it.
    pub draft: Option<McpDraft>,
    /// Why there is no draft, or what to check.
    pub note: Option<String>,
}

impl AnalyzerService {
    pub(super) fn mcp_view(&self, m: &AnalyzerManifest, grant: Option<&Grant>) -> Option<McpView> {
        if m.runner.kind != RunnerKind::Mcp {
            return None;
        }
        let latest = self.stores.tools.tool_snapshot(&m.id, ToolSlot::Latest);
        Some(McpView {
            transport: m.runner.transport,
            endpoint: endpoint_label(m),
            remote: m.is_remote(),
            remote_opt_in: grant.is_some_and(|g| g.remote_opt_in),
            tools: m.runner.tools.iter().map(|t| t.name.clone()).collect(),
            needs_tools: m.runner.tools.is_empty(),
            listing: latest.map(|s| ToolListing {
                count: s.tools.len(),
                flagged: s
                    .tools
                    .iter()
                    .filter(|t| !mcp_tools::flags(t).is_empty())
                    .count()
                    + usize::from(s.server.as_ref().is_some_and(|i| !i.flags().is_empty())),
                approved: grant.and_then(|g| g.tools_sha256.as_deref()) == Some(s.sha256.as_str()),
                server: s.server.clone(),
                listed_at_ms: s.listed_at_ms,
                pin: s.sha256.chars().take(12).collect(),
            }),
        })
    }

    /// Why an approved MCP analyzer's tools no longer match, if they don't.
    pub(super) fn tool_pin_change(
        &self,
        m: &AnalyzerManifest,
        grant: Option<&Grant>,
    ) -> Option<Vec<String>> {
        let pin = grant?.tools_sha256.as_deref();
        let latest = self.stores.tools.tool_snapshot(&m.id, ToolSlot::Latest);
        match (pin, latest) {
            (None, _) => Some(vec!["it has no approved tool list".into()]),
            (Some(p), Some(latest)) if latest.sha256 != p => Some(
                self.stores
                    .tools
                    .tool_snapshot(&m.id, ToolSlot::Approved)
                    .map(|a| latest.changes_since(&a))
                    .filter(|c| !c.is_empty())
                    .unwrap_or_else(|| vec!["its tool list changed".into()]),
            ),
            _ => None,
        }
    }

    /// Save a listing that differs from the approved one; returns what changed.
    pub(super) fn record_changed_tools(
        &self,
        m: &AnalyzerManifest,
        snapshot: &ToolSnapshot,
    ) -> Result<Vec<String>, ServiceError> {
        self.stores
            .tools
            .set_tool_snapshot(&m.id, ToolSlot::Latest, Some(snapshot))?;
        Ok(self
            .stores
            .tools
            .tool_snapshot(&m.id, ToolSlot::Approved)
            .map(|a| snapshot.changes_since(&a))
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| vec!["its tool list changed".into()]))
    }

    /// Approval rules for MCP analyzers; returns the pin to record.
    pub(super) fn check_mcp_approval(
        &self,
        m: &AnalyzerManifest,
        allow_remote: bool,
    ) -> Result<Option<String>, ServiceError> {
        if m.runner.kind != RunnerKind::Mcp {
            return Ok(None);
        }
        let refuse = |why: String| -> ServiceError { RunError::NotApproved(why).into() };
        if m.runner.tools.is_empty() {
            return Err(refuse(format!(
                "Choose which of {}'s tools to use first.",
                m.name
            )));
        }
        let snapshot = self
            .stores
            .tools
            .tool_snapshot(&m.id, ToolSlot::Latest)
            .filter(|s| s.endpoint == endpoint_label(m))
            .ok_or_else(|| {
                refuse(format!(
                    "List {}'s tools first, so you can review them.",
                    m.name
                ))
            })?;
        for spec in &m.runner.tools {
            let tool = snapshot.tool(&spec.name).ok_or_else(|| {
                refuse(format!(
                    "The server doesn't list the tool {:?}. List its tools again.",
                    spec.name
                ))
            })?;
            let missing: Vec<String> =
                mcp_map::check(&sample_args(spec, &tool.input_schema), &tool.input_schema);
            if !missing.is_empty() {
                return Err(refuse(format!(
                    "The mapping for {} doesn't fit its inputs: {}.",
                    spec.name,
                    missing.join("; ")
                )));
            }
        }
        if m.is_remote() && !allow_remote {
            return Err(refuse(format!(
                "{} is on another machine. Allow sending metadata there to approve it.",
                m.name
            )));
        }
        self.stores
            .tools
            .set_tool_snapshot(&m.id, ToolSlot::Approved, Some(&snapshot))?;
        Ok(Some(snapshot.sha256))
    }

    /// The exact calls for the first few subjects (MCP analyzers with a listing).
    pub(super) fn tool_call_preview(
        &self,
        m: &AnalyzerManifest,
        input: &AnalyzerInput,
    ) -> Vec<ToolCallPreview> {
        if m.runner.kind != RunnerKind::Mcp {
            return Vec::new();
        }
        let snapshot = self.stores.tools.tool_snapshot(&m.id, ToolSlot::Latest);
        let needs: Vec<Placeholder> = m
            .runner
            .tools
            .iter()
            .flat_map(|t| t.args.values().flat_map(mcp_map::used))
            .collect();
        let mut out = Vec::new();
        for s in input.subjects.iter().take(PREVIEW_SUBJECTS) {
            let Some(dir) = s.dir.as_deref() else {
                continue;
            };
            let mut data = SubjectData::read(dir, &s.subject.name, &s.subject.agents, &needs);
            data.dir = Some(format!("<private copy of {}>", s.subject.name));
            data.skill_md = data
                .skill_md
                .map(|t| format!("<text of SKILL.md, {} characters>", t.chars().count()));
            for spec in &m.runner.tools {
                let schema = snapshot
                    .as_ref()
                    .and_then(|sn| sn.tool(&spec.name))
                    .map(|t| t.input_schema.clone())
                    .unwrap_or(serde_json::Value::Null);
                let args = mcp_map::build(&spec.args, &data, &schema);
                out.push(ToolCallPreview {
                    tool: spec.name.clone(),
                    subject: s.subject.name.clone(),
                    problems: mcp_map::check(&args, &schema),
                    arguments: serde_json::Value::Object(args),
                });
            }
        }
        out
    }

    /// Register an MCP server as an analyzer. It stays off until its tools are listed, chosen
    /// and approved.
    pub fn add_mcp(&self, draft: &McpDraft) -> Result<AnalyzerView, ServiceError> {
        self.add_manifest(&render(draft)?)
    }

    /// Your agents' configured MCP servers, with a draft where holdmap knows enough.
    pub fn configured_servers(&self, report: &AgentsReport) -> Vec<ConfiguredServer> {
        let mut out: Vec<ConfiguredServer> = Vec::new();
        for (agent, caps) in crate::input::inventories(report) {
            for server in &caps.mcp_servers {
                if out.iter().any(|o| o.id == server.id) {
                    continue;
                }
                let (draft, note) = draft_for(agent, server);
                out.push(ConfiguredServer {
                    id: server.id.clone(),
                    agent: agent.to_string(),
                    name: server.name.clone(),
                    transport: server.transport.label().to_string(),
                    draft,
                    note,
                });
            }
        }
        out
    }

    /// Start or reach the server, list and pin its tools (nothing is called), and save the
    /// listing for review.
    pub fn list_tools(&self, id: &str) -> Result<ToolReport, ServiceError> {
        let m = &self.manifest(id)?;
        if m.runner.kind != RunnerKind::Mcp {
            return Err(ManifestError::Rule(format!("{} is not an MCP analyzer", m.name)).into());
        }
        let mut cx = RunContext::default();
        if m.runner.transport == Transport::Stdio {
            let info = self
                .probe
                .locate(m)
                .ok_or_else(|| RunError::Unavailable(format!("{} is not installed.", m.name)))?;
            cx.binary = Some(info.path);
        }
        cx.secrets = self.run_secrets(m).0;
        let snapshot = self.mcp.list_tools(m, &cx)?;
        self.stores
            .tools
            .set_tool_snapshot(&m.id, ToolSlot::Latest, Some(&snapshot))?;
        self.tools(id)
    }

    /// The saved tool listing (nothing is started).
    pub fn tools(&self, id: &str) -> Result<ToolReport, ServiceError> {
        let m = &self.manifest(id)?;
        let snapshot = self
            .stores
            .tools
            .tool_snapshot(&m.id, ToolSlot::Latest)
            .ok_or_else(|| {
                RunError::NotApproved(format!("{}'s tools haven't been listed yet.", m.name))
            })?;
        let approved = self.stores.tools.tool_snapshot(&m.id, ToolSlot::Approved);
        let remote = m.is_remote();
        Ok(ToolReport {
            flags: snapshot
                .tools
                .iter()
                .map(|t| (t.name.clone(), mcp_tools::flags(t)))
                .filter(|(_, f)| !f.is_empty())
                .collect(),
            server_flags: snapshot
                .server
                .as_ref()
                .map(ServerInfo::flags)
                .unwrap_or_default(),
            suggestions: snapshot
                .tools
                .iter()
                .map(|t| mcp_map::suggest(t, remote))
                .collect(),
            changes: approved
                .as_ref()
                .map(|a| snapshot.changes_since(a))
                .unwrap_or_default(),
            approved: self
                .stores
                .consent
                .grant(&m.id)
                .and_then(|g| g.tools_sha256)
                .is_some_and(|p| p == snapshot.sha256),
            chosen: m.runner.tools.clone(),
            output: OutputMapping {
                items: m.output.items.clone(),
                rule_id: m.output.rule_id.clone(),
                severity: m.output.severity.clone(),
                title: m.output.title.clone(),
                file: m.output.file.clone(),
                line: m.output.line.clone(),
                fix: m.output.fix.clone(),
            },
            output_suggestions: snapshot
                .tools
                .iter()
                .filter_map(|t| Some((t.name.clone(), mcp_map::suggest_output(t)?)))
                .collect(),
            placeholders: Placeholder::ALL
                .into_iter()
                .filter(|p| !(remote && p.is_content()))
                .map(|p| PlaceholderInfo {
                    token: p.token().into(),
                    label: p.label().into(),
                    list: p.is_list(),
                })
                .collect(),
            snapshot,
        })
    }

    /// Save which tools to call and how their inputs map. The analyzer needs approval again.
    pub fn map_tools(&self, id: &str, tools: &[ToolSpec]) -> Result<AnalyzerView, ServiceError> {
        self.edit_user_mcp(id, |doc| with_tools(doc, tools))
    }

    /// Save where findings sit in the tools' JSON output (an empty mapping goes back to
    /// auto-detection). The analyzer needs approval again.
    pub fn map_output(
        &self,
        id: &str,
        output: &OutputMapping,
    ) -> Result<AnalyzerView, ServiceError> {
        self.edit_user_mcp(id, |doc| with_output(doc, output))
    }

    fn edit_user_mcp(
        &self,
        id: &str,
        edit: impl FnOnce(&mut toml::Table) -> Result<(), ServiceError>,
    ) -> Result<AnalyzerView, ServiceError> {
        let m = &self.manifest(id)?;
        if !self.is_user(&m.id) || m.runner.kind != RunnerKind::Mcp {
            return Err(ManifestError::Rule(format!(
                "{} is not an MCP analyzer you added",
                m.name
            ))
            .into());
        }
        let text = self
            .stores
            .manifests
            .user_manifests()
            .into_iter()
            .find(|t| AnalyzerManifest::parse(t).is_ok_and(|p| p.id == m.id))
            .ok_or_else(|| ServiceError::Unknown(m.id.clone()))?;
        let mut doc: toml::Table =
            toml::from_str(&text).map_err(|_| bad("the saved manifest can't be read"))?;
        edit(&mut doc)?;
        let updated = toml::to_string(&doc).map_err(|_| bad("the manifest can't be saved"))?;
        AnalyzerManifest::parse(&updated)?;
        self.stores.manifests.add_user_manifest(&m.id, &updated)?;
        self.get(&m.id)
    }
}

fn bad(why: &str) -> ServiceError {
    ManifestError::Rule(why.into()).into()
}

/// Arguments for the approval check: placeholders filled with sample values.
fn sample_args(
    spec: &ToolSpec,
    schema: &serde_json::Value,
) -> serde_json::Map<String, serde_json::Value> {
    let data = SubjectData {
        dir: Some("/sample".into()),
        skill_md: Some("sample".into()),
        files: vec!["SKILL.md".into()],
        name: "sample".into(),
        agents: vec!["sample".into()],
    };
    mcp_map::build(&spec.args, &data, schema)
}

fn draft_for(
    agent: &str,
    server: &holdmap_core::agents::McpServerCapability,
) -> (Option<McpDraft>, Option<String>) {
    let slug: String = server
        .name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let secrets: Vec<String> = server
        .env_names
        .iter()
        .filter(|n| crate::secrets::valid_name(n))
        .take(crate::secrets::MAX_SECRETS)
        .cloned()
        .collect();
    let base = McpDraft {
        id: format!(
            "local.{}",
            if slug.is_empty() { "mcp".into() } else { slug }
        ),
        name: server.name.chars().take(60).collect(),
        summary: Some(format!(
            "The {} MCP server from {agent}'s configuration.",
            server.name
        )),
        secrets,
        ..McpDraft::default()
    };
    match server.transport {
        McpTransport::Stdio => {
            let Some(launch) = &server.launch else {
                return (None, Some("holdmap doesn't know how this server is started. Add it from its command.".into()));
            };
            let package = launch.package.clone().unwrap_or_default();
            let unpinned = (launch.pinned == Some(false))
                .then(|| "It isn't pinned to a version, so each run may start a different version. Pin it in the command if you can.".to_string());
            match launch.runner {
                Launcher::Npx if !package.is_empty() => (
                    Some(McpDraft {
                        command: Some("npx".into()),
                        args: vec!["-y".into(), package],
                        ..base
                    }),
                    unpinned,
                ),
                Launcher::Uvx if !package.is_empty() => (
                    Some(McpDraft {
                        command: Some("uvx".into()),
                        args: vec![package],
                        ..base
                    }),
                    unpinned,
                ),
                Launcher::Binary if !package.is_empty() => (
                    Some(McpDraft {
                        command: Some(package),
                        ..base
                    }),
                    Some("holdmap doesn't keep this server's arguments. Add any it needs.".into()),
                ),
                _ => (
                    None,
                    Some("holdmap doesn't keep this server's full command. Add it from its command.".into()),
                ),
            }
        }
        McpTransport::Http => match &server.remote_host {
            Some(host) => (
                Some(McpDraft {
                    transport: Transport::Http,
                    url: Some(format!("http://{host}/mcp")),
                    secrets: Vec::new(),
                    ..base
                }),
                Some("holdmap keeps only the host. Check the URL path (often /mcp) and the scheme before adding it.".into()),
            ),
            None => (None, Some("holdmap doesn't know this server's address.".into())),
        },
        _ => (None, Some("Only stdio and Streamable HTTP servers can be analyzers.".into())),
    }
}

fn quote(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

fn list(items: &[String]) -> String {
    format!(
        "[{}]",
        items
            .iter()
            .map(|s| quote(s))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// The manifest text for a draft (validated when added).
pub fn render(d: &McpDraft) -> Result<String, ServiceError> {
    let mut out = format!(
        "schema = 1\nid = {}\nname = {}\npublisher = \"unknown\"\nlicense = \"unknown\"\nsummary = {}\n\n[runner]\nkind = \"mcp\"\n",
        quote(d.id.trim()),
        quote(d.name.trim()),
        quote(d.summary.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or("An MCP server you added as an analyzer.")),
    );
    let mut network = d.network.clone();
    let mut reason = d.network_reason.clone();
    let mut sends: Vec<String> = Vec::new();
    match d.transport {
        Transport::Stdio => {
            let command = d
                .command
                .as_deref()
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .ok_or_else(|| bad("a command is required"))?;
            out += &format!(
                "transport = \"stdio\"\ncommand = {}\nargs = {}\n",
                quote(command),
                list(&d.args)
            );
        }
        Transport::Http => {
            let url = d.url.as_deref().map(str::trim).unwrap_or("");
            let endpoint = crate::manifest::Endpoint::parse(url)
                .ok_or_else(|| bad("a valid http(s) URL is required"))?;
            out += &format!("transport = \"http\"\nurl = {}\n", quote(url));
            if let Some(a) = &d.auth_secret {
                out += &format!("auth_secret = {}\n", quote(a));
            }
            if !endpoint.loopback {
                if !network.contains(&endpoint.host) {
                    network.push(endpoint.host.clone());
                }
                reason
                    .get_or_insert_with(|| "It is the MCP server you chose to connect to.".into());
                sends.push(format!(
                    "Skill names, file names and agent ids go to {}.",
                    endpoint.host
                ));
            }
        }
    }
    if !network.is_empty() && sends.is_empty() {
        sends.push(format!(
            "Whatever the server sends to {}.",
            network.join(", ")
        ));
    }
    out += &format!(
        "per_subject = \"skill\"\ntimeout_secs = {}\n\n[permissions]\n",
        d.timeout_secs.unwrap_or(120)
    );
    if d.transport == Transport::Stdio {
        out += "read = [\"{subject.dir}\"]\nspawns_mcp_servers = true\n";
    } else {
        out += "spawns_mcp_servers = false\n";
    }
    if !network.is_empty() {
        out += &format!("network = {}\n", list(&network));
        out += &format!(
            "network_reason = {}\n",
            quote(reason.as_deref().unwrap_or(""))
        );
        out += &format!("sends = {}\n", list(&sends));
    }
    let secrets: Vec<String> = match d.transport {
        Transport::Stdio => d.secrets.clone(),
        Transport::Http => d.auth_secret.iter().cloned().collect(),
    };
    if !secrets.is_empty() {
        out += &format!("secrets = {}\n", list(&secrets));
    }
    out += "\n[output]\nformat = \"auto\"\n";
    Ok(out)
}

/// Replace the tool list in a manifest.
fn with_tools(doc: &mut toml::Table, tools: &[ToolSpec]) -> Result<(), ServiceError> {
    let runner = doc
        .get_mut("runner")
        .and_then(toml::Value::as_table_mut)
        .ok_or_else(|| bad("the saved manifest has no [runner]"))?;
    runner.remove("tool");
    runner.remove("tool_args");
    let mut list = Vec::new();
    for t in tools {
        let mut table = toml::Table::new();
        table.insert("name".into(), toml::Value::String(t.name.clone()));
        let mut args = toml::Table::new();
        for (k, v) in &t.args {
            let value =
                toml::Value::try_from(v).map_err(|_| bad("an argument value can't be saved"))?;
            args.insert(k.clone(), value);
        }
        table.insert("args".into(), toml::Value::Table(args));
        list.push(toml::Value::Table(table));
    }
    runner.insert("tools".into(), toml::Value::Array(list));
    Ok(())
}

/// Replace the output section with `format = "auto"` and the mapping.
fn with_output(doc: &mut toml::Table, o: &OutputMapping) -> Result<(), ServiceError> {
    let mut table = toml::Table::new();
    table.insert("format".into(), toml::Value::String("auto".into()));
    if !o.is_empty() {
        let fields = [
            ("items", &o.items),
            ("rule_id", &o.rule_id),
            ("severity", &o.severity),
            ("title", &o.title),
            ("file", &o.file),
            ("line", &o.line),
            ("fix", &o.fix),
        ];
        for (k, v) in fields {
            if let Some(v) = v.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                table.insert(k.into(), toml::Value::String(v.into()));
            }
        }
    }
    doc.insert("output".into(), toml::Value::Table(table));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn drafts_render_to_valid_manifests() {
        let stdio = McpDraft {
            id: "local.scan".into(),
            name: "Scan".into(),
            command: Some("scan-mcp".into()),
            args: vec!["--stdio".into()],
            secrets: vec!["SCAN_KEY".into()],
            ..McpDraft::default()
        };
        let m = AnalyzerManifest::parse(&render(&stdio).unwrap()).unwrap();
        assert_eq!(m.runner.transport, Transport::Stdio);
        assert!(m.runner.tools.is_empty());
        let local = McpDraft {
            transport: Transport::Http,
            url: Some("http://127.0.0.1:8765/mcp".into()),
            command: None,
            secrets: vec![],
            ..stdio.clone()
        };
        let m = AnalyzerManifest::parse(&render(&local).unwrap()).unwrap();
        assert!(m.endpoint().unwrap().loopback && !m.is_remote());
        let remote = McpDraft {
            url: Some("https://scan.example.com/mcp".into()),
            auth_secret: Some("SCAN_TOKEN".into()),
            ..local.clone()
        };
        let m = AnalyzerManifest::parse(&render(&remote).unwrap()).unwrap();
        assert!(m.is_remote());
        assert_eq!(m.permissions.network, ["scan.example.com"]);
        assert_eq!(m.permissions.secrets, ["SCAN_TOKEN"]);
        let plain_remote = McpDraft {
            url: Some("http://scan.example.com/mcp".into()),
            ..local
        };
        assert!(
            AnalyzerManifest::parse(&render(&plain_remote).unwrap()).is_err(),
            "remote needs https"
        );
    }

    #[test]
    fn tool_mappings_are_written_into_the_manifest() {
        let draft = McpDraft {
            id: "local.scan".into(),
            name: "Scan".into(),
            command: Some("scan-mcp".into()),
            ..McpDraft::default()
        };
        let text = render(&draft).unwrap();
        let tools = vec![ToolSpec {
            name: "scan_skill".into(),
            args: [
                ("path".to_string(), json!("{subject.dir}")),
                ("depth".to_string(), json!(2)),
            ]
            .into_iter()
            .collect(),
        }];
        let mut doc: toml::Table = toml::from_str(&text).unwrap();
        with_tools(&mut doc, &tools).unwrap();
        let mapping = OutputMapping {
            items: Some("$.issues[*]".into()),
            rule_id: Some("code".into()),
            severity: Some(" level ".into()),
            title: Some(String::new()),
            ..OutputMapping::default()
        };
        with_output(&mut doc, &mapping).unwrap();
        let updated = toml::to_string(&doc).unwrap();
        let m = AnalyzerManifest::parse(&updated).unwrap();
        assert_eq!(m.runner.tools, tools);
        assert_eq!(m.output.items.as_deref(), Some("$.issues[*]"));
        assert_eq!(m.output.severity.as_deref(), Some("level"));
        assert_eq!(m.output.title, None, "blank fields are dropped");
        assert_ne!(m.sha256, AnalyzerManifest::parse(&text).unwrap().sha256);
        // Items without a rule id are refused.
        let mut doc: toml::Table = toml::from_str(&updated).unwrap();
        with_output(
            &mut doc,
            &OutputMapping {
                items: Some("$.issues[*]".into()),
                ..OutputMapping::default()
            },
        )
        .unwrap();
        assert!(AnalyzerManifest::parse(&toml::to_string(&doc).unwrap()).is_err());
    }
}

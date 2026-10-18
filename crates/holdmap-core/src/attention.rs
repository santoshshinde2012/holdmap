//! "Needs you": the short list of things worth a look, shared by the CLI, MCP and desktop.
//!
//! Each rule is a small [`AttentionRule`]; [`attention`] runs the default set over one scan and
//! agents report, adds items from other sources (opt-in analyzers) and sorts the result.
//! Only facts already in the report are used; nothing is read or probed here.

use crate::agents::{AccessLevel, AccessTopic, Agent, AgentsReport, McpServerCapability};
use crate::model::{Exposure, PortEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Which rule raised an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleId {
    /// An agent listens on all interfaces.
    Exposed,
    /// An agent runs as root.
    Root,
    /// An agent was started with wider access than usual.
    Elevated,
    /// A documented setting widens what an agent may do without asking.
    Settings,
    /// MCP servers whose runner downloads an unpinned package.
    Unpinned,
    /// A dev server no agent started is open to the network.
    ExposedPort,
    /// Open findings from an approved analyzer.
    Analyzer,
}

/// How urgent an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    /// Worth a look.
    Medium,
    /// Look now.
    High,
}

/// Where an item leads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "view", rename_all = "snake_case")]
pub enum Target {
    /// An agent card.
    Agents {
        /// Agent id.
        #[serde(rename = "agentId")]
        agent_id: String,
    },
    /// A port row.
    List {
        /// Port entry id.
        #[serde(rename = "entryId")]
        entry_id: String,
    },
}

/// A stop to review when stopping is the usual fix. Front ends always show the plan first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stop {
    /// One port entry.
    Entry {
        /// Port entry id.
        #[serde(rename = "entryId")]
        entry_id: String,
    },
    /// Everything an agent started.
    Agent {
        /// Agent id.
        #[serde(rename = "agentId")]
        agent_id: String,
    },
}

/// One "needs you" item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionItem {
    /// Stable id (`rule:subject`).
    pub id: String,
    /// The rule that raised it.
    pub rule: RuleId,
    /// How urgent.
    pub severity: Urgency,
    /// One line.
    pub title: String,
    /// Why it matters or what to check.
    pub detail: String,
    /// Where it leads.
    pub target: Target,
    /// The stop to review, when stopping is the usual fix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<Stop>,
}

/// What rules look at.
pub struct Facts<'a> {
    /// The latest port scan, when there is one.
    pub entries: Option<&'a [PortEntry]>,
    /// The latest agents report, when there is one.
    pub report: Option<&'a AgentsReport>,
}

/// One rule: facts in, items out.
pub trait AttentionRule: Send + Sync {
    /// Items this rule raises.
    fn items(&self, facts: &Facts<'_>) -> Vec<AttentionItem>;
}

/// The access headline shown on agent cards: level and one short phrase.
pub fn access_headline(agent: &Agent) -> (AccessLevel, String) {
    let fact = |topic: AccessTopic| agent.access.facts.iter().find(|f| f.topic == topic);
    let order = [
        AccessTopic::User,
        AccessTopic::Sandbox,
        AccessTopic::Approvals,
        AccessTopic::Network,
        AccessTopic::Privacy,
    ];
    if agent.access.root {
        return (AccessLevel::Elevated, "Runs as root".into());
    }
    let elevated: Vec<&str> = order
        .iter()
        .filter(|t| fact(**t).is_some_and(|f| f.level == AccessLevel::Elevated))
        .map(|t| topic_label(*t))
        .collect();
    if !elevated.is_empty() {
        return (
            AccessLevel::Elevated,
            format!("{}: wider than usual", elevated.join(" & ")),
        );
    }
    if let Some(f) = order
        .iter()
        .filter_map(|t| fact(*t))
        .find(|f| f.level == AccessLevel::Restricted)
    {
        return (AccessLevel::Restricted, f.summary.clone());
    }
    if order
        .iter()
        .all(|t| fact(*t).is_none_or(|f| f.level == AccessLevel::Unknown))
    {
        return (AccessLevel::Unknown, "Access not reported".into());
    }
    let text = if agent.access.mine {
        "Your account's access".to_string()
    } else {
        format!(
            "Runs as {}",
            agent.access.user.as_deref().unwrap_or("another user")
        )
    };
    (AccessLevel::Standard, text)
}

fn topic_label(t: AccessTopic) -> &'static str {
    match t {
        AccessTopic::User => "Account",
        AccessTopic::Sandbox => "Sandbox",
        AccessTopic::Approvals => "Approvals",
        AccessTopic::Network => "Network",
        AccessTopic::Privacy => "Privacy",
    }
}

fn agent_target(agent: &Agent) -> Target {
    Target::Agents {
        agent_id: agent.id.clone(),
    }
}

fn agents<'a>(facts: &'a Facts<'_>) -> &'a [Agent] {
    facts.report.map(|r| r.agents.as_slice()).unwrap_or(&[])
}

/// Agents listening on all interfaces.
pub struct ExposedAgents;

impl AttentionRule for ExposedAgents {
    fn items(&self, facts: &Facts<'_>) -> Vec<AttentionItem> {
        agents(facts)
            .iter()
            .filter_map(|a| {
                let open: Vec<String> = a
                    .ports
                    .iter()
                    .filter(|p| p.exposure == Exposure::AllInterfaces)
                    .map(|p| format!(":{}", p.port))
                    .collect();
                (!open.is_empty()).then(|| AttentionItem {
                    id: format!("exposed:{}", a.id),
                    rule: RuleId::Exposed,
                    severity: Urgency::High,
                    title: format!("{} is listening on all interfaces", a.name),
                    detail: format!(
                        "{} can be reached from other machines on your network.",
                        open.join(", ")
                    ),
                    target: agent_target(a),
                    stop: Some(Stop::Agent {
                        agent_id: a.id.clone(),
                    }),
                })
            })
            .collect()
    }
}

/// Root agents, else agents started with wider access than usual.
pub struct WideAccess;

impl AttentionRule for WideAccess {
    fn items(&self, facts: &Facts<'_>) -> Vec<AttentionItem> {
        agents(facts)
            .iter()
            .filter_map(|a| {
                if a.access.root {
                    return Some(AttentionItem {
                        id: format!("root:{}", a.id),
                        rule: RuleId::Root,
                        severity: Urgency::High,
                        title: format!("{} runs as root", a.name),
                        detail: "Everything it starts gets administrator rights.".into(),
                        target: agent_target(a),
                        stop: None,
                    });
                }
                let (level, text) = access_headline(a);
                (level == AccessLevel::Elevated).then(|| AttentionItem {
                    id: format!("elevated:{}", a.id),
                    rule: RuleId::Elevated,
                    severity: Urgency::Medium,
                    title: format!("{}: {text}", a.name),
                    detail: "Started with flags that skip approvals or the sandbox.".into(),
                    target: agent_target(a),
                    stop: None,
                })
            })
            .collect()
    }
}

/// Documented settings that widen what an agent may do without asking.
pub const RISKY_SETTINGS: &[(&str, &str, &str)] = &[
    (
        "permissions.defaultMode",
        "bypassPermissions",
        "skips every permission prompt",
    ),
    (
        "approval_policy",
        "never",
        "never asks before running commands",
    ),
    (
        "sandbox_mode",
        "danger-full-access",
        "runs without a sandbox",
    ),
];

/// Risky documented settings.
pub struct RiskySettings;

impl AttentionRule for RiskySettings {
    fn items(&self, facts: &Facts<'_>) -> Vec<AttentionItem> {
        let mut out = Vec::new();
        for a in agents(facts) {
            for s in &a.capabilities.settings {
                let Some((_, _, why)) = RISKY_SETTINGS
                    .iter()
                    .find(|(k, v, _)| *k == s.key && *v == s.value)
                else {
                    continue;
                };
                out.push(AttentionItem {
                    id: format!("settings:{}:{}", a.id, s.key),
                    rule: RuleId::Settings,
                    severity: Urgency::Medium,
                    title: format!("{} {why}", a.name),
                    detail: format!("{} = {} in {}", s.key, s.value, short_path(&s.source_path)),
                    target: agent_target(a),
                    stop: None,
                });
            }
        }
        out
    }
}

/// The last two path components, "/"-joined (enough to recognise the file).
fn short_path(path: &std::path::Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let parts: Vec<&str> = text.split('/').collect();
    parts[parts.len().saturating_sub(2)..].join("/")
}

/// Whether a server's runner downloads what it starts with no exact version or digest named.
pub fn is_unpinned(s: &McpServerCapability) -> bool {
    s.enabled != Some(false)
        && s.launch
            .as_ref()
            .is_some_and(|l| l.pinned == Some(false) && l.runner.fetches())
}

/// "npx @scope/pkg", or the remote host, or the transport.
pub fn launch_line(s: &McpServerCapability) -> String {
    use crate::agents::{McpRunner, McpTransport};
    if let Some(l) = &s.launch {
        let runner = match l.runner {
            McpRunner::Interpreter => "script",
            other => other.label(),
        };
        return match &l.package {
            Some(p) if !p.is_empty() => format!("{runner} {p}"),
            _ => runner.to_string(),
        };
    }
    if let Some(host) = &s.remote_host {
        return host.clone();
    }
    match s.transport {
        McpTransport::Unknown => "transport unknown".into(),
        McpTransport::Stdio => "stdio".into(),
        McpTransport::Http => "http".into(),
        McpTransport::Sse => "sse".into(),
    }
}

/// MCP servers that can change on the next launch.
pub struct UnpinnedMcp;

impl AttentionRule for UnpinnedMcp {
    fn items(&self, facts: &Facts<'_>) -> Vec<AttentionItem> {
        agents(facts)
            .iter()
            .filter_map(|a| {
                let unpinned: Vec<&McpServerCapability> = a
                    .capabilities
                    .mcp_servers
                    .iter()
                    .filter(|s| is_unpinned(s))
                    .collect();
                if unpinned.is_empty() {
                    return None;
                }
                let list = unpinned
                    .iter()
                    .take(2)
                    .map(|s| format!("{} ({})", s.name, launch_line(s)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let more = if unpinned.len() > 2 {
                    format!(" and {} more", unpinned.len() - 2)
                } else {
                    String::new()
                };
                let n = unpinned.len();
                Some(AttentionItem {
                    id: format!("unpinned:{}", a.id),
                    rule: RuleId::Unpinned,
                    severity: Urgency::Medium,
                    title: format!(
                        "{}: {n} MCP {} pinned",
                        a.name,
                        if n == 1 {
                            "server isn't"
                        } else {
                            "servers aren't"
                        }
                    ),
                    detail: format!("{list}{more} can change on the next launch."),
                    target: agent_target(a),
                    stop: None,
                })
            })
            .collect()
    }
}

/// Dev servers no agent started (shared with front-end lists).
pub fn unowned_dev_servers<'a>(
    entries: &'a [PortEntry],
    report: Option<&AgentsReport>,
) -> Vec<&'a PortEntry> {
    let owned: HashSet<&str> = report
        .map(|r| {
            r.agents
                .iter()
                .flat_map(|a| a.ports.iter().map(|p| p.entry_id.as_str()))
                .collect()
        })
        .unwrap_or_default();
    entries
        .iter()
        .filter(|e| e.is_dev && !owned.contains(e.id.as_str()))
        .collect()
}

/// Unowned dev servers open to the network.
pub struct ExposedDevServers;

impl AttentionRule for ExposedDevServers {
    fn items(&self, facts: &Facts<'_>) -> Vec<AttentionItem> {
        let Some(entries) = facts.entries else {
            return vec![];
        };
        unowned_dev_servers(entries, facts.report)
            .into_iter()
            .filter(|e| e.exposure == Exposure::AllInterfaces)
            .map(|e| AttentionItem {
                id: format!("exposed-port:{}", e.id),
                rule: RuleId::ExposedPort,
                severity: Urgency::Medium,
                title: format!("{} on :{} is open to the network", e.label, e.port),
                detail: "Dev servers usually only need localhost.".into(),
                target: Target::List {
                    entry_id: e.id.clone(),
                },
                stop: Some(Stop::Entry {
                    entry_id: e.id.clone(),
                }),
            })
            .collect()
    }
}

/// The built-in rules, in the order their items are gathered.
pub fn default_rules() -> Vec<Box<dyn AttentionRule>> {
    vec![
        Box::new(ExposedAgents),
        Box::new(WideAccess),
        Box::new(RiskySettings),
        Box::new(UnpinnedMcp),
        Box::new(ExposedDevServers),
    ]
}

/// Run `rules`, add `extra` items (for example analyzer findings), and sort: high first, then
/// by title.
pub fn collect(
    rules: &[Box<dyn AttentionRule>],
    facts: &Facts<'_>,
    extra: Vec<AttentionItem>,
) -> Vec<AttentionItem> {
    let mut items: Vec<AttentionItem> = rules.iter().flat_map(|r| r.items(facts)).collect();
    items.extend(extra);
    items.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.title.cmp(&b.title))
    });
    items
}

/// The default "needs you" list for one scan and agents report.
pub fn attention(
    entries: Option<&[PortEntry]>,
    report: Option<&AgentsReport>,
    extra: Vec<AttentionItem>,
) -> Vec<AttentionItem> {
    collect(&default_rules(), &Facts { entries, report }, extra)
}

/// Agents whose items concern the agent itself (their map node is ringed).
pub fn hot_agents(items: &[AttentionItem]) -> HashSet<String> {
    items
        .iter()
        .filter_map(|i| match (&i.target, i.rule) {
            (Target::Agents { agent_id }, RuleId::Root | RuleId::Elevated | RuleId::Settings) => {
                Some(agent_id.clone())
            }
            (Target::Agents { agent_id }, RuleId::Analyzer) if i.severity == Urgency::High => {
                Some(agent_id.clone())
            }
            _ => None,
        })
        .collect()
}

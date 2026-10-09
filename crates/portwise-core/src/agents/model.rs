//! The agents report: each running AI coding agent with its processes, folders, ports, the
//! services it talks to and what the OS says about its access. Every fact carries its
//! [`Evidence`], so a surface can tell what was seen from what was inferred or is unknown.

use super::catalog::AgentKind;
use crate::model::{Exposure, ProjectInfo, Protocol};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Everything portwise could see about the agents on this machine.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentsReport {
    /// Running agents, most active first.
    pub agents: Vec<Agent>,
    /// Platform name (`linux`, `macos`, `windows`).
    pub platform: String,
    /// When the underlying scan was taken (Unix epoch ms).
    pub taken_at_ms: u64,
    /// What portwise can't see or deliberately doesn't read, for the whole report.
    pub limits: Vec<String>,
}

/// How a fact was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    /// Read directly from the OS (process table, sockets) or a file's exact contents.
    Observed,
    /// Derived from observed facts (a folder name decoded back into a path).
    Inferred,
    /// Not knowable without reading something portwise doesn't read.
    Unknown,
}

/// One running agent: a product's top-level process and everything started under it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    /// Stable id: `agent:<pid>`.
    pub id: String,
    /// Catalog id, e.g. `claude-code`.
    pub product: String,
    /// Display name, e.g. "Claude Code".
    pub name: String,
    /// Who makes it.
    pub vendor: String,
    /// Terminal agent, AI editor, desktop app…
    pub kind: AgentKind,
    /// The agent's top-level process.
    pub pid: u32,
    /// Its process name.
    pub process_name: String,
    /// Its command line, secrets hidden.
    pub command: String,
    /// Start time (Unix seconds).
    pub started_at: u64,
    /// The agent this one was started from (Claude Code in Cursor's terminal), if any.
    pub parent: Option<String>,
    /// Resident memory of the agent and everything under it.
    pub memory_bytes: u64,
    /// CPU (percent of one core) of the agent and everything under it.
    pub cpu_percent: f32,
    /// Its processes: the agent, its helpers and what it started, agent first.
    pub processes: Vec<AgentProcess>,
    /// Processes not listed.
    pub more_processes: usize,
    /// Folders it works in.
    pub folders: Vec<AgentFolder>,
    /// Listening sockets held by the agent or anything it started.
    pub ports: Vec<AgentPort>,
    /// Live connections from the agent's processes, grouped by what they reach.
    pub links: Vec<AgentLink>,
    /// Link groups not listed.
    pub more_links: usize,
    /// Who it runs as and what the OS says about its reach.
    pub access: AgentAccess,
}

/// What a process is to its agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessRole {
    /// The agent's top-level process.
    Agent,
    /// Part of the same product (an Electron helper, a language server).
    Helper,
    /// Started by the agent or its terminal: shells, dev servers, tools, MCP servers.
    Child,
}

/// One process of an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentProcess {
    /// PID.
    pub pid: u32,
    /// Parent PID.
    pub ppid: Option<u32>,
    /// Process name.
    pub name: String,
    /// Command line, secrets hidden.
    pub command: String,
    /// Agent, helper or child.
    pub role: ProcessRole,
    /// Working directory, when visible.
    pub cwd: Option<PathBuf>,
    /// Resident memory.
    pub memory_bytes: u64,
    /// CPU percent of one core.
    pub cpu_percent: f32,
}

/// Where a folder came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderSource {
    /// The agent's own working directory.
    Agent,
    /// The working directory of something the agent started.
    Child,
    /// A project the product lists as recent or open in its own state files.
    Recent,
}

/// A folder an agent works in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentFolder {
    /// Absolute path.
    pub path: PathBuf,
    /// Project name, or the folder's own name.
    pub label: String,
    /// The project there, when portwise could look (see `note`).
    pub project: Option<ProjectInfo>,
    /// Working directory or recent project.
    pub source: FolderSource,
    /// Observed (a working directory, an exact path in a state file) or inferred.
    pub evidence: Evidence,
    /// Processes working there.
    pub pids: Vec<u32>,
    /// The privacy-protected area this is in (macOS: "Documents", "Desktop"…).
    pub privacy_area: Option<String>,
    /// Caveat, e.g. why the project wasn't inspected.
    pub note: Option<String>,
}

/// What a listening port is to its agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortRole {
    /// Held by the agent itself or one of its helpers (an IDE server, an auth callback).
    Agent,
    /// A dev server the agent started.
    DevServer,
    /// Any other service the agent started (an MCP server, a database).
    Service,
}

/// A listening socket of an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentPort {
    /// The port entry's id (links to the port list and details).
    pub entry_id: String,
    /// Port number.
    pub port: u16,
    /// TCP or UDP.
    pub protocol: Protocol,
    /// Loopback, all interfaces or a specific address.
    pub exposure: Exposure,
    /// Holder PID.
    pub pid: Option<u32>,
    /// Holder process name.
    pub process: Option<String>,
    /// The entry's label ("shop-web (Vite)").
    pub label: String,
    /// Agent-owned, dev server or other service.
    pub role: PortRole,
    /// Project name, when known.
    pub project: Option<String>,
    /// Framework name, when known.
    pub framework: Option<String>,
}

/// Local service or remote host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// A listener on this machine (a database, another app's server).
    Local,
    /// A host elsewhere, by IP address.
    Remote,
}

/// Live TCP connections from an agent's processes to one service or host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentLink {
    /// Stable id: `local:<entry id>` or `remote:<ip:port>`.
    pub id: String,
    /// Local service or remote host.
    pub kind: LinkKind,
    /// Display name: the local service's label, or the remote endpoint.
    pub label: String,
    /// `ip:port` of the far end.
    pub address: String,
    /// Port of the far end.
    pub port: u16,
    /// Open connections.
    pub connections: usize,
    /// The local listener's entry id.
    pub entry_id: Option<String>,
    /// The local listener's process.
    pub process: Option<String>,
    /// Its PID.
    pub pid: Option<u32>,
    /// What the port usually carries ("HTTPS", "PostgreSQL"), a hint, not a probe.
    pub service: Option<String>,
}

/// How much reach a fact points to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessLevel {
    /// Narrower than the user's own rights (sandboxed, plan-only).
    Restricted,
    /// The user's own rights.
    Standard,
    /// Wider than usual (root, auto-approve, sandbox bypassed, reachable from the network).
    Elevated,
    /// Can't tell.
    Unknown,
}

/// Which part of an agent's access a fact is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessTopic {
    /// The account it runs as.
    User,
    /// OS-level sandboxing of the commands it runs.
    Sandbox,
    /// Whether it asks before running commands or editing files.
    Approvals,
    /// Listening sockets and their exposure.
    Network,
    /// OS privacy-protected folders (macOS TCC).
    Privacy,
}

/// One statement about an agent's access, with how it was established.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccessFact {
    /// What it's about.
    pub topic: AccessTopic,
    /// How much reach it points to.
    pub level: AccessLevel,
    /// One plain sentence.
    pub summary: String,
    /// Seen, inferred or unknown.
    pub evidence: Evidence,
}

/// Who an agent runs as and what the OS says about its reach.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentAccess {
    /// User name.
    pub user: Option<String>,
    /// Numeric uid (Unix).
    pub uid: Option<u32>,
    /// Runs as root / uid 0.
    pub root: bool,
    /// Runs as the same user as portwise.
    pub mine: bool,
    /// One fact per topic, in [`AccessTopic`] order.
    pub facts: Vec<AccessFact>,
}

impl AgentAccess {
    /// The fact about `topic`.
    pub fn fact(&self, topic: AccessTopic) -> Option<&AccessFact> {
        self.facts.iter().find(|f| f.topic == topic)
    }
}

impl Agent {
    /// Listening ports this agent started that are safe candidates to stop: its own IDE /
    /// auth callbacks stay listed as [`PortRole::Agent`] and are skipped; only the tools and
    /// apps it launched ([`PortRole::DevServer`] / [`PortRole::Service`]) are returned.
    pub fn stoppable_ports(&self) -> impl Iterator<Item = &AgentPort> {
        self.ports.iter().filter(|p| {
            matches!(p.role, PortRole::DevServer | PortRole::Service)
        })
    }

    /// Absolute folder paths this agent is known to work in (working dirs and recent projects).
    pub fn known_folders(&self) -> impl Iterator<Item = &PathBuf> {
        self.folders.iter().map(|f| &f.path)
    }
}

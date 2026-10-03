//! Serialisable value types shared by every surface: owners, plans, steps, reports, targets.

use crate::model::{ContainerInfo, PortEntry, Protocol};
use serde::{Deserialize, Serialize};

use super::text::fmt_ms;

/// Supervisors that would restart a process if it were simply killed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Supervisor {
    SystemdService {
        unit: String,
        user: bool,
    },
    SystemdSocket {
        unit: String,
        user: bool,
        service: Option<String>,
    },
    BrewService {
        formula: String,
    },
    Pm2 {
        name: String,
        id: String,
    },
}

/// The *effective* owner of a port: what you actually have to stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Owner {
    Free,
    Process {
        pid: u32,
        name: String,
    },
    ProcessTree {
        root_pid: u32,
        root_name: String,
        pids: Vec<u32>,
    },
    Container {
        container: ContainerInfo,
        forwarder_pid: Option<u32>,
    },
    Supervised {
        supervisor: Supervisor,
        pid: u32,
        name: String,
    },
    OsService {
        service: String,
        pid: Option<u32>,
    },
    Protected {
        pid: u32,
        name: String,
        reason: String,
    },
    Hidden {
        uid: Option<u32>,
        user: Option<String>,
    },
    Reserved {
        start: u16,
        end: u16,
    },
    TimeWait {
        connections: usize,
    },
}

/// A process to signal, pinned by its start token (PID-reuse guard).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcRef {
    pub pid: u32,
    pub name: String,
    pub start_token: u64,
    pub command: String,
}

/// One step of an [`ActionPlan`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Step {
    /// Send SIGTERM (or SIGKILL with `force`) to every process; escalate after the timeout.
    SignalProcesses {
        processes: Vec<ProcRef>,
        force: bool,
        timeout_ms: u64,
    },
    /// Stop a container via its runtime API (graceful, runtime-side timeout).
    StopContainer {
        id: String,
        name: String,
        runtime: String,
        endpoint: String,
        timeout_s: u64,
    },
    /// Run a supervisor command (`systemctl --user stop …`, `brew services stop …`, `pm2 stop …`).
    RunCommand {
        program: String,
        args: Vec<String>,
        reason: String,
    },
    /// Confirm the port is actually free afterwards.
    VerifyFree {
        port: u16,
        protocol: Protocol,
        timeout_ms: u64,
    },
}

impl Step {
    /// One-line human description.
    pub fn describe(&self) -> String {
        match self {
            Step::SignalProcesses {
                processes,
                force,
                timeout_ms,
            } => {
                let list = processes
                    .iter()
                    .map(|p| format!("{} ({})", p.name, p.pid))
                    .collect::<Vec<_>>()
                    .join(", ");
                if *force {
                    format!(
                        "Force-kill (SIGKILL) {} process(es): {list}",
                        processes.len()
                    )
                } else {
                    format!(
                        "Send SIGTERM to {} process(es): {list}; SIGKILL any still running after {}",
                        processes.len(),
                        fmt_ms(*timeout_ms)
                    )
                }
            }
            Step::StopContainer {
                name,
                runtime,
                timeout_s,
                ..
            } => {
                format!("Stop {runtime} container {name} (graceful, {timeout_s}s timeout)")
            }
            Step::RunCommand { program, args, .. } => format!("Run `{program} {}`", args.join(" ")),
            Step::VerifyFree {
                port,
                protocol,
                timeout_ms,
            } => {
                format!(
                    "Verify {protocol} port {port} is free (wait up to {})",
                    fmt_ms(*timeout_ms)
                )
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    /// Protected system / editor / terminal process.
    Protected,
    /// Owned by another user or root.
    NeedsElevation,
    /// An OS feature, not a process you should kill (AirPlay, HTTP.sys, reservations).
    OsService,
    /// Nothing to stop.
    NothingToStop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocked {
    pub kind: BlockKind,
    pub message: String,
    /// True when an explicit override (`--allow-protected`, the GUI's "stop anyway") would
    /// unblock it. Hard protections (portwise's own process tree, core OS processes) never are.
    #[serde(default)]
    pub overridable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    /// Your own dev server / container.
    Low,
    /// A non-dev process of yours (e.g. a desktop app).
    Medium,
    /// Protected (with override), another user's process, or several unrelated processes.
    High,
}

/// A serialisable, previewable plan (`--dry-run`, GUI confirm dialog, MCP).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionPlan {
    pub target: String,
    pub owners: Vec<Owner>,
    pub summary: String,
    pub steps: Vec<Step>,
    pub blocked: Option<Blocked>,
    pub warnings: Vec<String>,
    pub risk: Risk,
}

impl ActionPlan {
    pub fn is_blocked(&self) -> bool {
        self.blocked.is_some()
    }
}

/// Options controlling how a stop is planned.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StopOptions {
    /// Skip SIGTERM and go straight to SIGKILL / TerminateProcess.
    pub force: bool,
    /// How long to wait after SIGTERM before escalating.
    pub timeout_ms: u64,
    /// Permit stopping soft-protected processes (system services, editors…).
    pub allow_protected: bool,
    /// Stop the whole dev-server tree (launcher + children), not just the socket holder.
    pub tree: bool,
    /// How long to wait for the port to become free afterwards.
    pub verify_timeout_ms: u64,
    /// Restrict a port target to one protocol.
    pub protocol: Option<Protocol>,
}

impl Default for StopOptions {
    fn default() -> Self {
        Self {
            force: false,
            timeout_ms: 5000,
            allow_protected: false,
            tree: true,
            verify_timeout_ms: 3000,
            protocol: None,
        }
    }
}

/// What to stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Target {
    Port(u16),
    Pid(u32),
    Name(String),
    /// A topology cluster by id or name (`cluster:acme-shop`), stopped in dependency order.
    Cluster(String),
}

impl Target {
    /// Parse "3000", ":3000", "pid:1234", "cluster:NAME" or a process name.
    pub fn parse(s: &str) -> Target {
        let t = s.trim();
        if let Some(c) = t.strip_prefix("cluster:").filter(|c| !c.is_empty()) {
            return Target::Cluster(c.to_string());
        }
        if let Some(p) = t.strip_prefix(':').and_then(|p| p.parse().ok()) {
            return Target::Port(p);
        }
        if let Some(p) = t.strip_prefix("pid:").and_then(|p| p.parse().ok()) {
            return Target::Pid(p);
        }
        match t.parse::<u16>() {
            Ok(p) => Target::Port(p),
            Err(_) => Target::Name(t.to_string()),
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::Port(p) => write!(f, ":{p}"),
            Target::Pid(p) => write!(f, "PID {p}"),
            Target::Name(n) => write!(f, "\"{n}\""),
            Target::Cluster(c) => write!(f, "cluster {c}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortStatus {
    Free,
    Busy,
    /// Reserved by the OS (Windows excluded range) or privileged.
    Reserved,
}

/// A plain-English answer to "why is this port busy, and what should I do?".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Explanation {
    pub port: u16,
    pub status: PortStatus,
    pub owners: Vec<Owner>,
    pub headline: String,
    pub details: Vec<String>,
    pub recommendation: String,
    /// Copy-pasteable commands (portwise and native).
    pub commands: Vec<String>,
    pub entries: Vec<PortEntry>,
    pub plan: Option<ActionPlan>,
}

/// Result of executing a plan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopReport {
    pub target: String,
    pub success: bool,
    /// Every verified port is free.
    pub freed: bool,
    pub ports_still_busy: Vec<u16>,
    pub signalled: Vec<u32>,
    pub escalated: bool,
    pub survivors: Vec<u32>,
    pub elapsed_ms: u64,
    pub log: Vec<String>,
    pub error: Option<String>,
}

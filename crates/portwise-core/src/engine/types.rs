//! Serialisable value types shared by every surface: owners, plans, steps, reports, targets.

use crate::model::{ContainerInfo, PortEntry, Protocol};
use serde::{Deserialize, Serialize};

use super::text::fmt_ms;
use crate::util::count;

/// Supervisors that would restart a process if it were simply killed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Supervisor {
    /// A systemd service unit.
    SystemdService {
        /// Unit name, e.g. `postgresql.service`.
        unit: String,
        /// True for the user manager (`systemctl --user`).
        user: bool,
    },
    /// A socket-activated systemd unit (stopping the service alone would re-activate it).
    SystemdSocket {
        /// Socket unit name, e.g. `cups.socket`.
        unit: String,
        /// True for the user manager (`systemctl --user`).
        user: bool,
        /// The service unit the socket activates, when known.
        service: Option<String>,
    },
    /// A Homebrew service (`brew services`).
    BrewService {
        /// Formula name, e.g. `postgresql@16`.
        formula: String,
    },
    /// A pm2-managed app.
    Pm2 {
        /// pm2 app name.
        name: String,
        /// pm2 app id.
        id: String,
    },
}

/// The *effective* owner of a port: what you actually have to stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Owner {
    /// Nothing holds the port.
    Free,
    /// A single process with no dev-server tree around it.
    Process {
        /// Process ID.
        pid: u32,
        /// Process name.
        name: String,
    },
    /// A dev-server tree rooted at a launcher (`npm → sh → node`).
    ProcessTree {
        /// PID of the tree root.
        root_pid: u32,
        /// Name of the tree root.
        root_name: String,
        /// Every PID in the tree.
        pids: Vec<u32>,
    },
    /// A container publishing the port.
    Container {
        /// The container.
        container: ContainerInfo,
        /// PID of the host-side forwarder (`docker-proxy`, `com.docker.backend`), if any.
        forwarder_pid: Option<u32>,
    },
    /// A process managed by a supervisor that would restart it.
    Supervised {
        /// The supervisor.
        supervisor: Supervisor,
        /// Process ID.
        pid: u32,
        /// Process name.
        name: String,
    },
    /// An OS feature or service (AirPlay Receiver, HTTP.sys, WSL relay).
    OsService {
        /// Service name.
        service: String,
        /// PID, when there is one.
        pid: Option<u32>,
    },
    /// A protected process (system, editor, terminal, agent host or portwise itself).
    Protected {
        /// Process ID.
        pid: u32,
        /// Process name.
        name: String,
        /// Why it is protected.
        reason: String,
    },
    /// The owning process is not visible (another user or root).
    Hidden {
        /// Owner UID, when known.
        uid: Option<u32>,
        /// Owner user name, when known.
        user: Option<String>,
    },
    /// The port is in an OS-reserved range (Windows excluded port range).
    Reserved {
        /// First port of the range.
        start: u16,
        /// Last port of the range (inclusive).
        end: u16,
    },
    /// Only TIME_WAIT connections remain; the port frees itself shortly.
    TimeWait {
        /// Number of TIME_WAIT connections.
        connections: usize,
    },
}

/// A process to signal, pinned by its start token (PID-reuse guard).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcRef {
    /// Process ID.
    pub pid: u32,
    /// Process name.
    pub name: String,
    /// Process start token; must match again right before the signal.
    pub start_token: u64,
    /// Command line, for display.
    pub command: String,
}

/// One step of an [`ActionPlan`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Step {
    /// Send SIGTERM (or SIGKILL with `force`) to every process; escalate after the timeout.
    SignalProcesses {
        /// Processes to signal.
        processes: Vec<ProcRef>,
        /// SIGKILL straight away instead of SIGTERM first.
        force: bool,
        /// Grace period before escalating to SIGKILL, in milliseconds.
        timeout_ms: u64,
    },
    /// Stop a container via its runtime API (graceful, runtime-side timeout).
    StopContainer {
        /// Container ID.
        id: String,
        /// Container name.
        name: String,
        /// Runtime label (Docker, OrbStack, Podman…).
        runtime: String,
        /// Runtime API endpoint.
        endpoint: String,
        /// Graceful stop timeout passed to the runtime, in seconds.
        timeout_s: u64,
    },
    /// Run a supervisor command (`systemctl --user stop …`, `brew services stop …`, `pm2 stop …`).
    RunCommand {
        /// Program to run.
        program: String,
        /// Arguments.
        args: Vec<String>,
        /// Why this command is needed.
        reason: String,
    },
    /// Confirm the port is actually free afterwards.
    VerifyFree {
        /// Port to check.
        port: u16,
        /// Protocol to check.
        protocol: Protocol,
        /// How long to wait for the port to free up, in milliseconds.
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
                        "Force-kill (SIGKILL) {}: {list}",
                        count(processes.len(), "process", "processes")
                    )
                } else {
                    format!(
                        "Send SIGTERM to {}: {list}; SIGKILL any still running after {}",
                        count(processes.len(), "process", "processes"),
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
/// Why a plan is blocked.
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
/// A blocked plan: the kind, a message and whether an override is allowed.
pub struct Blocked {
    /// What blocks the plan.
    pub kind: BlockKind,
    /// User-facing explanation.
    pub message: String,
    /// True when an explicit override (`--allow-protected`, the GUI's "stop anyway") would
    /// unblock it. Hard protections (portwise's own process tree, core OS processes) never are.
    #[serde(default)]
    pub overridable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// How risky executing a plan is.
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
    /// The target as the user wrote it.
    pub target: String,
    /// Effective owners being stopped.
    pub owners: Vec<Owner>,
    /// One-line summary of the plan.
    pub summary: String,
    /// Ordered steps.
    pub steps: Vec<Step>,
    /// Why the plan can't run, if it can't.
    pub blocked: Option<Blocked>,
    /// Things the user should know before confirming.
    pub warnings: Vec<String>,
    /// Overall risk.
    pub risk: Risk,
}

impl ActionPlan {
    /// True when the plan can't run without an override (or at all).
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
    /// A port number.
    Port(u16),
    /// A process ID.
    Pid(u32),
    /// A process name (exact match).
    Name(String),
    /// A topology cluster by id or name (`cluster:acme-shop`), stopped in dependency order.
    Cluster(String),
    /// Every dev server the current user owns (`dev:all`), except protected processes and
    /// containers.
    AllDev,
}

impl Target {
    /// Parse "3000", ":3000", "pid:1234", "cluster:NAME", "dev:all" or a process name.
    pub fn parse(s: &str) -> Target {
        let t = s.trim();
        if t == "dev:all" {
            return Target::AllDev;
        }
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
            Target::AllDev => write!(f, "all dev servers"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Whether a port is in use.
pub enum PortStatus {
    /// Nothing listens on the port.
    Free,
    /// Something listens on the port.
    Busy,
    /// Reserved by the OS (Windows excluded range) or privileged.
    Reserved,
}

/// A plain-English answer to "why is this port busy, and what should I do?".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Explanation {
    /// The port.
    pub port: u16,
    /// Free, busy or reserved.
    pub status: PortStatus,
    /// Effective owners.
    pub owners: Vec<Owner>,
    /// One-line answer.
    pub headline: String,
    /// Supporting facts.
    pub details: Vec<String>,
    /// What to do.
    pub recommendation: String,
    /// Copy-pasteable commands (portwise and native).
    pub commands: Vec<String>,
    /// Port entries on the port.
    pub entries: Vec<PortEntry>,
    /// The stop plan, when the port is busy.
    pub plan: Option<ActionPlan>,
}

/// Result of executing a plan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopReport {
    /// The target that was executed.
    pub target: String,
    /// Every step succeeded.
    pub success: bool,
    /// Every verified port is free.
    pub freed: bool,
    /// Ports that were still busy after the plan ran.
    pub ports_still_busy: Vec<u16>,
    /// PIDs that were signalled.
    pub signalled: Vec<u32>,
    /// True when SIGTERM had to be escalated to SIGKILL.
    pub escalated: bool,
    /// PIDs still alive at the end.
    pub survivors: Vec<u32>,
    /// Total time taken, in milliseconds.
    pub elapsed_ms: u64,
    /// Human-readable progress log.
    pub log: Vec<String>,
    /// The first error, if any step failed.
    pub error: Option<String>,
}

//! Owner resolution, the `explain` engine, stop planning and plan execution.
//!
//! Flow: [`Scan`] → [`Engine::explain`] / [`Engine::plan`] → (user confirms) → [`execute`] →
//! [`StopReport`] (with a port-free verification).

use crate::docker;
use crate::model::*;
use crate::probe::{self, ProbeResult};
use crate::process::ProcessTable;
use crate::safety::{protection, Protection};
use crate::scan::{self, Scan, ScanOptions};
use crate::sys;
use crate::util::{human_bytes, human_duration, now_secs, run_with_timeout};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::time::Duration;

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

fn fmt_ms(ms: u64) -> String {
    if ms.is_multiple_of(1000) {
        format!("{}s", ms / 1000)
    } else {
        format!("{ms}ms")
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
}

impl Target {
    /// Parse "3000", ":3000", "pid:1234" or a process name.
    pub fn parse(s: &str) -> Target {
        let t = s.trim();
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Per-entry resolution result.
struct Resolution {
    owner: Owner,
    headline: String,
    details: Vec<String>,
    recommendation: String,
    commands: Vec<String>,
    steps: Vec<Step>,
    blocked: Option<Blocked>,
    warnings: Vec<String>,
    risk: Risk,
}

impl Resolution {
    fn new(owner: Owner, headline: String) -> Self {
        Self {
            owner,
            headline,
            details: Vec::new(),
            recommendation: String::new(),
            commands: Vec::new(),
            steps: Vec::new(),
            blocked: None,
            warnings: Vec::new(),
            risk: Risk::Low,
        }
    }
    fn block(mut self, kind: BlockKind, msg: impl Into<String>) -> Self {
        self.blocked = Some(Blocked {
            kind,
            message: msg.into(),
            overridable: false,
        });
        self
    }

    fn block_overridable(mut self, kind: BlockKind, msg: impl Into<String>) -> Self {
        self = self.block(kind, msg);
        if let Some(b) = &mut self.blocked {
            b.overridable = true;
        }
        self
    }
}

fn is_root() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no preconditions.
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn sudo() -> &'static str {
    if cfg!(windows) {
        ""
    } else {
        "sudo "
    }
}

/// Replace the home directory prefix with `~`.
pub fn tilde(p: &std::path::Path) -> String {
    let s = p.display().to_string();
    if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        let h = home.to_string_lossy();
        if let Some(rest) = s.strip_prefix(h.as_ref()) {
            return format!("~{rest}");
        }
    }
    s
}

/// The engine: a consistent view of the machine plus the operations on it.
#[derive(Debug, Clone)]
pub struct Engine {
    pub scan: Scan,
}

impl Engine {
    pub fn new(opts: &ScanOptions) -> std::io::Result<Self> {
        Ok(Self {
            scan: scan::scan(opts)?,
        })
    }

    pub fn from_scan(scan: Scan) -> Self {
        Self { scan }
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.scan.snapshot
    }

    pub fn table(&self) -> &ProcessTable {
        &self.scan.table
    }

    fn listeners(&self, port: u16, proto: Option<Protocol>) -> Vec<&PortEntry> {
        self.scan
            .snapshot
            .entries
            .iter()
            .filter(|e| {
                e.port == port && e.state.is_listening() && proto.is_none_or(|p| p == e.protocol)
            })
            .collect()
    }

    fn time_wait_count(&self, port: u16) -> usize {
        self.scan
            .raw
            .iter()
            .filter(|s| s.local_port == port && s.state == SocketState::TimeWait)
            .count()
    }

    /// Explain why `port` is busy (or that it's free) and what to do.
    pub fn explain(&self, port: u16, opts: &StopOptions) -> Explanation {
        let entries: Vec<PortEntry> = self
            .listeners(port, opts.protocol)
            .into_iter()
            .cloned()
            .collect();
        if entries.is_empty() {
            return self.explain_unlisted(port, opts);
        }
        let mut resolutions: Vec<Resolution> = Vec::new();
        for e in &entries {
            let r = self.resolve_entry(e, opts);
            if !resolutions.iter().any(|x| x.owner == r.owner) {
                resolutions.push(r);
            }
        }
        let plan = self.combine(format!(":{port}"), &resolutions, &entries, opts);
        let first = &resolutions[0];
        let mut details = first.details.clone();
        for r in resolutions.iter().skip(1) {
            details.push(format!("Also: {}", r.headline));
            details.extend(r.details.iter().map(|d| format!("  {d}")));
        }
        let mut commands: Vec<String> = Vec::new();
        for c in resolutions.iter().flat_map(|r| r.commands.iter()) {
            if !commands.contains(c) {
                commands.push(c.clone());
            }
        }
        Explanation {
            port,
            status: PortStatus::Busy,
            owners: resolutions.iter().map(|r| r.owner.clone()).collect(),
            headline: first.headline.clone(),
            details,
            recommendation: first.recommendation.clone(),
            commands,
            entries,
            plan: Some(plan),
        }
    }

    fn explain_unlisted(&self, port: u16, opts: &StopOptions) -> Explanation {
        let tw = self.time_wait_count(port);
        let tcp = opts.protocol != Some(Protocol::Udp);
        let probe = if tcp {
            probe::probe_tcp(port)
        } else {
            probe::probe_udp(port)
        };
        let mut ex = Explanation {
            port,
            status: PortStatus::Free,
            owners: vec![Owner::Free],
            headline: format!("Port {port} is free."),
            details: Vec::new(),
            recommendation: "Nothing to do — you can start your server on this port.".into(),
            commands: Vec::new(),
            entries: Vec::new(),
            plan: None,
        };
        match probe {
            ProbeResult::InUse => {
                ex.status = PortStatus::Busy;
                ex.owners = vec![Owner::Hidden {
                    uid: None,
                    user: None,
                }];
                ex.headline = format!(
                    "Port {port} is in use, but its owner isn't visible to portwise: it belongs to another user or root{}.",
                    if cfg!(target_os = "macos") {
                        " (macOS hides other users' sockets from unprivileged tools)"
                    } else {
                        ""
                    }
                );
                ex.details
                    .push("The kernel refused a test bind with \"address already in use\".".into());
                ex.recommendation =
                    "Re-run with elevated privileges to see and stop the owner.".into();
                ex.commands
                    .push(format!("{}portwise explain {port}", sudo()));
                ex.plan = Some(blocked_plan(
                    format!(":{port}"),
                    ex.owners.clone(),
                    BlockKind::NeedsElevation,
                    "The owner belongs to another user or root; run portwise with sudo / as Administrator.",
                ));
            }
            ProbeResult::Denied => {
                let ranges =
                    crate::windiag::excluded_ranges(opts.protocol.unwrap_or(Protocol::Tcp));
                ex.status = PortStatus::Reserved;
                if let Some(r) = ranges.iter().find(|r| r.contains(port)) {
                    ex.owners = vec![Owner::Reserved {
                        start: r.start,
                        end: r.end,
                    }];
                    ex.headline = format!(
                        "Port {port} is inside a Windows excluded port range ({}–{}), reserved by Hyper-V/WinNAT/WSL/Docker. No process owns it, so there is nothing to kill.",
                        r.start, r.end
                    );
                    ex.details.push("Binding fails with \"An attempt was made to access a socket in a way forbidden by its access permissions\" (WSAEACCES).".into());
                    ex.recommendation = "Pick a port outside the range, or restart WinNAT to release dynamic reservations (admin).".into();
                    ex.commands.extend([
                        "netsh interface ipv4 show excludedportrange protocol=tcp".to_string(),
                        "net stop winnat && net start winnat   (elevated)".to_string(),
                        format!("portwise free-port --near {port}"),
                    ]);
                } else {
                    ex.headline = format!(
                        "Port {port} is free, but binding it requires elevated privileges."
                    );
                    if port < 1024 && cfg!(unix) {
                        ex.details
                            .push("Ports below 1024 are privileged on Unix-like systems.".into());
                    }
                    ex.recommendation = "Use a port ≥ 1024 for development, or run your server with the needed privileges.".into();
                }
            }
            ProbeResult::Free => {
                if tw > 0 {
                    ex.owners = vec![Owner::TimeWait { connections: tw }];
                    ex.headline = format!(
                        "Nothing is listening on port {port}; {tw} recently closed connection(s) are in TIME_WAIT."
                    );
                    ex.details.push("TIME_WAIT sockets have no owning process and clear by themselves (typically within 60 s).".into());
                    ex.details.push("Servers that set SO_REUSEADDR (Node, Python, Go, Rust and most frameworks do) can bind immediately.".into());
                    ex.recommendation =
                        "Start your server, or wait a minute if it doesn't use SO_REUSEADDR."
                            .into();
                }
            }
        }
        if cfg!(target_os = "macos")
            && (port == 5000 || port == 7000)
            && ex.status == PortStatus::Free
        {
            ex.details.push("Tip: on macOS, AirPlay Receiver often claims 5000/7000; portwise flags it when it does.".into());
        }
        ex
    }

    /// Build a stop plan for any target.
    pub fn plan(&self, target: &Target, opts: &StopOptions) -> ActionPlan {
        match target {
            Target::Port(p) => self.explain(*p, opts).plan.unwrap_or_else(|| {
                blocked_plan(
                    target.to_string(),
                    vec![Owner::Free],
                    BlockKind::NothingToStop,
                    format!("Nothing is listening on port {p}."),
                )
            }),
            Target::Pid(pid) => self.plan_pids(target.to_string(), &[*pid], opts),
            Target::Name(name) => {
                let n = name.to_ascii_lowercase();
                let mut pids: Vec<u32> = self
                    .table()
                    .iter()
                    .filter(|p| {
                        let pn = p.name.to_ascii_lowercase();
                        pn == n || pn.strip_suffix(".exe") == Some(n.as_str())
                    })
                    .map(|p| p.pid)
                    .collect();
                pids.sort_unstable();
                // Children of a matched process are covered by its tree.
                let set: BTreeSet<u32> = pids.iter().copied().collect();
                pids.retain(|p| !self.table().ancestors(*p).iter().any(|a| set.contains(a)));
                if pids.is_empty() {
                    return blocked_plan(
                        target.to_string(),
                        vec![],
                        BlockKind::NothingToStop,
                        format!("No running process named {name}."),
                    );
                }
                let mut plan = self.plan_pids(target.to_string(), &pids, opts);
                if pids.len() > 1 {
                    plan.risk = plan.risk.max(Risk::High);
                    plan.warnings.push(format!(
                        "{} separate processes match \"{name}\".",
                        pids.len()
                    ));
                }
                plan
            }
        }
    }

    fn plan_pids(&self, target: String, pids: &[u32], opts: &StopOptions) -> ActionPlan {
        let t = self.table();
        let mut resolutions = Vec::new();
        for &pid in pids {
            let Some(p) = t.get(pid) else {
                resolutions.push(
                    Resolution::new(
                        Owner::Hidden {
                            uid: None,
                            user: None,
                        },
                        format!("PID {pid} is not running or not visible."),
                    )
                    .block(
                        BlockKind::NothingToStop,
                        format!("PID {pid} is not running or belongs to another user."),
                    ),
                );
                continue;
            };
            let mut r = Resolution::new(
                Owner::Process {
                    pid,
                    name: p.name.clone(),
                },
                format!("{} (PID {pid})", p.name),
            );
            match protection(p, t) {
                Protection::Hard(reason) => {
                    r.owner = Owner::Protected {
                        pid,
                        name: p.name.clone(),
                        reason: reason.clone(),
                    };
                    r = r.block(BlockKind::Protected, format!("Refusing: {reason}."));
                }
                Protection::Soft(reason) if !opts.allow_protected => {
                    r.owner = Owner::Protected {
                        pid,
                        name: p.name.clone(),
                        reason: reason.clone(),
                    };
                    r = r.block_overridable(
                        BlockKind::Protected,
                        format!("{reason}. Use --allow-protected if you really mean it."),
                    );
                }
                prot => {
                    let mut tree = vec![pid];
                    if opts.tree {
                        tree.extend(self.safe_descendants(pid, opts));
                    }
                    if tree.len() > 1 {
                        r.owner = Owner::ProcessTree {
                            root_pid: pid,
                            root_name: p.name.clone(),
                            pids: tree.clone(),
                        };
                    }
                    if prot.is_protected() {
                        r.risk = Risk::High;
                    } else if !t.is_mine(pid) {
                        r.risk = Risk::High;
                        r.warnings
                            .push(format!("{} (PID {pid}) belongs to another user.", p.name));
                    } else {
                        r.risk = Risk::Medium;
                    }
                    r.recommendation = Step::describe(&self.signal_step(&tree, opts));
                    r.steps.push(self.signal_step(&tree, opts));
                }
            }
            resolutions.push(r);
        }
        let entries: Vec<PortEntry> = self
            .scan
            .snapshot
            .entries
            .iter()
            .filter(|e| e.state.is_listening() && e.pids.iter().any(|p| pids.contains(p)))
            .cloned()
            .collect();
        if !entries.is_empty() && entries.iter().all(|e| e.is_dev) {
            for r in &mut resolutions {
                if r.risk == Risk::Medium {
                    r.risk = Risk::Low;
                }
            }
        }
        self.combine(target, &resolutions, &entries, opts)
    }

    fn combine(
        &self,
        target: String,
        rs: &[Resolution],
        entries: &[PortEntry],
        opts: &StopOptions,
    ) -> ActionPlan {
        // A hard block wins over one the user could override.
        let blocked = rs
            .iter()
            .filter_map(|r| r.blocked.clone())
            .min_by_key(|b| b.overridable);
        let mut steps: Vec<Step> = Vec::new();
        for s in rs.iter().flat_map(|r| r.steps.iter()) {
            if !steps.contains(s) {
                steps.push(s.clone());
            }
        }
        if blocked.is_none() && !steps.is_empty() {
            let mut seen = BTreeSet::new();
            for e in entries {
                if seen.insert((e.port, e.protocol)) {
                    steps.push(Step::VerifyFree {
                        port: e.port,
                        protocol: e.protocol,
                        timeout_ms: opts.verify_timeout_ms,
                    });
                }
            }
        }
        let summary = match &blocked {
            Some(b) => b.message.clone(),
            None => rs
                .iter()
                .map(|r| r.recommendation.clone())
                .find(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    steps
                        .first()
                        .map(|s| s.describe())
                        .unwrap_or_else(|| "Nothing to do.".into())
                }),
        };
        ActionPlan {
            target,
            owners: rs.iter().map(|r| r.owner.clone()).collect(),
            summary,
            steps: if blocked.is_some() { Vec::new() } else { steps },
            blocked,
            warnings: rs.iter().flat_map(|r| r.warnings.clone()).collect(),
            risk: rs.iter().map(|r| r.risk).max().unwrap_or(Risk::Low),
        }
    }

    fn proc_ref(&self, pid: u32) -> Option<ProcRef> {
        self.table().get(pid).map(|p| ProcRef {
            pid,
            name: p.name.clone(),
            start_token: p.start_token,
            command: p.command(),
        })
    }

    fn signal_step(&self, pids: &[u32], opts: &StopOptions) -> Step {
        Step::SignalProcesses {
            processes: pids.iter().filter_map(|p| self.proc_ref(*p)).collect(),
            force: opts.force,
            timeout_ms: opts.timeout_ms,
        }
    }

    /// Descendants that are safe to stop along with their parent.
    fn safe_descendants(&self, pid: u32, opts: &StopOptions) -> Vec<u32> {
        let t = self.table();
        t.descendants(pid)
            .into_iter()
            .filter(|d| {
                t.get(*d).is_some_and(|p| match protection(p, t) {
                    Protection::None => true,
                    Protection::Soft(_) => opts.allow_protected,
                    Protection::Hard(_) => false,
                })
            })
            .collect()
    }

    /// Climb from the socket holder to the dev-session root (e.g. node → sh -c → npm).
    pub fn tree_root(&self, pid: u32) -> u32 {
        let t = self.table();
        let mut root = pid;
        let mut child = t.get(pid);
        for anc in t.ancestors(pid) {
            let Some(a) = t.get(anc) else { break };
            if t.is_self_or_ancestor(anc)
                || protection(a, t).is_protected()
                || (t.is_mine(pid) && !t.is_mine(anc))
            {
                break;
            }
            let reloader = child.is_some_and(|c| !c.cmdline.is_empty() && c.cmdline == a.cmdline);
            if !(is_launcher(a) || reloader) {
                break;
            }
            root = anc;
            child = Some(a);
        }
        root
    }

    fn resolve_entry(&self, e: &PortEntry, opts: &StopOptions) -> Resolution {
        let port = e.port;
        let t = self.table();
        let mut details = Vec::new();
        details.push(exposure_detail(e));

        // 1. Containers: stop the container, never the forwarder.
        if let Some(c) = &e.container {
            let pp = self
                .scan
                .published
                .iter()
                .find(|pp| pp.container.id == c.id);
            let svc = match (&c.compose_project, &c.compose_service) {
                (Some(p), Some(s)) => format!(", compose project {p} / service {s}"),
                _ => String::new(),
            };
            let mut r = Resolution::new(
                Owner::Container { container: c.clone(), forwarder_pid: e.pid },
                format!(
                    "Port {port} is published by {} container {} ({}{svc}), mapped to port {} inside the container.",
                    c.runtime, c.name, c.image, c.private_port
                ),
            );
            if let Some(p) = &e.process {
                details.push(format!(
                    "The host-side listener {} (PID {}) is the runtime's port forwarder: killing it would not stop the container and can leave the runtime in a bad state.",
                    p.name, p.pid
                ));
            }
            r.details = details;
            r.recommendation = format!(
                "Stop the container {} (portwise does this via the {} API).",
                c.name, c.runtime
            );
            r.commands.push(format!("docker stop {}", c.name));
            if let (Some(p), Some(s)) = (&c.compose_project, &c.compose_service) {
                r.commands.push(format!("docker compose -p {p} stop {s}"));
            }
            match pp {
                Some(pp) => r.steps.push(Step::StopContainer {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    runtime: c.runtime.clone(),
                    endpoint: endpoint_string(&pp.runtime.endpoint),
                    timeout_s: (opts.timeout_ms / 1000).clamp(1, 60),
                }),
                None => {
                    r = r.block(
                        BlockKind::NothingToStop,
                        "Container runtime endpoint unknown.",
                    )
                }
            }
            return r;
        }

        // 2. Hidden owner.
        let Some(p) = e.process.clone() else {
            let who = e
                .user
                .clone()
                .map(|u| format!("user {u}"))
                .or(e.uid.map(|u| format!("uid {u}")))
                .unwrap_or_else(|| "another user or root".into());
            let mut r = Resolution::new(
                Owner::Hidden { uid: e.uid, user: e.user.clone() },
                format!("Port {port} is held by a process owned by {who}, which portwise can't inspect without elevated privileges."),
            );
            r.details = details;
            r.recommendation =
                "Re-run portwise with elevated privileges to identify and stop it.".into();
            r.commands
                .push(format!("{}portwise explain {port}", sudo()));
            return r.block(
                BlockKind::NeedsElevation,
                format!("The owner of port {port} belongs to {who}; elevation is required."),
            );
        };
        let pid = p.pid;
        let name_l = p.name.to_ascii_lowercase();

        // 3. OS services that must never be killed.
        if cfg!(target_os = "macos") && name_l == "controlcenter" {
            let mut r = Resolution::new(
                Owner::OsService {
                    service: "AirPlay Receiver".into(),
                    pid: Some(pid),
                },
                format!(
                    "Port {port} is held by macOS AirPlay Receiver (ControlCenter, PID {pid})."
                ),
            );
            details.push("Since macOS Monterey, AirPlay Receiver listens on 5000 and 7000, the defaults for Flask and others.".into());
            r.details = details;
            r.recommendation = "Turn off AirPlay Receiver (System Settings → General → AirDrop & Handoff → AirPlay Receiver) or use another port.".into();
            r.commands.push(
                "open \"x-apple.systempreferences:com.apple.AirDrop-Handoff-Settings.extension\""
                    .into(),
            );
            r.commands.push(format!("portwise free-port --near {port}"));
            return r.block(
                BlockKind::OsService,
                "AirPlay Receiver is part of macOS; disable it in System Settings instead of killing it.",
            );
        }
        if cfg!(windows) && (pid == 4 || name_l == "system") {
            let mut r = Resolution::new(
                Owner::OsService { service: "HTTP.sys".into(), pid: Some(pid) },
                format!("Port {port} is owned by the Windows kernel HTTP server (HTTP.sys, PID 4) on behalf of a service such as IIS, WinRM, SSRS or Windows Media sharing."),
            );
            r.details = details;
            r.recommendation = "Find the registered URL/service and stop or reconfigure it (elevated), or use another port.".into();
            r.commands
                .push("netsh http show servicestate view=requestq verbose=no".into());
            r.commands.push("netsh http show urlacl".into());
            return r.block(
                BlockKind::OsService,
                "HTTP.sys is part of Windows; stop the service that registered the URL instead.",
            );
        }
        if name_l.starts_with("wslrelay") {
            let mut r = Resolution::new(
                Owner::OsService { service: "WSL relay".into(), pid: Some(pid) },
                format!("Port {port} is forwarded from WSL by wslrelay.exe (PID {pid}): a server inside your WSL distro is listening."),
            );
            r.details = details;
            r.recommendation =
                "Stop the server inside WSL (e.g. `wsl -- portwise stop <port>`), or restart WSL."
                    .into();
            r.commands.push(format!("wsl -- portwise stop {port}"));
            r.commands.push("wsl --shutdown".into());
            return r.block(
                BlockKind::OsService,
                "This is the WSL localhost relay; stop the server inside WSL instead.",
            );
        }

        // 4. Supervisors (would respawn a plain kill).
        if let Some(r) = self.resolve_supervisor(e, &p, details.clone()) {
            return r;
        }

        // 5. Protected processes.
        let prot = protection(&p, t);
        if let Protection::Hard(reason) | Protection::Soft(reason) = &prot {
            let soft = matches!(prot, Protection::Soft(_));
            if !soft || !opts.allow_protected {
                let mut r = Resolution::new(
                    Owner::Protected {
                        pid,
                        name: p.name.clone(),
                        reason: reason.clone(),
                    },
                    format!("Port {port} is held by {} (PID {pid}): {reason}.", p.name),
                );
                details.extend(process_details(&p, e));
                r.details = details;
                r.recommendation = if soft {
                    "It's protected by default. If you're sure, stop it with --allow-protected, or use another port.".into()
                } else {
                    "portwise will never stop this process. Use another port.".into()
                };
                r.commands.push(format!("portwise free-port --near {port}"));
                if soft {
                    r.commands
                        .push(format!("portwise stop {port} --allow-protected"));
                }
                let msg = format!("{} is protected: {reason}.", p.name);
                return if soft {
                    r.block_overridable(BlockKind::Protected, msg)
                } else {
                    r.block(BlockKind::Protected, msg)
                };
            }
        }

        // 6. Other users' processes.
        if !t.is_mine(pid) && !is_root() {
            let who = p.user.clone().unwrap_or_else(|| "another user".into());
            let mut r = Resolution::new(
                Owner::Process {
                    pid,
                    name: p.name.clone(),
                },
                format!(
                    "Port {port} is held by {} (PID {pid}), owned by {who}.",
                    p.name
                ),
            );
            details.extend(process_details(&p, e));
            r.details = details;
            r.recommendation = "Only the owner or an administrator can stop it.".into();
            r.commands.push(format!("{}portwise stop {port}", sudo()));
            return r.block(
                BlockKind::NeedsElevation,
                format!("{} belongs to {who}; elevation is required.", p.name),
            );
        }

        // 7. Plain process / dev-server tree.
        let root = if opts.tree { self.tree_root(pid) } else { pid };
        let mut pids = vec![root];
        if opts.tree {
            pids.extend(self.safe_descendants(root, opts));
        }
        for extra in &e.pids {
            if !pids.contains(extra)
                && t.get(*extra)
                    .is_some_and(|x| !protection(x, t).is_protected())
            {
                pids.push(*extra);
            }
        }
        let root_p = t.get(root).cloned().unwrap_or_else(|| p.clone());
        let what_s = match &e.framework {
            Some(f) if f.category == FrameworkCategory::DevServer => {
                format!("{} {} dev server", article(&f.name), f.name)
            }
            Some(f) => format!("{} {} process", article(&f.name), f.name),
            None => p.name.clone(),
        };
        let where_s = e
            .project
            .as_ref()
            .map(|pr| {
                format!(
                    " in {}{}",
                    tilde(&pr.root),
                    pr.git_branch
                        .as_ref()
                        .map(|b| format!(" (branch {b})"))
                        .unwrap_or_default()
                )
            })
            .unwrap_or_default();
        let started_by = if root != pid {
            format!(", started by `{}`", short_cmd(&root_p))
        } else {
            String::new()
        };
        let age = now_secs().saturating_sub(p.start_time);
        let owner = if pids.len() > 1 {
            Owner::ProcessTree {
                root_pid: root,
                root_name: root_p.name.clone(),
                pids: pids.clone(),
            }
        } else {
            Owner::Process {
                pid,
                name: p.name.clone(),
            }
        };
        let mut r = Resolution::new(
            owner,
            format!(
                "Port {port} is held by {what_s} ({}, PID {pid}){started_by}{where_s}, running for {}.",
                p.name,
                human_duration(age)
            ),
        );
        details.extend(process_details(&p, e));
        if pids.len() > 1 {
            let chain: Vec<String> = pids
                .iter()
                .filter_map(|x| t.get(*x))
                .map(|x| format!("{} ({})", x.name, x.pid))
                .collect();
            details.push(format!("Process tree to stop: {}", chain.join(" → ")));
        }
        if cfg!(target_os = "macos") && p.ppid == Some(1) && root == pid {
            r.warnings.push(format!(
                "{} was started by launchd. If it's a LaunchAgent with KeepAlive it will be respawned; use `launchctl bootout` for it instead.",
                p.name
            ));
        }
        if !e.is_dev {
            r.risk = Risk::Medium;
            r.warnings
                .push(format!("{} doesn't look like a dev server.", p.name));
        }
        r.details = details;
        r.recommendation = if opts.force {
            format!(
                "Force-kill {} process(es) and verify port {port} is free.",
                pids.len()
            )
        } else if pids.len() > 1 {
            format!(
                "Gracefully stop the {} process tree ({} processes): SIGTERM, then SIGKILL after {}, then verify port {port} is free.",
                root_p.name,
                pids.len(),
                fmt_ms(opts.timeout_ms)
            )
        } else {
            format!(
                "Gracefully stop {} (SIGTERM, then SIGKILL after {}) and verify port {port} is free.",
                p.name,
                fmt_ms(opts.timeout_ms)
            )
        };
        r.commands.push(format!("portwise stop {port}"));
        r.commands.push(if cfg!(windows) {
            format!("taskkill /PID {root} /T")
        } else {
            format!("kill -TERM {root}")
        });
        r.steps.push(self.signal_step(&pids, opts));
        r
    }

    fn resolve_supervisor(
        &self,
        e: &PortEntry,
        p: &ProcessInfo,
        details: Vec<String>,
    ) -> Option<Resolution> {
        let port = e.port;
        let t = self.table();
        #[cfg(target_os = "linux")]
        {
            if let Some(r) = self.resolve_systemd(e, p, details.clone()) {
                return Some(r);
            }
        }
        // pm2: the God Daemon respawns killed apps.
        let ancestors = t.ancestors(p.pid);
        let under_pm2 = ancestors.iter().filter_map(|a| t.get(*a)).any(|a| {
            a.name.starts_with("PM2") || a.cmdline.first().is_some_and(|c| c.starts_with("PM2"))
        });
        if under_pm2 {
            let (name, id) =
                pm2_app(p.pid, &ancestors).unwrap_or_else(|| (p.name.clone(), p.name.clone()));
            let mut r = Resolution::new(
                Owner::Supervised {
                    supervisor: Supervisor::Pm2 { name: name.clone(), id: id.clone() },
                    pid: p.pid,
                    name: p.name.clone(),
                },
                format!("Port {port} is held by pm2 app \"{name}\" ({}, PID {}). pm2 restarts apps that are killed, so stop it through pm2.", p.name, p.pid),
            );
            let mut d = details;
            d.extend(process_details(p, e));
            r.details = d;
            r.recommendation = format!("Stop it with `pm2 stop {id}`.");
            r.commands.push(format!("pm2 stop {id}"));
            r.steps.push(Step::RunCommand {
                program: "pm2".into(),
                args: vec!["stop".into(), id],
                reason: "stop the pm2-managed app".into(),
            });
            return Some(r);
        }
        // Homebrew services (launchd agents) on macOS.
        if cfg!(target_os = "macos") {
            if let Some(formula) = brew_formula(p) {
                let mut r = Resolution::new(
                    Owner::Supervised {
                        supervisor: Supervisor::BrewService { formula: formula.clone() },
                        pid: p.pid,
                        name: p.name.clone(),
                    },
                    format!("Port {port} is held by {} (PID {}), started by `brew services` ({formula}). launchd would restart it if killed.", p.name, p.pid),
                );
                let mut d = details;
                d.extend(process_details(p, e));
                r.details = d;
                r.recommendation = format!("Stop the service: `brew services stop {formula}`.");
                r.commands.push(format!("brew services stop {formula}"));
                r.steps.push(Step::RunCommand {
                    program: "brew".into(),
                    args: vec!["services".into(), "stop".into(), formula],
                    reason: "stop the Homebrew service".into(),
                });
                return Some(r);
            }
        }
        None
    }

    #[cfg(target_os = "linux")]
    fn resolve_systemd(
        &self,
        e: &PortEntry,
        p: &ProcessInfo,
        details: Vec<String>,
    ) -> Option<Resolution> {
        let port = e.port;
        let t = self.table();
        // Socket activation: the listening socket is held by PID 1 / `systemd --user`.
        let is_systemd =
            p.pid == 1 || (p.name == "systemd" && p.cmdline.iter().any(|a| a == "--user"));
        if is_systemd {
            let user = p.pid != 1;
            let (unit, service) = systemd_socket_unit(port, user)
                .unwrap_or_else(|| (format!("<unit>.socket (port {port})"), None));
            let mut r = Resolution::new(
                Owner::Supervised {
                    supervisor: Supervisor::SystemdSocket { unit: unit.clone(), user, service: service.clone() },
                    pid: p.pid,
                    name: p.name.clone(),
                },
                format!("Port {port} is held by systemd socket activation ({unit}). systemd itself owns the socket, so killing the service wouldn't free it: systemd would start it again on the next connection."),
            );
            r.details = details;
            let mut units = vec![unit.clone()];
            units.extend(service);
            let cmd = format!(
                "{}systemctl {}stop {}",
                if user { "" } else { "sudo " },
                if user { "--user " } else { "" },
                units.join(" ")
            );
            r.recommendation = format!("Stop the socket unit (and its service): `{cmd}`.");
            r.commands.push(cmd);
            if unit.contains('<') {
                return Some(r.block(
                    BlockKind::NothingToStop,
                    "Couldn't determine the socket unit; see `systemctl list-sockets`.",
                ));
            }
            if user || is_root() {
                let mut args: Vec<String> = if user { vec!["--user".into()] } else { vec![] };
                args.push("stop".into());
                args.extend(units);
                r.steps.push(Step::RunCommand {
                    program: "systemctl".into(),
                    args,
                    reason: "stop the socket-activated unit".into(),
                });
            } else {
                r = r.block(
                    BlockKind::NeedsElevation,
                    "Stopping a system unit requires root.",
                );
            }
            return Some(r);
        }
        let cg = sys::linux::cgroup_unit(p.pid)?;
        // Our own unit (e.g. the terminal or agent service portwise runs in) is not a supervisor
        // of the dev servers started from it.
        if sys::linux::cgroup_unit(t.self_pid()).as_ref() == Some(&cg) {
            return None;
        }
        let main = systemd_main_pid(&cg.unit, cg.user)?;
        let anc = t.ancestors(p.pid);
        if !(main == p.pid || (anc.contains(&main) && !t.is_self_or_ancestor(main))) {
            return None;
        }
        let mut r = Resolution::new(
            Owner::Supervised {
                supervisor: Supervisor::SystemdService { unit: cg.unit.clone(), user: cg.user },
                pid: p.pid,
                name: p.name.clone(),
            },
            format!(
                "Port {port} is held by {} (PID {}), run by the systemd {}service {}. Killing it would let systemd restart it (Restart=), so stop the unit instead.",
                p.name,
                p.pid,
                if cg.user { "user " } else { "" },
                cg.unit
            ),
        );
        let mut d = details;
        d.extend(process_details(p, e));
        r.details = d;
        let cmd = if cg.user {
            format!("systemctl --user stop {}", cg.unit)
        } else {
            format!("sudo systemctl stop {}", cg.unit)
        };
        r.recommendation = format!("Stop the unit: `{cmd}`.");
        r.commands.push(cmd);
        if cg.user || is_root() {
            let mut args: Vec<String> = if cg.user {
                vec!["--user".into()]
            } else {
                vec![]
            };
            args.extend(["stop".to_string(), cg.unit.clone()]);
            r.steps.push(Step::RunCommand {
                program: "systemctl".into(),
                args,
                reason: "stop the systemd unit".into(),
            });
        } else {
            r = r.block(
                BlockKind::NeedsElevation,
                format!(
                    "{} is a system service; stopping it requires root.",
                    cg.unit
                ),
            );
        }
        Some(r)
    }
}

fn article(word: &str) -> &'static str {
    match word.chars().next().map(|c| c.to_ascii_lowercase()) {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

fn blocked_plan(
    target: String,
    owners: Vec<Owner>,
    kind: BlockKind,
    msg: impl Into<String>,
) -> ActionPlan {
    let msg = msg.into();
    ActionPlan {
        target,
        owners,
        summary: msg.clone(),
        steps: vec![],
        blocked: Some(Blocked {
            kind,
            message: msg,
            overridable: false,
        }),
        warnings: vec![],
        risk: Risk::Low,
    }
}

fn endpoint_string(ep: &docker::Endpoint) -> String {
    match ep {
        docker::Endpoint::Unix(p) => format!("unix://{}", p.display()),
        docker::Endpoint::Tcp(a) => format!("tcp://{a}"),
        docker::Endpoint::Pipe(p) => format!("npipe://{}", p.replace('\\', "/")),
    }
}

fn exposure_detail(e: &PortEntry) -> String {
    match e.exposure {
        Exposure::AllInterfaces => format!(
            "Listening on all interfaces ({}): reachable from other devices on your network.",
            e.addresses.join(", ")
        ),
        Exposure::Loopback => format!(
            "Listening on {} (this machine only).",
            e.addresses.join(", ")
        ),
        Exposure::Specific => format!("Listening on {}.", e.addresses.join(", ")),
    }
}

fn short_cmd(p: &ProcessInfo) -> String {
    let parts: Vec<String> = p
        .cmdline
        .iter()
        .take(4)
        .map(|a| {
            std::path::Path::new(a)
                .file_name()
                .filter(|_| a.contains('/') || a.contains('\\'))
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| a.clone())
        })
        .collect();
    if parts.is_empty() {
        p.name.clone()
    } else {
        parts.join(" ")
    }
}

fn process_details(p: &ProcessInfo, e: &PortEntry) -> Vec<String> {
    let mut d = vec![format!(
        "Process: {} (PID {}){}{}",
        p.name,
        p.pid,
        p.user
            .as_ref()
            .map(|u| format!(", user {u}"))
            .unwrap_or_default(),
        if p.memory_bytes > 0 {
            format!(", {} memory", human_bytes(p.memory_bytes))
        } else {
            String::new()
        }
    )];
    if !p.cmdline.is_empty() {
        let mut c = p.command();
        if c.chars().count() > 160 {
            c = c.chars().take(157).collect();
            c.push('…');
        }
        d.push(format!("Command: {c}"));
    }
    if let Some(pr) = &e.project {
        d.push(format!(
            "Project: {} ({}{})",
            pr.name,
            tilde(&pr.root),
            pr.git_branch
                .as_ref()
                .map(|b| format!(", branch {b}"))
                .unwrap_or_default()
        ));
    } else if let Some(cwd) = &p.cwd {
        d.push(format!("Working directory: {}", tilde(cwd)));
    }
    d
}

/// Launchers whose children are part of the same dev session.
fn is_launcher(p: &ProcessInfo) -> bool {
    // npm/yarn/pnpm rename themselves ("npm run dev"), so look at the first word only.
    let full = p.name.to_ascii_lowercase();
    let first = full.split_whitespace().next().unwrap_or("");
    let n = first.strip_suffix(".exe").unwrap_or(first);
    const LAUNCHERS: &[&str] = &[
        "npm",
        "npx",
        "pnpm",
        "pnpx",
        "yarn",
        "bun",
        "bunx",
        "nodemon",
        "turbo",
        "nx",
        "concurrently",
        "cargo",
        "cargo-watch",
        "watchexec",
        "air",
        "reflex",
        "foreman",
        "overmind",
        "honcho",
        "make",
        "just",
        "task",
        "uv",
        "poetry",
        "pipenv",
        "rye",
        "hatch",
        "mix",
        "bundle",
        "dotnet",
        "deno",
        "tsx",
        "ts-node",
        "vite-node",
        "go",
    ];
    if LAUNCHERS.contains(&n) {
        return true;
    }
    let args: Vec<String> = p.cmdline.iter().map(|a| a.to_ascii_lowercase()).collect();
    if n.starts_with("node") {
        const JS_LAUNCHERS: &[&str] = &[
            "npm-cli.js",
            "npx-cli.js",
            "yarn.js",
            "yarn.cjs",
            "pnpm.cjs",
            "pnpm.js",
            "/bin/npm",
            "/bin/yarn",
            "/bin/pnpm",
            "nodemon",
            "concurrently",
            "turbo",
        ];
        return args
            .iter()
            .skip(1)
            .take(2)
            .any(|a| JS_LAUNCHERS.iter().any(|l| a.contains(l)));
    }
    // Non-interactive shells (`sh -c "next dev"`) spawned to run a script.
    const SHELLS: &[&str] = &[
        "sh",
        "bash",
        "zsh",
        "dash",
        "fish",
        "ash",
        "cmd",
        "powershell",
        "pwsh",
    ];
    if SHELLS.contains(&n) {
        return args
            .iter()
            .skip(1)
            .any(|a| a == "-c" || a == "/c" || a == "-command");
    }
    false
}

#[cfg(target_os = "linux")]
fn systemd_socket_unit(port: u16, user: bool) -> Option<(String, Option<String>)> {
    let mut args = vec!["list-sockets", "--all", "--no-legend", "--no-pager"];
    if user {
        args.insert(0, "--user");
    }
    let out = run_with_timeout("systemctl", &args, Duration::from_secs(3))?;
    parse_list_sockets(&out.stdout, port)
}

/// Parse `systemctl list-sockets --no-legend` lines: `LISTEN UNIT ACTIVATES`.
pub fn parse_list_sockets(text: &str, port: u16) -> Option<(String, Option<String>)> {
    let suffix = format!(":{port}");
    text.lines().find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        let listen = *f.first()?;
        (listen.ends_with(&suffix) || listen == port.to_string()).then(|| {
            let unit = f
                .iter()
                .find(|x| x.ends_with(".socket"))
                .map(|s| s.to_string())
                .unwrap_or_default();
            let service = f
                .iter()
                .find(|x| x.ends_with(".service"))
                .map(|s| s.to_string());
            (unit, service)
        })
    })
}

#[cfg(target_os = "linux")]
fn systemd_main_pid(unit: &str, user: bool) -> Option<u32> {
    let mut args = vec!["show", "-p", "MainPID", "--value", unit];
    if user {
        args.insert(0, "--user");
    }
    let out = run_with_timeout("systemctl", &args, Duration::from_secs(3))?;
    out.stdout.trim().parse().ok().filter(|p| *p != 0)
}

fn pm2_app(pid: u32, ancestors: &[u32]) -> Option<(String, String)> {
    let out = run_with_timeout("pm2", &["jlist"], Duration::from_secs(5))?;
    let v: serde_json::Value = serde_json::from_str(out.stdout.trim()).ok()?;
    v.as_array()?.iter().find_map(|app| {
        let apid = app["pid"].as_u64()? as u32;
        let name = app["name"].as_str().unwrap_or("app").to_string();
        (apid == pid || ancestors.contains(&apid)).then(|| {
            let id = app["pm_id"]
                .as_u64()
                .map(|i| i.to_string())
                .unwrap_or_else(|| name.clone());
            (name, id)
        })
    })
}

/// Homebrew formula for a launchd-started process under `/opt/homebrew` or `/usr/local`.
fn brew_formula(p: &ProcessInfo) -> Option<String> {
    if p.ppid != Some(1) {
        return None;
    }
    let exe = p.exe.as_ref()?.to_string_lossy().into_owned();
    let formula = exe
        .split("/Cellar/")
        .nth(1)
        .or_else(|| exe.split("/opt/homebrew/opt/").nth(1))
        .or_else(|| exe.split("/usr/local/opt/").nth(1))?
        .split('/')
        .next()?
        .to_string();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    let base = formula.split('@').next().unwrap_or(&formula).to_string();
    [formula.clone(), base].into_iter().find(|f| {
        let plist = format!("homebrew.mxcl.{f}.plist");
        home.join("Library/LaunchAgents").join(&plist).exists()
            || std::path::Path::new("/Library/LaunchDaemons")
                .join(&plist)
                .exists()
    })
}

/// Is anything still listening on `port`/`protocol`? (socket table + bind probe).
pub fn port_busy(port: u16, protocol: Protocol) -> bool {
    let listed = sys::list_sockets()
        .map(|s| {
            s.iter()
                .any(|s| s.local_port == port && s.protocol == protocol && s.state.is_listening())
        })
        .unwrap_or(false);
    if listed {
        return true;
    }
    let probe = match protocol {
        Protocol::Tcp => probe::probe_tcp(port),
        Protocol::Udp => probe::probe_udp(port),
    };
    probe == ProbeResult::InUse
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::execute;
    use crate::process::tests::{proc, table};

    fn engine(procs: Vec<ProcessInfo>, raw: Vec<RawSocket>, self_pid: u32) -> Engine {
        let t = table(procs, self_pid);
        let (entries, hidden) = scan::build_entries(&raw, &t, &[], false);
        Engine::from_scan(Scan {
            snapshot: Snapshot {
                entries,
                hidden_sockets: hidden,
                platform: "test".into(),
                taken_at_ms: 0,
                scan_ms: 0,
                docker_available: false,
                warnings: vec![],
            },
            table: t,
            published: vec![],
            raw,
        })
    }

    fn listen(port: u16, pids: &[u32]) -> RawSocket {
        RawSocket {
            protocol: Protocol::Tcp,
            family: Family::V4,
            local_addr: "127.0.0.1".parse().unwrap(),
            local_port: port,
            remote_addr: None,
            remote_port: None,
            state: SocketState::Listen,
            uid: Some(1000),
            inode: Some(9),
            pids: pids.to_vec(),
        }
    }

    fn dev_tree() -> Vec<ProcessInfo> {
        vec![
            proc(1, 0, "init", &[]),
            proc(10, 1, "zsh", &["-zsh"]),
            proc(20, 10, "npm", &["npm", "run", "dev"]),
            proc(21, 20, "sh", &["sh", "-c", "next dev"]),
            proc(
                22,
                21,
                "node",
                &["node", "/app/node_modules/.bin/next", "dev"],
            ),
            proc(23, 22, "node", &["node", "worker.js"]),
            proc(90, 1, "sshd", &["sshd"]),
            proc(98, 1, "bash", &["bash"]),
            proc(99, 98, "portwise", &["portwise"]),
        ]
    }

    #[test]
    fn tree_root_climbs_launchers_but_not_interactive_shell() {
        let e = engine(dev_tree(), vec![listen(3000, &[22])], 99);
        assert_eq!(e.tree_root(22), 20);
    }

    #[test]
    fn explain_dev_server_tree() {
        let e = engine(dev_tree(), vec![listen(3000, &[22])], 99);
        let ex = e.explain(3000, &StopOptions::default());
        assert_eq!(ex.status, PortStatus::Busy);
        assert!(
            ex.headline.contains("a Next.js dev server"),
            "{}",
            ex.headline
        );
        assert!(
            ex.headline.contains("started by `npm run dev`"),
            "{}",
            ex.headline
        );
        assert_eq!(
            ex.owners[0],
            Owner::ProcessTree {
                root_pid: 20,
                root_name: "npm".into(),
                pids: vec![20, 21, 22, 23]
            }
        );
        let plan = ex.plan.unwrap();
        assert!(plan.blocked.is_none());
        assert_eq!(plan.risk, Risk::Low);
        match &plan.steps[0] {
            Step::SignalProcesses {
                processes, force, ..
            } => {
                assert_eq!(
                    processes.iter().map(|p| p.pid).collect::<Vec<_>>(),
                    vec![20, 21, 22, 23]
                );
                assert!(!force);
                assert_eq!(
                    processes[2].start_token, 1022,
                    "start token pinned for PID-reuse guard"
                );
            }
            s => panic!("unexpected {s:?}"),
        }
        assert!(matches!(
            plan.steps.last(),
            Some(Step::VerifyFree { port: 3000, .. })
        ));
    }

    #[test]
    fn protected_owner_is_blocked() {
        let e = engine(dev_tree(), vec![listen(22, &[90])], 99);
        let plan = e.explain(22, &StopOptions::default()).plan.unwrap();
        assert_eq!(plan.blocked.as_ref().unwrap().kind, BlockKind::Protected);
        assert!(plan.steps.is_empty());
        // --allow-protected lifts soft protection only.
        let opts = StopOptions {
            allow_protected: true,
            ..Default::default()
        };
        assert!(e.explain(22, &opts).plan.unwrap().blocked.is_none());
    }

    #[test]
    fn never_targets_self_or_ancestors() {
        let e = engine(dev_tree(), vec![], 99);
        for pid in [99, 98, 1] {
            let opts = StopOptions {
                allow_protected: true,
                ..Default::default()
            };
            let plan = e.plan(&Target::Pid(pid), &opts);
            let b = plan.blocked.unwrap();
            assert_eq!(b.kind, BlockKind::Protected, "pid {pid}");
            assert!(
                !b.overridable,
                "pid {pid}: hard protection is never overridable"
            );
        }
    }

    #[test]
    fn session_hosts_are_blocked_unless_allowed() {
        // An IDE remote server holding a forwarded port: refused by default, allowed explicitly.
        let mut procs = dev_tree();
        procs.push(proc(
            70,
            1,
            "node",
            &["node", "/home/dev/.vscode-server/bin/x/out/server-main.js"],
        ));
        procs.push(proc(71, 70, "zsh", &["/bin/zsh", "-l"]));
        let e = engine(procs, vec![listen(41000, &[70])], 99);
        let entry = e
            .snapshot()
            .entries
            .iter()
            .find(|x| x.port == 41000)
            .unwrap();
        assert!(entry.protected);
        let plan = e.plan(&Target::Port(41000), &StopOptions::default());
        let b = plan.blocked.unwrap();
        assert_eq!(b.kind, BlockKind::Protected);
        assert!(b.overridable, "soft protection can be overridden");
        let opts = StopOptions {
            allow_protected: true,
            ..Default::default()
        };
        assert!(e.plan(&Target::Port(41000), &opts).blocked.is_none());
    }

    #[test]
    fn hidden_owner_needs_elevation() {
        let e = engine(dev_tree(), vec![listen(631, &[])], 99);
        let plan = e.explain(631, &StopOptions::default()).plan.unwrap();
        assert_eq!(plan.blocked.unwrap().kind, BlockKind::NeedsElevation);
    }

    #[test]
    fn plan_by_name_and_pid() {
        let e = engine(dev_tree(), vec![listen(3000, &[22])], 99);
        let plan = e.plan(&Target::Name("npm".into()), &StopOptions::default());
        assert!(plan.blocked.is_none());
        let plan = e.plan(&Target::Name("nope".into()), &StopOptions::default());
        assert_eq!(plan.blocked.unwrap().kind, BlockKind::NothingToStop);
        let opts = StopOptions {
            tree: false,
            force: true,
            ..Default::default()
        };
        let plan = e.plan(&Target::Pid(22), &opts);
        match &plan.steps[0] {
            Step::SignalProcesses {
                processes, force, ..
            } => {
                assert_eq!(processes.len(), 1);
                assert!(force);
            }
            s => panic!("{s:?}"),
        }
    }

    #[test]
    fn target_parsing() {
        assert_eq!(Target::parse("3000"), Target::Port(3000));
        assert_eq!(Target::parse(":8080"), Target::Port(8080));
        assert_eq!(Target::parse("pid:42"), Target::Pid(42));
        assert_eq!(Target::parse("vite"), Target::Name("vite".into()));
        assert_eq!(Target::parse("70000"), Target::Name("70000".into()));
    }

    #[test]
    fn list_sockets_parsing() {
        let text = "[::]:8080   app.socket   app.service\n/run/x.sock  x.socket  x.service\n";
        assert_eq!(
            parse_list_sockets(text, 8080),
            Some(("app.socket".into(), Some("app.service".into())))
        );
        assert_eq!(parse_list_sockets(text, 80), None);
    }

    #[test]
    fn launchers() {
        assert!(is_launcher(&proc(1, 0, "npm", &["npm", "run", "dev"])));
        assert!(is_launcher(&proc(
            1,
            0,
            "node",
            &[
                "node",
                "/usr/lib/node_modules/npm/bin/npm-cli.js",
                "run",
                "dev"
            ]
        )));
        assert!(is_launcher(&proc(1, 0, "sh", &["sh", "-c", "vite"])));
        assert!(!is_launcher(&proc(1, 0, "zsh", &["-zsh"])));
        assert!(!is_launcher(&proc(1, 0, "node", &["node", "server.js"])));
        assert!(is_launcher(&proc(1, 0, "npm run dev", &["npm run dev"])));
    }

    #[test]
    fn blocked_plan_executes_nothing() {
        let plan = blocked_plan(":1".into(), vec![], BlockKind::Protected, "nope");
        let r = execute(&plan, &mut |_| {});
        assert!(!r.success);
        assert!(r.signalled.is_empty());
        assert_eq!(r.error.as_deref(), Some("nope"));
    }

    #[test]
    fn plan_round_trips_through_json() {
        let e = engine(dev_tree(), vec![listen(3000, &[22])], 99);
        let plan = e.plan(&Target::Port(3000), &StopOptions::default());
        let json = serde_json::to_string(&plan).unwrap();
        let back: ActionPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan, back);
    }

    #[test]
    fn free_port_explanation() {
        let e = engine(dev_tree(), vec![], 99);
        let port = crate::probe::ephemeral_port().unwrap();
        let ex = e.explain(port, &StopOptions::default());
        assert_eq!(ex.status, PortStatus::Free);
        assert!(ex.plan.is_none());
        let plan = e.plan(&Target::Port(port), &StopOptions::default());
        assert_eq!(plan.blocked.unwrap().kind, BlockKind::NothingToStop);
    }
}

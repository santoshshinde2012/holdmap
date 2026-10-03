//! Owner resolution, the `explain` engine and stop planning.
//!
//! Flow: [`Scan`] → [`Engine::explain`] / [`Engine::plan`] → (user confirms) →
//! [`execute`](crate::exec::execute) → [`StopReport`] (with a port-free verification).
//!
//! The engine is a thin facade: *who* owns a port and *how* to stop it is decided by the
//! [`StopStrategy`] chain in [`strategies`], *whether* a process may be touched by the injected
//! [`ProtectionPolicy`], and the machine state comes from a [`Scan`] (built by any
//! [`Scanner`](crate::scan::Scanner)).

mod context;
mod resolution;
pub mod strategies;
mod text;
mod types;

pub use context::ResolveCtx;
pub use resolution::{blocked_plan, Resolution};
pub use strategies::{is_launcher, parse_list_sockets, StopStrategy, StrategyRegistry};
pub use text::tilde;
pub use types::*;

use crate::model::*;
use crate::probe::{self, ProbeResult};
use crate::process::ProcessTable;
use crate::safety::{DefaultProtectionPolicy, Protection, ProtectionPolicy};
use crate::scan::{self, Scan, ScanOptions};
use crate::sys;
use std::collections::BTreeSet;
use std::sync::Arc;
use text::{exposure_detail, sudo};

/// The engine: a consistent view of the machine plus the operations on it.
#[derive(Debug, Clone)]
pub struct Engine {
    pub scan: Scan,
    policy: Arc<dyn ProtectionPolicy>,
    strategies: StrategyRegistry,
}

impl Engine {
    /// Scan the live system with the default policy and strategies.
    pub fn new(opts: &ScanOptions) -> std::io::Result<Self> {
        Ok(Self::from_scan(scan::scan(opts)?))
    }

    pub fn from_scan(scan: Scan) -> Self {
        Self {
            scan,
            policy: Arc::new(DefaultProtectionPolicy),
            strategies: StrategyRegistry::default(),
        }
    }

    /// Replace the protection policy (dependency injection, e.g. for tests or embedders).
    pub fn with_policy(mut self, policy: Arc<dyn ProtectionPolicy>) -> Self {
        self.policy = policy;
        self
    }

    /// Replace the stop-strategy chain.
    pub fn with_strategies(mut self, strategies: StrategyRegistry) -> Self {
        self.strategies = strategies;
        self
    }

    pub fn policy(&self) -> &dyn ProtectionPolicy {
        self.policy.as_ref()
    }

    pub fn strategies(&self) -> &StrategyRegistry {
        &self.strategies
    }

    fn ctx<'a>(&'a self, opts: &'a StopOptions) -> ResolveCtx<'a> {
        ResolveCtx::new(&self.scan, self.policy.as_ref(), opts)
    }

    /// Climb from the socket holder to the dev-session root (e.g. node → sh -c → npm).
    pub fn tree_root(&self, pid: u32) -> u32 {
        self.ctx(&StopOptions::default()).tree_root(pid)
    }

    fn resolve_entry(&self, e: &PortEntry, opts: &StopOptions) -> Resolution {
        let details = vec![exposure_detail(e)];
        self.strategies.resolve(&self.ctx(opts), e, &details)
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

    pub(crate) fn plan_pids(&self, target: String, pids: &[u32], opts: &StopOptions) -> ActionPlan {
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
            match self.policy.protection(p, t) {
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
                        tree.extend(self.ctx(opts).safe_descendants(pid));
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
                    let step = self.ctx(opts).signal_step(&tree);
                    r.recommendation = step.describe();
                    r.steps.push(step);
                }
            }
            resolutions.push(r);
        }
        let entries = self.ctx(opts).entries_held_by(pids);
        if !entries.is_empty() && entries.iter().all(|e| e.is_dev) {
            for r in &mut resolutions {
                if r.risk == Risk::Medium {
                    r.risk = Risk::Low;
                }
            }
        }
        self.combine(target, &resolutions, &entries, opts)
    }

    pub(crate) fn combine(
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

    #[derive(Debug)]
    struct ProtectEverything;
    impl ProtectionPolicy for ProtectEverything {
        fn protection(&self, _: &ProcessInfo, _: &ProcessTable) -> Protection {
            Protection::Hard("test policy protects everything".into())
        }
    }

    #[test]
    fn injected_policy_is_respected() {
        let e = engine(dev_tree(), vec![listen(3000, &[22])], 99)
            .with_policy(std::sync::Arc::new(ProtectEverything));
        let plan = e.plan(&Target::Port(3000), &StopOptions::default());
        assert!(plan.is_blocked());
        assert_eq!(plan.blocked.unwrap().kind, BlockKind::Protected);
    }

    struct AlwaysRefuse;
    impl StopStrategy for AlwaysRefuse {
        fn name(&self) -> &'static str {
            "refuse"
        }
        fn resolve(&self, _: &ResolveCtx, e: &PortEntry, _: &[String]) -> Option<Resolution> {
            Some(
                Resolution::new(Owner::Free, format!("custom {}", e.port))
                    .block(BlockKind::NothingToStop, "custom strategy"),
            )
        }
    }

    #[test]
    fn strategies_are_open_for_extension() {
        let e = engine(dev_tree(), vec![listen(3000, &[22])], 99);
        assert_eq!(e.strategies().names().last(), Some(&"process-tree"));
        let e = e.with_strategies(StrategyRegistry::default().with_first(AlwaysRefuse));
        let ex = e.explain(3000, &StopOptions::default());
        assert_eq!(ex.headline, "custom 3000");
        // An empty registry still falls back to the process-tree strategy.
        let e = e.with_strategies(StrategyRegistry::empty());
        let plan = e.plan(&Target::Port(3000), &StopOptions::default());
        assert!(!plan.is_blocked(), "{plan:?}");
    }
}

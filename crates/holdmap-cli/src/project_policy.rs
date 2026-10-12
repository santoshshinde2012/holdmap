//! Project port protection is composed by local CLI/TUI actions, outside the shared engine.

use anyhow::{Context, Result};
use holdmap_core::stack::{self, Stack};
use holdmap_core::{
    ActionPlan, BlockKind, Blocked, Engine, Explanation, Owner, ScanOptions, Step, StopOptions,
    Supervisor,
};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

#[derive(Debug)]
pub(crate) struct ProtectionRefusal(String);

impl std::fmt::Display for ProtectionRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProtectionRefusal {}

#[derive(Debug, Default)]
pub(crate) struct ProjectProtection {
    ports: BTreeSet<u16>,
}

impl ProjectProtection {
    #[cfg(test)]
    pub(crate) fn testing(ports: &[u16]) -> Self {
        Self {
            ports: ports.iter().copied().collect(),
        }
    }
    pub fn nearest() -> Result<Self> {
        Self::from_dir(&std::env::current_dir().context("cannot locate the current project")?)
    }

    fn from_dir(cwd: &Path) -> Result<Self> {
        // An existing but malformed, unreadable, dangling or non-regular project file must
        // not silently disable protection or let an outer project take its place.
        for dir in cwd.ancestors() {
            for name in [stack::FILE_NAME, stack::LEGACY_FILE_NAME] {
                let path = dir.join(name);
                match std::fs::symlink_metadata(&path) {
                    Ok(_) => {
                        let mut policy = Self::default();
                        policy.include_file(&path)?;
                        return Ok(policy);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error).context("cannot inspect project protection"),
                }
            }
        }
        Ok(Self::default())
    }

    pub fn for_stack(stack: &Stack) -> Result<Self> {
        let mut policy = Self::nearest()?;
        policy.ports.extend(&stack.protect);
        Ok(policy)
    }

    fn include_file(&mut self, path: &Path) -> Result<()> {
        let project = stack::load(path).with_context(|| {
            format!(
                "cannot validate project protection in {}; no action was taken",
                path.display()
            )
        })?;
        self.ports.extend(project.protect);
        Ok(())
    }

    pub fn check_port(&self, port: u16) -> Result<()> {
        if self.ports.contains(&port) {
            return Err(ProtectionRefusal(format!(
                "Port {port} is listed in project `protect`; no action was taken."
            ))
            .into());
        }
        Ok(())
    }

    pub fn apply(&self, engine: &Engine, mut plan: ActionPlan) -> ActionPlan {
        if plan.is_blocked() || plan.allow_protected || self.ports.is_empty() {
            return plan;
        }
        let ports = self.affected_ports(engine, &plan);
        if !ports.is_empty() {
            let ports = ports
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let message = format!("This action affects port(s) {ports} listed in project `protect`. Pass --allow-protected to `holdmap stop` or `holdmap kill` to override project protection.");
            plan.summary = message.clone();
            plan.blocked = Some(Blocked {
                kind: BlockKind::Protected,
                message,
                overridable: true,
            });
            plan.steps.clear();
        }
        plan
    }

    pub fn explain(&self, engine: &Engine, mut explanation: Explanation) -> Explanation {
        if let Some(plan) = explanation.plan.take() {
            let previous = plan.blocked.clone();
            let plan = self.apply(engine, plan);
            if let Some(blocked) = plan.blocked.as_ref().filter(|_| plan.blocked != previous) {
                explanation.recommendation = blocked.message.clone();
                explanation.commands.clear();
            }
            explanation.plan = Some(plan);
        }
        explanation
    }

    fn affected_ports(&self, engine: &Engine, plan: &ActionPlan) -> BTreeSet<u16> {
        let mut affected = BTreeSet::new();
        let mut pids = HashSet::new();
        for step in &plan.steps {
            let references: &[holdmap_core::ProcRef] = match step {
                Step::SignalProcesses { processes, .. } => processes,
                Step::RunCommand {
                    guard: Some(guard), ..
                } => std::slice::from_ref(guard),
                _ => &[],
            };
            for reference in references {
                if engine.table().get(reference.pid).is_some_and(|p| {
                    reference.start_token != 0 && p.start_token == reference.start_token
                }) {
                    pids.insert(reference.pid);
                }
            }
            match step {
                Step::VerifyFree { port, .. } if self.ports.contains(port) => {
                    affected.insert(*port);
                }
                Step::StopContainer { id, endpoint, .. } => {
                    let runtime = holdmap_core::docker::parse_docker_host(endpoint);
                    for published in &engine.scan.published {
                        if published.container.id == *id
                            && runtime.as_ref() == Some(&published.runtime.endpoint)
                            && self.ports.contains(&published.host_port)
                        {
                            affected.insert(published.host_port);
                        }
                    }
                }
                _ => {}
            }
        }
        // Traverse the union once: plans may already contain thousands of related PIDs.
        // A stopped launcher can shut down children even when --no-tree signals only it.
        let mut queue: std::collections::VecDeque<_> = pids.iter().copied().collect();
        while let Some(pid) = queue.pop_front() {
            for &child in engine.table().children(pid) {
                if pids.insert(child) {
                    queue.push_back(child);
                }
            }
        }
        for socket in &engine.scan.raw {
            if socket.state.is_listening()
                && self.ports.contains(&socket.local_port)
                && socket.pids.iter().any(|pid| pids.contains(pid))
            {
                affected.insert(socket.local_port);
            }
        }
        // A supervisor command stops a whole unit/app, including other processes in it.
        // Resolve protected rows through the same supervisor strategies as the stop plan.
        for entry in &engine.snapshot().entries {
            if !entry.state.is_listening() || !self.ports.contains(&entry.port) {
                continue;
            }
            if entry.pids.iter().any(|pid| pids.contains(pid)) {
                affected.insert(entry.port);
            }
            if plan
                .steps
                .iter()
                .any(|s| matches!(s, Step::RunCommand { .. }))
            {
                let explanation = engine.explain(
                    entry.port,
                    &StopOptions {
                        protocol: Some(entry.protocol),
                        ..Default::default()
                    },
                );
                if explanation.owners.iter().any(|owner| match owner {
                    Owner::Supervised { supervisor, .. } => {
                        plan.steps.iter().any(|step| match step {
                            Step::RunCommand { program, args, .. } => {
                                supervisor_affected(supervisor, program, args)
                            }
                            _ => false,
                        })
                    }
                    _ => false,
                }) {
                    affected.insert(entry.port);
                }
            }
        }
        affected
    }
}

fn supervisor_affected(supervisor: &Supervisor, program: &str, args: &[String]) -> bool {
    match supervisor {
        Supervisor::SystemdService { unit, user }
        | Supervisor::SystemdSocket { unit, user, .. } => {
            if program != "systemctl" || args.first().is_some_and(|s| s == "--user") != *user {
                return false;
            }
            let units = if *user { &args[1..] } else { args };
            units.first().is_some_and(|s| s == "stop")
                && (units[1..].contains(unit)
                    || matches!(supervisor, Supervisor::SystemdSocket { service: Some(service), .. } if units[1..].contains(service)))
        }
        Supervisor::BrewService { formula } => {
            program == "brew" && args == ["services", "stop", formula]
        }
        Supervisor::Pm2 { id, name } => {
            program == "pm2"
                && args.len() == 2
                && args[0] == "stop"
                && (args[1] == *id || args[1] == *name)
        }
    }
}

/// Refresh after confirmation and check all currently known effects of the pinned plan.
/// The core executor still rechecks each process identity immediately before signalling.
pub(crate) fn checked_engine(
    docker: bool,
    plan: &ActionPlan,
    stack_file: Option<&Path>,
) -> Result<Engine> {
    let engine = Engine::new(&ScanOptions {
        all_states: true,
        docker,
    })
    .context("cannot refresh project socket ownership; no action was taken")?;
    let mut policy = ProjectProtection::nearest()?;
    if let Some(file) = stack_file {
        policy.include_file(file)?;
    }
    let checked = policy.apply(&engine, plan.clone());
    if let Some(blocked) = checked.blocked {
        return Err(ProtectionRefusal(blocked.message).into());
    }
    Ok(engine)
}

/// Agent batches already refresh authority once. Avoid another socket scan when no project
/// port protection exists; reload configuration on every effect so a new policy is observed.
pub(crate) fn check_before_execute(docker: bool, plan: &ActionPlan) -> Result<()> {
    let policy = ProjectProtection::nearest()?;
    if policy.ports.is_empty() {
        return Ok(());
    }
    checked_engine(docker, plan, None)?;
    Ok(())
}

/// TUI previews cannot return errors, so display a non-executable refusal on invalid config.
pub(crate) fn preview(engine: &Engine, plan: ActionPlan) -> ActionPlan {
    match ProjectProtection::nearest() {
        Ok(policy) => policy.apply(engine, plan),
        Err(error) => {
            let mut plan = plan;
            let message =
                format!("Cannot validate project protection: {error:#}. No action was taken.");
            plan.summary = message.clone();
            plan.blocked = Some(Blocked {
                kind: BlockKind::Protected,
                message,
                overridable: false,
            });
            plan.steps.clear();
            plan
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_core::docker::{Endpoint, PublishedPort, Runtime};
    use holdmap_core::engine::{Resolution, ResolveCtx, StopStrategy, StrategyRegistry};
    use holdmap_core::process::ProcessTable;
    use holdmap_core::scan::Scanner;
    use holdmap_core::{
        ContainerInfo, Family, PortEntry, ProcessInfo, Protocol, RawSocket, SocketState, Target,
    };

    fn process(pid: u32, parent: u32) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid: Some(parent),
            name: "vite".into(),
            exe: None,
            cmdline: vec!["vite".into()],
            cwd: None,
            uid: Some(1000),
            user: Some("dev".into()),
            start_time: 1_700_000_000,
            start_token: u64::from(pid) + 1000,
            memory_bytes: 0,
            cpu_percent: 0.0,
        }
    }

    fn socket(pid: u32, port: u16, protocol: Protocol) -> RawSocket {
        RawSocket {
            protocol,
            family: Family::V4,
            local_addr: "127.0.0.1".parse().unwrap(),
            local_port: port,
            remote_addr: None,
            remote_port: None,
            state: if protocol == Protocol::Tcp {
                SocketState::Listen
            } else {
                SocketState::Bound
            },
            uid: Some(1000),
            inode: None,
            pids: vec![pid],
        }
    }

    fn engine(
        mut processes: Vec<ProcessInfo>,
        raw: Vec<RawSocket>,
        published: Vec<PublishedPort>,
    ) -> Engine {
        let mut own = process(999, 1);
        own.name = "holdmap".into();
        own.cmdline = vec!["holdmap".into()];
        processes.push(own);
        let table =
            ProcessTable::from_processes(processes.into_iter().map(|p| (p.pid, p)).collect(), 999);
        Engine::from_scan(
            Scanner::fixed(raw, table, published)
                .scan(&ScanOptions {
                    all_states: false,
                    docker: true,
                })
                .unwrap(),
        )
    }

    fn protect(ports: &[u16]) -> ProjectProtection {
        ProjectProtection::testing(ports)
    }

    fn assert_blocked(plan: &ActionPlan, port: u16) {
        assert_eq!(plan.blocked.as_ref().unwrap().kind, BlockKind::Protected);
        assert!(plan.steps.is_empty());
        assert!(plan.summary.contains(&port.to_string()));
    }

    #[test]
    fn numeric_pid_name_and_all_dev_plans_protect_other_protocols_on_the_same_process() {
        let e = engine(
            vec![process(101, 1)],
            vec![
                socket(101, 3000, Protocol::Tcp),
                socket(101, 5432, Protocol::Udp),
            ],
            vec![],
        );
        for target in [
            Target::Port(3000),
            Target::Pid(101),
            Target::Name("vite".into()),
            Target::AllDev,
        ] {
            let original = e.plan(&target, &StopOptions::default());
            assert!(!original.is_blocked(), "{target}: {original:?}");
            assert_blocked(&protect(&[5432]).apply(&e, original), 5432);
        }
    }

    #[test]
    fn cluster_stop_cannot_skip_project_protection_of_a_member() {
        let mut e = engine(
            vec![process(101, 1), process(102, 1)],
            vec![
                socket(101, 3000, Protocol::Tcp),
                socket(102, 5432, Protocol::Tcp),
            ],
            vec![],
        );
        for entry in &mut e.scan.snapshot.entries {
            entry.project = Some(holdmap_core::ProjectInfo {
                name: "fixture".into(),
                root: "/fixture".into(),
                kind: "package.json".into(),
                git_branch: None,
                workspace: Some(holdmap_core::Workspace {
                    name: "fixture".into(),
                    root: "/fixture".into(),
                    kind: "pnpm".into(),
                }),
                git_root: None,
            });
        }
        let original = e.plan(&Target::Cluster("fixture".into()), &StopOptions::default());
        assert!(!original.is_blocked(), "{original:?}");
        assert_blocked(&protect(&[5432]).apply(&e, original), 5432);
    }

    #[test]
    fn launcher_stop_protects_children_even_without_explicit_tree_signals() {
        let e = engine(
            vec![process(101, 1), process(102, 101)],
            vec![
                socket(101, 3000, Protocol::Tcp),
                socket(102, 5432, Protocol::Tcp),
            ],
            vec![],
        );
        let options = StopOptions {
            tree: false,
            ..Default::default()
        };
        for target in [Target::Pid(101), Target::Port(3000)] {
            let original = e.plan(&target, &options);
            assert!(!original.is_blocked());
            assert_blocked(&protect(&[5432]).apply(&e, original), 5432);
        }
    }

    #[test]
    fn a_new_listener_after_review_is_protected_without_substituting_a_reused_pid() {
        let e = engine(
            vec![process(101, 1)],
            vec![socket(101, 3000, Protocol::Tcp)],
            vec![],
        );
        let reviewed = e.plan(&Target::Pid(101), &StopOptions::default());
        assert!(!protect(&[5432]).apply(&e, reviewed.clone()).is_blocked());
        let fresh = engine(
            vec![process(101, 1)],
            vec![
                socket(101, 3000, Protocol::Tcp),
                socket(101, 5432, Protocol::Tcp),
            ],
            vec![],
        );
        assert_blocked(&protect(&[5432]).apply(&fresh, reviewed.clone()), 5432);
        let mut reused = process(101, 1);
        reused.start_token += 1;
        let fresh = engine(vec![reused], vec![socket(101, 5432, Protocol::Tcp)], vec![]);
        // The executor refuses this stale token. Project policy must not ascribe a new
        // owner's unrelated endpoints to the captured process identity.
        assert!(!protect(&[5432]).apply(&fresh, reviewed).is_blocked());
    }

    fn published(port: u16, endpoint: &str) -> PublishedPort {
        PublishedPort {
            host_port: port,
            protocol: "tcp".into(),
            host_ip: Some("127.0.0.1".into()),
            container: ContainerInfo {
                id: "owned-container".into(),
                name: "fixture".into(),
                image: "fixture".into(),
                runtime: "Docker".into(),
                compose_project: None,
                compose_service: None,
                private_port: port,
            },
            runtime: Runtime {
                endpoint: Endpoint::Tcp(endpoint.into()),
                label: "Docker".into(),
            },
        }
    }

    #[test]
    fn stopping_a_container_protects_all_its_published_ports_on_the_exact_runtime() {
        let e = engine(
            vec![],
            vec![],
            vec![
                published(3000, "127.0.0.1:2375"),
                published(5432, "127.0.0.1:2375"),
            ],
        );
        let original = e.plan(&Target::Port(3000), &StopOptions::default());
        assert!(!original.is_blocked());
        assert_blocked(&protect(&[5432]).apply(&e, original.clone()), 5432);
        let other_runtime = engine(
            vec![],
            vec![],
            vec![
                published(3000, "127.0.0.1:2375"),
                published(5432, "127.0.0.1:2376"),
            ],
        );
        assert!(!protect(&[5432])
            .apply(&other_runtime, original)
            .is_blocked());
    }

    struct UnitStrategy;
    impl StopStrategy for UnitStrategy {
        fn name(&self) -> &'static str {
            "fixture-unit"
        }
        fn resolve(&self, ctx: &ResolveCtx, entry: &PortEntry, _: &[String]) -> Option<Resolution> {
            let unit = if entry.port == 6379 {
                "other.service"
            } else {
                "fixture.service"
            };
            let supervisor = Supervisor::SystemdService {
                unit: unit.into(),
                user: true,
            };
            let mut resolution = Resolution::new(
                Owner::Supervised {
                    supervisor,
                    pid: entry.pid?,
                    name: "fixture".into(),
                },
                "fixture managed service",
            );
            resolution.steps.push(Step::RunCommand {
                program: "systemctl".into(),
                args: vec!["--user".into(), "stop".into(), unit.into()],
                reason: "fixture".into(),
                guard: ctx.proc_ref(entry.pid?),
            });
            Some(resolution)
        }
    }

    #[test]
    fn a_whole_service_stop_protects_independent_processes_in_the_same_unit() {
        let e = engine(
            vec![process(101, 1), process(102, 1), process(103, 1)],
            vec![
                socket(101, 3000, Protocol::Tcp),
                socket(102, 5432, Protocol::Tcp),
                socket(103, 6379, Protocol::Tcp),
            ],
            vec![],
        )
        .with_strategies(StrategyRegistry::empty().with_first(UnitStrategy));
        let original = e.plan(&Target::Port(3000), &StopOptions::default());
        assert!(!original.is_blocked());
        assert_blocked(&protect(&[5432]).apply(&e, original.clone()), 5432);
        assert!(!protect(&[6379]).apply(&e, original).is_blocked());
    }

    #[test]
    fn explicit_stop_override_preserves_the_pinned_plan_and_hard_blocks_stay_blocked() {
        let e = engine(
            vec![process(101, 1)],
            vec![socket(101, 5432, Protocol::Tcp)],
            vec![],
        );
        let original = e.plan(
            &Target::Port(5432),
            &StopOptions {
                allow_protected: true,
                ..Default::default()
            },
        );
        assert!(!original.is_blocked());
        assert_eq!(protect(&[5432]).apply(&e, original.clone()), original);
        let self_plan = e.plan(
            &Target::Pid(999),
            &StopOptions {
                allow_protected: true,
                ..Default::default()
            },
        );
        assert!(protect(&[5432]).apply(&e, self_plan).is_blocked());
    }

    #[test]
    fn nearest_invalid_files_fail_closed_and_reloading_observes_protection_edits() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("nested");
        std::fs::create_dir(&inner).unwrap();
        let file = dir.path().join(stack::FILE_NAME);
        std::fs::write(&file, "protect = [5432]\n").unwrap();
        assert!(ProjectProtection::from_dir(&inner)
            .unwrap()
            .check_port(5432)
            .is_err());
        std::fs::write(&file, "protect = [6379]\n").unwrap();
        let current = ProjectProtection::from_dir(&inner).unwrap();
        assert!(current.check_port(5432).is_ok());
        assert!(current.check_port(6379).is_err());
        std::fs::write(inner.join(stack::FILE_NAME), "protect = [").unwrap();
        assert!(ProjectProtection::from_dir(&inner).is_err());
        std::fs::remove_file(inner.join(stack::FILE_NAME)).unwrap();
        std::fs::create_dir(inner.join(stack::FILE_NAME)).unwrap();
        assert!(ProjectProtection::from_dir(&inner).is_err());
    }

    #[test]
    fn socket_activated_commands_protect_the_service_and_correct_manager() {
        let socket = Supervisor::SystemdSocket {
            unit: "fixture.socket".into(),
            user: true,
            service: Some("fixture.service".into()),
        };
        assert!(supervisor_affected(
            &socket,
            "systemctl",
            &["--user".into(), "stop".into(), "fixture.service".into()]
        ));
        assert!(!supervisor_affected(
            &socket,
            "systemctl",
            &["stop".into(), "fixture.service".into()]
        ));
        assert!(!supervisor_affected(&socket, "systemctl", &[]));
    }
}

//! Stop previews, confirmation binding and execution. Inspection and stop policy stay in core.

use crate::confirmation::{self, Confirmations, Observation, Request};
use crate::tools::{engine, port_arg, ToolError};
use holdmap_core::exec::CompletedSupervisorStops;
use holdmap_core::{ActionPlan, Engine, ProcRef, Step, StopOptions, StopReport, Target};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) fn call(
    name: &str,
    args: &Value,
    confirmations: &mut Confirmations,
) -> Result<(String, Value), ToolError> {
    call_with(name, args, confirmations, &mut LocalBackend::default())
}

trait Backend {
    fn observe(&mut self, request: &Request) -> Result<Observation, ToolError>;
    fn execute(&mut self, original: &ActionPlan, execution: &ActionPlan) -> Stopped;
}

/// Process control and saving its restart recording have independent outcomes.
#[derive(Serialize)]
struct Stopped {
    #[serde(flatten)]
    report: StopReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    history_error: Option<String>,
}

#[derive(Default)]
struct LocalBackend {
    engine: Option<Engine>,
}

impl Backend for LocalBackend {
    fn observe(&mut self, request: &Request) -> Result<Observation, ToolError> {
        let current = engine()?;
        let observation = observe(&current, request)?;
        self.engine = Some(current);
        Ok(observation)
    }

    fn execute(&mut self, original: &ActionPlan, execution: &ActionPlan) -> Stopped {
        let report = holdmap_core::execute(execution, &mut |_| {});
        let engine = self
            .engine
            .as_ref()
            .expect("observation precedes execution");
        let history_error = holdmap_core::store::Store::open_default()
            .record(&holdmap_core::history::entries_from_plan(
                &engine.scan,
                original,
                &report,
            ))
            .err()
            .map(|error| holdmap_core::redact::line(&error.to_string()));
        Stopped {
            report,
            history_error,
        }
    }
}

fn call_with(
    name: &str,
    args: &Value,
    confirmations: &mut Confirmations,
    backend: &mut impl Backend,
) -> Result<(String, Value), ToolError> {
    crate::tools::validate_args(name, args)?;
    let request = request(name, args)?;
    let dry_run = args["dry_run"].as_bool().unwrap_or(true);
    if dry_run {
        if args.get("confirmation_id").is_some() {
            return Err(ToolError::Failed(
                "Send confirmation_id only with dry_run=false. Request a fresh preview without it."
                    .into(),
            ));
        }
        let observed = backend.observe(&request)?;
        let mut data = preview_data(&observed);
        let mut summary = format!(
            "Plan (dry run): {}",
            observed
                .plans
                .iter()
                .map(|plan| plan.summary.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        );
        if observed.plans.is_empty() {
            summary = "Matching agents have no stoppable ports; no confirmation was issued.".into();
        } else if observed.plans.iter().any(|plan| !request.permits(plan)) {
            summary.push_str("\nRefused: not every plan is permitted by the agent safety policy; no confirmation was issued. Use a narrower target or ask the user to use the CLI.");
        } else {
            let id = confirmations.issue(observed).map_err(ToolError::Failed)?;
            data["confirmation_id"] = json!(id);
            data["confirmation_expires_in_s"] = json!(300);
            summary.push_str("\nShow all plans and obtain the user's authorization. Execute with dry_run=false, this confirmation_id and identical target/options within 5 minutes. Execution rechecks every plan first.");
        }
        return Ok((summary, data));
    }

    let id = args["confirmation_id"].as_str().ok_or_else(|| ToolError::Failed(
        "Execution requires confirmation_id from a dry-run preview in this session. Request a preview, show it and obtain authorization first.".into()
    ))?;
    // Schema and envelope validation happen before this consumes a handle; even a failed
    // execution attempt cannot be replayed or broadened with different options.
    let confirmed = confirmations
        .take(id, &request)
        .map_err(ToolError::Failed)?;
    let current = backend.observe(&request)?;
    let reports = execute_checked(&confirmed, &current, |original, execution| {
        backend.execute(original, execution)
    })?;
    let data = match &request {
        Request::Port { .. } => serde_json::to_value(&reports[0]).unwrap_or_default(),
        Request::Agents { .. } => json!({
            "agents": confirmed.agents.iter().map(|(id, _)| id).collect::<Vec<_>>(),
            "plans": confirmed.plans, "reports": reports
        }),
    };
    if let Some(failed) = reports.iter().find(|stopped| {
        !stopped.report.success || !stopped.report.freed || stopped.history_error.is_some()
    }) {
        let outcome = if failed.report.success && failed.report.freed {
            "The stop succeeded and its listener is free, but the restart recording could not be saved."
        } else {
            "Stop failed."
        };
        let history = failed
            .history_error
            .as_ref()
            .map(|error| format!("\nHistory error: {error}"))
            .unwrap_or_default();
        return Err(ToolError::FailedWithData(
            format!(
                "{outcome}{history}\nNo further batch targets were attempted. The confirmation was consumed; request a new preview before retrying.\n{}",
                reports
                    .iter()
                    .flat_map(|stopped| stopped.report.log.iter())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            data,
        ));
    }
    if let Request::Port { port, .. } = request {
        return Ok((format!("Port {port} is free."), data));
    }
    Ok((
        "Confirmed agent ports stopped; no new target was added to the reviewed plans.".into(),
        data,
    ))
}

fn request(name: &str, args: &Value) -> Result<Request, ToolError> {
    let force = args["force"].as_bool().unwrap_or(false);
    match name {
        "stop_port" => Ok(Request::Port {
            port: port_arg(args, "port")?,
            force,
            allow_non_dev: args["allow_non_dev"].as_bool().unwrap_or(false),
        }),
        "stop_agent_ports" => {
            let query = args["agent"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if query.is_empty() {
                return Err(ToolError::Failed(
                    "`agent` must be a specific nonempty filter".into(),
                ));
            }
            Ok(Request::Agents { query, force })
        }
        _ => Err(ToolError::Unknown),
    }
}

fn observe(engine: &Engine, request: &Request) -> Result<Observation, ToolError> {
    let force = match request {
        Request::Port { force, .. } | Request::Agents { force, .. } => *force,
    };
    let options = StopOptions {
        force,
        ..Default::default()
    };
    let (agents, plans) = match request {
        Request::Port { port, .. } => {
            let plan = engine.plan(&Target::Port(*port), &options);
            if let Some(blocked) = &plan.blocked {
                return Err(ToolError::Failed(format!("Refused: {}", blocked.message)));
            }
            if !request.permits(&plan) {
                return Err(ToolError::Failed(format!(
                    "Refused by agent safety policy (risk: {:?}). Protected/high-risk processes cannot be stopped; an authorized non-dev preview needs allow_non_dev=true.", plan.risk
                )));
            }
            (Vec::new(), vec![plan])
        }
        Request::Agents { query, .. } => {
            let mut report = engine.agents();
            report.agents.retain(|agent| agent.matches(query));
            if report.agents.is_empty() {
                return Err(ToolError::Failed(format!(
                    "No agent matching `{query}` is running. Request a new preview."
                )));
            }
            let mut agents: Vec<_> = report
                .agents
                .iter()
                .map(|agent| {
                    (
                        agent.id.clone(),
                        engine
                            .table()
                            .get(agent.pid)
                            .map_or(0, |process| process.start_token),
                    )
                })
                .collect();
            agents.sort();
            let mut ports = BTreeMap::new();
            for agent in &report.agents {
                for port in agent.stoppable_ports() {
                    ports.entry((port.port, port.protocol)).or_insert_with(|| {
                        let options = StopOptions {
                            protocol: Some(port.protocol),
                            ..options.clone()
                        };
                        engine.plan(&Target::Port(port.port), &options)
                    });
                }
            }
            (agents, ports.into_values().collect())
        }
    };
    validate_ownership(engine, &plans)?;
    Ok(Observation {
        request: request.clone(),
        agents,
        plans,
    })
}

/// Supervisor strategies can intentionally authorize administrator CLI actions. MCP has
/// no such override, so risk classification alone cannot authorize a foreign owner.
fn validate_ownership(engine: &Engine, plans: &[ActionPlan]) -> Result<(), ToolError> {
    let owns = |reference: &ProcRef| {
        let Some(current) = engine.table().get(reference.pid).filter(|process| {
            reference.start_token != 0 && process.start_token == reference.start_token
        }) else {
            return false;
        };
        match (current.uid, engine.table().current_uid()) {
            (Some(owner), Some(account)) => owner == account,
            _ => engine.table().is_mine(reference.pid),
        }
    };
    for step in plans.iter().flat_map(|plan| &plan.steps) {
        let permitted = match step {
            Step::SignalProcesses { processes, .. } => {
                !processes.is_empty() && processes.iter().all(&owns)
            }
            Step::RunCommand { guard, .. } => guard.as_ref().is_some_and(owns),
            // Container authority stays bound to the reviewed runtime/endpoint and ID.
            Step::StopContainer { .. } | Step::VerifyFree { .. } => true,
        };
        if !permitted {
            return Err(ToolError::Failed(
                "Refused by agent safety policy: every process or supervisor command needs a pinned current-account owner. Foreign or unknown ownership and unpinned service-manager commands are not permitted.".into(),
            ));
        }
    }
    Ok(())
}

fn preview_data(observation: &Observation) -> Value {
    match observation.request {
        Request::Port { .. } => serde_json::to_value(&observation.plans[0]).unwrap_or_default(),
        Request::Agents { .. } => json!({
            "agents": observation.agents.iter().map(|(id, _)| id).collect::<Vec<_>>(),
            "plans": observation.plans, "reports": []
        }),
    }
}

/// Preflight the whole bulk action, then execute only retained plans, never fresh replacements.
fn execute_checked(
    confirmed: &Observation,
    current: &Observation,
    mut execute: impl FnMut(&ActionPlan, &ActionPlan) -> Stopped,
) -> Result<Vec<Stopped>, ToolError> {
    confirmation::validate_fresh(confirmed, current).map_err(ToolError::Failed)?;
    let mut reports = Vec::new();
    let mut completed = CompletedSupervisorStops::default();
    for plan in &confirmed.plans {
        let execution = completed.remaining(plan);
        let mut stopped = execute(plan, &execution);
        if execution.steps.len() < plan.steps.len() {
            stopped.report.log.insert(
                0,
                "The same reviewed supervisor stop completed earlier in this batch; verifying this listener without repeating that command.".into(),
            );
        }
        let complete =
            stopped.report.success && stopped.report.freed && stopped.history_error.is_none();
        if complete {
            completed.remember(&execution, &stopped.report);
        }
        reports.push(stopped);
        if !complete {
            break;
        }
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_core::engine::{
        Resolution, ResolveCtx, StopStrategy, StrategyRegistry, Supervisor,
    };
    use holdmap_core::model::{Family, PortEntry, ProcessInfo, RawSocket, Snapshot, SocketState};
    use holdmap_core::process::ProcessTable;
    use holdmap_core::scan::{build_entries, Scan};
    use holdmap_core::{Owner, ProcRef, Protocol, Risk, Step};

    const AGENT_PID: u32 = 4_100_100;
    const OWNED_PID: u32 = 4_100_200;
    const SHARED_PORT: u16 = 53123;

    fn stopped(report: StopReport) -> Stopped {
        Stopped {
            report,
            history_error: None,
        }
    }

    /// Production agent discovery/planning over synthetic metadata, with no live listeners.
    /// Codex has no home-history reader, and the fixture binds no actual sockets.
    fn protocol_engine(
        owned_protocols: &[Protocol],
        unrelated_udp_pid: Option<u32>,
        owned_start_token: u64,
    ) -> Engine {
        let process = |pid, ppid, name: &str, command: &[&str], start_token| ProcessInfo {
            pid,
            ppid: Some(ppid),
            name: name.into(),
            exe: None,
            cmdline: command.iter().map(|part| (*part).into()).collect(),
            cwd: None,
            uid: Some(1000),
            user: Some("fixture".into()),
            start_time: 1,
            start_token,
            memory_bytes: 1024,
            cpu_percent: 0.0,
        };
        let mut processes = vec![
            process(AGENT_PID, 1, "codex", &["codex"], 10),
            process(
                OWNED_PID,
                AGENT_PID,
                "node",
                &["node", "node_modules/.bin/vite"],
                owned_start_token,
            ),
            process(4_100_999, 1, "holdmap", &["holdmap", "mcp"], 99),
        ];
        let listener = |protocol, pid| RawSocket {
            protocol,
            family: Family::V4,
            local_addr: "127.0.0.1".parse().unwrap(),
            local_port: SHARED_PORT,
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
        };
        let mut raw: Vec<_> = owned_protocols
            .iter()
            .map(|protocol| listener(*protocol, OWNED_PID))
            .collect();
        if let Some(pid) = unrelated_udp_pid {
            processes.push(process(
                pid,
                1,
                "node",
                &["node", "node_modules/.bin/vite"],
                u64::from(pid),
            ));
            raw.push(listener(Protocol::Udp, pid));
        }
        let table = ProcessTable::from_processes(
            processes
                .into_iter()
                .map(|process| (process.pid, process))
                .collect(),
            4_100_999,
        );
        let (entries, hidden_sockets) = build_entries(&raw, &table, &[], false);
        Engine::from_scan(Scan {
            snapshot: Snapshot {
                entries,
                hidden_sockets,
                platform: "fixture".into(),
                taken_at_ms: 1,
                scan_ms: 0,
                docker_available: false,
                warnings: vec![],
            },
            table,
            published: vec![],
            raw,
        })
    }

    fn signalled_pids(plans: &[ActionPlan]) -> Vec<u32> {
        plans
            .iter()
            .flat_map(|plan| &plan.steps)
            .flat_map(|step| match step {
                Step::SignalProcesses { processes, .. } => {
                    processes.iter().map(|process| process.pid).collect()
                }
                _ => Vec::new(),
            })
            .collect()
    }

    fn verified_protocols(plans: &[ActionPlan]) -> Vec<Protocol> {
        plans
            .iter()
            .flat_map(|plan| &plan.steps)
            .filter_map(|step| match step {
                Step::VerifyFree { protocol, .. } => Some(*protocol),
                _ => None,
            })
            .collect()
    }

    #[derive(Clone)]
    struct LowRiskStop(Option<Supervisor>);

    impl StopStrategy for LowRiskStop {
        fn name(&self) -> &'static str {
            "low-risk-fixture"
        }

        fn resolve(&self, ctx: &ResolveCtx, entry: &PortEntry, _: &[String]) -> Option<Resolution> {
            let pid = entry.pid?;
            let owner = match &self.0 {
                Some(supervisor) => Owner::Supervised {
                    supervisor: supervisor.clone(),
                    pid,
                    name: "fixture".into(),
                },
                None => Owner::Process {
                    pid,
                    name: "fixture".into(),
                },
            };
            let mut resolution = Resolution::new(owner, "Low-risk supervisor decision");
            let step = match &self.0 {
                Some(supervisor) => {
                    let (program, args, pinned) = match supervisor {
                        Supervisor::Pm2 { id, .. } => {
                            ("pm2", vec!["stop".into(), id.clone()], true)
                        }
                        Supervisor::BrewService { formula } => (
                            "brew",
                            vec!["services".into(), "stop".into(), formula.clone()],
                            true,
                        ),
                        Supervisor::SystemdService { unit, .. } => {
                            ("systemctl", vec!["stop".into(), unit.clone()], true)
                        }
                        Supervisor::SystemdSocket { unit, service, .. } => {
                            let mut args = vec!["stop".into(), unit.clone()];
                            args.extend(service.iter().cloned());
                            ("systemctl", args, false)
                        }
                    };
                    Step::RunCommand {
                        program: program.into(),
                        args,
                        reason: "fixture supervisor stop".into(),
                        guard: pinned.then(|| ctx.proc_ref(pid)).flatten(),
                    }
                }
                None => ctx.signal_step(&[pid]),
            };
            resolution.steps.push(step);
            Some(resolution)
        }
    }

    fn reassign_owner(engine: &Engine, pid: u32, uid: Option<u32>, user: Option<&str>) -> Engine {
        let mut scan = engine.scan.clone();
        let mut processes: std::collections::HashMap<_, _> = scan
            .table
            .iter()
            .cloned()
            .map(|process| (process.pid, process))
            .collect();
        let process = processes.get_mut(&pid).unwrap();
        process.uid = uid;
        process.user = user.map(str::to_owned);
        scan.table = ProcessTable::from_processes(processes, scan.table.self_pid());
        let (entries, hidden_sockets) =
            build_entries(&scan.raw, &scan.table, &scan.published, false);
        scan.snapshot.entries = entries;
        scan.snapshot.hidden_sockets = hidden_sockets;
        Engine::from_scan(scan).with_strategies(engine.strategies().clone())
    }

    fn low_risk_engine(supervisor: Option<Supervisor>) -> Engine {
        protocol_engine(&[Protocol::Tcp], None, 20)
            .with_strategies(StrategyRegistry::empty().with(LowRiskStop(supervisor)))
    }

    #[test]
    fn production_observe_refuses_foreign_or_unknown_owners_despite_low_risk() {
        let request = Request::Port {
            port: SHARED_PORT,
            force: false,
            allow_non_dev: true,
        };
        let supervisors = [
            None,
            Some(Supervisor::Pm2 {
                name: "fixture".into(),
                id: "3".into(),
            }),
            Some(Supervisor::BrewService {
                formula: "fixture".into(),
            }),
            Some(Supervisor::SystemdService {
                unit: "fixture.service".into(),
                user: false,
            }),
        ];
        for supervisor in supervisors {
            let owned = low_risk_engine(supervisor.clone());
            assert!(
                observe(&owned, &request).is_ok(),
                "known owner: {supervisor:?}"
            );
            for (uid, user) in [
                (Some(1001), Some("fixture")),
                (None, None),
                (None, Some("foreign")),
            ] {
                let refused = reassign_owner(&owned, OWNED_PID, uid, user);
                let plan = refused.plan(&Target::Port(SHARED_PORT), &StopOptions::default());
                assert_eq!(plan.risk, Risk::Low);
                assert!(
                    !plan.is_blocked(),
                    "fixture must reach the adapter ownership check"
                );
                assert!(
                    observe(&refused, &request).is_err(),
                    "{supervisor:?}, uid={uid:?}, user={user:?}"
                );
            }
            let unknown_account = reassign_owner(&owned, 4_100_999, None, None);
            assert!(observe(&unknown_account, &request).is_err());
            let named_account = reassign_owner(&owned, 4_100_999, None, Some("fixture"));
            let named_owner = reassign_owner(&named_account, OWNED_PID, None, Some("fixture"));
            assert!(
                observe(&named_owner, &request).is_ok(),
                "known account-name fallback supports Windows"
            );
        }
    }

    #[test]
    fn production_observe_refuses_unpinned_logical_systemd_socket_commands() {
        let engine = low_risk_engine(Some(Supervisor::SystemdSocket {
            unit: "fixture.socket".into(),
            user: false,
            service: Some("fixture.service".into()),
        }));
        let plan = engine.plan(&Target::Port(SHARED_PORT), &StopOptions::default());
        assert!(
            plan.same_effects(&plan),
            "core permits an authorized logical socket-unit plan"
        );
        assert!(observe(
            &engine,
            &Request::Port {
                port: SHARED_PORT,
                force: false,
                allow_non_dev: true
            }
        )
        .is_err());
    }

    #[test]
    fn production_ownership_recheck_refuses_execution_and_consumes_the_preview() {
        struct ObservedBackend {
            current: Engine,
            executed: usize,
        }
        impl Backend for ObservedBackend {
            fn observe(&mut self, request: &Request) -> Result<Observation, ToolError> {
                observe(&self.current, request)
            }
            fn execute(&mut self, _: &ActionPlan, _: &ActionPlan) -> Stopped {
                self.executed += 1;
                stopped(StopReport {
                    success: true,
                    freed: true,
                    ..Default::default()
                })
            }
        }
        let owned = low_risk_engine(Some(Supervisor::Pm2 {
            name: "fixture".into(),
            id: "3".into(),
        }));
        let mut backend = ObservedBackend {
            current: owned.clone(),
            executed: 0,
        };
        let mut cache = Confirmations::default();
        let (_, preview) = call_with(
            "stop_port",
            &json!({"port":SHARED_PORT}),
            &mut cache,
            &mut backend,
        )
        .ok()
        .unwrap();
        backend.current = reassign_owner(&owned, OWNED_PID, Some(1001), Some("fixture"));
        let execute = json!({"port":SHARED_PORT,"dry_run":false,"confirmation_id":preview["confirmation_id"]});
        assert!(call_with("stop_port", &execute, &mut cache, &mut backend).is_err());
        assert_eq!(backend.executed, 0);
        backend.current = owned;
        assert!(
            call_with("stop_port", &execute, &mut cache, &mut backend).is_err(),
            "failed authorization consumes a valid execution attempt"
        );
        assert_eq!(backend.executed, 0);
    }

    #[test]
    fn production_bulk_plans_exclude_unrelated_protocol_and_compare_only_owned_effects() {
        let request = Request::Agents {
            query: "codex".into(),
            force: false,
        };
        let original = protocol_engine(&[Protocol::Tcp], Some(4_100_300), 20);
        let confirmed = observe(&original, &request).ok().unwrap();
        assert_eq!(confirmed.plans.len(), 1);
        assert_eq!(signalled_pids(&confirmed.plans), [OWNED_PID]);
        assert_eq!(verified_protocols(&confirmed.plans), [Protocol::Tcp]);

        // A same-number UDP listener changing owners cannot redirect or invalidate this TCP stop.
        let unrelated_changed = protocol_engine(&[Protocol::Tcp], Some(4_100_301), 20);
        let current = observe(&unrelated_changed, &request).ok().unwrap();
        assert!(confirmation::validate_fresh(&confirmed, &current).is_ok());
        let owned_changed = protocol_engine(&[Protocol::Tcp], Some(4_100_300), 21);
        let current = observe(&owned_changed, &request).ok().unwrap();
        let mut calls = 0;
        assert!(execute_checked(&confirmed, &current, |_, _| {
            calls += 1;
            stopped(StopReport::default())
        })
        .is_err());
        assert_eq!(
            calls, 0,
            "changed owned identities must fail before execution"
        );

        // The explicit single-port tool intentionally retains its existing all-protocol scope.
        let whole_port = observe(
            &original,
            &Request::Port {
                port: SHARED_PORT,
                force: false,
                allow_non_dev: false,
            },
        )
        .ok()
        .unwrap();
        assert_eq!(signalled_pids(&whole_port.plans), [OWNED_PID, 4_100_300]);
        assert_eq!(
            verified_protocols(&whole_port.plans),
            [Protocol::Tcp, Protocol::Udp]
        );
    }

    #[test]
    fn production_bulk_keeps_both_owned_protocols_at_the_same_number() {
        let engine = protocol_engine(&[Protocol::Tcp, Protocol::Udp], None, 20);
        let observed = observe(
            &engine,
            &Request::Agents {
                query: "codex".into(),
                force: false,
            },
        )
        .ok()
        .unwrap();
        assert_eq!(observed.plans.len(), 2);
        assert_eq!(
            verified_protocols(&observed.plans),
            [Protocol::Tcp, Protocol::Udp]
        );
        assert!(confirmation::validate_fresh(&observed, &observed).is_ok());
    }

    fn plan(pid: u32, port: u16) -> ActionPlan {
        ActionPlan {
            target: format!(":{port}"),
            summary: "Reviewed plan".into(),
            owners: vec![Owner::Process {
                pid,
                name: "node".into(),
            }],
            steps: vec![Step::SignalProcesses {
                processes: vec![ProcRef {
                    pid,
                    name: "node".into(),
                    start_token: u64::from(pid),
                    command: "node fixture.js".into(),
                }],
                force: false,
                timeout_ms: 5000,
            }],
            blocked: None,
            warnings: vec![],
            risk: Risk::Low,
            allow_protected: false,
        }
    }

    #[test]
    fn stale_second_bulk_owner_executes_nothing_and_valid_preview_uses_retained_plans() {
        let confirmed = Observation {
            request: Request::Agents {
                query: "claude".into(),
                force: false,
            },
            agents: vec![("agent:50".into(), 5)],
            plans: vec![plan(100, 3000), plan(200, 3001)],
        };
        let mut current = confirmed.clone();
        current.plans[1] = plan(300, 3001);
        let mut count = 0;
        assert!(execute_checked(&confirmed, &current, |_, _| {
            count += 1;
            stopped(StopReport::default())
        })
        .is_err());
        assert_eq!(count, 0);
        current = confirmed.clone();
        for plan in &mut current.plans {
            plan.summary = "Fresh display text".into();
        }
        let reports = execute_checked(&confirmed, &current, |plan, execution| {
            assert_eq!(plan.summary, "Reviewed plan");
            assert_eq!(plan, execution);
            count += 1;
            stopped(StopReport {
                success: true,
                freed: true,
                ..Default::default()
            })
        })
        .ok()
        .unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(count, 2);
    }

    struct FixtureBackend {
        current: Observation,
        scans: usize,
        executed: Vec<u32>,
    }
    impl Backend for FixtureBackend {
        fn observe(&mut self, request: &Request) -> Result<Observation, ToolError> {
            self.scans += 1;
            let mut observation = self.current.clone();
            observation.request = request.clone();
            Ok(observation)
        }
        fn execute(&mut self, _: &ActionPlan, execution: &ActionPlan) -> Stopped {
            let Step::SignalProcesses { processes, .. } = &execution.steps[0] else {
                panic!("fixture step");
            };
            self.executed
                .extend(processes.iter().map(|process| process.pid));
            stopped(StopReport {
                success: true,
                freed: true,
                ..Default::default()
            })
        }
    }

    #[test]
    fn saved_history_failure_preserves_the_successful_stop_and_halts_remaining_targets() {
        struct PersistenceFailureBackend {
            current: Observation,
            executed: Vec<String>,
        }
        impl Backend for PersistenceFailureBackend {
            fn observe(&mut self, request: &Request) -> Result<Observation, ToolError> {
                let mut current = self.current.clone();
                current.request = request.clone();
                Ok(current)
            }
            fn execute(&mut self, original: &ActionPlan, execution: &ActionPlan) -> Stopped {
                assert_eq!(original, execution);
                self.executed.push(original.target.clone());
                Stopped {
                    report: StopReport {
                        target: original.target.clone(),
                        success: true,
                        freed: true,
                        signalled: vec![100],
                        log: vec!["The reviewed listener is free".into()],
                        ..Default::default()
                    },
                    history_error: Some("fixture history write failed".into()),
                }
            }
        }

        for (name, args, request, plans) in [
            (
                "stop_port",
                json!({"port":3000}),
                Request::Port {
                    port: 3000,
                    force: false,
                    allow_non_dev: false,
                },
                vec![plan(100, 3000)],
            ),
            (
                "stop_agent_ports",
                json!({"agent":"fixture"}),
                Request::Agents {
                    query: "fixture".into(),
                    force: false,
                },
                vec![plan(100, 3000), plan(200, 3001)],
            ),
        ] {
            let mut backend = PersistenceFailureBackend {
                current: Observation {
                    request,
                    agents: if name == "stop_agent_ports" {
                        vec![("agent:50".into(), 5)]
                    } else {
                        vec![]
                    },
                    plans,
                },
                executed: vec![],
            };
            let mut cache = Confirmations::default();
            let (_, preview) = call_with(name, &args, &mut cache, &mut backend)
                .ok()
                .unwrap();
            let mut execution = args.clone();
            execution["dry_run"] = json!(false);
            execution["confirmation_id"] = preview["confirmation_id"].clone();
            let outcome = call_with(name, &execution, &mut cache, &mut backend);
            assert!(matches!(&outcome, Err(ToolError::FailedWithData(..))));
            let response = crate::tool_response(json!(7), outcome, name);
            assert_eq!(response["result"]["isError"], true);
            let data = &response["result"]["structuredContent"];
            let report = if name == "stop_agent_ports" {
                assert_eq!(data["plans"].as_array().unwrap().len(), 2);
                assert_eq!(data["reports"].as_array().unwrap().len(), 1);
                &data["reports"][0]
            } else {
                data
            };
            assert_eq!(report["success"], true);
            assert_eq!(report["freed"], true);
            assert_eq!(report["signalled"], json!([100]));
            assert!(report["error"].is_null());
            assert_eq!(report["history_error"], "fixture history write failed");
            let text = response["result"]["content"][0]["text"].as_str().unwrap();
            assert!(text.contains("stop succeeded"));
            assert!(text.contains("No further batch targets were attempted"));
            assert!(!text.contains("Stop failed."));
            crate::validation::validate(
                data,
                &crate::catalog::definition(name).unwrap()["outputSchema"],
            )
            .unwrap();
            assert_eq!(backend.executed, [":3000"]);
            assert!(call_with(name, &execution, &mut cache, &mut backend).is_err());
            assert_eq!(
                backend.executed,
                [":3000"],
                "a history error cannot replay its consumed confirmation"
            );
        }
    }

    #[test]
    fn production_supervisor_batch_runs_once_and_keeps_original_history_and_each_port_verification()
    {
        struct SupervisorBackend {
            engine: Engine,
            commands: usize,
            verified: Vec<u16>,
            recorded: Vec<ActionPlan>,
        }
        impl Backend for SupervisorBackend {
            fn observe(&mut self, request: &Request) -> Result<Observation, ToolError> {
                observe(&self.engine, request)
            }
            fn execute(&mut self, original: &ActionPlan, execution: &ActionPlan) -> Stopped {
                // Saving history receives the original supervisor command even when its
                // guard exited after the first stop and the execution copy omits it.
                assert!(matches!(
                    original.steps[0],
                    Step::RunCommand { guard: Some(_), .. }
                ));
                self.recorded.push(original.clone());
                for step in &execution.steps {
                    match step {
                        Step::RunCommand {
                            guard: Some(guard),
                            program,
                            args,
                            ..
                        } => {
                            assert_eq!(
                                self.commands, 0,
                                "the first command exited the shared guard"
                            );
                            assert_eq!(guard.pid, OWNED_PID);
                            assert_eq!(program, "pm2");
                            assert_eq!(args, &["stop", "3"]);
                            self.commands += 1;
                        }
                        Step::VerifyFree {
                            port,
                            protocol: Protocol::Tcp,
                            ..
                        } => {
                            self.verified.push(*port);
                        }
                        _ => panic!("unexpected fixture effect"),
                    }
                }
                stopped(StopReport {
                    target: original.target.clone(),
                    success: true,
                    freed: true,
                    ..Default::default()
                })
            }
        }

        let mut scan = protocol_engine(&[Protocol::Tcp], None, 20).scan;
        let mut processes: std::collections::HashMap<_, _> = scan
            .table
            .iter()
            .cloned()
            .map(|process| (process.pid, process))
            .collect();
        processes
            .get_mut(&OWNED_PID)
            .unwrap()
            .cmdline
            .extend(["--token".into(), "fixture-private-token".into()]);
        scan.table = ProcessTable::from_processes(processes, scan.table.self_pid());
        let mut second_listener = scan.raw[0].clone();
        second_listener.local_port += 1;
        scan.raw.push(second_listener);
        let (entries, hidden_sockets) = build_entries(&scan.raw, &scan.table, &[], false);
        scan.snapshot.entries = entries;
        scan.snapshot.hidden_sockets = hidden_sockets;
        let engine = Engine::from_scan(scan).with_strategies(StrategyRegistry::empty().with(
            LowRiskStop(Some(Supervisor::Pm2 {
                name: "fixture".into(),
                id: "3".into(),
            })),
        ));
        let mut backend = SupervisorBackend {
            engine,
            commands: 0,
            verified: vec![],
            recorded: vec![],
        };
        let mut cache = Confirmations::default();
        let (_, preview) = call_with(
            "stop_agent_ports",
            &json!({"agent":"codex"}),
            &mut cache,
            &mut backend,
        )
        .ok()
        .unwrap();
        assert_eq!(preview["plans"].as_array().unwrap().len(), 2);
        assert!(!preview.to_string().contains("fixture-private-token"));
        let (text, data) = call_with(
            "stop_agent_ports",
            &json!({"agent":"codex","dry_run":false,"confirmation_id":preview["confirmation_id"]}),
            &mut cache,
            &mut backend,
        )
        .ok()
        .unwrap();
        assert!(text.contains("Confirmed agent ports stopped"));
        assert_eq!(backend.commands, 1);
        assert_eq!(backend.verified, [SHARED_PORT, SHARED_PORT + 1]);
        assert_eq!(backend.recorded.len(), 2);
        assert_eq!(data["plans"], preview["plans"]);
        assert_eq!(data["reports"].as_array().unwrap().len(), 2);
        for report in data["reports"].as_array().unwrap() {
            assert_eq!(report["success"], true);
            assert_eq!(report["freed"], true);
            assert!(
                report.get("history_error").is_none(),
                "healthy response shape is unchanged"
            );
        }
        assert!(data["reports"][1]["log"][0]
            .as_str()
            .unwrap()
            .contains("without repeating that command"));
        assert!(!data.to_string().contains("fixture-private-token"));
        crate::validation::validate(
            &data,
            &crate::catalog::definition("stop_agent_ports").unwrap()["outputSchema"],
        )
        .unwrap();
    }

    #[test]
    fn unsuccessful_stop_or_busy_listener_halts_the_batch_before_the_next_effect() {
        let confirmed = Observation {
            request: Request::Agents {
                query: "fixture".into(),
                force: false,
            },
            agents: vec![("agent:50".into(), 5)],
            plans: vec![plan(100, 3000), plan(200, 3001)],
        };
        for (success, freed) in [(false, false), (false, true), (true, false)] {
            let mut attempted = 0;
            let reports = execute_checked(&confirmed, &confirmed, |_, _| {
                attempted += 1;
                stopped(StopReport {
                    success,
                    freed,
                    ..Default::default()
                })
            })
            .ok()
            .unwrap();
            assert_eq!(attempted, 1);
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].report.success, success);
            assert_eq!(reports[0].report.freed, freed);
        }
    }

    #[test]
    fn two_call_stop_flow_requires_preview_validates_before_consuming_and_executes_once() {
        let mut backend = FixtureBackend {
            current: Observation {
                request: Request::Port {
                    port: 3000,
                    force: false,
                    allow_non_dev: false,
                },
                agents: vec![],
                plans: vec![plan(100, 3000)],
            },
            scans: 0,
            executed: Vec::new(),
        };
        let mut cache = Confirmations::default();
        assert!(call_with(
            "stop_port",
            &json!({"port":3000,"dry_run":false}),
            &mut cache,
            &mut backend
        )
        .is_err());
        assert_eq!(backend.scans, 0);
        let (_, preview) = call_with("stop_port", &json!({"port":3000}), &mut cache, &mut backend)
            .ok()
            .unwrap();
        let id = preview["confirmation_id"].as_str().unwrap();
        assert!(backend.executed.is_empty());
        for invalid in [
            json!({"port":3000,"dry_run":false,"confirmation_id":id,"force":"false"}),
            json!({"port":3000,"dry_run":false,"confirmation_id":id,"allow_protected":true}),
            json!({"port":3000,"dry_run":false,"confirmation_id":id,"unexpected":true}),
        ] {
            assert!(call_with("stop_port", &invalid, &mut cache, &mut backend).is_err());
        }
        assert_eq!(backend.scans, 1);
        let execute = json!({"port":3000,"dry_run":false,"confirmation_id":id});
        assert!(call_with("stop_port", &execute, &mut cache, &mut backend).is_ok());
        assert_eq!(backend.executed, [100]);
        assert!(call_with("stop_port", &execute, &mut cache, &mut backend).is_err());
        assert_eq!(backend.executed, [100]);
        assert_eq!(backend.scans, 2);

        let (_, preview) = call_with("stop_port", &json!({"port":3000}), &mut cache, &mut backend)
            .ok()
            .unwrap();
        backend.current.plans[0] = plan(200, 3000);
        assert!(call_with(
            "stop_port",
            &json!({"port":3000,"dry_run":false,"confirmation_id":preview["confirmation_id"]}),
            &mut cache,
            &mut backend
        )
        .is_err());
        assert_eq!(backend.executed, [100]);
    }

    #[test]
    fn agent_bulk_preview_refuses_partial_plans_and_rechecks_selection_before_all_execution() {
        let mut backend = FixtureBackend {
            current: Observation {
                request: Request::Agents {
                    query: "fixture".into(),
                    force: false,
                },
                agents: vec![("agent:50".into(), 5)],
                plans: vec![plan(100, 3000), plan(200, 3001)],
            },
            scans: 0,
            executed: vec![],
        };
        let mut cache = Confirmations::default();
        backend.current.plans[1].risk = holdmap_core::Risk::Medium;
        let (text, preview) = call_with(
            "stop_agent_ports",
            &json!({"agent":"fixture"}),
            &mut cache,
            &mut backend,
        )
        .ok()
        .unwrap();
        assert!(text.contains("no confirmation was issued"));
        assert!(preview.get("confirmation_id").is_none());
        assert!(backend.executed.is_empty());
        backend.current.plans[1].risk = holdmap_core::Risk::Low;
        let (_, preview) = call_with(
            "stop_agent_ports",
            &json!({"agent":"fixture"}),
            &mut cache,
            &mut backend,
        )
        .ok()
        .unwrap();
        backend.current.agents.push(("agent:51".into(), 6));
        assert!(call_with("stop_agent_ports", &json!({"agent":"fixture","dry_run":false,"confirmation_id":preview["confirmation_id"]}), &mut cache, &mut backend).is_err());
        assert!(backend.executed.is_empty());
        let (_, preview) = call_with(
            "stop_agent_ports",
            &json!({"agent":"fixture"}),
            &mut cache,
            &mut backend,
        )
        .ok()
        .unwrap();
        let (_, result) = call_with("stop_agent_ports", &json!({"agent":"fixture","dry_run":false,"confirmation_id":preview["confirmation_id"]}), &mut cache, &mut backend).ok().unwrap();
        assert_eq!(backend.executed, [100, 200]);
        assert_eq!(result["reports"].as_array().unwrap().len(), 2);
        crate::validation::validate(
            &result,
            &crate::catalog::definition("stop_agent_ports").unwrap()["outputSchema"],
        )
        .unwrap();
    }
}

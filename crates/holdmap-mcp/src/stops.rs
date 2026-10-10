//! Stop previews, confirmation binding and execution. Inspection and stop policy stay in core.

use crate::confirmation::{self, Confirmations, Observation, Request};
use crate::tools::{engine, port_arg, ToolError};
use holdmap_core::{ActionPlan, Engine, ProcRef, Step, StopOptions, StopReport, Target};
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
    fn execute(&mut self, plan: &ActionPlan) -> StopReport;
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

    fn execute(&mut self, plan: &ActionPlan) -> StopReport {
        let report = holdmap_core::execute(plan, &mut |_| {});
        let engine = self
            .engine
            .as_ref()
            .expect("observation precedes execution");
        let _ = holdmap_core::store::Store::open_default().record(
            &holdmap_core::history::entries_from_plan(&engine.scan, plan, &report),
        );
        report
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
    let reports = execute_checked(&confirmed, &current, |plan| backend.execute(plan))?;
    if reports.iter().any(|report| !report.success) {
        return Err(ToolError::Failed(format!(
            "Stop failed; the confirmation was consumed. Review a new preview before retrying.\n{}",
            reports
                .iter()
                .flat_map(|report| report.log.iter())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        )));
    }
    let data = match request {
        Request::Port { port, .. } => {
            return Ok((
                format!("Port {port} is free."),
                serde_json::to_value(&reports[0]).unwrap_or_default(),
            ));
        }
        Request::Agents { .. } => json!({
            "agents": confirmed.agents.iter().map(|(id, _)| id).collect::<Vec<_>>(),
            "plans": confirmed.plans, "reports": reports
        }),
    };
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
    mut execute: impl FnMut(&ActionPlan) -> StopReport,
) -> Result<Vec<StopReport>, ToolError> {
    confirmation::validate_fresh(confirmed, current).map_err(ToolError::Failed)?;
    let mut reports = Vec::new();
    for plan in &confirmed.plans {
        let report = execute(plan);
        let success = report.success;
        reports.push(report);
        if !success {
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
            fn execute(&mut self, _: &ActionPlan) -> StopReport {
                self.executed += 1;
                StopReport {
                    success: true,
                    ..Default::default()
                }
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
        assert!(execute_checked(&confirmed, &current, |_| {
            calls += 1;
            StopReport::default()
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
        assert!(execute_checked(&confirmed, &current, |_| {
            count += 1;
            StopReport::default()
        })
        .is_err());
        assert_eq!(count, 0);
        current = confirmed.clone();
        for plan in &mut current.plans {
            plan.summary = "Fresh display text".into();
        }
        let reports = execute_checked(&confirmed, &current, |plan| {
            assert_eq!(plan.summary, "Reviewed plan");
            count += 1;
            StopReport {
                success: true,
                freed: true,
                ..Default::default()
            }
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
        fn execute(&mut self, plan: &ActionPlan) -> StopReport {
            let Step::SignalProcesses { processes, .. } = &plan.steps[0] else {
                panic!("fixture step");
            };
            self.executed
                .extend(processes.iter().map(|process| process.pid));
            StopReport {
                success: true,
                freed: true,
                ..Default::default()
            }
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

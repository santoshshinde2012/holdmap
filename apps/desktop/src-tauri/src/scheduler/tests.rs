//! Fixture-only policy, ownership, clock and cancellation tests.

use super::model::ScheduleStatus;
use super::runner::execute_stops;
use super::stop_authorization::{delayed_plan_allowed, listeners, validate_ownership};
use super::*;
use crate::commands::stop_request;
use crate::confirmation::validate_fresh;
use holdmap_core::{Engine, Protocol, Step};
use std::time::{Duration, Instant};
const MAX_ACTIVE: usize = 32;
const MAX_HISTORY: usize = 64;
use holdmap_core::engine::strategies::{ProcessTreeStrategy, StrategyRegistry};
use holdmap_core::{
    BlockKind, Blocked, Exposure, Family, PortEntry, ProcessInfo, ProcessTable, Scan, Snapshot,
    SocketState,
};
use std::cell::{Cell, RefCell};

fn process(pid: u32, parent: Option<u32>, name: &str, token: u64) -> ProcessInfo {
    ProcessInfo {
        pid,
        ppid: parent,
        name: name.into(),
        exe: None,
        cmdline: vec![name.into(), "server.js".into()],
        cwd: None,
        uid: Some(1000),
        user: Some("fixture".into()),
        start_time: token,
        start_token: token,
        memory_bytes: 0,
        cpu_percent: 0.0,
    }
}

fn fixture_engine(second_token: u64) -> Engine {
    let agent = process(100, None, "claude", 1);
    let first = process(101, Some(100), "node", 10);
    let second = process(102, Some(100), "node", second_token);
    let entries = [(3000, first.clone()), (4000, second.clone())]
        .into_iter()
        .map(|(port, process)| PortEntry {
            id: format!("tcp:{port}:{}:listen", process.pid),
            port,
            protocol: Protocol::Tcp,
            state: SocketState::Listen,
            addresses: vec!["127.0.0.1".into()],
            families: vec![Family::V4],
            remote: None,
            exposure: Exposure::Loopback,
            pid: Some(process.pid),
            pids: vec![process.pid],
            uid: process.uid,
            user: process.user.clone(),
            process: Some(process),
            project: None,
            framework: None,
            container: None,
            label: "fixture server".into(),
            is_dev: true,
            is_mine: true,
            protected: false,
            tunnel: None,
            app_memory_bytes: 0,
            helper_count: 0,
        })
        .collect();
    Engine::from_scan(Scan {
        snapshot: Snapshot {
            entries,
            hidden_sockets: 0,
            platform: "fixture".into(),
            taken_at_ms: 1,
            scan_ms: 0,
            docker_available: false,
            warnings: vec![],
        },
        table: ProcessTable::from_processes(
            [
                (100, agent),
                (101, first),
                (102, second),
                (999, process(999, None, "holdmap", 100)),
            ]
            .into(),
            999,
        ),
        published: vec![],
        raw: vec![],
    })
    .with_strategies(StrategyRegistry::empty().with(ProcessTreeStrategy))
}

fn plans(engine: &Engine) -> Vec<PinnedStop> {
    ["3000/tcp", "4000/tcp"]
        .into_iter()
        .map(|target| {
            let (parsed, options) = stop_request(target, false, false);
            PinnedStop {
                target: target.into(),
                plan: engine.plan(&parsed, &options),
            }
        })
        .collect()
}

#[test]
fn immediate_batch_checks_every_target_and_agent_before_dispatch() {
    let original = fixture_engine(20);
    let reviewed = plans(&original);
    for (engine, agent_id) in [
        (fixture_engine(21), "agent:100"),
        (original.clone(), "agent:999"),
    ] {
        assert!(
            crate::stop_batch::run(&engine, agent_id, &reviewed, |_, _| {
                panic!("batch dispatched before every target passed preflight")
            })
            .is_err()
        );
    }
    let mut foreign = original.clone();
    let mut child = process(102, Some(100), "node", 20);
    child.uid = Some(2000);
    child.user = Some("foreign".into());
    foreign.scan.table = ProcessTable::from_processes(
        [
            (100, process(100, None, "claude", 1)),
            (101, process(101, Some(100), "node", 10)),
            (102, child),
            (999, process(999, None, "holdmap", 100)),
        ]
        .into(),
        999,
    );
    assert!(
        crate::stop_batch::run(&foreign, "agent:100", &reviewed, |_, _| {
            panic!("foreign owner dispatched")
        })
        .is_err()
    );
}

#[test]
fn immediate_batch_consumes_exact_handles_and_never_replays() {
    let engine = fixture_engine(20);
    let mut confirmations = crate::confirmation::Confirmations::default();
    let requests: Vec<_> = plans(&engine)
        .into_iter()
        .map(|pinned| ScheduledStopRequest {
            confirmation_id: confirmations
                .preview(pinned.target.clone(), false, false, pinned.plan)
                .confirmation_id,
            target: pinned.target,
            force: false,
            allow_protected: false,
        })
        .collect();
    let taken = crate::stop_batch::take(&mut confirmations, &requests).unwrap();
    assert_eq!(taken.len(), 2);
    assert!(crate::stop_batch::take(&mut confirmations, &requests).is_err());
    let mut requests = requests;
    requests[0].confirmation_id = confirmations
        .preview(taken[0].target.clone(), false, false, taken[0].plan.clone())
        .confirmation_id;
    requests[1].confirmation_id = "unknown".into();
    assert!(crate::stop_batch::take(&mut confirmations, &requests).is_err());
    // A failed later handle cannot leave a reusable partially accepted batch.
    assert!(confirmations
        .take(
            &requests[0].confirmation_id,
            &requests[0].target,
            false,
            false
        )
        .is_err());
}

#[test]
fn immediate_batch_halts_after_failed_stop_verification_or_history_save() {
    let engine = fixture_engine(20);
    let reviewed = plans(&engine);
    for (success, freed, history_error) in [
        (false, false, None),
        (false, true, None),
        (true, false, None),
        (true, true, Some("fixture persistence failure".into())),
    ] {
        let attempts = Cell::new(0);
        let result = crate::stop_batch::run(&engine, "agent:100", &reviewed, |pinned, _| {
            attempts.set(attempts.get() + 1);
            crate::commands::Stopped {
                report: holdmap_core::StopReport {
                    target: pinned.target.clone(),
                    success,
                    freed,
                    ..Default::default()
                },
                history_error: history_error.clone(),
            }
        })
        .unwrap();
        assert_eq!(attempts.get(), 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].report.success, success);
        assert_eq!(result[0].report.freed, freed);
        assert_eq!(result[0].history_error, history_error);
    }
}

#[test]
fn immediate_batch_retains_shared_process_plans_after_both_listeners_are_released() {
    use std::net::TcpListener;
    let listeners = [
        TcpListener::bind("127.0.0.1:0").unwrap(),
        TcpListener::bind("127.0.0.1:0").unwrap(),
    ];
    let ports = listeners
        .each_ref()
        .map(|socket| socket.local_addr().unwrap().port());
    let mut engine = fixture_engine(20);
    // A synthetic identity outside OS PID ranges cannot authorize a real signal.
    let child = process(2_020_202_020, Some(100), "node", 10);
    engine.scan.table = ProcessTable::from_processes(
        [
            (100, process(100, None, "claude", 1)),
            (child.pid, child.clone()),
            (999, process(999, None, "holdmap", 100)),
        ]
        .into(),
        999,
    );
    for (entry, port) in engine.scan.snapshot.entries.iter_mut().zip(ports) {
        entry.id = format!("tcp:{port}:{}:listen", child.pid);
        entry.port = port;
        entry.pid = Some(child.pid);
        entry.pids = vec![child.pid];
        entry.process = Some(child.clone());
    }
    let reviewed: Vec<_> = ports
        .into_iter()
        .map(|port| {
            let target = format!("{port}/tcp");
            let (parsed, options) = stop_request(&target, false, false);
            PinnedStop {
                target,
                plan: engine.plan(&parsed, &options),
            }
        })
        .collect();
    assert!(reviewed.iter().all(|pinned| !pinned.plan.is_blocked()));
    let mut attempts = 0;
    let mut listeners = Some(listeners);
    let results = crate::stop_batch::run(&engine, "agent:100", &reviewed, |_, execution| {
        // One shared process exit releases both listeners. Later plans still verify their
        // own real bind probes instead of being replaced with a NothingToStop plan.
        drop(listeners.take());
        attempts += 1;
        let report = holdmap_core::execute(execution, &mut |_| {});
        assert!(report.signalled.is_empty());
        crate::commands::Stopped {
            report,
            history_error: None,
        }
    })
    .unwrap();
    assert_eq!(attempts, 2);
    assert_eq!(results.len(), 2);
    assert!(results
        .iter()
        .all(|result| result.report.success && result.report.freed));
}

#[test]
fn immediate_batch_reuses_only_successful_supervisor_commands_and_keeps_each_verification() {
    use holdmap_core::engine::strategies::StopStrategy;
    use holdmap_core::engine::{Resolution, ResolveCtx};
    use holdmap_core::{Owner, Supervisor};

    struct FixtureSupervisor;
    impl StopStrategy for FixtureSupervisor {
        fn name(&self) -> &'static str {
            "fixture supervisor"
        }
        fn resolve(
            &self,
            context: &ResolveCtx,
            entry: &PortEntry,
            _: &[String],
        ) -> Option<Resolution> {
            let guard = context.proc_ref(entry.pid?)?;
            let mut resolution = Resolution::new(
                Owner::Supervised {
                    supervisor: Supervisor::Pm2 {
                        name: "fixture".into(),
                        id: "0".into(),
                    },
                    pid: guard.pid,
                    name: guard.name.clone(),
                },
                "Stop the fixture supervisor",
            );
            resolution.steps.push(Step::RunCommand {
                program: "pm2".into(),
                args: vec!["stop".into(), "0".into()],
                reason: format!("release port {}", entry.port),
                guard: Some(guard),
            });
            Some(resolution)
        }
    }

    let mut engine =
        fixture_engine(20).with_strategies(StrategyRegistry::empty().with(FixtureSupervisor));
    let first = engine.scan.snapshot.entries[0].clone();
    engine.scan.snapshot.entries[1].pid = first.pid;
    engine.scan.snapshot.entries[1].pids = first.pids;
    engine.scan.snapshot.entries[1].process = first.process;
    let reviewed = plans(&engine);
    for success in [true, false] {
        let mut commands = 0;
        let mut verified = Vec::new();
        let mut attempted = 0;
        let results =
            crate::stop_batch::run(&engine, "agent:100", &reviewed, |original, execution| {
                attempted += 1;
                // History must still use the original guarded supervisor effect.
                assert!(matches!(
                    original.plan.steps[0],
                    Step::RunCommand { guard: Some(_), .. }
                ));
                for step in &execution.steps {
                    match step {
                        Step::RunCommand { .. } => {
                            commands += 1;
                            assert_eq!(
                                commands, 1,
                                "an exited supervisor guard cannot authorize a second command"
                            );
                        }
                        Step::VerifyFree { port, .. } => verified.push(*port),
                        _ => panic!("unexpected fixture effect"),
                    }
                }
                crate::commands::Stopped {
                    report: holdmap_core::StopReport {
                        target: original.target.clone(),
                        success,
                        freed: true,
                        ..Default::default()
                    },
                    history_error: None,
                }
            })
            .unwrap();
        assert_eq!(commands, 1);
        if success {
            assert_eq!(attempted, 2);
            assert_eq!(verified, [3000, 4000]);
            assert_eq!(results.len(), 2);
            assert!(results[1].report.log[0].contains("without repeating that command"));
        } else {
            assert_eq!(attempted, 1);
            assert_eq!(verified, [3000]);
            assert_eq!(results.len(), 1);
        }
    }
    let mut commands = 0;
    let mut verified = Vec::new();
    let mut recorded = 0;
    let outcome = execute_stops(
        &reviewed,
        |execution| {
            for step in &execution.steps {
                match step {
                    Step::RunCommand { .. } => commands += 1,
                    Step::VerifyFree { port, .. } => verified.push(*port),
                    _ => panic!("unexpected scheduled fixture effect"),
                }
            }
            holdmap_core::StopReport {
                success: true,
                freed: true,
                ..Default::default()
            }
        },
        |original, _| {
            assert!(matches!(
                original.steps[0],
                Step::RunCommand { guard: Some(_), .. }
            ));
            recorded += 1;
            Ok(())
        },
    )
    .unwrap();
    assert!(outcome.unwrap().contains("Stopped 2"));
    assert_eq!(
        commands, 1,
        "scheduled batch cannot repeat an exited guard's command"
    );
    assert_eq!(verified, [3000, 4000]);
    assert_eq!(recorded, 2);
}

fn operation() -> Operation {
    Operation::Stops {
        plans: plans(&fixture_engine(20)),
        agent: None,
    }
}

fn clock() -> ClockTime {
    ClockTime {
        mono: Instant::now(),
        wall_ms: 1_000_000,
    }
}
fn later(created: ClockTime, seconds: u64) -> ClockTime {
    ClockTime {
        mono: created.mono + Duration::from_secs(seconds),
        wall_ms: created.wall_ms + seconds * 1000,
    }
}
fn insert(state: &SchedulerState, created: ClockTime) -> ScheduledAction {
    state
        .queue
        .lock()
        .unwrap()
        .insert(
            operation(),
            "fixture".into(),
            vec!["3000/tcp".into(), "4000/tcp".into()],
            60,
            created,
        )
        .unwrap()
}

#[test]
fn request_contract_rejects_broad_force_protected_duplicate_and_invalid_targets() {
    let request = |target: &str| ScheduledStopRequest {
        target: target.into(),
        confirmation_id: "1".into(),
        force: false,
        allow_protected: false,
    };
    for target in [
        "3000",
        "pid:1",
        "node",
        "cluster:shop",
        "dev:all",
        "3000/TCP",
        "3000/sctp",
        "0/tcp",
        "03000/tcp",
        "3000/tcp/udp",
    ] {
        assert!(validate_requests(&[request(target)]).is_err(), "{target}");
    }
    assert!(validate_requests(&[request("3000/tcp"), request("3000/udp")]).is_ok());
    assert!(validate_requests(&[request("3000/tcp"), request("3000/tcp")]).is_err());
    assert!(validate_requests(&[]).is_err());
    assert!(validate_requests(&vec![request("3000/tcp"); 33]).is_err());
    let mut unsafe_request = request("3000/tcp");
    unsafe_request.force = true;
    assert!(validate_requests(&[unsafe_request.clone()]).is_err());
    unsafe_request.force = false;
    unsafe_request.allow_protected = true;
    assert!(validate_requests(&[unsafe_request]).is_err());
    for seconds in [0, 59, 86401, u64::MAX] {
        assert!(validate_delay(seconds).is_err());
    }
    for seconds in [60, 3600, 86400] {
        assert!(validate_delay(seconds).is_ok());
    }
}

#[test]
fn delayed_plans_require_pinned_normal_nonprotected_effects() {
    let mut plan = plans(&fixture_engine(20))[0].plan.clone();
    assert!(delayed_plan_allowed(&plan).is_ok());
    plan.allow_protected = true;
    assert!(delayed_plan_allowed(&plan).is_err());
    plan.allow_protected = false;
    plan.blocked = Some(Blocked {
        kind: BlockKind::Protected,
        message: "fixture protected".into(),
        overridable: true,
    });
    assert!(delayed_plan_allowed(&plan).is_err());
    plan.blocked = None;
    let original = plan.clone();
    if let Step::SignalProcesses { force, .. } = &mut plan.steps[0] {
        *force = true;
    }
    assert!(delayed_plan_allowed(&plan).is_err());
    plan = original.clone();
    if let Step::SignalProcesses { processes, .. } = &mut plan.steps[0] {
        processes[0].start_token = 0;
    }
    assert!(delayed_plan_allowed(&plan).is_err());
    plan = original;
    plan.steps[0] = Step::RunCommand {
        program: "systemctl".into(),
        args: vec!["stop".into(), "fixture.socket".into()],
        reason: "fixture".into(),
        guard: None,
    };
    assert!(delayed_plan_allowed(&plan).is_err());
}

#[test]
fn changed_second_plan_preflight_blocks_entire_batch_before_any_effect() {
    let created = clock();
    let state = SchedulerState::default();
    insert(&state, created);
    state.tick(
        || later(created, 60),
        |operation| {
            let Operation::Stops { plans, .. } = operation else {
                unreachable!()
            };
            validate_stops(&fixture_engine(21), plans, None)
        },
        |_, _| panic!("stale second target executed"),
        |_| {},
        || {},
    );
    assert_eq!(state.list()[0].status, ScheduleStatus::NeedsReview);
    assert!(state.list()[0]
        .message
        .as_ref()
        .unwrap()
        .contains("No action was taken"));
}

#[test]
fn agent_root_identity_and_listener_membership_are_pinned() {
    let engine = fixture_engine(20);
    let plans = plans(&engine);
    let (identity, _) = agent_identity(&engine, "agent:100", &plans).unwrap();
    assert!(validate_stops(&engine, &plans, Some(&identity)).is_ok());
    let mut changed = engine.clone();
    let mut root = changed.scan.table.get(100).unwrap().clone();
    root.start_token = 99;
    changed.scan.table = ProcessTable::from_processes(
        [
            (100, root),
            (101, process(101, Some(100), "node", 10)),
            (102, process(102, Some(100), "node", 20)),
            (999, process(999, None, "holdmap", 100)),
        ]
        .into(),
        999,
    );
    assert!(validate_stops(&changed, &plans, Some(&identity)).is_err());
    changed = engine.clone();
    changed.scan.snapshot.entries[1].id.push_str(":changed");
    assert!(validate_stops(&changed, &plans, Some(&identity)).is_err());
    changed = engine.clone();
    changed.scan.snapshot.entries.remove(1);
    assert!(validate_stops(&changed, &plans, Some(&identity)).is_err());
    let mut extra = engine.clone();
    let mut listener = extra.scan.snapshot.entries[0].clone();
    listener.id.push_str(":new");
    extra.scan.snapshot.entries.push(listener);
    assert!(validate_stops(&extra, &plans, Some(&identity)).is_err());
}

#[test]
fn agent_plan_cannot_include_other_owned_process_or_agent_app() {
    let engine = fixture_engine(20);
    let mut plans = plans(&engine);
    if let Step::SignalProcesses { processes, .. } = &mut plans[0].plan.steps[0] {
        processes[0].pid = 999;
    }
    assert!(agent_identity(&engine, "agent:100", &plans).is_err());
    let report = engine.agents();
    let mut agent = report.agents[0].clone();
    agent.ports[0].role = holdmap_core::agents::PortRole::Agent;
    assert!(listeners(&agent, "3000/tcp").is_err());
    assert!(listeners(&agent, "3000/udp").is_err());
}

#[test]
fn schedule_never_dispatches_early_and_success_is_never_replayed() {
    let created = clock();
    let state = SchedulerState::default();
    insert(&state, created);
    state.tick(
        || later(created, 59),
        |_| panic!("early preflight"),
        |_, ()| panic!("early dispatch"),
        |_| {},
        || {},
    );
    assert_eq!(state.list()[0].status, ScheduleStatus::Pending);
    let effects = Cell::new(0);
    state.tick(
        || later(created, 60),
        |_| Ok(()),
        |_, ()| {
            effects.set(effects.get() + 1);
            Ok(Some("fixture completed".into()))
        },
        |_| {},
        || {},
    );
    assert_eq!(state.list()[0].status, ScheduleStatus::Completed);
    assert!(!state.list()[0].cancellable);
    state.tick(
        || later(created, 61),
        |_| panic!("replay preflight"),
        |_, ()| panic!("replay dispatch"),
        |_| {},
        || {},
    );
    assert_eq!(effects.get(), 1);
}

#[test]
fn cancellation_during_checking_prevents_dispatch_and_releases_once() {
    let created = clock();
    let state = SchedulerState::default();
    let action = insert(&state, created);
    let releases = Cell::new(0);
    state.tick(
        || later(created, 60),
        |_| {
            assert_eq!(state.list()[0].status, ScheduleStatus::Checking);
            state
                .cancel(&action.id, |_| releases.set(releases.get() + 1))
                .unwrap();
            Ok(())
        },
        |_, ()| panic!("cancelled check dispatched"),
        |_| releases.set(releases.get() + 1),
        || {},
    );
    assert_eq!(state.list()[0].status, ScheduleStatus::Cancelled);
    assert_eq!(releases.get(), 1);
    assert!(state
        .cancel(&action.id, |_| panic!("released twice"))
        .is_err());
}

#[test]
fn running_dispatch_cannot_be_cancelled_and_failure_is_not_retried() {
    let created = clock();
    let state = SchedulerState::default();
    let action = insert(&state, created);
    state.tick(
        || later(created, 60),
        |_| Ok(()),
        |_, ()| {
            assert_eq!(state.list()[0].status, ScheduleStatus::Running);
            assert!(!state.list()[0].cancellable);
            assert!(state
                .cancel(&action.id, |_| panic!("running action released"))
                .is_err());
            Err("1 of 2 targets completed; fixture failed. No automatic retry.".into())
        },
        |_| {},
        || {},
    );
    assert_eq!(state.list()[0].status, ScheduleStatus::Failed);
    assert!(state.list()[0].message.as_ref().unwrap().contains("1 of 2"));
    state.tick(
        || later(created, 61),
        |_| panic!("failed action retried"),
        |_, ()| panic!("failed action dispatched"),
        |_| {},
        || {},
    );
}

#[test]
fn sleep_clock_forward_backward_and_late_deadlines_fail_closed() {
    let created = clock();
    for now in [
        later(created, 121),
        ClockTime {
            mono: created.mono + Duration::from_secs(1),
            wall_ms: created.wall_ms + 60_000,
        },
        ClockTime {
            mono: created.mono + Duration::from_secs(40),
            wall_ms: created.wall_ms,
        },
        ClockTime {
            mono: created.mono,
            wall_ms: created.wall_ms - 31_000,
        },
    ] {
        let state = SchedulerState::default();
        insert(&state, created);
        let released = Cell::new(false);
        state.tick(
            || now,
            |_| panic!("missed action preflight"),
            |_, ()| panic!("missed action dispatched"),
            |_| released.set(true),
            || {},
        );
        assert_eq!(state.list()[0].status, ScheduleStatus::NeedsReview);
        assert!(released.get());
    }
}

#[test]
fn threshold_equality_allowed_but_long_preflight_fails_closed() {
    let created = clock();
    let state = SchedulerState::default();
    insert(&state, created);
    let now = Cell::new(later(created, 60));
    state.tick(
        || now.get(),
        |_| {
            now.set(later(created, 121));
            Ok(())
        },
        |_, ()| panic!("late preflight dispatched"),
        |_| {},
        || {},
    );
    assert_eq!(state.list()[0].status, ScheduleStatus::NeedsReview);
    let state = SchedulerState::default();
    insert(&state, created);
    state.tick(
        || later(created, 120),
        |_| Ok(()),
        |_, ()| Ok(None),
        |_| {},
        || {},
    );
    assert_eq!(state.list()[0].status, ScheduleStatus::Completed);
    let state = SchedulerState::default();
    insert(&state, created);
    state.tick(
        || ClockTime {
            mono: created.mono + Duration::from_secs(60),
            wall_ms: created.wall_ms + 90_000,
        },
        |_| Ok(()),
        |_, ()| Ok(None),
        |_| {},
        || {},
    );
    assert_eq!(state.list()[0].status, ScheduleStatus::Completed);
}

#[test]
fn batch_dispatch_retains_reviewed_display_and_exact_plans() {
    let created = clock();
    let state = SchedulerState::default();
    let expected = plans(&fixture_engine(20));
    insert(&state, created);
    let observed = RefCell::new(Vec::new());
    state.tick(
        || later(created, 60),
        |operation| {
            let Operation::Stops { plans, .. } = operation else {
                unreachable!()
            };
            for pinned in plans {
                let mut fresh = pinned.plan.clone();
                fresh.summary = "new display label".into();
                validate_fresh(&pinned.plan, &fresh)?;
            }
            Ok(())
        },
        |operation, ()| {
            let Operation::Stops { plans, .. } = operation else {
                unreachable!()
            };
            for pinned in plans {
                observed.borrow_mut().push(pinned.plan.clone());
            }
            Ok(None)
        },
        |_| {},
        || {},
    );
    assert_eq!(
        observed.into_inner(),
        expected.into_iter().map(|p| p.plan).collect::<Vec<_>>()
    );
}

#[test]
fn active_capacity_and_terminal_history_remain_bounded_without_dropping_active_jobs() {
    let created = clock();
    let state = SchedulerState::default();
    for _ in 0..MAX_ACTIVE {
        insert(&state, created);
    }
    assert!(state
        .queue
        .lock()
        .unwrap()
        .insert(operation(), "overflow".into(), vec![], 60, created)
        .is_err());
    let ids: Vec<_> = state.list().iter().map(|j| j.id.clone()).collect();
    for id in ids {
        state.cancel(&id, |_| {}).unwrap();
    }
    let active = insert(&state, created);
    for _ in 0..100 {
        let next = insert(&state, created);
        state.cancel(&next.id, |_| {}).unwrap();
    }
    let jobs = state.list();
    assert_eq!(
        jobs.iter().filter(|j| !j.status.active()).count(),
        MAX_HISTORY
    );
    assert!(jobs
        .iter()
        .any(|j| j.id == active.id && j.status == ScheduleStatus::Pending));
}

#[test]
fn event_snapshots_expose_only_ui_metadata_not_plans_tokens_or_commands() {
    let state = SchedulerState::default();
    insert(&state, clock());
    let value = serde_json::to_string(&state.list()).unwrap();
    for private in [
        "confirmation_id",
        "start_token",
        "server.js",
        "executable",
        "steps",
    ] {
        assert!(!value.contains(private), "{private}");
    }
    assert!(value.contains("due_at_ms"));
    assert!(value.contains("cancellable"));
    assert!(value.contains("\"kind\":\"port\""));
}

#[test]
fn delayed_effects_reject_foreign_and_unknown_current_account_ownership() {
    let engine = fixture_engine(20);
    let original = plans(&engine);
    assert!(validate_ownership(&engine, &original).is_ok());
    for (uid, user) in [
        (Some(2000), Some("fixture")),
        (None, None),
        (Some(2000), Some("other")),
    ] {
        let mut changed = engine.clone();
        let table = engine
            .scan
            .table
            .iter()
            .cloned()
            .map(|mut process| {
                if process.pid == 102 {
                    process.uid = uid;
                    process.user = user.map(str::to_owned);
                }
                (process.pid, process)
            })
            .collect();
        changed.scan.table = ProcessTable::from_processes(table, 999);
        assert!(validate_ownership(&changed, &original).is_err());
        assert!(validate_stops(&changed, &original, None)
            .unwrap_err()
            .contains("current account"));
    }
    let mut supervisor = original.clone();
    let Step::SignalProcesses { processes, .. } = &supervisor[1].plan.steps[0] else {
        unreachable!()
    };
    let guard = processes[0].clone();
    supervisor[1].plan.steps[0] = Step::RunCommand {
        program: "fixture-service-manager".into(),
        args: vec!["stop".into()],
        reason: "fixture".into(),
        guard: Some(guard),
    };
    assert!(validate_ownership(&engine, &supervisor).is_ok());
    let mut foreign = engine.clone();
    foreign.scan.table = ProcessTable::from_processes(
        engine
            .scan
            .table
            .iter()
            .cloned()
            .map(|mut process| {
                if process.pid == 102 {
                    process.uid = Some(0);
                }
                (process.pid, process)
            })
            .collect(),
        999,
    );
    assert!(validate_ownership(&foreign, &supervisor).is_err());
    // Windows-style username fallback is permitted only when numeric identities are absent.
    let mut names = engine.clone();
    names.scan.table = ProcessTable::from_processes(
        engine
            .scan
            .table
            .iter()
            .cloned()
            .map(|mut process| {
                process.uid = None;
                (process.pid, process)
            })
            .collect(),
        999,
    );
    assert!(validate_ownership(&names, &original).is_ok());
}

#[test]
fn small_backwards_clock_change_during_checking_waits_until_both_due_conditions() {
    let created = clock();
    let state = SchedulerState::default();
    insert(&state, created);
    let now = Cell::new(later(created, 60));
    state.tick(
        || now.get(),
        |_| {
            now.set(ClockTime {
                mono: created.mono + Duration::from_secs(61),
                wall_ms: created.wall_ms + 40_000,
            });
            Ok(())
        },
        |_, ()| panic!("clock moved dispatch before reviewed time"),
        |_| panic!("valid pending job lost authority"),
        || {},
    );
    assert_eq!(state.list()[0].status, ScheduleStatus::Pending);
    assert!(state.list()[0].cancellable);
    state.tick(
        || ClockTime {
            mono: created.mono + Duration::from_secs(81),
            wall_ms: created.wall_ms + 60_000,
        },
        |_| Ok(()),
        |_, ()| Ok(None),
        |_| {},
        || {},
    );
    assert_eq!(state.list()[0].status, ScheduleStatus::Completed);
}

#[test]
fn delayed_containers_cannot_use_network_runtime_endpoints() {
    let original = plans(&fixture_engine(20))[0].plan.clone();
    for endpoint in [
        "unix:///var/run/docker.sock",
        "npipe:////./pipe/docker_engine",
    ] {
        let mut plan = original.clone();
        plan.steps[0] = Step::StopContainer {
            id: "0123456789abcdef".into(),
            name: "fixture".into(),
            runtime: "Docker".into(),
            endpoint: endpoint.into(),
            timeout_s: 5,
        };
        #[cfg(unix)]
        let platform_local = endpoint.starts_with("unix://");
        #[cfg(target_os = "windows")]
        let platform_local = endpoint.starts_with("npipe://");
        #[cfg(not(any(unix, target_os = "windows")))]
        let platform_local = false;
        assert_eq!(
            delayed_plan_allowed(&plan).is_ok(),
            platform_local,
            "{endpoint}"
        );
    }
    for endpoint in [
        "tcp://remote.example:2375",
        "tcp://127.0.0.1:2375",
        "unix://relative.sock",
        "npipe:////server/pipe/docker_engine",
        "npipe:////./pipe/",
        "npipe:////./pipe/../other",
        "ssh://remote",
    ] {
        let mut plan = original.clone();
        plan.steps[0] = Step::StopContainer {
            id: "0123456789abcdef".into(),
            name: "fixture".into(),
            runtime: "Docker".into(),
            endpoint: endpoint.into(),
            timeout_s: 5,
        };
        assert!(delayed_plan_allowed(&plan).is_err(), "{endpoint}");
    }
}

#[test]
fn listener_still_busy_is_failed_and_remaining_plans_are_not_dispatched() {
    let plans = plans(&fixture_engine(20));
    let executions = Cell::new(0);
    let recorded = RefCell::new(Vec::new());
    let result = execute_stops(
        &plans,
        |plan| {
            executions.set(executions.get() + 1);
            holdmap_core::StopReport {
                target: plan.target.clone(),
                success: true,
                freed: false,
                ports_still_busy: vec![3000],
                signalled: vec![101],
                ..Default::default()
            }
        },
        |plan, report| {
            recorded
                .borrow_mut()
                .push((plan.target.clone(), report.signalled.clone()));
            Ok(())
        },
    );
    let error = result.unwrap_err();
    assert!(error.contains("0 of 2 targets completed"));
    assert!(error.contains("3000"));
    assert_eq!(executions.get(), 1);
    assert_eq!(recorded.borrow().len(), 1);
    assert_eq!(recorded.borrow()[0].1, vec![101]);
}

#[test]
fn partial_batch_failure_records_attempts_and_history_failure_is_reported() {
    let plans = plans(&fixture_engine(20));
    let executions = Cell::new(0);
    let recorded = Cell::new(0);
    let result = execute_stops(
        &plans,
        |plan| {
            let index = executions.get();
            executions.set(index + 1);
            holdmap_core::StopReport {
                target: plan.target.clone(),
                success: index == 0,
                freed: index == 0,
                error: (index != 0).then(|| "fixture permission denied".into()),
                ..Default::default()
            }
        },
        |_, _| {
            recorded.set(recorded.get() + 1);
            if recorded.get() == 2 {
                Err("fixture disk full".into())
            } else {
                Ok(())
            }
        },
    );
    let error = result.unwrap_err();
    assert!(error.contains("1 of 2 targets completed"));
    assert!(error.contains("permission denied"));
    assert!(error.contains("history could not be saved"));
    assert!(error.contains("disk full"));
    assert_eq!(executions.get(), 2);
    assert_eq!(recorded.get(), 2);
    let executions = Cell::new(0);
    let result = execute_stops(
        &plans,
        |plan| {
            executions.set(executions.get() + 1);
            holdmap_core::StopReport {
                target: plan.target.clone(),
                success: true,
                freed: true,
                ..Default::default()
            }
        },
        |_, _| Err("fixture disk full".into()),
    );
    assert!(result.unwrap_err().contains("1 of 2 targets stopped"));
    assert_eq!(executions.get(), 1);
}

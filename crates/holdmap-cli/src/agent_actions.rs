//! Reviewed agent service stops shared by the CLI and terminal UI.
//!
//! A review retains exact listener protocols and process identities. A fresh scan must
//! authorize the whole batch before execution; no newly discovered owner is substituted.

use anyhow::{bail, Context, Result};
use holdmap_core::agents::{Agent, ProcessRole};
use holdmap_core::{ActionPlan, Engine, ProcRef, Protocol, Step, StopOptions, StopReport, Target};
use std::collections::HashSet;

#[derive(Debug, Clone)]
struct AgentIdentity {
    id: String,
    product: String,
    pid: u32,
    start_token: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewedAgentPort {
    pub name: String,
    pub port: u16,
    pub protocol: Protocol,
    pub plan: ActionPlan,
    agent: AgentIdentity,
    listeners: Vec<String>,
}

impl ReviewedAgentPort {
    pub fn target(&self) -> String {
        format!("{}/{}", self.port, self.protocol.to_string().to_lowercase())
    }
}

#[derive(Debug)]
pub(crate) struct ReviewedAgentStops {
    plans: Vec<ReviewedAgentPort>,
}

/// Execution is intentionally not transactional. Completed reports remain available when
/// a later service fails; unattempted plans are counted and never retried automatically.
#[derive(Debug)]
pub(crate) struct AgentStopOutcome {
    pub reports: Vec<StopReport>,
    pub remaining: usize,
    pub history_errors: Vec<String>,
}

impl AgentStopOutcome {
    pub fn succeeded(&self) -> bool {
        self.remaining == 0
            && self.history_errors.is_empty()
            && !self.reports.is_empty()
            && self.reports.iter().all(|r| r.success && r.freed)
    }
}

impl ReviewedAgentStops {
    pub fn preview(engine: &Engine, agents: &[Agent]) -> Result<Self> {
        let mut plans = Vec::new();
        let mut seen = HashSet::new();
        for agent in agents {
            if agent.stoppable_ports().next().is_none() {
                continue;
            }
            let identity = identity(engine, agent)?;
            for listener in agent.stoppable_ports() {
                // Multiple bind addresses describe one protocol-qualified target. An
                // ambiguous target shared by agents is rejected by effect ownership below.
                if !seen.insert((listener.port, listener.protocol)) {
                    continue;
                }
                let plan = engine.plan(&Target::Port(listener.port), &options(listener.protocol));
                verify_effects(engine, agent, &plan)?;
                plans.push(ReviewedAgentPort {
                    name: agent.name.clone(),
                    port: listener.port,
                    protocol: listener.protocol,
                    listeners: listener_ids(agent, listener.port, listener.protocol),
                    agent: identity.clone(),
                    plan,
                });
            }
        }
        Ok(Self { plans })
    }

    pub fn plans(&self) -> &[ReviewedAgentPort] {
        &self.plans
    }

    pub fn validate_fresh(&self, engine: &Engine) -> Result<()> {
        let report = engine.agents();
        for reviewed in &self.plans {
            let agent = report
                .agents
                .iter()
                .find(|a| a.id == reviewed.agent.id)
                .context("The agent has exited. Refresh and review its services again.")?;
            let current = identity(engine, agent)?;
            if current.pid != reviewed.agent.pid
                || current.product != reviewed.agent.product
                || current.start_token != reviewed.agent.start_token
                || listener_ids(agent, reviewed.port, reviewed.protocol) != reviewed.listeners
            {
                bail!("The agent or its service ownership changed. Refresh and review again.");
            }
            verify_effects(engine, agent, &reviewed.plan)?;
            let current = engine.plan(&Target::Port(reviewed.port), &options(reviewed.protocol));
            verify_effects(engine, agent, &current)?;
            if current.risk > reviewed.plan.risk || !reviewed.plan.same_effects(&current) {
                bail!(
                    "{} changed since review. Refresh and review the stop plans again.",
                    reviewed.target()
                );
            }
        }
        Ok(())
    }

    /// Consume a single review after checking every target against the same fresh scan.
    /// The core executor rechecks each process identity and protection before every effect.
    pub fn execute_with(
        self,
        fresh: &Engine,
        mut execute: impl FnMut(&ReviewedAgentPort) -> StopReport,
        mut record: impl FnMut(&ActionPlan, &StopReport) -> Result<(), String>,
    ) -> Result<AgentStopOutcome> {
        self.validate_fresh(fresh)?;
        let total = self.plans.len();
        let mut reports = Vec::new();
        let mut history_errors = Vec::new();
        for reviewed in &self.plans {
            let mut report = execute(reviewed);
            report.target = reviewed.target();
            if let Err(error) = record(&reviewed.plan, &report) {
                history_errors.push(error);
            }
            let successful = report.success && report.freed;
            reports.push(report);
            if !successful || !history_errors.is_empty() {
                break;
            }
        }
        Ok(AgentStopOutcome {
            remaining: total - reports.len(),
            reports,
            history_errors,
        })
    }
}

fn options(protocol: Protocol) -> StopOptions {
    StopOptions {
        protocol: Some(protocol),
        ..Default::default()
    }
}

fn identity(engine: &Engine, agent: &Agent) -> Result<AgentIdentity> {
    let start_token = engine
        .table()
        .get(agent.pid)
        .map(|p| p.start_token)
        .filter(|token| *token != 0)
        .context("The agent process identity is unavailable. Refresh and review again.")?;
    Ok(AgentIdentity {
        id: agent.id.clone(),
        product: agent.product.clone(),
        pid: agent.pid,
        start_token,
    })
}

fn listener_ids(agent: &Agent, port: u16, protocol: Protocol) -> Vec<String> {
    let mut ids: Vec<_> = agent
        .stoppable_ports()
        .filter(|p| p.port == port && p.protocol == protocol)
        .map(|p| p.entry_id.clone())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

fn verify_effects(engine: &Engine, agent: &Agent, plan: &ActionPlan) -> Result<()> {
    if plan.is_blocked() {
        bail!(
            "Cannot stop {} services: {}",
            holdmap_core::util::printable(&agent.name),
            holdmap_core::util::printable(&holdmap_core::redact::line(
                &plan.blocked.as_ref().expect("blocked plan").message
            ))
        );
    }
    if plan.allow_protected || !plan.same_effects(plan) {
        bail!("This service has no safely pinned stop plan. Refresh and review again.");
    }
    let owns = |reference: &ProcRef| {
        reference.pid != agent.pid
            && agent.process_ids.contains(&reference.pid)
            && !agent.processes.iter().any(|p| {
                p.pid == reference.pid && matches!(p.role, ProcessRole::Agent | ProcessRole::Helper)
            })
            && engine.table().get(reference.pid).is_some_and(|p| {
                p.start_token == reference.start_token
                    && reference.start_token != 0
                    && match (p.uid, engine.table().current_uid()) {
                        (Some(owner), Some(account)) => owner == account,
                        _ => engine.table().is_mine(p.pid),
                    }
                    && !engine.policy().protection(p, engine.table()).is_protected()
            })
    };
    for step in &plan.steps {
        let allowed = match step {
            Step::SignalProcesses {
                processes, force, ..
            } => !force && !processes.is_empty() && processes.iter().all(&owns),
            Step::RunCommand { guard, .. } => guard.as_ref().is_some_and(owns),
            // A container or unguarded supervisor is not a child process owned by this agent.
            Step::StopContainer { .. } => false,
            Step::VerifyFree { .. } => true,
        };
        if !allowed {
            bail!("A stop plan includes a protected session or a process outside this agent's services. No service was stopped.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_core::agents::AgentsBuilder;
    use holdmap_core::process::ProcessTable;
    use holdmap_core::scan::{ScanOptions, Scanner};
    use holdmap_core::{Family, ProcessInfo, RawSocket, SocketState};

    fn process(pid: u32, parent: u32, name: &str, command: &[&str]) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid: Some(parent),
            name: name.into(),
            exe: None,
            cmdline: command.iter().map(|s| s.to_string()).collect(),
            cwd: None,
            uid: Some(1000),
            user: Some("dev".into()),
            start_time: 1_700_000_000,
            start_token: u64::from(pid) + 1000,
            memory_bytes: 1024,
            cpu_percent: 0.0,
        }
    }

    fn listener(pid: u32, port: u16, protocol: Protocol) -> RawSocket {
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

    fn fixture() -> (Vec<ProcessInfo>, Vec<RawSocket>) {
        (
            vec![
                process(1, 0, "init", &["init"]),
                process(100, 1, "claude", &["claude"]),
                process(101, 100, "npm", &["npm", "run", "dev"]),
                process(102, 101, "node", &["node", "vite"]),
                process(104, 100, "node", &["node", "udp-server.js"]),
                process(999, 1, "holdmap", &["holdmap"]),
            ],
            vec![
                listener(102, 3000, Protocol::Tcp),
                listener(104, 3000, Protocol::Udp),
            ],
        )
    }

    fn engine(procs: Vec<ProcessInfo>, raw: Vec<RawSocket>) -> Engine {
        let table =
            ProcessTable::from_processes(procs.into_iter().map(|p| (p.pid, p)).collect(), 999);
        let scan = Scanner::fixed(raw, table, vec![])
            .scan(&ScanOptions {
                all_states: false,
                docker: false,
            })
            .unwrap();
        Engine::from_scan(scan)
    }

    fn preview(engine: &Engine) -> ReviewedAgentStops {
        let agents = AgentsBuilder::new(&engine.scan)
            .with_project_detection(false)
            .build()
            .agents;
        ReviewedAgentStops::preview(engine, &agents).unwrap()
    }

    fn success(plan: &ActionPlan) -> StopReport {
        StopReport {
            target: plan.target.clone(),
            success: true,
            freed: true,
            ..Default::default()
        }
    }

    #[test]
    fn one_numeric_port_keeps_two_protocols_and_bind_rows_are_deduplicated() {
        let (procs, mut raw) = fixture();
        let mut second_bind = raw[0].clone();
        second_bind.local_addr = "127.0.0.2".parse().unwrap();
        raw.push(second_bind);
        let engine = engine(procs, raw);
        let review = preview(&engine);
        assert_eq!(review.plans().len(), 2);
        for protocol in [Protocol::Tcp, Protocol::Udp] {
            let reviewed = review
                .plans()
                .iter()
                .find(|p| p.protocol == protocol)
                .unwrap();
            let verifies: Vec<_> = reviewed
                .plan
                .steps
                .iter()
                .filter_map(|step| match step {
                    Step::VerifyFree { protocol, .. } => Some(*protocol),
                    _ => None,
                })
                .collect();
            assert_eq!(verifies, vec![protocol]);
        }
        assert!(review.validate_fresh(&engine).is_ok());
    }

    #[test]
    fn unrelated_listener_or_foreign_uid_never_grants_agent_authority() {
        let (mut procs, mut raw) = fixture();
        procs.push(process(200, 1, "node", &["node", "other-server.js"]));
        raw.push(listener(200, 3000, Protocol::Tcp));
        let e = engine(procs, raw);
        let agents = e.agents().agents;
        assert!(ReviewedAgentStops::preview(&e, &agents).is_err());

        let (mut procs, raw) = fixture();
        // Identical display names cannot grant ownership across numeric accounts.
        procs.iter_mut().find(|p| p.pid == 104).unwrap().uid = Some(2000);
        let e = engine(procs, raw);
        assert!(ReviewedAgentStops::preview(&e, &e.agents().agents).is_err());
    }

    #[test]
    fn missing_root_or_child_identity_fails_closed() {
        for pid in [100, 102] {
            let (mut procs, raw) = fixture();
            procs.iter_mut().find(|p| p.pid == pid).unwrap().start_token = 0;
            let e = engine(procs, raw);
            assert!(
                ReviewedAgentStops::preview(&e, &e.agents().agents).is_err(),
                "PID {pid}"
            );
        }
    }

    #[test]
    fn root_restart_product_change_and_child_reuse_require_new_review() {
        for change in ["root", "product", "child", "ownership", "session"] {
            let (procs, raw) = fixture();
            let review = preview(&engine(procs.clone(), raw.clone()));
            let mut changed = procs;
            match change {
                "root" => {
                    changed
                        .iter_mut()
                        .find(|p| p.pid == 100)
                        .unwrap()
                        .start_token += 1
                }
                "product" => {
                    let p = changed.iter_mut().find(|p| p.pid == 100).unwrap();
                    p.name = "codex".into();
                    p.cmdline = vec!["codex".into()];
                }
                "child" => {
                    changed
                        .iter_mut()
                        .find(|p| p.pid == 104)
                        .unwrap()
                        .start_token += 1
                }
                "ownership" => changed.iter_mut().find(|p| p.pid == 104).unwrap().ppid = Some(1),
                _ => {
                    let p = changed.iter_mut().find(|p| p.pid == 104).unwrap();
                    p.name = "zsh".into();
                    p.cmdline = vec!["-zsh".into()];
                }
            }
            assert!(
                review.validate_fresh(&engine(changed, raw)).is_err(),
                "{change}"
            );
        }
    }

    #[test]
    fn new_tree_member_does_not_expand_reviewed_authority() {
        let (procs, raw) = fixture();
        let review = preview(&engine(procs.clone(), raw.clone()));
        let mut changed = procs;
        changed.push(process(103, 102, "node", &["node", "worker.js"]));
        assert!(review.validate_fresh(&engine(changed, raw)).is_err());
    }

    #[test]
    fn invalid_later_plan_prevents_every_effect_and_history_write() {
        let (procs, raw) = fixture();
        let review = preview(&engine(procs.clone(), raw.clone()));
        let mut changed = procs;
        changed
            .iter_mut()
            .find(|p| p.pid == 104)
            .unwrap()
            .start_token += 1;
        let e = engine(changed, raw);
        let mut executions = 0;
        let mut records = 0;
        let result = review.execute_with(
            &e,
            |p| {
                executions += 1;
                success(&p.plan)
            },
            |_, _| {
                records += 1;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!((executions, records), (0, 0));
    }

    #[test]
    fn execution_uses_original_plans_and_reports_complete_success() {
        let (procs, raw) = fixture();
        let e = engine(procs, raw);
        let review = preview(&e);
        let original: Vec<_> = review.plans().iter().map(|p| p.plan.clone()).collect();
        let mut executed = Vec::new();
        let mut recorded = Vec::new();
        let result = review
            .execute_with(
                &e,
                |p| {
                    executed.push(p.plan.clone());
                    success(&p.plan)
                },
                |p, r| {
                    recorded.push((p.clone(), r.clone()));
                    Ok(())
                },
            )
            .unwrap();
        assert!(result.succeeded());
        assert_eq!(executed, original);
        assert_eq!(recorded.len(), 2);
        assert_eq!(result.remaining, 0);
    }

    #[test]
    fn a_partial_or_failed_stop_retains_effect_reports_without_retrying() {
        for freed in [false, true] {
            let (procs, raw) = fixture();
            let e = engine(procs, raw);
            let review = preview(&e);
            let mut records = 0;
            let outcome = review
                .execute_with(
                    &e,
                    |p| StopReport {
                        target: p.plan.target.clone(),
                        success: false,
                        freed,
                        signalled: vec![102],
                        error: Some("fixture refusal".into()),
                        ..Default::default()
                    },
                    |_, _| {
                        records += 1;
                        Ok(())
                    },
                )
                .unwrap();
            assert!(!outcome.succeeded());
            assert_eq!(outcome.remaining, 1);
            assert_eq!(outcome.reports.len(), 1);
            assert_eq!(outcome.reports[0].signalled, vec![102]);
            assert_eq!(records, 1);
        }
        let (procs, raw) = fixture();
        let e = engine(procs, raw);
        let outcome = preview(&e)
            .execute_with(
                &e,
                |p| StopReport {
                    success: true,
                    freed: false,
                    ..success(&p.plan)
                },
                |_, _| Ok(()),
            )
            .unwrap();
        assert!(!outcome.succeeded());
        assert_eq!(outcome.remaining, 1);
    }

    #[test]
    fn history_failure_is_visible_after_effect_and_halts_unattempted_services() {
        let (procs, raw) = fixture();
        let e = engine(procs, raw);
        let mut executions = 0;
        let outcome = preview(&e)
            .execute_with(
                &e,
                |p| {
                    executions += 1;
                    success(&p.plan)
                },
                |_, _| Err("disk is full".into()),
            )
            .unwrap();
        assert!(!outcome.succeeded());
        assert_eq!(outcome.history_errors, vec!["disk is full"]);
        assert_eq!(
            (executions, outcome.reports.len(), outcome.remaining),
            (1, 1, 1)
        );
        assert!(outcome.reports[0].success);
    }
}

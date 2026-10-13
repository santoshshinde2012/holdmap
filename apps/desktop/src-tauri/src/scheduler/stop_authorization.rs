//! Retain exact reviewed effects and verify fresh listener and agent identity.

use super::model::*;
use crate::commands::stop_request;
use crate::confirmation::validate_fresh;
use holdmap_core::agents::Agent;
use holdmap_core::{ActionPlan, Engine, ProcRef, Protocol, Step};
use std::collections::HashSet;

const MAX_TARGETS: usize = 32;

fn exact_listener(target: &str) -> Result<(u16, Protocol), String> {
    let Some((port, protocol)) = target.split_once('/') else {
        return Err("Choose an exact TCP or UDP listener.".into());
    };
    let port: u16 = port.parse().map_err(|_| "Invalid listener port.")?;
    let protocol = match protocol {
        "tcp" => Protocol::Tcp,
        "udp" => Protocol::Udp,
        _ => return Err("Choose an exact TCP or UDP listener.".into()),
    };
    if port == 0
        || target
            != format!(
                "{port}/{}",
                match protocol {
                    Protocol::Tcp => "tcp",
                    Protocol::Udp => "udp",
                }
            )
    {
        return Err("Choose a canonical nonzero port and protocol.".into());
    }
    Ok((port, protocol))
}

pub(super) fn validate_requests(requests: &[ScheduledStopRequest]) -> Result<(), String> {
    if requests.is_empty() || requests.len() > MAX_TARGETS {
        return Err("Choose between 1 and 32 listener targets.".into());
    }
    let mut seen = HashSet::new();
    for request in requests {
        exact_listener(&request.target)?;
        if request.force || request.allow_protected {
            return Err(
                "These listener stops require normal mode without protected-process overrides."
                    .into(),
            );
        }
        if request.confirmation_id.is_empty() || request.confirmation_id.len() > 64 {
            return Err("Review a fresh stop plan for every listener.".into());
        }
        if !seen.insert(&request.target) {
            return Err("A listener target appears more than once.".into());
        }
    }
    Ok(())
}

/// Local scheduling cannot inherit authority over a remote DOCKER_HOST or network pipe.
/// TCP runtime endpoints are deliberately excluded, including localhost proxies.
fn local_container_endpoint(endpoint: &str) -> bool {
    match holdmap_core::docker::parse_docker_host(endpoint) {
        Some(holdmap_core::docker::Endpoint::Unix(path)) => cfg!(unix) && path.is_absolute(),
        Some(holdmap_core::docker::Endpoint::Pipe(path)) => {
            cfg!(target_os = "windows")
                && path.strip_prefix(r"\\.\pipe\").is_some_and(|name| {
                    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
                })
        }
        _ => false,
    }
}

pub(super) fn delayed_plan_allowed(plan: &ActionPlan) -> Result<(), String> {
    if plan.allow_protected || plan.is_blocked() || plan.steps.is_empty() {
        return Err("This stop plan cannot be scheduled safely.".into());
    }
    for step in &plan.steps {
        match step {
            Step::SignalProcesses {
                processes, force, ..
            } if *force || processes.iter().any(|p| p.start_token == 0) => {
                return Err(
                    "A delayed stop requires normal stop mode and pinned process identities."
                        .into(),
                );
            }
            Step::RunCommand { guard: None, .. } => {
                return Err("An unpinned supervisor command cannot be scheduled.".into());
            }
            Step::StopContainer { endpoint, .. } if !local_container_endpoint(endpoint) => {
                return Err("Container schedules require a local Unix socket or local named-pipe runtime. Network runtime endpoints cannot be scheduled.".into());
            }
            _ => {}
        }
    }
    // Reuse the core's complete effect identity validation, including containers and guards.
    validate_fresh(plan, plan)
}

pub(super) fn listeners(agent: &Agent, target: &str) -> Result<Vec<String>, String> {
    let (port, protocol) = exact_listener(target)?;
    let mut ids: Vec<String> = agent
        .stoppable_ports()
        .filter(|p| p.port == port && p.protocol == protocol)
        .map(|p| p.entry_id.clone())
        .collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Err("The listener no longer belongs to this agent's stoppable services.".into());
    }
    Ok(ids)
}

pub(super) fn agent_identity(
    engine: &Engine,
    id: &str,
    plans: &[PinnedStop],
) -> Result<(AgentIdentity, String), String> {
    if id.is_empty() || id.len() > 64 {
        return Err("Invalid agent identity.".into());
    }
    let report = engine.agents();
    let agent = report
        .agents
        .iter()
        .find(|a| a.id == id)
        .ok_or("The agent is no longer running.")?;
    let start_token = engine
        .scan
        .table
        .get(agent.pid)
        .map(|p| p.start_token)
        .filter(|token| *token != 0)
        .ok_or("The agent process identity is unavailable.")?;
    let mut owned = Vec::new();
    for pinned in plans {
        owned.push((pinned.target.clone(), listeners(agent, &pinned.target)?));
        verify_agent_effects(agent, &pinned.plan)?;
    }
    Ok((
        AgentIdentity {
            id: id.into(),
            product: agent.product.clone(),
            pid: agent.pid,
            start_token,
            listeners: owned,
        },
        format!("Stop {} services", agent.name),
    ))
}

pub(super) fn verify_agent_effects(agent: &Agent, plan: &ActionPlan) -> Result<(), String> {
    let owns = |pid: u32| agent.process_ids.contains(&pid);
    if plan.steps.iter().any(|step| match step {
        Step::SignalProcesses { processes, .. } => processes.iter().any(|p| !owns(p.pid)),
        Step::RunCommand {
            guard: Some(process),
            ..
        } => !owns(process.pid),
        Step::StopContainer { .. } | Step::RunCommand { guard: None, .. } => true,
        Step::VerifyFree { .. } => false,
    }) {
        return Err("The stop plan includes a process outside this agent's services.".into());
    }
    Ok(())
}

/// Supervisor plans can authorize explicit administrator actions elsewhere. A delayed
/// desktop job has no such override: identity, account ownership, and policy must agree.
pub(super) fn validate_ownership(engine: &Engine, plans: &[PinnedStop]) -> Result<(), String> {
    let owns = |reference: &ProcRef| {
        let Some(current) = engine.scan.table.get(reference.pid).filter(|process| {
            reference.start_token != 0 && process.start_token == reference.start_token
        }) else {
            return false;
        };
        match (current.uid, engine.scan.table.current_uid()) {
            (Some(owner), Some(account)) => owner == account,
            _ => engine.scan.table.is_mine(reference.pid),
        }
    };
    for step in plans.iter().flat_map(|pinned| &pinned.plan.steps) {
        let allowed = match step {
            Step::SignalProcesses { processes, .. } => {
                !processes.is_empty() && processes.iter().all(&owns)
            }
            Step::RunCommand { guard, .. } => guard.as_ref().is_some_and(owns),
            Step::StopContainer { .. } | Step::VerifyFree { .. } => true,
        };
        if !allowed {
            return Err("Every process or supervisor stop needs a pinned owner from the current account. Foreign or unknown ownership is not permitted.".into());
        }
    }
    Ok(())
}

pub(super) fn validate_stops(
    engine: &Engine,
    plans: &[PinnedStop],
    agent: Option<&AgentIdentity>,
) -> Result<(), String> {
    validate_ownership(engine, plans)?;
    if let Some(identity) = agent {
        let (current, _) = agent_identity(engine, &identity.id, plans)?;
        if current.pid != identity.pid
            || current.product != identity.product
            || current.start_token != identity.start_token
            || current.listeners != identity.listeners
        {
            return Err("The agent or its listener ownership changed since confirmation.".into());
        }
    }
    for pinned in plans {
        delayed_plan_allowed(&pinned.plan)?;
        let (target, options) = stop_request(&pinned.target, false, false);
        validate_fresh(&pinned.plan, &engine.plan(&target, &options))?;
    }
    Ok(())
}

//! Bounded session-local handles bind execution to the effects a client reviewed.
//!
//! Handles are not authentication credentials: access to the stdio session is the trust
//! boundary. A random session namespace prevents accidental reuse after a server restart.

use holdmap_core::{ActionPlan, Risk};
use serde::Serialize;
use std::collections::{hash_map::RandomState, VecDeque};
use std::hash::BuildHasher;
use std::time::{Duration, Instant};

const LIFETIME: Duration = Duration::from_secs(300);
const CAPACITY: usize = 32;
const MAX_PLANS: usize = 128;
const MAX_PREVIEW_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) enum Request {
    Port {
        port: u16,
        force: bool,
        allow_non_dev: bool,
    },
    Agents {
        query: String,
        force: bool,
    },
}

impl Request {
    pub(super) fn permits(&self, plan: &ActionPlan) -> bool {
        !plan.is_blocked()
            && !plan.allow_protected
            && match self {
                Self::Port {
                    allow_non_dev: true,
                    ..
                } => plan.risk != Risk::High,
                _ => plan.risk == Risk::Low,
            }
    }
}

#[derive(Clone, Serialize)]
pub(super) struct Observation {
    pub(super) request: Request,
    /// Sorted selected agent IDs and their root process start tokens, independent of metrics.
    pub(super) agents: Vec<(String, u64)>,
    /// Sorted, unique per-port plans. Every plan is checked before any one executes.
    pub(super) plans: Vec<ActionPlan>,
}

impl Observation {
    fn same_effects(&self, current: &Self) -> bool {
        self.request == current.request
            && self.agents == current.agents
            && self.plans.len() == current.plans.len()
            && self.plans.iter().zip(&current.plans).all(|(old, new)| {
                self.request.permits(new) && old.risk == new.risk && old.same_effects(new)
            })
    }
}

struct Pending {
    id: String,
    issued: Instant,
    observation: Observation,
}

pub(super) struct Confirmations {
    namespace: u64,
    next_id: u64,
    pending: VecDeque<Pending>,
}

impl Default for Confirmations {
    fn default() -> Self {
        Self {
            namespace: RandomState::new().hash_one((std::process::id(), Instant::now())),
            next_id: 0,
            pending: VecDeque::new(),
        }
    }
}

impl Confirmations {
    pub(super) fn clear(&mut self) {
        self.pending.clear();
    }

    pub(super) fn issue(&mut self, observation: Observation) -> Result<String, String> {
        self.issue_at(observation, Instant::now())
    }

    fn issue_at(&mut self, observation: Observation, now: Instant) -> Result<String, String> {
        if observation.plans.is_empty()
            || observation
                .plans
                .iter()
                .any(|plan| !observation.request.permits(plan))
            || !observation.same_effects(&observation)
            || observation.agents.iter().any(|(_, token)| *token == 0)
        {
            return Err(
                "No complete executable stop preview is available. Review a narrower target."
                    .into(),
            );
        }
        if observation.plans.len() > MAX_PLANS || observation.agents.len() > MAX_PLANS {
            return Err(
                "Stop preview is too broad (maximum 128 ports or agents). Use a narrower target."
                    .into(),
            );
        }
        // Count serialized bytes without retaining a second copy of large process metadata.
        struct SizeLimit(usize);
        impl std::io::Write for SizeLimit {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if bytes.len() > MAX_PREVIEW_BYTES.saturating_sub(self.0) {
                    return Err(std::io::Error::other("preview size limit"));
                }
                self.0 += bytes.len();
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        if serde_json::to_writer(SizeLimit(0), &observation).is_err() {
            return Err("Stop preview exceeds 1 MiB. Use a narrower target.".into());
        }
        self.prune(now);
        while self.pending.len() >= CAPACITY {
            self.pending.pop_front();
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("Confirmation counter exhausted; restart the server.")?;
        let id = format!("{:016x}-{}", self.namespace, self.next_id);
        self.pending.push_back(Pending {
            id: id.clone(),
            issued: now,
            observation,
        });
        Ok(id)
    }

    /// Consume before re-observing; a mismatch or collection failure requires a new preview.
    /// Callers validate the entire wire request before reaching this method.
    pub(super) fn take(&mut self, id: &str, request: &Request) -> Result<Observation, String> {
        self.take_at(id, request, Instant::now())
    }

    fn take_at(
        &mut self,
        id: &str,
        request: &Request,
        now: Instant,
    ) -> Result<Observation, String> {
        self.prune(now);
        let index = self.pending.iter().position(|pending| pending.id == id)
            .ok_or("Stop confirmation expired, belongs to another session, or was already used. Review a new preview.")?;
        let pending = self
            .pending
            .remove(index)
            .expect("located pending confirmation");
        if &pending.observation.request != request {
            return Err("Stop target or options changed. Review a new preview.".into());
        }
        Ok(pending.observation)
    }

    fn prune(&mut self, now: Instant) {
        self.pending
            .retain(|pending| now.saturating_duration_since(pending.issued) < LIFETIME);
    }
}

pub(super) fn validate_fresh(confirmed: &Observation, current: &Observation) -> Result<(), String> {
    if !confirmed.same_effects(current) {
        return Err("Stop owners, services or selected agents changed. Review a new preview; nothing was stopped.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_core::{Owner, ProcRef, Step};

    pub(super) fn plan(port: u16, pid: u32, token: u64) -> ActionPlan {
        ActionPlan {
            target: format!(":{port}"),
            summary: "Stop fixture".into(),
            owners: vec![Owner::Process {
                pid,
                name: "node".into(),
            }],
            steps: vec![Step::SignalProcesses {
                processes: vec![ProcRef {
                    pid,
                    name: "node".into(),
                    start_token: token,
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

    fn observation() -> Observation {
        Observation {
            request: Request::Port {
                port: 3000,
                force: false,
                allow_non_dev: false,
            },
            agents: vec![],
            plans: vec![plan(3000, 100, 10)],
        }
    }

    #[test]
    fn mismatched_options_replay_expiry_eviction_and_cross_session_handles_fail_closed() {
        let now = Instant::now();
        let mut cache = Confirmations::default();
        let request = observation().request;
        let id = cache.issue_at(observation(), now).unwrap();
        assert!(cache
            .take_at(
                &id,
                &Request::Port {
                    port: 3000,
                    force: true,
                    allow_non_dev: false
                },
                now
            )
            .is_err());
        assert!(cache.take_at(&id, &request, now).is_err());
        let id = cache.issue_at(observation(), now).unwrap();
        assert!(cache.take_at(&id, &request, now + LIFETIME).is_err());
        let id = cache.issue_at(observation(), now).unwrap();
        assert!(cache.take_at(&id, &request, now).is_ok());
        assert!(cache.take_at(&id, &request, now).is_err());
        let id = cache.issue_at(observation(), now).unwrap();
        let mut second = Confirmations::default();
        second.issue_at(observation(), now).unwrap();
        assert!(second.take_at(&id, &request, now).is_err());
        for _ in 0..CAPACITY {
            cache.issue_at(observation(), now).unwrap();
        }
        assert_eq!(cache.pending.len(), CAPACITY);
        assert!(cache.take_at(&id, &request, now).is_err());
        cache.clear();
        assert!(cache.pending.is_empty());
    }

    #[test]
    fn every_bulk_plan_and_selected_agent_must_remain_the_same_before_execution() {
        let mut confirmed = observation();
        confirmed.request = Request::Agents {
            query: "claude".into(),
            force: false,
        };
        confirmed.agents = vec![("agent:40".into(), 4)];
        confirmed.plans.push(plan(3001, 200, 20));
        assert!(validate_fresh(&confirmed, &confirmed).is_ok());
        for index in 0..confirmed.plans.len() {
            let mut changed = confirmed.clone();
            changed.plans[index] = plan(3000 + index as u16, 300, 30);
            assert!(validate_fresh(&confirmed, &changed).is_err());
            let mut reused = confirmed.clone();
            reused.plans[index] = plan(3000 + index as u16, 100 + index as u32 * 100, 99);
            assert!(validate_fresh(&confirmed, &reused).is_err());
        }
        let mut changed = confirmed.clone();
        changed.plans.pop();
        assert!(validate_fresh(&confirmed, &changed).is_err());
        let mut changed = confirmed.clone();
        changed.plans.push(plan(3002, 300, 30));
        assert!(validate_fresh(&confirmed, &changed).is_err());
        let mut changed = confirmed.clone();
        changed.agents.push(("agent:41".into(), 5));
        assert!(validate_fresh(&confirmed, &changed).is_err());
        let mut changed = confirmed.clone();
        changed.agents[0].1 += 1;
        assert!(validate_fresh(&confirmed, &changed).is_err());
    }

    #[test]
    fn unpinned_protected_large_or_non_executable_previews_never_get_handles() {
        let mut cache = Confirmations::default();
        let mut unpinned = observation();
        unpinned.plans[0] = plan(3000, 100, 0);
        assert!(cache.issue(unpinned).is_err());
        let mut protected = observation();
        protected.plans[0].allow_protected = true;
        assert!(cache.issue(protected).is_err());
        let mut broad = observation();
        broad.plans = (0..=MAX_PLANS)
            .map(|index| plan(3000 + index as u16, 100, 10))
            .collect();
        assert!(cache.issue(broad).is_err());
        let mut large = observation();
        large.plans[0].summary = "x".repeat(MAX_PREVIEW_BYTES);
        assert!(cache.issue(large).is_err());
        assert!(cache.pending.is_empty());
    }
}

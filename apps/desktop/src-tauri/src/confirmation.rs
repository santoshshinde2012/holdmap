//! Bind destructive commands to the plan the window actually reviewed.

use holdmap_core::ActionPlan;
use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const LIFETIME: Duration = Duration::from_secs(300);
const CAPACITY: usize = 128;

#[derive(Serialize)]
pub struct Preview {
    #[serde(flatten)]
    pub plan: ActionPlan,
    pub confirmation_id: String,
}

struct Pending {
    id: String,
    at: Instant,
    target: String,
    force: bool,
    allow_protected: bool,
    plan: ActionPlan,
}

#[derive(Default)]
pub struct Confirmations {
    next_id: u64,
    pending: VecDeque<Pending>,
}

impl Confirmations {
    pub fn preview(
        &mut self,
        target: String,
        force: bool,
        allow_protected: bool,
        plan: ActionPlan,
    ) -> Preview {
        self.preview_at(target, force, allow_protected, plan, Instant::now())
    }

    fn preview_at(
        &mut self,
        target: String,
        force: bool,
        allow_protected: bool,
        plan: ActionPlan,
        now: Instant,
    ) -> Preview {
        self.prune(now);
        while self.pending.len() >= CAPACITY {
            self.pending.pop_front();
        }
        self.next_id += 1;
        // This is an in-memory plan handle, not an authentication credential. Native IPC
        // origin restrictions remain the access boundary.
        let id = self.next_id.to_string();
        self.pending.push_back(Pending {
            id: id.clone(),
            at: now,
            target,
            force,
            allow_protected,
            plan: plan.clone(),
        });
        Preview {
            plan,
            confirmation_id: id,
        }
    }

    pub fn take(
        &mut self,
        id: &str,
        target: &str,
        force: bool,
        allow_protected: bool,
    ) -> Result<ActionPlan, String> {
        self.take_at(id, target, force, allow_protected, Instant::now())
    }

    fn take_at(
        &mut self,
        id: &str,
        target: &str,
        force: bool,
        allow_protected: bool,
        now: Instant,
    ) -> Result<ActionPlan, String> {
        self.prune(now);
        let index = self
            .pending
            .iter()
            .position(|p| p.id == id)
            .ok_or("Stop confirmation expired or was already used. Review the plan again.")?;
        let pending = self.pending.remove(index).expect("pending plan index");
        if pending.target != target
            || pending.force != force
            || pending.allow_protected != allow_protected
        {
            return Err("Stop options changed. Review the plan again.".into());
        }
        if let Some(blocked) = &pending.plan.blocked {
            return Err(blocked.message.clone());
        }
        Ok(pending.plan)
    }

    fn prune(&mut self, now: Instant) {
        self.pending
            .retain(|p| now.saturating_duration_since(p.at) < LIFETIME);
    }
}

pub fn validate_fresh(confirmed: &ActionPlan, current: &ActionPlan) -> Result<(), String> {
    if !confirmed.same_effects(current) {
        return Err("The target changed since confirmation. Review a new stop plan.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_core::{Owner, ProcRef, Risk, Step};

    fn fixture(pid: u32, start_token: u64) -> ActionPlan {
        ActionPlan {
            target: "3000".into(),
            owners: vec![Owner::Process {
                pid,
                name: "node".into(),
            }],
            summary: "Stop the server".into(),
            steps: vec![Step::SignalProcesses {
                processes: vec![ProcRef {
                    pid,
                    name: "node".into(),
                    start_token,
                    command: "node server.js".into(),
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
    fn changed_port_owner_and_pid_reuse_never_authorize_new_effects() {
        let original = fixture(100, 10);
        // Replanning :3000 alone would choose this new owner, although the dialog showed 100.
        assert!(validate_fresh(&original, &fixture(200, 20)).is_err());
        assert!(validate_fresh(&original, &fixture(100, 11)).is_err());
        assert!(validate_fresh(&original, &original).is_ok());
    }

    #[test]
    fn handles_bind_options_expire_and_cannot_be_replayed() {
        let now = Instant::now();
        let mut plans = Confirmations::default();
        let issue = |plans: &mut Confirmations| {
            plans
                .preview_at("3000".into(), false, false, fixture(100, 10), now)
                .confirmation_id
        };
        let id = issue(&mut plans);
        assert!(plans.take_at(&id, "3000", true, false, now).is_err());
        assert!(plans.take_at(&id, "3000", false, false, now).is_err());
        let id = issue(&mut plans);
        assert!(plans.take_at(&id, "4000", false, false, now).is_err());
        let id = issue(&mut plans);
        assert!(plans.take_at(&id, "3000", false, true, now).is_err());
        let id = issue(&mut plans);
        assert!(plans
            .take_at(&id, "3000", false, false, now + LIFETIME)
            .is_err());
        let id = issue(&mut plans);
        assert!(plans.take_at(&id, "3000", false, false, now).is_ok());
        assert!(plans.take_at(&id, "3000", false, false, now).is_err());
        assert!(plans.take_at("unknown", "3000", false, false, now).is_err());
    }

    #[test]
    fn abandoned_previews_are_bounded() {
        let now = Instant::now();
        let mut plans = Confirmations::default();
        let first = plans.preview_at("3000".into(), false, false, fixture(100, 10), now);
        for _ in 0..CAPACITY {
            plans.preview_at("3000".into(), false, false, fixture(100, 10), now);
        }
        assert_eq!(plans.pending.len(), CAPACITY);
        assert!(plans
            .take_at(&first.confirmation_id, "3000", false, false, now)
            .is_err());
    }
}

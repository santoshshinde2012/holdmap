//! "Stop every dev server": one combined, previewable plan over the dev servers the current user
//! owns. Protected processes, containers and other users' processes are never included.

use super::resolution::{blocked_plan, Resolution};
use super::types::{ActionPlan, BlockKind, Risk, StopOptions};
use super::Engine;
use crate::model::PortEntry;
use std::collections::BTreeSet;

impl Engine {
    /// The listeners [`Target::AllDev`](super::Target::AllDev) would stop: dev servers you own
    /// that aren't protected or containers.
    pub fn dev_servers(&self) -> Vec<&PortEntry> {
        self.scan
            .snapshot
            .entries
            .iter()
            .filter(|e| {
                e.state.is_listening()
                    && e.is_dev
                    && e.is_mine
                    && !e.protected
                    && e.container.is_none()
                    && e.process.is_some()
            })
            .collect()
    }

    pub(crate) fn plan_all_dev(&self, opts: &StopOptions) -> ActionPlan {
        let target = "all dev servers".to_string();
        let mut resolved_roots = BTreeSet::new();
        let mut blocked_roots = BTreeSet::new();
        let mut resolutions: Vec<Resolution> = Vec::new();
        let mut entries = Vec::new();
        let mut labels = Vec::new();
        let mut skipped = Vec::new();
        for e in self.dev_servers() {
            let Some(pid) = e.pid else { continue };
            let root = self.tree_root(pid);
            if resolved_roots.contains(&root) {
                entries.push(e.clone()); // another port of a tree already in the plan
                continue;
            }
            if blocked_roots.contains(&root) {
                continue;
            }
            let r = self.resolve_entry(e, opts);
            match &r.blocked {
                Some(b) => {
                    blocked_roots.insert(root);
                    skipped.push(format!("Skipping :{} ({}): {}", e.port, e.label, b.message));
                }
                None => {
                    resolved_roots.insert(root);
                    if !resolutions.iter().any(|x| x.owner == r.owner) {
                        resolutions.push(r);
                    }
                    labels.push(format!(":{} {}", e.port, e.label));
                    entries.push(e.clone());
                }
            }
        }
        if resolutions.is_empty() {
            let mut plan = blocked_plan(
                target,
                vec![],
                BlockKind::NothingToStop,
                if skipped.is_empty() {
                    "No dev servers of yours are running.".to_string()
                } else {
                    "None of your dev servers can be stopped safely.".to_string()
                },
            );
            plan.warnings = skipped;
            return plan;
        }
        let mut plan = self.combine(target, &resolutions, &entries, opts);
        plan.summary = format!(
            "Stop {} dev server{}: {}.",
            labels.len(),
            if labels.len() == 1 { "" } else { "s" },
            labels.join(", ")
        );
        if labels.len() > 1 {
            plan.risk = plan.risk.max(Risk::Medium);
        }
        skipped.append(&mut plan.warnings);
        plan.warnings = skipped;
        plan
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::{Engine, Step, StopOptions, Target};

    #[test]
    fn one_plan_for_every_dev_server() {
        let tmp = tempfile::tempdir().unwrap();
        let e = Engine::from_scan(crate::topology::tests::acme(tmp.path()));
        assert_eq!(Target::parse("dev:all"), Target::AllDev);
        assert_eq!(Target::AllDev.to_string(), "all dev servers");
        let plan = e.plan(&Target::AllDev, &StopOptions::default());
        assert!(!plan.is_blocked(), "{plan:?}");
        assert!(
            plan.summary.contains(":3000") && plan.summary.contains(":8080"),
            "{}",
            plan.summary
        );
        let pids: Vec<u32> = plan
            .steps
            .iter()
            .flat_map(|s| match s {
                Step::SignalProcesses { processes, .. } => {
                    processes.iter().map(|p| p.pid).collect()
                }
                _ => vec![],
            })
            .collect();
        assert!(
            pids.contains(&20) && pids.contains(&21) && pids.contains(&30),
            "{pids:?}"
        );
        assert!(!pids.contains(&99), "never portwise itself");
        assert!(!pids.contains(&10), "never the shell");
        // Every dev server listed is one the user owns and isn't protected.
        assert!(e.dev_servers().iter().all(|x| x.is_mine && !x.protected));
    }
}

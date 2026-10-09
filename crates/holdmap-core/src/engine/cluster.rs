//! Topology-aware planning: stop a whole cluster in dependency order, and warn when stopping a
//! service that others depend on.

use super::resolution::{blocked_plan, Resolution};
use super::types::{ActionPlan, BlockKind, Risk, StopOptions};
use super::Engine;
use crate::model::PortEntry;
use crate::topology::{stop_order, Graph, Node, NodeKind, TopologyBuilder};
use std::collections::HashSet;

impl Engine {
    /// The service graph of the scanned machine.
    pub fn topology(&self) -> Graph {
        TopologyBuilder::new(&self.scan)
            .with_policy(self.policy())
            .build()
    }

    fn node_entries(&self, n: &Node) -> Vec<PortEntry> {
        let ids: HashSet<&str> = n.ports.iter().map(|p| p.entry_id.as_str()).collect();
        self.scan
            .snapshot
            .entries
            .iter()
            .filter(|e| ids.contains(e.id.as_str()))
            .cloned()
            .collect()
    }

    fn resolve_node(&self, n: &Node, opts: &StopOptions) -> Option<(Resolution, Vec<PortEntry>)> {
        let entries = self.node_entries(n);
        match entries.first() {
            Some(first) => Some((self.resolve_entry(first, opts), entries)),
            None => n.root_pid.map(|pid| (self.resolve_pid(pid, opts), entries)),
        }
    }

    /// One combined plan that stops every service in a cluster, dependents first
    /// (web → api → db), skipping members that can't be stopped.
    pub(crate) fn plan_cluster(&self, query: &str, opts: &StopOptions) -> ActionPlan {
        let g = self.topology();
        let target = format!("cluster {query}");
        let Some(cluster) = g.find_cluster(query) else {
            let known: Vec<&str> = g.clusters.iter().map(|c| c.name.as_str()).collect();
            return blocked_plan(
                target,
                vec![],
                BlockKind::NothingToStop,
                if known.is_empty() {
                    format!("No cluster named \"{query}\"; no clusters were detected.")
                } else {
                    format!(
                        "No cluster named \"{query}\". Known clusters: {}.",
                        known.join(", ")
                    )
                },
            );
        };
        let order = stop_order(&g, &cluster.nodes);
        let mut resolutions = Vec::new();
        let mut entries = Vec::new();
        let mut skipped = Vec::new();
        let mut first_block = None;
        let mut stopped: Vec<&Node> = Vec::new();
        for id in &order.order {
            let Some(n) = g.node(id).filter(|n| n.kind != NodeKind::External) else {
                continue;
            };
            let Some((r, es)) = self.resolve_node(n, opts) else {
                continue;
            };
            match &r.blocked {
                Some(b) => {
                    skipped.push(format!("Skipping {}: {}", n.display(), b.message));
                    first_block.get_or_insert_with(|| b.clone());
                }
                None => {
                    if !resolutions.iter().any(|x: &Resolution| x.owner == r.owner) {
                        resolutions.push(r);
                    }
                    entries.extend(es);
                    stopped.push(n);
                }
            }
        }
        if resolutions.is_empty() {
            let b = first_block.unwrap_or(crate::engine::Blocked {
                kind: BlockKind::NothingToStop,
                message: format!("Nothing in cluster {} can be stopped.", cluster.name),
                overridable: false,
            });
            let mut plan = blocked_plan(target, vec![], b.kind, b.message);
            plan.blocked = plan.blocked.map(|mut x| {
                x.overridable = b.overridable;
                x
            });
            plan.warnings = skipped;
            return plan;
        }
        let mut plan = self.combine(target, &resolutions, &entries, opts);
        let seq: Vec<String> = stopped.iter().map(|n| n.display()).collect();
        plan.summary = format!(
            "Stop cluster {} ({} service{}) in dependency order: {}.",
            cluster.name,
            stopped.len(),
            if stopped.len() == 1 { "" } else { "s" },
            seq.join(" → ")
        );
        let mut warnings = skipped;
        if !order.cyclic.is_empty() {
            let names: Vec<String> = order
                .cyclic
                .iter()
                .filter_map(|id| g.node(id))
                .map(|n| n.label.clone())
                .collect();
            warnings.push(format!(
                "Circular dependency involving {}; that part of the order is arbitrary.",
                names.join(", ")
            ));
        }
        let ids: Vec<&str> = stopped.iter().map(|n| n.id.as_str()).collect();
        warnings.extend(self.outside_dependents(&g, &ids));
        warnings.append(&mut plan.warnings);
        plan.warnings = warnings;
        if stopped.len() > 1 {
            plan.risk = plan.risk.max(Risk::Medium);
        }
        plan
    }

    /// "X depends on Y" warnings for services outside the set being stopped.
    fn outside_dependents(&self, g: &Graph, stopping: &[&str]) -> Vec<String> {
        let set: HashSet<&str> = stopping.iter().copied().collect();
        let mut out = Vec::new();
        for id in stopping {
            let Some(n) = g.node(id) else { continue };
            let deps: Vec<String> = g
                .dependents(id)
                .into_iter()
                .filter(|d| !set.contains(d.id.as_str()))
                .map(|d| match d.root_pid {
                    Some(pid) => format!("{} (PID {pid})", d.label),
                    None => d.label.clone(),
                })
                .collect();
            if !deps.is_empty() {
                out.push(format!(
                    "{} depend{} on {}: {} will lose {} connection.",
                    deps.join(", "),
                    if deps.len() == 1 { "s" } else { "" },
                    n.display(),
                    if deps.len() == 1 { "it" } else { "they" },
                    if deps.len() == 1 { "its" } else { "their" },
                ));
            }
        }
        out
    }

    /// Warnings for a port stop: who is connected to the services holding `entries`.
    pub(crate) fn dependent_warnings(&self, g: &Graph, entries: &[PortEntry]) -> Vec<String> {
        let mut ids: Vec<&str> = Vec::new();
        for e in entries {
            if let Some(n) = g.node_for_entry(&e.id) {
                if !ids.contains(&n.id.as_str()) {
                    ids.push(n.id.as_str());
                }
            }
        }
        self.outside_dependents(g, &ids)
    }
}

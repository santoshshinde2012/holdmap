//! Processes an agent started that kept running after the agent exited or detached them.
//!
//! Attribution needs memory across scans: once a process is re-parented to PID 1 (or a
//! subreaper) its ancestry no longer names the agent. A [`LeftoverLedger`] remembers, for the
//! top process of each long-lived subtree an agent started, which agent started it. When that
//! agent's root process is gone, or the subtree left its tree, the entry becomes a leftover
//! until the process itself exits.
//!
//! The ledger keeps only identity metadata (PIDs, start tokens, process and agent names and
//! timestamps): no command lines, folders or environment. It is bounded and stored with the
//! other private state (see [`crate::store::Store::update_leftovers`]). Cleanup is never
//! automatic: callers plan a reviewed `pid:<n>` stop through the usual engine and protection.

use super::{catalog, ownership, AgentDetector, CatalogDetector};
use crate::model::{Exposure, PortEntry, Protocol};
use crate::process::ProcessTable;
use crate::scan::Scan;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Entries kept at most; the oldest still-attributed entries are dropped first.
pub const MAX_ENTRIES: usize = 256;
/// A child must have run this long to be remembered, so short tool commands never churn
/// the ledger.
pub const MIN_AGE_SECS: u64 = 30;

/// One process identity: PID plus the platform start token that guards against PID reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProcessKey {
    /// Process ID.
    pub pid: u32,
    /// Start token (see [`crate::ProcessInfo::start_token`]).
    pub start_token: u64,
}

/// Why an attributed process counts as left behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeftReason {
    /// The agent's root process exited.
    AgentExited,
    /// The agent is still running, but the process left its process tree.
    Detached,
}

/// What the ledger remembers about one process an agent started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribution {
    /// The process.
    pub process: ProcessKey,
    /// Its name when last attributed.
    pub name: String,
    /// The starting agent's catalog product id, e.g. `claude-code`.
    pub agent_product: String,
    /// The agent's display name.
    pub agent_name: String,
    /// The agent's root process.
    pub agent: ProcessKey,
    /// Unix milliseconds when the process was last seen under the agent.
    pub seen_ms: u64,
    /// Unix milliseconds when it was first seen without the agent; `None` while attributed.
    pub left_ms: Option<u64>,
    /// Set together with `left_ms`.
    pub reason: Option<LeftReason>,
}

/// Bounded memory of which agent started which long-lived process.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeftoverLedger {
    /// Format version.
    #[serde(default = "version")]
    pub version: u32,
    /// Attributed and left-behind processes.
    #[serde(default)]
    pub entries: Vec<Attribution>,
}

fn version() -> u32 {
    1
}

impl LeftoverLedger {
    /// Update from one scan with the default catalog. Returns whether anything changed.
    pub fn observe(&mut self, scan: &Scan, now_ms: u64) -> bool {
        self.observe_with(&scan.table, &CatalogDetector, now_ms)
    }

    /// [`Self::observe`] with a custom detector.
    pub fn observe_with(
        &mut self,
        table: &ProcessTable,
        detector: &dyn AgentDetector,
        now_ms: u64,
    ) -> bool {
        let before = self.clone();
        self.version = version();
        let alive = |key: &ProcessKey| {
            table
                .get(key.pid)
                .is_some_and(|p| p.start_token == key.start_token)
        };
        // A process that exited (or whose PID was reused) is forgotten.
        self.entries.retain(|e| alive(&e.process));
        let current = attributions(table, detector, now_ms);
        let mut index: HashMap<ProcessKey, usize> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.process, i))
            .collect();
        let mut refreshed = HashSet::new();
        for a in current {
            refreshed.insert(a.process);
            match index.get(&a.process) {
                Some(&i) => {
                    // Seen under an agent again (for example after a restart re-adopted it).
                    let e = &mut self.entries[i];
                    let seen = if e.agent == a.agent && e.left_ms.is_none() {
                        e.seen_ms
                    } else {
                        a.seen_ms
                    };
                    *e = Attribution { seen_ms: seen, ..a };
                }
                None => {
                    index.insert(a.process, self.entries.len());
                    self.entries.push(a);
                }
            }
        }
        for e in &mut self.entries {
            if refreshed.contains(&e.process) || e.left_ms.is_some() {
                continue;
            }
            e.left_ms = Some(now_ms);
            e.reason = Some(if alive(&e.agent) {
                LeftReason::Detached
            } else {
                LeftReason::AgentExited
            });
        }
        self.trim();
        // Timestamps of still-attributed entries are kept from first sight, so an unchanged
        // machine produces an unchanged ledger and no write.
        *self != before
    }

    /// Left-behind entries whose process is still the same one.
    pub fn left<'a>(&'a self, table: &'a ProcessTable) -> impl Iterator<Item = &'a Attribution> {
        self.entries.iter().filter(move |e| {
            e.left_ms.is_some()
                && table
                    .get(e.process.pid)
                    .is_some_and(|p| p.start_token == e.process.start_token)
        })
    }

    fn trim(&mut self) {
        if self.entries.len() <= MAX_ENTRIES {
            return;
        }
        // Leftovers are what users act on; drop the oldest attributed entries first.
        self.entries
            .sort_by_key(|e| (e.left_ms.is_some(), e.left_ms.unwrap_or(e.seen_ms)));
        let extra = self.entries.len() - MAX_ENTRIES;
        self.entries.drain(..extra);
    }
}

/// The top process of every long-lived subtree an agent started, with its agent.
fn attributions(
    table: &ProcessTable,
    detector: &dyn AgentDetector,
    now_ms: u64,
) -> Vec<Attribution> {
    let (roots, owner) = ownership::assign(table, detector);
    let products: HashMap<u32, &catalog::AgentProduct> = roots.iter().copied().collect();
    let now_secs = now_ms / 1000;
    let mut out = Vec::new();
    for p in table.iter() {
        let Some(root_pid) = owner.get(&p.pid).copied() else {
            continue;
        };
        let (Some(product), Some(root)) = (products.get(&root_pid), table.get(root_pid)) else {
            continue;
        };
        let same_product = |pid: u32| {
            table
                .get(pid)
                .and_then(|x| detector.identify(x))
                .is_some_and(|x| x.id == product.id)
        };
        // Only the first process outside the product: its parent is the agent or a helper.
        if p.pid == root_pid || same_product(p.pid) || !p.ppid.is_some_and(same_product) {
            continue;
        }
        if p.start_time == 0 || now_secs.saturating_sub(p.start_time) < MIN_AGE_SECS {
            continue;
        }
        out.push(Attribution {
            process: ProcessKey {
                pid: p.pid,
                start_token: p.start_token,
            },
            name: p.name.clone(),
            agent_product: product.id.to_string(),
            agent_name: product.name.to_string(),
            agent: ProcessKey {
                pid: root.pid,
                start_token: root.start_token,
            },
            seen_ms: now_ms,
            left_ms: None,
            reason: None,
        });
    }
    out.sort_by_key(|a| a.process);
    out
}

/// A listener held by a leftover process or its descendants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeftoverPort {
    /// Port number.
    pub port: u16,
    /// Transport.
    pub protocol: Protocol,
    /// Where it can be reached from.
    pub exposure: Exposure,
    /// The port entry id, for opening details.
    pub entry_id: String,
}

/// A left-behind process as shown to users, enriched from the current scan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Leftover {
    /// `leftover:<pid>:<start token>`.
    pub id: String,
    /// Process ID; clean up with a reviewed `pid:<n>` stop.
    pub pid: u32,
    /// Start token, to bind a review to this exact process.
    pub start_token: u64,
    /// Current process name.
    pub name: String,
    /// Command line with secrets hidden.
    pub command: String,
    /// Working directory, when readable.
    pub cwd: Option<String>,
    /// Parent PID now (1 when re-parented to init).
    pub ppid: Option<u32>,
    /// The agent that started it.
    pub agent_name: String,
    /// The agent's catalog product id.
    pub agent_product: String,
    /// The agent's root PID at the time.
    pub agent_pid: u32,
    /// Why it counts as left behind.
    pub reason: LeftReason,
    /// Unix milliseconds when holdmap first saw it without the agent.
    pub left_ms: u64,
    /// Listeners held by it or its descendants.
    pub ports: Vec<LeftoverPort>,
    /// Processes in its subtree, itself included.
    pub processes: usize,
    /// Resident memory of the subtree in bytes.
    pub memory_bytes: u64,
}

/// The current user's leftovers, newest first. Processes of other accounts are omitted.
pub fn leftovers(ledger: &LeftoverLedger, scan: &Scan) -> Vec<Leftover> {
    let t = &scan.table;
    let mut out: Vec<Leftover> = ledger
        .left(t)
        .filter(|e| t.is_mine(e.process.pid) || owned_by_current_uid(t, e.process.pid))
        .filter_map(|e| {
            let p = t.get(e.process.pid)?;
            let tree: Vec<u32> = std::iter::once(p.pid).chain(t.descendants(p.pid)).collect();
            let members: HashSet<u32> = tree.iter().copied().collect();
            let mut ports: Vec<LeftoverPort> = scan
                .snapshot
                .entries
                .iter()
                .filter(|en| holds(en, &members))
                .map(|en| LeftoverPort {
                    port: en.port,
                    protocol: en.protocol,
                    exposure: en.exposure,
                    entry_id: en.id.clone(),
                })
                .collect();
            ports.sort_by_key(|x| (x.port, x.protocol as u8));
            ports.dedup_by_key(|x| (x.port, x.protocol as u8));
            Some(Leftover {
                id: format!("leftover:{}:{}", p.pid, p.start_token),
                pid: p.pid,
                start_token: p.start_token,
                name: p.name.clone(),
                command: p.command(),
                cwd: p.cwd.as_ref().map(|c| c.display().to_string()),
                ppid: p.ppid,
                agent_name: e.agent_name.clone(),
                agent_product: e.agent_product.clone(),
                agent_pid: e.agent.pid,
                reason: e.reason.unwrap_or(LeftReason::AgentExited),
                left_ms: e.left_ms.unwrap_or_default(),
                ports,
                processes: tree.len(),
                memory_bytes: tree
                    .iter()
                    .filter_map(|pid| t.get(*pid))
                    .map(|x| x.memory_bytes)
                    .sum(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.left_ms.cmp(&a.left_ms).then(a.pid.cmp(&b.pid)));
    out
}

fn owned_by_current_uid(t: &ProcessTable, pid: u32) -> bool {
    matches!((t.get(pid).and_then(|p| p.uid), t.current_uid()), (Some(a), Some(b)) if a == b)
}

fn holds(entry: &PortEntry, members: &HashSet<u32>) -> bool {
    entry.pid.is_some_and(|p| members.contains(&p))
        || entry.pids.iter().any(|p| members.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::{proc, table};
    use crate::ProcessInfo;

    const NOW: u64 = 1_700_000_000_000; // ms; fixtures start at 1_700_000_000 s

    fn old(mut p: ProcessInfo) -> ProcessInfo {
        p.start_time = 1_700_000_000 - 120;
        p
    }

    fn claude() -> ProcessInfo {
        old(proc(100, 1, "claude", &["claude"]))
    }

    fn server(ppid: u32) -> ProcessInfo {
        old(proc(200, ppid, "node", &["node", "server.js"]))
    }

    #[test]
    fn remembers_a_long_lived_child_and_reports_it_after_the_agent_exits() {
        let mut ledger = LeftoverLedger::default();
        let running = table(vec![claude(), server(100)], 999);
        assert!(ledger.observe_with(&running, &CatalogDetector, NOW));
        assert_eq!(ledger.entries.len(), 1);
        assert_eq!(ledger.entries[0].agent_name, "Claude Code");
        assert_eq!(ledger.left(&running).count(), 0);
        // An unchanged machine writes nothing.
        assert!(!ledger.observe_with(&running, &CatalogDetector, NOW + 3000));

        let orphaned = table(vec![server(1)], 999);
        assert!(ledger.observe_with(&orphaned, &CatalogDetector, NOW + 60_000));
        let left: Vec<_> = ledger.left(&orphaned).collect();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].left_ms, Some(NOW + 60_000));
        assert_eq!(left[0].reason, Some(LeftReason::AgentExited));
    }

    #[test]
    fn a_detached_child_of_a_running_agent_is_reported_as_detached() {
        let mut ledger = LeftoverLedger::default();
        ledger.observe_with(
            &table(vec![claude(), server(100)], 999),
            &CatalogDetector,
            NOW,
        );
        let detached = table(vec![claude(), server(1)], 999);
        ledger.observe_with(&detached, &CatalogDetector, NOW + 5000);
        assert_eq!(ledger.entries[0].reason, Some(LeftReason::Detached));
    }

    #[test]
    fn short_lived_commands_and_reused_pids_are_never_attributed() {
        let mut ledger = LeftoverLedger::default();
        let young = proc(200, 100, "bash", &["bash", "-c", "ls"]); // started "now"
        ledger.observe_with(&table(vec![claude(), young], 999), &CatalogDetector, NOW);
        assert!(ledger.entries.is_empty());

        ledger.observe_with(
            &table(vec![claude(), server(100)], 999),
            &CatalogDetector,
            NOW,
        );
        let mut reused = server(1);
        reused.start_token += 1;
        ledger.observe_with(&table(vec![reused], 999), &CatalogDetector, NOW + 1000);
        assert!(
            ledger.entries.is_empty(),
            "a new process on the same PID is not a leftover"
        );
    }

    #[test]
    fn only_the_top_of_each_started_subtree_is_tracked() {
        let mut ledger = LeftoverLedger::default();
        let grandchild = old(proc(300, 200, "node", &["node", "worker.js"]));
        ledger.observe_with(
            &table(vec![claude(), server(100), grandchild], 999),
            &CatalogDetector,
            NOW,
        );
        assert_eq!(
            ledger
                .entries
                .iter()
                .map(|e| e.process.pid)
                .collect::<Vec<_>>(),
            vec![200]
        );
    }

    #[test]
    fn stays_bounded_and_keeps_leftovers_over_attributed_entries() {
        let mut ledger = LeftoverLedger::default();
        for i in 0..(MAX_ENTRIES as u32 + 10) {
            ledger.entries.push(Attribution {
                process: ProcessKey {
                    pid: 10_000 + i,
                    start_token: 1,
                },
                name: "x".into(),
                agent_product: "claude-code".into(),
                agent_name: "Claude Code".into(),
                agent: ProcessKey {
                    pid: 100,
                    start_token: 1,
                },
                seen_ms: i as u64,
                left_ms: (i == 0).then_some(5),
                reason: (i == 0).then_some(LeftReason::AgentExited),
            });
        }
        ledger.trim();
        assert_eq!(ledger.entries.len(), MAX_ENTRIES);
        assert!(ledger.entries.iter().any(|e| e.left_ms.is_some()));
    }

    #[test]
    fn leftovers_report_ports_memory_and_only_the_current_users_processes() {
        use crate::model::Snapshot;
        let mut ledger = LeftoverLedger::default();
        ledger.observe_with(
            &table(vec![claude(), server(100)], 999),
            &CatalogDetector,
            NOW,
        );
        let mut foreign = old(proc(400, 1, "node", &["node"]));
        foreign.user = Some("someone".into());
        foreign.uid = Some(2000);
        let me = proc(999, 1, "holdmap", &["holdmap"]);
        let t = table(vec![server(1), foreign, me], 999);
        ledger.observe_with(&t, &CatalogDetector, NOW + 1000);
        let mut entry: PortEntry = serde_json::from_value(serde_json::json!({
            "id": "tcp:3000", "port": 3000, "protocol": "tcp", "state": "listen", "addresses": ["0.0.0.0"],
            "families": ["v4"], "remote": null, "exposure": "all_interfaces", "pid": 200, "pids": [200],
            "uid": 1000, "user": "dev", "process": null, "project": null, "framework": null, "container": null,
            "label": "node", "is_dev": true, "is_mine": true, "protected": false
        }))
        .unwrap();
        entry.pid = Some(200);
        let scan = Scan {
            snapshot: Snapshot {
                entries: vec![entry],
                hidden_sockets: 0,
                platform: "test".into(),
                taken_at_ms: NOW,
                scan_ms: 0,
                docker_available: false,
                warnings: vec![],
            },
            table: t,
            published: vec![],
            raw: vec![],
        };
        let list = leftovers(&ledger, &scan);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].pid, 200);
        assert_eq!(list[0].agent_name, "Claude Code");
        assert_eq!(list[0].ports[0].port, 3000);
        assert_eq!(list[0].ppid, Some(1));
        assert_eq!(list[0].command, "node server.js");
    }
}

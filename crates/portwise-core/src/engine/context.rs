//! [`ResolveCtx`]: the read-only world a stop strategy reasons about (scan + policy + options),
//! plus process-tree helpers shared by several strategies.

use super::types::{ProcRef, Step, StopOptions};
use crate::model::{PortEntry, ProcessInfo};
use crate::process::ProcessTable;
use crate::safety::{Protection, ProtectionPolicy};
use crate::scan::Scan;

pub struct ResolveCtx<'a> {
    pub scan: &'a Scan,
    pub policy: &'a dyn ProtectionPolicy,
    pub opts: &'a StopOptions,
}

impl<'a> ResolveCtx<'a> {
    pub fn new(scan: &'a Scan, policy: &'a dyn ProtectionPolicy, opts: &'a StopOptions) -> Self {
        Self { scan, policy, opts }
    }

    pub fn table(&self) -> &'a ProcessTable {
        &self.scan.table
    }

    pub fn protection(&self, p: &ProcessInfo) -> Protection {
        self.policy.protection(p, self.table())
    }

    pub fn proc_ref(&self, pid: u32) -> Option<ProcRef> {
        self.table().get(pid).map(|p| ProcRef {
            pid,
            name: p.name.clone(),
            start_token: p.start_token,
            command: p.command(),
        })
    }

    pub fn signal_step(&self, pids: &[u32]) -> Step {
        Step::SignalProcesses {
            processes: pids.iter().filter_map(|p| self.proc_ref(*p)).collect(),
            force: self.opts.force,
            timeout_ms: self.opts.timeout_ms,
        }
    }

    /// Descendants that are safe to stop along with their parent.
    pub fn safe_descendants(&self, pid: u32) -> Vec<u32> {
        let t = self.table();
        t.descendants(pid)
            .into_iter()
            .filter(|d| {
                t.get(*d).is_some_and(|p| match self.protection(p) {
                    Protection::None => true,
                    Protection::Soft(_) => self.opts.allow_protected,
                    Protection::Hard(_) => false,
                })
            })
            .collect()
    }

    /// Climb from the socket holder to the dev-session root (e.g. node → sh -c → npm).
    pub fn tree_root(&self, pid: u32) -> u32 {
        let t = self.table();
        let mut root = pid;
        let mut child = t.get(pid);
        for anc in t.ancestors(pid) {
            let Some(a) = t.get(anc) else { break };
            if t.is_self_or_ancestor(anc)
                || self.protection(a).is_protected()
                || (t.is_mine(pid) && !t.is_mine(anc))
            {
                break;
            }
            let reloader = child.is_some_and(|c| !c.cmdline.is_empty() && c.cmdline == a.cmdline);
            if !(super::strategies::is_launcher(a) || reloader) {
                break;
            }
            root = anc;
            child = Some(a);
        }
        root
    }

    /// Every listening entry held by any of `pids`.
    pub fn entries_held_by(&self, pids: &[u32]) -> Vec<PortEntry> {
        self.scan
            .snapshot
            .entries
            .iter()
            .filter(|e| e.state.is_listening() && e.pids.iter().any(|p| pids.contains(p)))
            .cloned()
            .collect()
    }
}

//! Process table: metadata for every visible process plus parent/child relationships.

use crate::model::ProcessInfo;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};

/// A point-in-time view of all processes.
#[derive(Debug, Clone, Default)]
pub struct ProcessTable {
    procs: HashMap<u32, ProcessInfo>,
    children: HashMap<u32, Vec<u32>>,
    self_pid: u32,
    self_user: Option<String>,
    self_ancestors: HashSet<u32>,
    self_descendants: HashSet<u32>,
    /// Process → the user session (interactive shell, terminal, IDE, agent) it hosts.
    session_hosts: HashMap<u32, (u32, crate::safety::SessionKind)>,
    user_names: HashMap<u32, String>,
    self_uid: Option<u32>,
}

fn refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cmd(UpdateKind::Always)
        .with_exe(UpdateKind::Always)
        .with_cwd(UpdateKind::Always)
        .with_user(UpdateKind::Always)
        .with_memory()
        .with_cpu()
}

impl ProcessTable {
    /// Capture all processes visible to the current user.
    pub fn capture() -> Self {
        Self::capture_with(&mut System::new())
    }

    /// Capture using a long-lived [`System`], so CPU usage is measured since the previous call.
    pub fn capture_with(sys: &mut System) -> Self {
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        let users = Users::new_with_refreshed_list();
        let procs = sys
            .processes()
            .values()
            .filter(|p| p.thread_kind().is_none()) // skip Linux threads
            .map(|p| {
                let pid = p.pid().as_u32();
                let user = p
                    .user_id()
                    .and_then(|u| users.get_user_by_id(u))
                    .map(|u| u.name().to_string());
                #[cfg(unix)]
                let uid = p.user_id().map(|u| **u);
                #[cfg(not(unix))]
                let uid = None;
                let start_time = p.start_time();
                let start_token = crate::sys::start_token(pid).unwrap_or(start_time);
                ProcessInfo {
                    pid,
                    ppid: p.parent().map(|pp| pp.as_u32()),
                    name: p.name().to_string_lossy().into_owned(),
                    exe: p.exe().map(PathBuf::from),
                    cmdline: p
                        .cmd()
                        .iter()
                        .map(|s| s.to_string_lossy().into_owned())
                        .collect(),
                    // Linux appends " (deleted)" to a removed working directory.
                    cwd: p.cwd().map(|c| {
                        let s = c.to_string_lossy();
                        PathBuf::from(s.strip_suffix(" (deleted)").unwrap_or(&s))
                    }),
                    uid,
                    user,
                    start_time,
                    start_token,
                    memory_bytes: p.memory(),
                    cpu_percent: p.cpu_usage(),
                }
            })
            .map(|p| (p.pid, p))
            .collect();
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut t = Self::from_processes(procs, std::process::id());
        #[cfg(unix)]
        {
            t.user_names = users
                .list()
                .iter()
                .map(|u| (**u.id(), u.name().to_string()))
                .collect();
        }
        t
    }

    /// Build a table from known processes (used by tests and fixtures).
    pub fn from_processes(procs: HashMap<u32, ProcessInfo>, self_pid: u32) -> Self {
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for p in procs.values() {
            if let Some(pp) = p.ppid {
                if pp != p.pid {
                    children.entry(pp).or_default().push(p.pid);
                }
            }
        }
        for c in children.values_mut() {
            c.sort_unstable();
        }
        let self_user = procs.get(&self_pid).and_then(|p| p.user.clone());
        let mut t = ProcessTable {
            procs,
            children,
            self_pid,
            self_user,
            self_ancestors: HashSet::new(),
            self_descendants: HashSet::new(),
            session_hosts: HashMap::new(),
            user_names: HashMap::new(),
            self_uid: None,
        };
        t.self_uid = t.procs.get(&self_pid).and_then(|p| p.uid);
        t.self_ancestors = t.ancestors(self_pid).into_iter().collect();
        t.self_descendants = t.descendants(self_pid).into_iter().collect();
        let mut pids: Vec<u32> = t.procs.keys().copied().collect();
        pids.sort_unstable();
        for pid in pids {
            let Some(kind) = crate::safety::session_kind(&t.procs[&pid]) else {
                continue;
            };
            for a in t.ancestors(pid) {
                if a > 1 {
                    t.session_hosts.entry(a).or_insert((pid, kind));
                }
            }
        }
        t
    }

    /// True if `pid` was started (directly or indirectly) by this process.
    pub fn is_self_descendant(&self, pid: u32) -> bool {
        self.self_descendants.contains(&pid)
    }

    /// The user session (shell, terminal, IDE or agent) that `pid` is an ancestor of, if any.
    pub fn hosted_session(&self, pid: u32) -> Option<(&ProcessInfo, crate::safety::SessionKind)> {
        let (s, k) = self.session_hosts.get(&pid)?;
        Some((self.get(*s)?, *k))
    }

    /// Look up a process.
    pub fn get(&self, pid: u32) -> Option<&ProcessInfo> {
        self.procs.get(&pid)
    }

    /// Number of processes.
    pub fn len(&self) -> usize {
        self.procs.len()
    }

    /// True when the table is empty.
    pub fn is_empty(&self) -> bool {
        self.procs.is_empty()
    }

    /// PID of the current (portwise) process.
    pub fn self_pid(&self) -> u32 {
        self.self_pid
    }

    /// True if `pid` is this process or one of its ancestors (our terminal, shell, IDE…).
    pub fn is_self_or_ancestor(&self, pid: u32) -> bool {
        pid == self.self_pid || self.self_ancestors.contains(&pid)
    }

    /// True if the process runs as the same user as portwise.
    pub fn is_mine(&self, pid: u32) -> bool {
        match (self.get(pid).and_then(|p| p.user.as_ref()), &self.self_user) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// Uid of the current user (Unix).
    pub fn current_uid(&self) -> Option<u32> {
        self.self_uid
    }

    /// Resolve a numeric uid to a user name (Unix).
    pub fn user_name(&self, uid: u32) -> Option<String> {
        self.user_names.get(&uid).cloned()
    }

    /// User name of the current user.
    pub fn current_user(&self) -> Option<&str> {
        self.self_user.as_deref()
    }

    /// Parent chain from the immediate parent upwards (cycle-safe).
    pub fn ancestors(&self, pid: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut seen = HashSet::from([pid]);
        let mut cur = self.get(pid).and_then(|p| p.ppid);
        while let Some(pp) = cur {
            if pp == 0 || !seen.insert(pp) {
                break;
            }
            out.push(pp);
            cur = self.get(pp).and_then(|p| p.ppid);
        }
        out
    }

    /// Direct children of `pid`.
    pub fn children(&self, pid: u32) -> &[u32] {
        self.children.get(&pid).map(Vec::as_slice).unwrap_or(&[])
    }

    /// All descendants of `pid` in breadth-first order (excluding `pid`).
    pub fn descendants(&self, pid: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut seen = HashSet::from([pid]);
        let mut queue = std::collections::VecDeque::from([pid]);
        while let Some(p) = queue.pop_front() {
            for &c in self.children(p) {
                if seen.insert(c) {
                    out.push(c);
                    queue.push_back(c);
                }
            }
        }
        out
    }

    /// Iterate over every process.
    pub fn iter(&self) -> impl Iterator<Item = &ProcessInfo> {
        self.procs.values()
    }
}

/// sysinfo start time for a single pid (seconds since epoch); used as a fallback identity.
pub fn sysinfo_start_time(pid: u32) -> Option<u64> {
    let mut sys = System::new();
    let p = Pid::from_u32(pid);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[p]),
        true,
        ProcessRefreshKind::nothing(),
    );
    sys.process(p).map(|p| p.start_time())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn proc(pid: u32, ppid: u32, name: &str, cmd: &[&str]) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid: Some(ppid),
            name: name.into(),
            exe: None,
            cmdline: cmd.iter().map(|s| s.to_string()).collect(),
            cwd: None,
            uid: Some(1000),
            user: Some("dev".into()),
            start_time: 1_700_000_000,
            start_token: 1000 + pid as u64,
            memory_bytes: 1024,
            cpu_percent: 0.0,
        }
    }

    pub fn table(list: Vec<ProcessInfo>, self_pid: u32) -> ProcessTable {
        ProcessTable::from_processes(list.into_iter().map(|p| (p.pid, p)).collect(), self_pid)
    }

    #[test]
    fn tree_relationships() {
        let t = table(
            vec![
                proc(1, 0, "init", &[]),
                proc(10, 1, "zsh", &["-zsh"]),
                proc(20, 10, "npm", &["npm", "run", "dev"]),
                proc(21, 20, "sh", &["sh", "-c", "next dev"]),
                proc(22, 21, "node", &["node", "next", "dev"]),
                proc(23, 22, "node", &["node", "worker"]),
                proc(30, 10, "portwise", &["portwise"]),
            ],
            30,
        );
        assert_eq!(t.ancestors(22), vec![21, 20, 10, 1]);
        assert_eq!(t.descendants(20), vec![21, 22, 23]);
        assert!(t.is_self_or_ancestor(10));
        assert!(t.is_self_or_ancestor(30));
        assert!(!t.is_self_or_ancestor(20));
        assert!(t.is_mine(22));
    }

    #[test]
    fn capture_sees_self() {
        let t = ProcessTable::capture();
        let me = t.get(std::process::id()).expect("own process visible");
        assert!(me.start_token > 0);
        assert!(!t.is_empty());
    }
}

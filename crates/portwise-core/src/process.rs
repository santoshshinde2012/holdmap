//! Process table: metadata for every visible process plus parent/child relationships.

use crate::model::ProcessInfo;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};

/// The user list, cached. Listing users is a directory-service query (OpenDirectory on macOS)
/// that is slow and can stall for seconds, yet accounts almost never change: refresh once a
/// minute, or sooner (at most every few seconds) when a process runs as an unknown uid.
struct UserCache {
    users: Users,
    at: Instant,
    /// uids already missing from the list when it was read (system daemons on macOS): seeing
    /// them again is no reason to re-read it.
    missing: HashSet<sysinfo::Uid>,
}
static USERS: Mutex<Option<UserCache>> = Mutex::new(None);
const USERS_TTL: Duration = Duration::from_secs(60);
const USERS_MISS_TTL: Duration = Duration::from_secs(5);

/// Should the cached user list be re-read? `unknown` says whether a process has a uid the
/// list doesn't know.
fn users_stale(age: Option<Duration>, unknown: bool) -> bool {
    match age {
        None => true,
        Some(a) => a >= USERS_TTL || (unknown && a >= USERS_MISS_TTL),
    }
}

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

/// Linux cuts a process name (`comm`) to 15 bytes ("npm exec vite p", "containerd-shim"). When
/// argv[0] or the executable starts with that prefix, return the full name instead, so labels
/// read well and name-based rules (protection, agents) still match.
fn untruncated_name(comm: &str, cmdline: &[String], exe: Option<&Path>) -> Option<String> {
    const COMM_LEN: usize = 15;
    if !cfg!(target_os = "linux") || comm.len() != COMM_LEN {
        return None;
    }
    let argv0 = cmdline.first().map(String::as_str).unwrap_or("");
    // A retitled process (npm, node's process.title) puts its whole title in argv[0].
    if let Some(rest) = argv0.strip_prefix(comm) {
        let tail = rest.split(char::is_whitespace).next().unwrap_or("");
        return Some(format!("{comm}{tail}"));
    }
    let program = argv0.split_whitespace().next().unwrap_or("");
    let base = program.rsplit('/').next().unwrap_or("");
    let exe_base = exe
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .unwrap_or("");
    [base, exe_base]
        .into_iter()
        .find(|c| c.len() > COMM_LEN && c.starts_with(comm))
        .map(str::to_owned)
}

/// A process that rewrites its title in place (node's `process.title`: "next-server (v16.3.6)")
/// leaves macOS reporting the environment strings after the title as extra arguments. Drop them
/// so environment values (tokens included) never show up as part of a command line.
fn without_leaked_env(mut cmd: Vec<String>) -> Vec<String> {
    let retitled = cmd.first().is_some_and(|a| {
        a.contains(char::is_whitespace) && !a.starts_with(['/', '\\']) && a.get(1..2) != Some(":")
    });
    if retitled {
        let is_env = |a: &String| {
            a.split_once('=').is_some_and(|(k, _)| {
                k.chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
        };
        if let Some(i) = cmd.iter().skip(1).position(is_env) {
            cmd.truncate(i + 1);
        }
    }
    cmd
}

fn refresh_kind() -> ProcessRefreshKind {
    // Threads never own sockets separately from their process, and walking every
    // `/proc/<pid>/task` directory was the single largest cost of a scan on Linux.
    ProcessRefreshKind::nothing()
        .without_tasks()
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
        let mut cache = USERS.lock().unwrap_or_else(|e| e.into_inner());
        let unknown = |c: &UserCache| {
            sys.processes().values().any(|p| {
                p.user_id()
                    .is_some_and(|u| !c.missing.contains(u) && c.users.get_user_by_id(u).is_none())
            })
        };
        if users_stale(
            cache.as_ref().map(|c| c.at.elapsed()),
            cache.as_ref().is_some_and(unknown),
        ) {
            let users = Users::new_with_refreshed_list();
            let missing = sys
                .processes()
                .values()
                .filter_map(|p| p.user_id())
                .filter(|u| users.get_user_by_id(u).is_none())
                .cloned()
                .collect();
            *cache = Some(UserCache {
                users,
                at: Instant::now(),
                missing,
            });
        }
        let users = &cache.as_ref().expect("just filled").users;
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
                let cmdline = without_leaked_env(
                    p.cmd()
                        .iter()
                        .map(|s| s.to_string_lossy().into_owned())
                        .collect(),
                );
                let comm = p.name().to_string_lossy().into_owned();
                let name = untruncated_name(&comm, &cmdline, p.exe()).unwrap_or(comm);
                ProcessInfo {
                    pid,
                    ppid: p.parent().map(|pp| pp.as_u32()),
                    name,
                    exe: p.exe().map(PathBuf::from),
                    cmdline,
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
        ProcessRefreshKind::nothing().without_tasks(),
    );
    sys.process(p).map(|p| p.start_time())
}

#[cfg(test)]
pub(crate) mod tests {
    #[test]
    fn the_user_list_is_cached() {
        let s = Duration::from_secs;
        assert!(users_stale(None, false), "first scan reads it");
        assert!(!users_stale(Some(s(3)), false));
        assert!(
            !users_stale(Some(s(3)), true),
            "a new uid waits for the short TTL"
        );
        assert!(users_stale(Some(s(6)), true));
        assert!(!users_stale(Some(s(30)), false));
        assert!(users_stale(Some(s(61)), false));
    }

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
    fn retitled_processes_do_not_leak_their_environment() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let next = v(&[
            "next-server (v16.3.6)",
            "npm_package_x=^0.45.3",
            "API_TOKEN=secret",
        ]);
        assert_eq!(without_leaked_env(next), v(&["next-server (v16.3.6)"]));
        // Ordinary command lines keep their arguments, even ones that look like assignments.
        let make = v(&["make", "CC=clang"]);
        assert_eq!(without_leaked_env(make.clone()), make);
        let spaced = v(&[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "A=b",
        ]);
        assert_eq!(without_leaked_env(spaced.clone()), spaced);
        let npm = v(&["npm exec vite preview --port 4173", "--strictPort"]);
        assert_eq!(without_leaked_env(npm.clone()), npm);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn untruncated_name_recovers_linux_comm_names() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // A retitled npm process: the whole title is argv[0].
        let npm = v(&["npm exec vite preview --port 4173"]);
        assert_eq!(
            untruncated_name("npm exec vite p", &npm, None).as_deref(),
            Some("npm exec vite preview")
        );
        // A path in argv[0], or only the executable.
        let shim = v(&["/usr/bin/containerd-shim-runc-v2", "-namespace", "moby"]);
        assert_eq!(
            untruncated_name("containerd-shim", &shim, None).as_deref(),
            Some("containerd-shim-runc-v2")
        );
        let exe = Path::new("/opt/google/chrome/chrome_crashpad_handler");
        assert_eq!(
            untruncated_name("chrome_crashpad", &[], Some(exe)).as_deref(),
            Some("chrome_crashpad_handler")
        );
        // Short names, and argv[0] that doesn't match, keep the kernel name.
        assert_eq!(
            untruncated_name("node", &v(&["node", "server.js"]), None),
            None
        );
        assert_eq!(
            untruncated_name("abcdefghijklmno", &v(&["python3", "x.py"]), None),
            None
        );
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

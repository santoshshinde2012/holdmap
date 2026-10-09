//! Process memory sampling and app-level aggregation.
//!
//! Sampling is behind [`sample`]: each OS reports the figure users recognise (Activity Monitor
//! "Memory", Task Manager Working set, Linux VmRSS). Aggregation ([`app_usage`]) sums a
//! service's process tree so a browser or Electron app is shown as one number with a helper
//! count, without double-counting across ports that share the same tree.

use crate::process::ProcessTable;

/// Bytes of memory for one process, using the OS figure that matches the system monitor.
///
/// * **macOS** — `phys_footprint` via `proc_pid_rusage` (Activity Monitor's "Memory" column).
/// * **Linux** — `VmRSS` from `/proc/<pid>/status` (falls back to the value sysinfo already
///   filled in when the status file is unreadable).
/// * **Windows** — Working set (sysinfo's figure; matches Task Manager).
///
/// Returns `fallback` when the platform call fails (other users' processes, zombies).
pub fn sample(pid: u32, fallback: u64) -> u64 {
    platform::sample(pid).unwrap_or(fallback)
}

/// Memory of `root` plus every descendant, and how many of those descendants there are.
///
/// Used for the list / details / graph so a Chrome or Electron tree shows as one app. Callers
/// that attribute the same tree to several ports must share one total (don't sum per port).
pub fn app_usage(table: &ProcessTable, root: u32) -> AppMemory {
    let mut bytes = 0u64;
    let mut helpers = 0u32;
    if let Some(p) = table.get(root) {
        bytes = p.memory_bytes;
    }
    for d in table.descendants(root) {
        if let Some(p) = table.get(d) {
            bytes = bytes.saturating_add(p.memory_bytes);
            helpers += 1;
        }
    }
    AppMemory { bytes, helpers }
}

/// Total memory of an app (service root + helpers) and how many helpers it includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AppMemory {
    /// Sum of resident / footprint bytes for the root and its descendants.
    pub bytes: u64,
    /// Number of descendant processes included in [`Self::bytes`] (excludes the root).
    pub helpers: u32,
}

impl AppMemory {
    /// True when the tree is big enough to call out in the UI (≥ 256 MB, or ≥ 2 helpers and
    /// ≥ 64 MB). Keeps the list quiet for tiny tools.
    pub fn is_notable(self) -> bool {
        self.bytes >= 256 * 1024 * 1024 || (self.helpers >= 2 && self.bytes >= 64 * 1024 * 1024)
    }
}

/// A short hint like `"1.2 GB · 4 helpers"`, or `None` when there's nothing to say.
pub fn pressure_hint(mem: AppMemory) -> Option<String> {
    if !mem.is_notable() {
        return None;
    }
    let size = crate::util::human_bytes(mem.bytes);
    Some(if mem.helpers == 0 {
        size
    } else if mem.helpers == 1 {
        format!("{size} · 1 helper")
    } else {
        format!("{size} · {helpers} helpers", helpers = mem.helpers)
    })
}

/// Climb from a helper (Chrome Helper, Electron Helper, …) to the app the user recognises.
/// Stops at PID 1, at holdmap itself, or when the parent no longer looks like the same app.
pub fn app_root(table: &ProcessTable, pid: u32) -> u32 {
    let Some(mut cur) = table.get(pid) else {
        return pid;
    };
    let mut root = pid;
    for _ in 0..6 {
        let Some(ppid) = cur.ppid else { break };
        if ppid <= 1 || table.is_self_or_ancestor(ppid) {
            break;
        }
        let Some(parent) = table.get(ppid) else { break };
        if !same_app(cur, parent) {
            break;
        }
        root = ppid;
        cur = parent;
    }
    root
}

fn same_app(child: &crate::model::ProcessInfo, parent: &crate::model::ProcessInfo) -> bool {
    let c = child.name.to_ascii_lowercase();
    let p = parent.name.to_ascii_lowercase();
    if c.is_empty() || p.is_empty() {
        return false;
    }
    // "Google Chrome Helper" under "Google Chrome"; "electron Helper" under "electron".
    if c.starts_with(&p)
        || c.contains("helper") && p.split_whitespace().next() == c.split_whitespace().next()
    {
        return true;
    }
    // Same executable basename (Electron apps).
    match (&child.exe, &parent.exe) {
        (Some(a), Some(b)) => a.file_name() == b.file_name() && a.file_name().is_some(),
        _ => false,
    }
}

/// [`app_usage`] for the app that owns `pid` (climbing helpers first).
pub fn for_pid(table: &ProcessTable, pid: u32) -> AppMemory {
    app_usage(table, app_root(table, pid))
}

#[cfg(target_os = "macos")]
mod platform {
    use std::mem::MaybeUninit;

    pub fn sample(pid: u32) -> Option<u64> {
        if pid == 0 || pid > i32::MAX as u32 {
            return None;
        }
        let mut info = MaybeUninit::<libc::rusage_info_v2>::zeroed();
        // SAFETY: buffer is sized for rusage_info_v2; the kernel writes at most that many bytes.
        let rc = unsafe {
            libc::proc_pid_rusage(
                pid as libc::c_int,
                libc::RUSAGE_INFO_V2,
                info.as_mut_ptr().cast(),
            )
        };
        if rc != 0 {
            return None;
        }
        // SAFETY: fully initialised on success.
        let info = unsafe { info.assume_init() };
        // Activity Monitor's "Memory" column. Zero can be a short-lived process; treat as
        // unknown so the caller can fall back to sysinfo's resident size.
        (info.ri_phys_footprint > 0).then_some(info.ri_phys_footprint)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::fs;

    pub fn sample(pid: u32) -> Option<u64> {
        let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        for line in status.lines() {
            // "VmRSS:\t   12345 kB" (kernel uses a tab and a space before the unit)
            let Some(rest) = line.strip_prefix("VmRSS:") else {
                continue;
            };
            let rest = rest
                .trim()
                .trim_end_matches(['b', 'B'])
                .trim_end_matches(['k', 'K'])
                .trim();
            let kb: u64 = rest.parse().ok()?;
            return Some(kb.saturating_mul(1024));
        }
        None
    }
}

#[cfg(target_os = "windows")]
mod platform {
    // sysinfo already reads WorkingSetSize; we re-read via the fallback path in capture.
    pub fn sample(_pid: u32) -> Option<u64> {
        None
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    pub fn sample(_pid: u32) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::{proc, table};

    #[test]
    fn app_usage_sums_the_tree_without_double_counting_the_root() {
        let mut a = proc(10, 1, "chrome", &["chrome"]);
        a.memory_bytes = 100 * 1024 * 1024;
        let mut b = proc(11, 10, "chrome helper", &["chrome"]);
        b.memory_bytes = 50 * 1024 * 1024;
        let mut c = proc(12, 10, "chrome helper", &["chrome"]);
        c.memory_bytes = 25 * 1024 * 1024;
        let t = table(vec![proc(1, 0, "init", &[]), a, b, c], 99_999);
        let u = app_usage(&t, 10);
        assert_eq!(u.bytes, 175 * 1024 * 1024);
        assert_eq!(u.helpers, 2);
        assert!(u.is_notable());
        assert_eq!(pressure_hint(u).as_deref(), Some("175.0 MB · 2 helpers"));
    }

    #[test]
    fn a_lone_small_process_is_not_notable() {
        let mut p = proc(5, 1, "node", &["node"]);
        p.memory_bytes = 30 * 1024 * 1024;
        let t = table(vec![proc(1, 0, "init", &[]), p], 99_999);
        let u = app_usage(&t, 5);
        assert_eq!(u.helpers, 0);
        assert!(!u.is_notable());
        assert_eq!(pressure_hint(u), None);
    }

    #[test]
    fn app_root_climbs_chrome_helpers() {
        let mut main = proc(10, 1, "Google Chrome", &["Google Chrome"]);
        main.memory_bytes = 200 * 1024 * 1024;
        let mut h = proc(11, 10, "Google Chrome Helper", &["Google Chrome Helper"]);
        h.memory_bytes = 80 * 1024 * 1024;
        let t = table(vec![proc(1, 0, "init", &[]), main, h], 99_999);
        assert_eq!(app_root(&t, 11), 10);
        let u = for_pid(&t, 11);
        assert_eq!(u.bytes, 280 * 1024 * 1024);
        assert_eq!(u.helpers, 1);
    }

    #[test]
    fn sample_accepts_this_process() {
        let me = std::process::id();
        // Prefer the platform read; accept a non-zero fallback so the test still holds when
        // /proc is restricted (some sandboxes). A live scan always has sysinfo's figure.
        let n = sample(me, 4096);
        assert!(n >= 4096, "sample({me}) = {n}");
        if let Some(direct) = platform::sample(me) {
            assert!(direct > 0, "platform sample for self is {direct}");
        }
    }
}

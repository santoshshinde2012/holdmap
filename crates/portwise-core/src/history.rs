//! Stopped-port history and "restart": remember what was running on a port (command + working
//! directory) when portwise stopped it, so it can be started again later.

use crate::engine::{ActionPlan, Step, StopReport};
use crate::model::Protocol;
use crate::scan::Scan;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// One stopped port, remembered so it can be restarted (`history.jsonl`).
pub struct HistoryEntry {
    /// Unix epoch milliseconds when it was stopped.
    pub at_ms: u64,
    /// Port that was stopped.
    pub port: u16,
    /// Its protocol.
    pub protocol: Protocol,
    /// Display label at the time it was stopped.
    pub label: String,
    /// Command line of the process-tree root (e.g. `npm run dev`).
    pub command: Vec<String>,
    /// Working directory of the stopped process.
    pub cwd: Option<PathBuf>,
    /// Project name, when detected.
    pub project: Option<String>,
    /// Framework name, when detected.
    pub framework: Option<String>,
    /// PID of the stopped process.
    pub pid: u32,
}

impl HistoryEntry {
    /// The command line for display, with secrets hidden.
    pub fn command_line(&self) -> String {
        crate::redact::args(&self.command).join(" ")
    }

    /// A copy safe to show or export: secrets in the command hidden. The stored entry keeps
    /// the real command so a restart works; never hand that one to a UI or an agent.
    pub fn redacted(&self) -> HistoryEntry {
        HistoryEntry {
            command: crate::redact::args(&self.command),
            ..self.clone()
        }
    }

    /// Can this entry be started again? (we know a command, it isn't a container/supervisor)
    pub fn restartable(&self) -> bool {
        !self.command.is_empty()
    }
}

/// History entries for every port a successfully executed process plan freed.
pub fn entries_from_plan(scan: &Scan, plan: &ActionPlan, report: &StopReport) -> Vec<HistoryEntry> {
    if !report.success {
        return Vec::new();
    }
    let roots: Vec<&crate::engine::ProcRef> = plan
        .steps
        .iter()
        .filter_map(|s| match s {
            Step::SignalProcesses { processes, .. } => processes.first(),
            _ => None,
        })
        .collect();
    let now = crate::util::now_ms();
    let mut out = Vec::new();
    for step in &plan.steps {
        let Step::VerifyFree { port, protocol, .. } = step else {
            continue;
        };
        let Some(entry) = scan
            .snapshot
            .entries
            .iter()
            .find(|e| e.port == *port && e.protocol == *protocol && e.state.is_listening())
        else {
            continue;
        };
        // The root whose tree held this port.
        let root = roots.iter().find(|r| {
            entry.pid == Some(r.pid)
                || entry
                    .pid
                    .is_some_and(|p| scan.table.ancestors(p).contains(&r.pid))
        });
        let (command, cwd, pid) = match root {
            Some(r) => {
                let p = scan.table.get(r.pid);
                (
                    p.map(|p| p.cmdline.clone()).unwrap_or_default(),
                    p.and_then(|p| p.cwd.clone()),
                    r.pid,
                )
            }
            None => (Vec::new(), None, entry.pid.unwrap_or(0)),
        };
        out.push(HistoryEntry {
            at_ms: now,
            port: *port,
            protocol: *protocol,
            label: entry.label.clone(),
            command,
            cwd,
            project: entry.project.as_ref().map(|p| p.name.clone()),
            framework: entry.framework.as_ref().map(|f| f.name.clone()),
            pid,
        });
    }
    out
}

/// Start `entry`'s command again, detached from portwise, logging to `log_dir`.
/// Returns the new process id and the log path.
pub fn restart(entry: &HistoryEntry, log_dir: &Path) -> io::Result<(u32, PathBuf)> {
    let (prog, args) = entry
        .command
        .split_first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no command recorded"))?;
    let log = log_dir.join(format!("{}-{}.log", entry.port, crate::util::now_ms()));
    let mut cmd = Command::new(prog);
    cmd.args(args);
    if let Some(cwd) = entry.cwd.as_ref().filter(|c| c.is_dir()) {
        cmd.current_dir(cwd);
    }
    let mut child = crate::util::spawn_detached(&mut cmd, &log)?;
    let pid = child.id();
    // Reap it when it exits so long-lived frontends don't accumulate zombies.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok((pid, log))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Engine, StopOptions, Target};

    #[test]
    fn records_the_tree_root_command_and_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let e = Engine::from_scan(crate::topology::tests::acme(tmp.path()));
        let plan = e.plan(&Target::Port(3000), &StopOptions::default());
        let ok = StopReport {
            success: true,
            ..Default::default()
        };
        let h = entries_from_plan(&e.scan, &plan, &ok);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].port, 3000);
        assert_eq!(h[0].command_line(), "npm run dev");
        assert!(h[0].cwd.as_ref().unwrap().ends_with("apps/web"));
        assert_eq!(h[0].project.as_deref(), Some("web"));
        assert!(h[0].restartable());
        assert!(entries_from_plan(&e.scan, &plan, &StopReport::default()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn restart_spawns_the_command_in_its_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join("started");
        let entry = HistoryEntry {
            at_ms: 0,
            port: 1,
            protocol: Protocol::Tcp,
            label: "t".into(),
            command: vec!["sh".into(), "-c".into(), "pwd > started".into()],
            cwd: Some(tmp.path().to_path_buf()),
            project: None,
            framework: None,
            pid: 0,
        };
        let (pid, log) = restart(&entry, &tmp.path().join("logs")).unwrap();
        assert!(pid > 0 && log.exists());
        for _ in 0..100 {
            if marker.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let pwd = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(
            std::fs::canonicalize(pwd.trim()).unwrap(),
            std::fs::canonicalize(tmp.path()).unwrap()
        );
        let empty = HistoryEntry {
            command: vec![],
            ..entry
        };
        assert!(restart(&empty, tmp.path()).is_err());
    }
}

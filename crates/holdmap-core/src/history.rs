//! Stopped-port history and "restart": remember what was running on a port (command + working
//! directory) when holdmap stopped it, so it can be started again later.

use crate::engine::{ActionPlan, Step, StopReport};
use crate::model::Protocol;
use crate::scan::Scan;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Private persisted Windows restart state; always removed by [`HistoryEntry::redacted`].
/// This preserves native command syntax, rather than reconstructing it from parsed argv.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsCmdRestart {
    executable: PathBuf,
    arguments: String,
}

impl std::fmt::Debug for WindowsCmdRestart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WindowsCmdRestart { private command }")
    }
}

fn is_cmd(program: &str) -> bool {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    name.eq_ignore_ascii_case("cmd.exe") || name.eq_ignore_ascii_case("cmd")
}

impl WindowsCmdRestart {
    #[cfg(windows)]
    pub(crate) fn capture(process: &crate::model::ProcessInfo) -> Option<Self> {
        let executable = process.exe.as_ref()?;
        // Only cmd.exe needs this query. Other programs keep their existing argv path.
        if !executable.is_absolute() || !is_cmd(executable.to_str()?) {
            return None;
        }
        let raw = crate::sys::windows::command_line(process.pid, process.start_token)?;
        Self::from_raw(executable, &raw)
    }

    #[cfg(any(windows, test))]
    fn from_raw(executable: &Path, raw: &str) -> Option<Self> {
        let raw = raw.trim_start_matches([' ', '\t']);
        let (program, arguments) = if let Some(quoted) = raw.strip_prefix('"') {
            let end = quoted.find('"')?;
            let suffix = &quoted[end + 1..];
            if !suffix.is_empty() && !suffix.starts_with([' ', '\t']) {
                return None;
            }
            (&quoted[..end], suffix)
        } else {
            let end = raw.find([' ', '\t']).unwrap_or(raw.len());
            (&raw[..end], &raw[end..])
        };
        if !is_cmd(program) || arguments.is_empty() || raw.contains('\0') {
            return None;
        }
        // Only the executable token is separated. Every quote, operator and byte after it
        // remains in its original position; no shell script is assembled from argv.
        let captured = Self {
            executable: executable.to_path_buf(),
            arguments: arguments.to_owned(),
        };
        captured.within_command_limit().then_some(captured)
    }

    #[cfg(any(windows, test))]
    fn within_command_limit(&self) -> bool {
        // CreateProcessW's limit includes the quoted executable, separator and trailing NUL.
        self.executable.to_str().is_some_and(|exe| {
            exe.encode_utf16()
                .count()
                .saturating_add(self.arguments.encode_utf16().count())
                .saturating_add(4)
                <= 32767
        })
    }

    #[cfg(windows)]
    fn valid_for(&self, program: &str) -> bool {
        is_cmd(program)
            && self.executable.is_absolute()
            && self.executable.to_str().is_some_and(is_cmd)
            && !self.arguments.is_empty()
            && !self.arguments.contains('\0')
            && self.within_command_limit()
    }
}

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
    /// Private native Windows command syntax for faithful cmd.exe replay. Omitted on exports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows_restart: Option<WindowsCmdRestart>,
    /// Exported restart availability; never used as native execution authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restartable: Option<bool>,
    /// Exported copy availability; cmd.exe's private syntax cannot be rebuilt from display argv.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copyable: Option<bool>,
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
            windows_restart: None,
            restartable: Some(self.restartable()),
            copyable: Some(
                !self.command.is_empty() && !self.command.first().is_some_and(|p| is_cmd(p)),
            ),
            ..self.clone()
        }
    }

    /// Can this entry be started again? (we know a command, it isn't a container/supervisor)
    pub fn restartable(&self) -> bool {
        let Some(program) = self.command.first() else {
            return false;
        };
        #[cfg(windows)]
        if is_cmd(program) {
            return self
                .windows_restart
                .as_ref()
                .is_some_and(|raw| raw.valid_for(program));
        }
        !program.is_empty()
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
            #[cfg(windows)]
            windows_restart: scan.table.windows_cmd_restart(pid).cloned(),
            #[cfg(not(windows))]
            windows_restart: None,
            restartable: None,
            copyable: None,
        });
    }
    out
}

/// Start `entry`'s command again, detached from holdmap, logging to `log_dir`.
/// Returns the new process id and the log path.
pub fn restart(entry: &HistoryEntry, log_dir: &Path) -> io::Result<(u32, PathBuf)> {
    let (prog, args) = entry
        .command
        .split_first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no command recorded"))?;
    let log = log_dir.join(format!("{}-{}.log", entry.port, crate::util::now_ms()));
    #[cfg(windows)]
    let mut cmd = if is_cmd(prog) {
        use std::os::windows::process::CommandExt;
        let raw = entry.windows_restart.as_ref().filter(|raw| raw.valid_for(prog)).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput,
                "original Windows command syntax was not recorded; start this service normally before stopping it again")
        })?;
        let mut cmd = Command::new(&raw.executable);
        cmd.raw_arg(&raw.arguments);
        cmd
    } else {
        let mut cmd = Command::new(prog);
        cmd.args(args);
        cmd
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut cmd = Command::new(prog);
        cmd.args(args);
        cmd
    };
    if let Some(cwd) = entry.cwd.as_ref() {
        if !cwd.is_absolute() || !cwd.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "recorded working directory {} is unavailable",
                    cwd.display()
                ),
            ));
        }
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

    fn cmd_entry(raw: Option<WindowsCmdRestart>) -> HistoryEntry {
        HistoryEntry {
            at_ms: 7,
            port: 3000,
            protocol: Protocol::Tcp,
            label: "cmd fixture".into(),
            command: vec![
                "cmd.exe".into(),
                "/C".into(),
                "--token".into(),
                "display-secret".into(),
            ],
            cwd: None,
            project: None,
            framework: None,
            pid: 1,
            windows_restart: raw,
            restartable: None,
            copyable: None,
        }
    }

    #[test]
    fn captured_cmd_preserves_quotes_operators_and_whitespace() {
        let executable = Path::new(r"C:\Windows\System32\cmd.exe");
        let suffix = r#" /D /S /C ""C:\service with spaces\worker.exe" "a & b" && echo done > "result file""  "#;
        for program in ["cmd.exe", "CMD", r#""C:\Windows\System32\cmd.exe""#] {
            let captured =
                WindowsCmdRestart::from_raw(executable, &format!("\t {program}{suffix}")).unwrap();
            assert_eq!(captured.executable, executable);
            assert_eq!(captured.arguments, suffix);
        }
        for malformed in [
            r#""C:\Windows\System32\cmd.exe"/C echo unsafe"#,
            r#""cmd.exe /C echo missing quote"#,
            "powershell.exe /C echo wrong program",
            "cmd.exe",
            "cmd.exe /C echo nul\0suffix",
        ] {
            assert!(
                WindowsCmdRestart::from_raw(executable, malformed).is_none(),
                "accepted {malformed:?}"
            );
        }
    }

    #[test]
    fn captured_cmd_bounds_the_complete_create_process_command() {
        let executable = Path::new(r"C:\Windows\System32\cmd.exe");
        let available = 32767 - executable.to_str().unwrap().encode_utf16().count() - 4;
        let raw = format!("cmd.exe {}", "x".repeat(available - 1));
        assert!(WindowsCmdRestart::from_raw(executable, &raw).is_some());
        assert!(WindowsCmdRestart::from_raw(executable, &(raw + "x")).is_none());
        let supplementary = format!("cmd.exe {}", "😀".repeat(16384));
        assert!(WindowsCmdRestart::from_raw(executable, &supplementary).is_none());
    }

    #[test]
    fn private_cmd_state_survives_storage_but_never_display_or_export() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::store::Store::new(dir.path().join("state"));
        let raw = WindowsCmdRestart::from_raw(
            Path::new(r"C:\Windows\System32\cmd.exe"),
            "cmd.exe /D /C echo private-shell-secret",
        )
        .unwrap();
        let entry = cmd_entry(Some(raw));
        let entry = HistoryEntry {
            command: vec![
                "cmd.exe".into(),
                "/D".into(),
                "/C".into(),
                "echo --password display-secret".into(),
            ],
            ..entry
        };
        store.record(std::slice::from_ref(&entry)).unwrap();
        let stored = store.entry(entry.at_ms, entry.port).unwrap();
        assert_eq!(stored.windows_restart, entry.windows_restart);
        assert_eq!(
            stored.command, entry.command,
            "native restart arguments must remain unchanged"
        );
        let exported = stored.redacted();
        let json = serde_json::to_string(&exported).unwrap();
        assert!(!json.contains("windows_restart"));
        assert!(!json.contains("private-shell-secret"));
        assert!(!json.contains("display-secret"));
        assert_eq!(exported.copyable, Some(false));
        assert!(!format!("{stored:?}").contains("private-shell-secret"));
        let mut old = serde_json::to_value(cmd_entry(None)).unwrap();
        for field in ["windows_restart", "restartable", "copyable"] {
            old.as_object_mut().unwrap().remove(field);
        }
        let legacy: HistoryEntry = serde_json::from_value(old).unwrap();
        assert_eq!(legacy.windows_restart, None);
        assert_eq!(legacy.copyable, None);
    }

    #[cfg(windows)]
    #[test]
    fn legacy_cmd_restart_fails_closed_before_launch_or_log_creation() {
        let dir = tempfile::tempdir().unwrap();
        let mut entry = cmd_entry(None);
        // Exported availability flags cannot authorize native execution.
        entry.restartable = Some(true);
        assert!(!entry.restartable());
        assert_eq!(entry.redacted().restartable, Some(false));
        let logs = dir.path().join("logs");
        let error = restart(&entry, &logs).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error
            .to_string()
            .contains("original Windows command syntax"));
        assert!(!logs.exists());
    }

    #[cfg(windows)]
    #[test]
    fn owned_windows_cmd_fixture() {
        if std::fs::read_to_string("fixture-arm.txt").ok().as_deref()
            != Some("owned Windows history fixture")
        {
            return;
        }
        let port_file = Path::new("fixture-port.txt");
        let port = std::fs::read_to_string(port_file)
            .ok()
            .and_then(|text| text.trim().parse::<u16>().ok())
            .unwrap_or(0);
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::fs::write(port_file, port.to_string()).unwrap();
        std::fs::write(
            "fixture-started.txt",
            format!(
                "{}\n{}\n{}",
                std::process::id(),
                port,
                std::env::current_dir().unwrap().display()
            ),
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !Path::new("fixture-release.txt").exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "owned fixture timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[cfg(windows)]
    #[test]
    fn captured_windows_cmd_restarts_quoted_service_and_shell_operators() {
        use crate::engine::{ActionPlan, Owner, ProcRef, Risk};
        use crate::model::{Family, RawSocket, SocketState};
        use crate::process::ProcessTable;
        use std::process::Stdio;
        use std::time::{Duration, Instant};

        // This guard can only terminate processes whose native identity was captured for
        // this fixture. Even a failed assertion leaves no long-lived owned listener behind.
        struct Owned {
            processes: Vec<(u32, u64)>,
            release: PathBuf,
            shell: std::process::Child,
        }
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = std::fs::write(&self.release, "release");
                for &(pid, token) in self.processes.iter().rev() {
                    let _ = crate::sys::signal(pid, token, crate::sys::Sig::Kill);
                }
                let _ = self.shell.kill();
                let _ = self.shell.wait();
            }
        }
        fn started(path: &Path) -> (u32, u16, PathBuf) {
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                if let Ok(text) = std::fs::read_to_string(path) {
                    let mut lines = text.lines();
                    if let (Some(pid), Some(port), Some(cwd)) =
                        (lines.next(), lines.next(), lines.next())
                    {
                        if let (Ok(pid), Ok(port)) = (pid.parse(), port.parse()) {
                            return (pid, port, cwd.into());
                        }
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "owned cmd service did not become ready"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let service_dir = dir.path().join("quoted service & fixture");
        std::fs::create_dir(&service_dir).unwrap();
        std::fs::write(
            service_dir.join("fixture-arm.txt"),
            "owned Windows history fixture",
        )
        .unwrap();
        let executable = service_dir.join("owned fixture.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        let line = format!(
            "\"{}\" --exact history::tests::owned_windows_cmd_fixture --nocapture && echo quoted ^& operators > \"completed output.txt\"",
            executable.display()
        );
        let shell = crate::util::shell_command(&line)
            .current_dir(&service_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let shell_pid = shell.id();
        let mut owned = Owned {
            processes: vec![],
            release: service_dir.join("fixture-release.txt"),
            shell,
        };
        let marker = service_dir.join("fixture-started.txt");
        let (service_pid, port, cwd) = started(&marker);
        let table = ProcessTable::capture();
        let shell_info = table.get(shell_pid).unwrap();
        let service_info = table.get(service_pid).unwrap();
        owned.processes.extend([
            (shell_pid, shell_info.start_token),
            (service_pid, service_info.start_token),
        ]);
        assert!(
            table.windows_cmd_restart(shell_pid).is_some(),
            "original cmd syntax must be captured before stop"
        );
        assert!(crate::sys::windows::command_line(
            shell_pid,
            shell_info.start_token.wrapping_add(1)
        )
        .is_none());
        assert_eq!(
            std::fs::canonicalize(cwd).unwrap(),
            std::fs::canonicalize(&service_dir).unwrap()
        );
        let processes = [shell_info, service_info]
            .into_iter()
            .map(|p| ProcRef {
                pid: p.pid,
                name: p.name.clone(),
                start_token: p.start_token,
                command: p.command(),
            })
            .collect();
        let scan = crate::scan::Scanner::fixed(
            vec![RawSocket {
                protocol: Protocol::Tcp,
                family: Family::V4,
                local_addr: std::net::Ipv4Addr::LOCALHOST.into(),
                local_port: port,
                remote_addr: None,
                remote_port: None,
                state: SocketState::Listen,
                uid: None,
                inode: None,
                pids: vec![service_pid],
            }],
            table,
            vec![],
        )
        .scan(&crate::scan::ScanOptions {
            docker: false,
            ..Default::default()
        })
        .unwrap();
        let plan = ActionPlan {
            target: format!(":{port}"),
            owners: vec![Owner::Process {
                pid: shell_pid,
                name: "cmd.exe".into(),
            }],
            summary: "owned history fixture".into(),
            blocked: None,
            warnings: vec![],
            risk: Risk::Low,
            allow_protected: false,
            steps: vec![
                Step::SignalProcesses {
                    processes,
                    force: true,
                    timeout_ms: 1000,
                },
                Step::VerifyFree {
                    port,
                    protocol: Protocol::Tcp,
                    timeout_ms: 1000,
                },
            ],
        };
        // Stop only the two fixture identities; never run a plan against unrelated processes.
        for &(pid, token) in &owned.processes {
            crate::sys::signal(pid, token, crate::sys::Sig::Kill).unwrap();
        }
        owned.shell.wait().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).is_ok() {
            assert!(
                Instant::now() < deadline,
                "owned listener remained alive after stop"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let entries = entries_from_plan(
            &scan,
            &plan,
            &StopReport {
                success: true,
                freed: true,
                ..Default::default()
            },
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pid, shell_pid);
        assert!(entries[0].restartable());
        assert_eq!(entries[0].redacted().restartable, Some(true));
        assert_eq!(entries[0].redacted().copyable, Some(false));
        assert!(!serde_json::to_string(&entries[0].redacted())
            .unwrap()
            .contains("windows_restart"));
        let store = crate::store::Store::new(dir.path().join("state"));
        store.record(&entries).unwrap();
        let entry = store.entry(entries[0].at_ms, port).unwrap();
        std::fs::remove_file(&marker).unwrap();
        let (restarted_shell, log) = restart(&entry, &dir.path().join("logs")).unwrap();
        owned.processes.push((
            restarted_shell,
            crate::sys::start_token(restarted_shell).unwrap(),
        ));
        let (restarted_service, restarted_port, restarted_cwd) = started(&marker);
        owned.processes.push((
            restarted_service,
            crate::sys::start_token(restarted_service).unwrap(),
        ));
        assert_ne!(service_pid, restarted_service);
        assert_eq!(port, restarted_port);
        assert!(log.is_file());
        assert_eq!(
            std::fs::canonicalize(restarted_cwd).unwrap(),
            std::fs::canonicalize(&service_dir).unwrap()
        );
        assert!(std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).is_ok());
        std::fs::write(&owned.release, "release").unwrap();
        let output = service_dir.join("completed output.txt");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(text) = std::fs::read_to_string(&output) {
                if text.trim() == "quoted & operators" {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "restarted cmd lost its shell operators"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

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
            windows_restart: None,
            restartable: None,
            copyable: None,
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

    #[cfg(windows)]
    #[test]
    fn restart_spawns_a_windows_command_in_its_recorded_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("restarted-cwd.txt");
        let entry = HistoryEntry {
            at_ms: 0,
            port: 1,
            protocol: Protocol::Tcp,
            label: "owned Windows restart fixture".into(),
            command: vec![
                "cmd.exe".into(),
                "/D".into(),
                "/C".into(),
                "cd > restarted-cwd.txt".into(),
            ],
            cwd: Some(dir.path().to_path_buf()),
            project: None,
            framework: None,
            pid: 0,
            windows_restart: WindowsCmdRestart::from_raw(
                &crate::util::which("cmd.exe").unwrap(),
                "cmd.exe /D /S /C \"cd > restarted-cwd.txt\"",
            ),
            restartable: None,
            copyable: None,
        };
        let (pid, log) = restart(&entry, &dir.path().join("logs")).unwrap();
        assert!(pid > 0 && log.is_file());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let cwd = loop {
            if let Ok(value) = std::fs::read_to_string(&marker) {
                if !value.trim().is_empty() {
                    break value;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "owned restart command did not write its cwd"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert_eq!(
            std::fs::canonicalize(cwd.trim()).unwrap(),
            std::fs::canonicalize(dir.path()).unwrap()
        );
    }

    #[test]
    fn restart_refuses_to_run_in_an_unrelated_working_directory() {
        let dir = tempfile::tempdir().unwrap();
        let entry = HistoryEntry {
            at_ms: 0,
            port: 1,
            protocol: Protocol::Tcp,
            label: "fixture".into(),
            command: vec!["fixture-do-not-launch".into()],
            cwd: Some(dir.path().join("removed")),
            project: None,
            framework: None,
            pid: 0,
            windows_restart: None,
            restartable: None,
            copyable: None,
        };
        let logs = dir.path().join("logs");
        let error = restart(&entry, &logs).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(error.to_string().contains("recorded working directory"));
        assert!(!logs.exists());
        let relative = HistoryEntry {
            cwd: Some("relative".into()),
            ..entry
        };
        assert!(restart(&relative, &logs).is_err());
    }
}

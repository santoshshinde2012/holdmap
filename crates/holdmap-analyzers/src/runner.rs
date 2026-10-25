//! Running an analyzer: the [`AnalyzerRunner`] seam, its raw output, and the [`Sandbox`]
//! that prepares the child process. Parsing happens elsewhere ([`crate::parse`]).

use crate::finding::Subject;
use crate::input::AnalyzerInput;
use crate::manifest::AnalyzerManifest;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

/// What came back for one subject (or one batch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawOutput {
    /// The subjects this output covers.
    pub subjects: Vec<Subject>,
    /// Exit code, when a process ran.
    pub exit_code: Option<i32>,
    /// The report bytes (capped).
    pub bytes: Vec<u8>,
    /// Plain-language problems (timeout, cap, staging limits). Never producer output.
    pub notes: Vec<String>,
    /// Whether the run is unusable.
    pub failed: bool,
    /// The MCP tool that produced it (MCP analyzers).
    pub tool: Option<String>,
    /// The program exited with one of its `exit_no_findings` codes and wrote no report: it
    /// found nothing it could check (e.g. no dependencies), which is a clean run.
    pub nothing_to_check: bool,
}

/// The note on work a user stopped.
pub const CANCELLED: &str = "Stopped: the run was cancelled.";

/// Set to stop a run early; checked between calls and while waiting for one.
#[derive(Debug, Clone, Default)]
pub struct CancelFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl CancelFlag {
    /// Ask the run to stop.
    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    /// Whether it was asked to stop.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
    /// Clear it before a new run.
    pub fn reset(&self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

impl PartialEq for CancelFlag {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for CancelFlag {}

/// The step a run is on, for a live progress line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStep {
    /// Checking approval, the program and its version.
    Preparing,
    /// Copying one subject's folder into the private staging folder.
    Copying,
    /// The analyzer is looking at one subject (or the whole batch).
    Scanning,
    /// Reading and normalising what came back.
    Collecting,
}

/// One progress update. Carries names only: never file contents, output or environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEvent {
    /// The step.
    pub step: RunStep,
    /// 1-based position of the subject being worked on (0 when the step isn't per subject).
    pub index: usize,
    /// Subjects in this run.
    pub total: usize,
    /// The subject's display name, when the step is about one subject.
    pub subject: Option<String>,
    /// Catalog ids of the agents that load that subject.
    #[serde(default)]
    pub agents: Vec<String>,
}

impl RunEvent {
    /// A step that isn't about one subject.
    pub fn step(step: RunStep, total: usize) -> Self {
        Self {
            step,
            index: 0,
            total,
            subject: None,
            agents: Vec::new(),
        }
    }

    /// A step about subject `index` (1-based) of `total`.
    pub fn subject(step: RunStep, index: usize, total: usize, subject: &Subject) -> Self {
        Self {
            step,
            index,
            total,
            subject: Some(subject.name.clone()),
            agents: subject.agents.clone(),
        }
    }
}

/// Where progress updates go. The default drops them.
#[derive(Clone, Default)]
pub struct ProgressSink(Option<std::sync::Arc<dyn Fn(RunEvent) + Send + Sync>>);

impl ProgressSink {
    /// Send updates to `f`.
    pub fn new(f: impl Fn(RunEvent) + Send + Sync + 'static) -> Self {
        Self(Some(std::sync::Arc::new(f)))
    }
    /// Report one update.
    pub fn send(&self, event: RunEvent) {
        if let Some(f) = &self.0 {
            f(event);
        }
    }
}

impl std::fmt::Debug for ProgressSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "ProgressSink(on)"
        } else {
            "ProgressSink(off)"
        })
    }
}

impl PartialEq for ProgressSink {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}
impl Eq for ProgressSink {}

/// Facts about the run shared by every subject.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunContext {
    /// The approved, pinned executable (CLI analyzers).
    pub binary: Option<PathBuf>,
    /// API keys from the keychain for this analyzer only, set after the environment is cleared.
    pub secrets: Vec<(String, crate::secrets::Secret)>,
    /// The approved MCP tool-list pin; a different listing stops the run.
    pub tools_sha256: Option<String>,
    /// Stops the run early.
    pub cancel: CancelFlag,
    /// Live progress updates.
    pub progress: ProgressSink,
}

/// Why a run could not start.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RunError {
    /// The analyzer has not been approved, or changed since it was approved.
    #[error("{0}")]
    NotApproved(String),
    /// The analyzer is missing or can't be used.
    #[error("{0}")]
    Unavailable(String),
    /// Nothing to analyse.
    #[error("nothing to analyse: {0}")]
    NoSubjects(String),
    /// The MCP server's tools differ from the approved ones; nothing was called.
    #[error("its MCP tools changed since you approved it ({}). Review and approve it again.", .0.changes.join("; "))]
    ToolsChanged(Box<ToolsChanged>),
}

/// What an MCP server listed instead of the approved tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolsChanged {
    /// The new listing.
    pub snapshot: crate::mcp_tools::ToolSnapshot,
    /// What changed, in plain words (filled in by the service).
    pub changes: Vec<String>,
}

/// Runs one kind of analyzer.
pub trait AnalyzerRunner: Send + Sync {
    /// Run over every subject in the input.
    fn run(
        &self,
        manifest: &AnalyzerManifest,
        input: &AnalyzerInput,
        cx: &RunContext,
    ) -> Result<Vec<RawOutput>, RunError>;
}

/// How strongly a run is contained, stated honestly on the permissions sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxLevel {
    /// Staged copy, cleared environment, private HOME/TMPDIR, timeout and output caps.
    /// The OS does not stop it reading other files or using the network.
    Basic,
    /// [`SandboxLevel::Basic`] inside a macOS Seatbelt profile (`sandbox-exec`): writes only in
    /// its private folder, common credential folders unreadable, no network unless declared.
    /// Best effort: Apple deprecated `sandbox-exec`, and other reads are allowed.
    MacosSeatbelt,
    /// [`SandboxLevel::Basic`] inside bubblewrap: read-only system, writes only in its private
    /// folder, common credential folders hidden, own process tree, no network unless declared.
    LinuxBubblewrap,
    /// [`SandboxLevel::Basic`] under Linux Landlock (when bubblewrap is missing): writes only
    /// in its private folder, common credential paths unreadable, TCP denied unless declared
    /// (Linux 6.7+). Other reads and non-TCP traffic are allowed.
    LinuxLandlock,
    /// [`SandboxLevel::Basic`] inside a Windows Job Object: the whole process tree is stopped
    /// at the time limit and memory is capped. It does not limit files or network.
    WindowsJob,
}

impl SandboxLevel {
    /// One-line description for the permissions sheet; `network` is whether the analyzer
    /// declares network access.
    pub fn describe(self, network: bool) -> String {
        const BASIC: &str = "it gets a copy of the folder, a cleared environment and a private temporary home, and is stopped at the time limit.";
        let net = if network {
            "It may use the network, as its permissions say."
        } else {
            "The operating system blocks its network access."
        };
        match self {
            Self::Basic => format!("Basic: {BASIC} The operating system does not block it from reading other files or using the network."),
            Self::MacosSeatbelt => format!("macOS sandbox (best effort): {BASIC} It can only write inside its private folder and can't read common credential folders (.ssh, .aws, Keychains and similar). {net} It can still read your other files. Apple has deprecated the sandbox-exec tool this uses."),
            Self::LinuxBubblewrap => format!("Linux sandbox (bubblewrap): {BASIC} The system is read-only, it can only write inside its private folder, common credential folders (.ssh, .aws, .gnupg and similar) are hidden and it gets its own process tree. {net} It can still read your other files."),
            Self::LinuxLandlock => {
                let tcp = cfg!(target_os = "linux") && crate::sandbox::landlock_blocks_tcp();
                let net = match (network, tcp) {
                    (true, _) => "It may use the network, as its permissions say.",
                    (false, true) => "The operating system blocks its TCP connections; other network traffic (such as UDP) isn't blocked.",
                    (false, false) => "This Linux version can't block its network access.",
                };
                format!("Linux sandbox (Landlock): {BASIC} It can only write inside its private folder and can't read common credential folders (.ssh, .aws, .gnupg and similar) or signal other programs. {net} It can still read your other files.")
            }
            Self::WindowsJob => format!("Windows Job Object: {BASIC} Its whole process tree is stopped at the time limit and its memory is capped. Windows does not block it from reading other files or using the network."),
        }
    }
}

/// What a sandbox needs to know about one run.
#[derive(Debug, Clone, Copy)]
pub struct Containment<'a> {
    /// The private folder for this run (staged copy, report file, home). The only place it
    /// may write.
    pub work: &'a Path,
    /// Its private HOME/TMPDIR root, inside `work`.
    pub home: &'a Path,
    /// Whether the manifest declares network access.
    pub network: bool,
}

/// Kept alive for the length of a run (a Windows job handle); dropping it ends the run's tree.
pub type ContainGuard = Box<dyn std::any::Any>;

/// Prepares the child process.
pub trait Sandbox: Send + Sync {
    /// The containment level.
    fn level(&self) -> SandboxLevel;

    /// Configure `cmd` to run with `home` as its private HOME/TMPDIR root and a cleared
    /// environment.
    fn prepare(&self, cmd: &mut Command, home: &Path, network: bool) {
        basic_env(cmd, home, network);
    }

    /// The command that runs `binary args` contained at this level. Wrapping sandboxes start
    /// a launcher (`sandbox-exec`, `bwrap`) that then starts `binary`.
    fn command(&self, binary: &Path, args: &[String], c: &Containment<'_>) -> Command {
        let mut cmd = Command::new(binary);
        cmd.args(args);
        self.prepare(&mut cmd, c.home, c.network);
        cmd
    }

    /// Called right after the child starts (Windows puts it in a job object here).
    fn contain(&self, _process: ProcessRef) -> Option<ContainGuard> {
        None
    }
}

/// A started child process, as a sandbox needs to see it.
#[derive(Debug, Clone, Copy)]
pub struct ProcessRef {
    /// Process id.
    pub id: u32,
    /// Its process handle.
    #[cfg(windows)]
    pub handle: std::os::windows::io::RawHandle,
}

impl ProcessRef {
    /// A child started with `std::process`.
    pub fn of(child: &std::process::Child) -> Self {
        Self {
            id: child.id(),
            #[cfg(windows)]
            handle: std::os::windows::io::AsRawHandle::as_raw_handle(child),
        }
    }
}

/// Environment names a run may see, with values holdmap sets itself.
pub const ENV_NAMES: &[&str] = &["PATH", "HOME", "TMPDIR", "TMP", "TEMP", "LANG", "NO_COLOR"];

/// [`SandboxLevel::Basic`]: cleared environment and private temporary directories.
#[derive(Debug, Default, Clone, Copy)]
pub struct BasicSandbox;

impl Sandbox for BasicSandbox {
    fn level(&self) -> SandboxLevel {
        SandboxLevel::Basic
    }
}

/// The cleared environment every level starts from: fixed values set by holdmap only.
pub fn basic_env(cmd: &mut Command, home: &Path, network: bool) {
    let tmp = home.join("tmp");
    let _ = std::fs::create_dir_all(&tmp);
    cmd.env_clear()
        .env("PATH", SYSTEM_PATH)
        .env("HOME", home)
        .env("TMPDIR", &tmp)
        .env("TMP", &tmp)
        .env("TEMP", &tmp)
        .env("LANG", "C.UTF-8")
        .env("NO_COLOR", "1");
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot") {
        // Windows programs fail to start without it; it is a fixed system path, not a secret.
        cmd.env("SystemRoot", root);
    }
    if !network {
        // Best effort only: well-behaved HTTP clients honour these; nothing enforces it.
        for name in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
        ] {
            cmd.env(name, "http://127.0.0.1:9");
        }
    }
    cmd.stdin(std::process::Stdio::null());
}

#[cfg(windows)]
const SYSTEM_PATH: &str = r"C:\Windows\System32;C:\Windows";
#[cfg(not(windows))]
const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_sandbox_clears_the_environment() {
        let dir = tempfile::tempdir().unwrap();
        let mut cmd = Command::new("x");
        cmd.env("AWS_SECRET_ACCESS_KEY", "nope");
        BasicSandbox.prepare(&mut cmd, dir.path(), false);
        let names: Vec<String> = cmd
            .get_envs()
            .filter(|(_, v)| v.is_some())
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(!names.iter().any(|n| n.contains("AWS")), "{names:?}");
        for name in &names {
            let allowed = ENV_NAMES.contains(&name.as_str())
                || name.to_ascii_lowercase().ends_with("_proxy")
                || name == "SystemRoot";
            assert!(allowed, "{name}");
        }
        assert!(
            names.contains(&"HTTPS_PROXY".to_string()),
            "no network hint"
        );
    }
}

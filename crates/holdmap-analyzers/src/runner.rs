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
}

/// Facts about the run shared by every subject.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunContext {
    /// The approved, pinned executable (CLI analyzers).
    pub binary: Option<PathBuf>,
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
}

impl SandboxLevel {
    /// One-line description for the permissions sheet.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Basic => "Basic: it gets a copy of the folder, a cleared environment and a private temporary home, and is stopped at the time limit. The operating system does not block it from reading other files or using the network.",
        }
    }
}

/// Prepares the child process.
pub trait Sandbox: Send + Sync {
    /// The containment level.
    fn level(&self) -> SandboxLevel;
    /// Configure `cmd` to run with `home` as its private HOME/TMPDIR root.
    fn prepare(&self, cmd: &mut Command, home: &Path, network: bool);
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

    fn prepare(&self, cmd: &mut Command, home: &Path, network: bool) {
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

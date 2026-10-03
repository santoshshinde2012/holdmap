//! Platform backends (`cfg(target_os)`): socket enumeration, process identity and signalling.
//!
//! * Linux — `/proc/net/{tcp,tcp6,udp,udp6}` + `/proc/<pid>/fd` inode mapping,
//!   `pidfd_open(2)` / `pidfd_send_signal(2)` for race-free signalling.
//! * macOS — libproc (via the `netstat2` crate) for sockets, `proc_pidinfo(PROC_PIDTBSDINFO)`
//!   for start-time identity, `kill(2)`.
//! * Windows — `GetExtendedTcpTable`/`GetExtendedUdpTable` (via `netstat2`), `GetProcessTimes`
//!   identity, `taskkill` (graceful WM_CLOSE) then `TerminateProcess`.

use crate::model::RawSocket;
use std::io;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
use linux as imp;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod netstat_backend;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
use unsupported as imp;

/// Signals portwise sends. On Windows `Term` = graceful close request, `Kill` = TerminateProcess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Sig {
    /// Graceful termination request (SIGTERM).
    Term,
    /// Immediate kill (SIGKILL / TerminateProcess).
    Kill,
}

impl std::fmt::Display for Sig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Sig::Term => "SIGTERM",
            Sig::Kill => "SIGKILL",
        })
    }
}

/// Why a signal could not be delivered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SignalError {
    #[error("process has already exited")]
    /// The process has already exited.
    NotFound,
    #[error("PID was reused by a different process (start time changed); refusing to signal")]
    /// The PID now belongs to a different process.
    IdentityChanged,
    #[error("permission denied (process belongs to another user; elevation required)")]
    /// Not allowed to signal the process.
    PermissionDenied,
    #[error("{0}")]
    /// Any other OS error.
    Other(String),
}

/// Enumerate all TCP/UDP sockets (v4+v6, all states) with owning PIDs where visible.
pub fn list_sockets() -> io::Result<Vec<RawSocket>> {
    imp::list_sockets()
}

/// High-resolution process start token used to detect PID reuse.
pub fn start_token(pid: u32) -> Option<u64> {
    imp::start_token(pid)
}

/// Deliver `sig` to `pid` only if it is still the same process (`expected_token`).
pub fn signal(pid: u32, expected_token: u64, sig: Sig) -> Result<(), SignalError> {
    imp::signal(pid, expected_token, sig)
}

/// True if `pid` is alive *and* still has the expected start token.
pub fn is_alive(pid: u32, expected_token: u64) -> bool {
    match start_token(pid) {
        Some(t) => t == expected_token && !imp::is_zombie(pid),
        None => false,
    }
}

/// Name of this platform as used in JSON output.
pub fn platform_name() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}

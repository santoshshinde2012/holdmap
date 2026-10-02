//! macOS backend: sockets via libproc (`netstat2`), identity via `proc_pidinfo(PROC_PIDTBSDINFO)`
//! start time (µs resolution), signalling via `kill(2)` after an identity check.
//!
//! Limitation: unprivileged libproc cannot inspect other users' processes, so root-owned
//! listeners (e.g. system daemons) are invisible here; the explain engine detects them with a
//! bind probe and reports "owned by another user / root".

use super::{Sig, SignalError};
use crate::model::RawSocket;
use std::io;
use std::mem::MaybeUninit;

pub fn list_sockets() -> io::Result<Vec<RawSocket>> {
    super::netstat_backend::list_sockets()
}

fn bsdinfo(pid: u32) -> Option<libc::proc_bsdinfo> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: buffer is sized for proc_bsdinfo; libproc writes at most `size` bytes.
    let n = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if n == size {
        // SAFETY: fully initialised by the successful call above.
        Some(unsafe { info.assume_init() })
    } else {
        None
    }
}

pub fn start_token(pid: u32) -> Option<u64> {
    if let Some(info) = bsdinfo(pid) {
        return Some(info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec);
    }
    // Other users' processes: libproc refuses, but kill(pid, 0) tells us it exists.
    // Fall back to sysinfo's (second-resolution) start time in that case.
    // SAFETY: signal 0 performs only an existence/permission check.
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    let exists = rc == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
    if !exists {
        return None;
    }
    crate::process::sysinfo_start_time(pid)
}

pub fn is_zombie(pid: u32) -> bool {
    // SZOMB == 5 in <sys/proc.h>.
    bsdinfo(pid).is_some_and(|i| i.pbi_status == 5)
}

pub fn signal(pid: u32, expected_token: u64, sig: Sig) -> Result<(), SignalError> {
    if pid == 0 || pid > i32::MAX as u32 {
        return Err(SignalError::Other(format!("invalid pid {pid}")));
    }
    match start_token(pid) {
        None => return Err(SignalError::NotFound),
        Some(t) if t != expected_token => return Err(SignalError::IdentityChanged),
        Some(_) => {}
    }
    let signo = match sig {
        Sig::Term => libc::SIGTERM,
        Sig::Kill => libc::SIGKILL,
    };
    // SAFETY: kill with a validated pid; identity checked immediately before.
    let rc = unsafe { libc::kill(pid as libc::pid_t, signo) };
    if rc == 0 {
        return Ok(());
    }
    match io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Err(SignalError::NotFound),
        Some(libc::EPERM) => Err(SignalError::PermissionDenied),
        _ => Err(SignalError::Other(io::Error::last_os_error().to_string())),
    }
}

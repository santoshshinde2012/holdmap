//! Windows backend: sockets via `GetExtendedTcpTable`/`GetExtendedUdpTable` (`netstat2`),
//! identity via `GetProcessTimes` creation time, graceful stop via `taskkill` (posts WM_CLOSE to
//! GUI apps) and forced stop via `TerminateProcess` after an identity check.

use super::{Sig, SignalError};
use crate::model::RawSocket;
use std::io;
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, STILL_ACTIVE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, OpenProcess, TerminateProcess,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_INVALID_PARAMETER: i32 = 87;

pub fn list_sockets() -> io::Result<Vec<RawSocket>> {
    super::netstat_backend::list_sockets()
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: handle was returned by OpenProcess and is closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

fn open(pid: u32, access: u32) -> Result<Handle, i32> {
    // SAFETY: FFI call; a null handle signals failure.
    let h = unsafe { OpenProcess(access, 0, pid) };
    if h.is_null() {
        Err(io::Error::last_os_error().raw_os_error().unwrap_or(0))
    } else {
        Ok(Handle(h))
    }
}

fn creation_time(h: &Handle) -> Option<u64> {
    let mut c = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut e = c;
    let mut k = c;
    let mut u = c;
    // SAFETY: valid handle and out-pointers to stack FILETIMEs.
    let ok = unsafe { GetProcessTimes(h.0, &mut c, &mut e, &mut k, &mut u) };
    (ok != 0).then(|| ((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64)
}

pub fn start_token(pid: u32) -> Option<u64> {
    if pid == 0 || pid == 4 {
        return Some(0); // System Idle / System: no creation time, never signalled anyway.
    }
    let h = open(pid, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    if !still_active(&h) {
        return None;
    }
    creation_time(&h)
}

fn still_active(h: &Handle) -> bool {
    let mut code = 0u32;
    // SAFETY: valid handle, valid out pointer.
    let ok = unsafe { GetExitCodeProcess(h.0, &mut code) };
    ok != 0 && code == STILL_ACTIVE as u32
}

pub fn is_zombie(_pid: u32) -> bool {
    false
}

pub fn signal(pid: u32, expected_token: u64, sig: Sig) -> Result<(), SignalError> {
    if pid == 0 || pid == 4 {
        return Err(SignalError::Other(
            "refusing to signal a system process".into(),
        ));
    }
    match sig {
        Sig::Term => {
            match start_token(pid) {
                None => return Err(SignalError::NotFound),
                Some(t) if t != expected_token => return Err(SignalError::IdentityChanged),
                Some(_) => {}
            }
            // `taskkill` without /F asks the process to close (WM_CLOSE). Console apps usually
            // can't be closed this way; the executor then escalates to TerminateProcess.
            let out = Command::new("taskkill")
                .args(["/PID", &pid.to_string()])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| SignalError::Other(e.to_string()))?;
            if out.status.success() {
                Ok(())
            } else {
                Err(SignalError::Other(
                    String::from_utf8_lossy(&out.stderr).trim().to_string(),
                ))
            }
        }
        Sig::Kill => {
            let h = match open(pid, PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION) {
                Ok(h) => h,
                Err(ERROR_ACCESS_DENIED) => return Err(SignalError::PermissionDenied),
                Err(ERROR_INVALID_PARAMETER) => return Err(SignalError::NotFound),
                Err(e) => {
                    return Err(SignalError::Other(
                        io::Error::from_raw_os_error(e).to_string(),
                    ))
                }
            };
            // Holding the handle pins the process object, so the identity check below is
            // race-free (a PID cannot be reused while a handle to it is open).
            match creation_time(&h) {
                None => return Err(SignalError::NotFound),
                Some(t) if t != expected_token => return Err(SignalError::IdentityChanged),
                Some(_) => {}
            }
            // SAFETY: valid handle with PROCESS_TERMINATE access.
            let ok = unsafe { TerminateProcess(h.0, 1) };
            if ok != 0 {
                Ok(())
            } else {
                Err(SignalError::Other(io::Error::last_os_error().to_string()))
            }
        }
    }
}

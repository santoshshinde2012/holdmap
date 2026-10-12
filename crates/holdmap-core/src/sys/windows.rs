//! Windows backend: sockets via `GetExtendedTcpTable`/`GetExtendedUdpTable` (`netstat2`),
//! identity via `GetProcessTimes` creation time, graceful stop via `taskkill` (posts WM_CLOSE to
//! GUI apps) and forced stop via `TerminateProcess` after an identity check.

use super::{Sig, SignalError};
use crate::model::RawSocket;
use std::io;
use std::time::Duration;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, STILL_ACTIVE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessTimes, OpenProcess, TerminateProcess,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};

const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_INVALID_PARAMETER: i32 = 87;

/// Every TCP and UDP socket with its owning PID, from `GetExtendedTcpTable` /
/// `GetExtendedUdpTable` (via `netstat2`).
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
    (ok != 0).then_some(((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64)
}

/// Process creation time (`GetProcessTimes`), unique per PID incarnation; `None` once the process
/// has exited.
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

fn graceful_signal(
    pid: u32,
    expected_token: u64,
    run: impl FnOnce(&str, &[&str], Duration) -> Option<crate::util::CmdOutput>,
) -> Result<(), SignalError> {
    let identity = match open(pid, PROCESS_QUERY_LIMITED_INFORMATION) {
        Ok(handle) => handle,
        Err(ERROR_ACCESS_DENIED) => return Err(SignalError::PermissionDenied),
        Err(ERROR_INVALID_PARAMETER) => return Err(SignalError::NotFound),
        Err(error) => {
            return Err(SignalError::Other(
                io::Error::from_raw_os_error(error).to_string(),
            ))
        }
    };
    if !still_active(&identity) {
        return Err(SignalError::NotFound);
    }
    match creation_time(&identity) {
        None => return Err(SignalError::NotFound),
        Some(t) if t != expected_token => return Err(SignalError::IdentityChanged),
        Some(_) => {}
    }
    // `taskkill` without /F asks the process to close (WM_CLOSE). Console apps usually
    // can't be closed this way; the executor then escalates to TerminateProcess.
    // Keep the identity handle alive while taskkill resolves the PID; otherwise a
    // process exiting between the check and taskkill could release the PID for reuse.
    let out = run(
        "taskkill",
        &["/PID", &pid.to_string()],
        Duration::from_secs(5),
    )
    .ok_or_else(|| {
        SignalError::Other(
            "taskkill failed to start, exceeded its output limit or timed out after 5 seconds"
                .into(),
        )
    })?;
    if out.success {
        Ok(())
    } else {
        Err(SignalError::Other(crate::redact::line(out.stderr.trim())))
    }
}

/// Always false: Windows has no zombie processes.
pub fn is_zombie(_pid: u32) -> bool {
    false
}

/// Stop `pid` after checking its creation time still equals `expected_token`. [`Sig::Term`] runs
/// `taskkill` without `/F` (WM_CLOSE); [`Sig::Kill`] calls `TerminateProcess`.
pub fn signal(pid: u32, expected_token: u64, sig: Sig) -> Result<(), SignalError> {
    if pid == 0 || pid == 4 {
        return Err(SignalError::Other(
            "refusing to signal a system process".into(),
        ));
    }
    match sig {
        Sig::Term => graceful_signal(pid, expected_token, crate::util::run_with_timeout),
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

#[cfg(test)]
mod tests {
    use super::*;

    struct OwnedChild(std::process::Child);

    impl OwnedChild {
        fn start() -> Self {
            use std::io::Read;
            use std::os::windows::io::AsRawHandle;
            use std::os::windows::process::CommandExt;
            use std::process::{Command, Stdio};
            use std::time::Instant;
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "sys::windows::tests::owned_signal_fixture",
                    "--nocapture",
                ])
                .env("HOLDMAP_WINDOWS_SIGNAL_FIXTURE", "child")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
                .spawn()
                .unwrap();
            let mut fixture = Self(child);
            let mut output = fixture.0.stdout.take().unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut bytes = Vec::new();
            loop {
                assert!(
                    Instant::now() < deadline,
                    "owned child did not become ready"
                );
                assert!(
                    fixture.0.try_wait().unwrap().is_none(),
                    "owned fixture exited before readiness"
                );
                let mut available = 0;
                // SAFETY: output owns this live pipe handle; inspect availability before a
                // read so fixture startup cannot block the test beyond its deadline.
                assert_ne!(
                    unsafe {
                        windows_sys::Win32::System::Pipes::PeekNamedPipe(
                            output.as_raw_handle(),
                            std::ptr::null_mut(),
                            0,
                            std::ptr::null_mut(),
                            &mut available,
                            std::ptr::null_mut(),
                        )
                    },
                    0
                );
                if available > 0 {
                    let mut chunk = [0; 1024];
                    let count = output
                        .read(&mut chunk[..(available as usize).min(1024)])
                        .unwrap();
                    bytes.extend_from_slice(&chunk[..count]);
                    assert!(bytes.len() <= 4096, "unexpected owned fixture output");
                    if bytes
                        .windows(b"signal-fixture-ready".len())
                        .any(|value| value == b"signal-fixture-ready")
                    {
                        return fixture;
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }

        fn wait_for_exit(&mut self) {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            loop {
                if self.0.try_wait().unwrap().is_some() {
                    return;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "owned child did not exit"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while self.0.try_wait().is_ok_and(|status| status.is_none())
                && std::time::Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    #[test]
    fn owned_signal_fixture() {
        if std::env::var("HOLDMAP_WINDOWS_SIGNAL_FIXTURE").as_deref() != Ok("child") {
            return;
        }
        use std::io::Write;
        println!("signal-fixture-ready");
        std::io::stdout().flush().unwrap();
        // A watchdog also closes this owned fixture if its parent is interrupted.
        std::thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn real_windows_signals_refuse_stale_tokens_and_force_stop_owned_child() {
        let mut child = OwnedChild::start();
        let pid = child.0.id();
        let token = start_token(pid).expect("owned process creation time");
        let pinned = open(pid, PROCESS_QUERY_LIMITED_INFORMATION).unwrap();
        for sig in [Sig::Term, Sig::Kill] {
            assert_eq!(
                signal(pid, token.wrapping_add(1), sig),
                Err(SignalError::IdentityChanged)
            );
            assert!(child.0.try_wait().unwrap().is_none());
            assert!(still_active(&pinned));
        }
        signal(pid, token, Sig::Kill).unwrap();
        child.wait_for_exit();
        assert!(!still_active(&pinned));
        assert_eq!(
            creation_time(&pinned),
            Some(token),
            "open handles must retain the terminated process identity"
        );
        assert_eq!(start_token(pid), None);
    }

    #[test]
    fn real_windows_graceful_request_is_bounded_and_allows_forced_fallback() {
        let mut child = OwnedChild::start();
        let pid = child.0.id();
        let token = start_token(pid).expect("owned process creation time");
        let began = std::time::Instant::now();
        let result = signal(pid, token, Sig::Term);
        assert!(
            began.elapsed() < Duration::from_secs(8),
            "graceful taskkill exceeded its deadline"
        );
        assert!(
            matches!(result, Ok(()) | Err(SignalError::Other(_))),
            "an owned live process should pass the identity and permission guard"
        );
        // Console processes have no GUI WM_CLOSE handler, so a failed graceful request is
        // supported behavior. Verify the same pinned identity can be stopped by escalation.
        if child.0.try_wait().unwrap().is_none() {
            signal(pid, token, Sig::Kill).unwrap();
        }
        child.wait_for_exit();
        assert_eq!(start_token(pid), None);
    }

    #[test]
    fn changed_identity_never_invokes_taskkill() {
        let pid = std::process::id();
        let token = start_token(pid).expect("current process creation time");
        assert_eq!(
            graceful_signal(pid, token.wrapping_add(1), |_, _, _| {
                panic!("taskkill must not run for a reused PID")
            }),
            Err(SignalError::IdentityChanged)
        );
    }

    #[test]
    fn graceful_stop_bounds_taskkill_and_surfaces_failure() {
        let pid = std::process::id();
        let token = start_token(pid).expect("current process creation time");
        let result = graceful_signal(pid, token, |program, args, timeout| {
            assert_eq!(program, "taskkill");
            assert_eq!(args, ["/PID", &pid.to_string()]);
            assert_eq!(timeout, Duration::from_secs(5));
            None
        });
        assert!(
            matches!(result, Err(SignalError::Other(message)) if message.contains("timed out after 5 seconds"))
        );
    }

    #[test]
    fn graceful_stop_redacts_command_failure_credentials() {
        let pid = std::process::id();
        let token = start_token(pid).expect("current process creation time");
        let result = graceful_signal(pid, token, |_, _, _| {
            Some(crate::util::CmdOutput {
                success: false,
                stdout: String::new(),
                stderr: "password=private-credential".into(),
            })
        });
        assert_eq!(
            result,
            Err(SignalError::Other(format!(
                "password={}",
                crate::redact::MASK
            )))
        );
    }
}

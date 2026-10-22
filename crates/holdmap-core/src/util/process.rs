//! Running external commands: timeouts, the platform shell and detached children.

#[allow(unused_imports)]
use super::*;
#[allow(unused_imports)]
use std::io::Read;
#[allow(unused_imports)]
use std::process::{Command, Stdio};
#[allow(unused_imports)]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Output of a finished command.
#[derive(Debug, Clone)]
pub struct CmdOutput {
    /// True when the command exited with status 0.
    pub success: bool,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
}

/// Run a command, killing it if it exceeds `timeout`. Returns `None` if it couldn't start or
/// timed out or produced more than 4 MiB on either stream. The timeout covers inherited
/// pipes too, even if the direct child exits first. Never inherits stdin.
pub fn run_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<CmdOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;
    let deadline = Instant::now() + timeout;
    let stdout = child.stdout.take()?;
    let stderr = child.stderr.take()?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        for fd in [stdout.as_raw_fd(), stderr.as_raw_fd()] {
            // SAFETY: these are the live pipe descriptors owned by stdout/stderr.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    #[cfg(windows)]
    let handles = {
        use std::os::windows::io::AsRawHandle;
        (
            stdout.as_raw_handle() as usize,
            stderr.as_raw_handle() as usize,
        )
    };
    #[cfg(not(windows))]
    let handles = (0, 0);
    // Drain both pipes while waiting: a child that writes more than the pipe buffer (64 KB on
    // Linux) would otherwise block forever and only end at the timeout.
    let read = |mut pipe: Box<dyn Read + Send>, pipe_handle: usize| {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut chunk = [0; 8192];
            #[cfg(not(windows))]
            let _ = pipe_handle;
            loop {
                if Instant::now() >= deadline {
                    let _ = sender.send(None);
                    return;
                }
                #[cfg(windows)]
                {
                    let mut available = 0;
                    // SAFETY: pipe owns this handle for the closure lifetime; only query the
                    // byte count, avoiding a blocking read while a descendant holds it open.
                    if unsafe {
                        windows_sys::Win32::System::Pipes::PeekNamedPipe(
                            pipe_handle as windows_sys::Win32::Foundation::HANDLE,
                            std::ptr::null_mut(),
                            0,
                            std::ptr::null_mut(),
                            &mut available,
                            std::ptr::null_mut(),
                        )
                    } == 0
                    {
                        use windows_sys::Win32::Foundation::{
                            GetLastError, ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED,
                        };
                        // SAFETY: read this thread's error immediately after the failed call.
                        let error = unsafe { GetLastError() };
                        let output = matches!(error, ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED)
                            .then(|| String::from_utf8_lossy(&buf).into_owned());
                        let _ = sender.send(output);
                        return;
                    }
                    if available == 0 {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                }
                match pipe.read(&mut chunk) {
                    Ok(0) => {
                        let _ = sender.send(Some(String::from_utf8_lossy(&buf).into_owned()));
                        return;
                    }
                    Ok(n) if buf.len() + n <= 4 * 1024 * 1024 => buf.extend_from_slice(&chunk[..n]),
                    Ok(_) => {
                        let _ = sender.send(None);
                        return;
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => {
                        let _ = sender.send(None);
                        return;
                    }
                }
            }
        });
        receiver
    };
    let out = read(Box::new(stdout), handles.0);
    let err = read(Box::new(stderr), handles.1);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let status = status?;
    Some(CmdOutput {
        success: status.success(),
        stdout: out
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok()??,
        stderr: err
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok()??,
    })
}

/// Find an executable on PATH (plus the usual Homebrew / local bins a GUI app doesn't see).
/// Only bare names or absolute paths; never a relative path from the current directory.
pub fn which(name: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(name);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    if name.contains(['/', '\\']) || name.is_empty() {
        return None;
    }
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    dirs.extend(
        ["/opt/homebrew/bin", "/usr/local/bin"]
            .iter()
            .map(std::path::PathBuf::from),
    );
    dirs.push(home_dir().join(".local/bin"));
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ""]
    } else {
        &[""]
    };
    dirs.iter().filter(|d| d.is_absolute()).find_map(|d| {
        exts.iter()
            .map(|e| d.join(format!("{name}{e}")))
            .find(|c| c.is_file())
    })
}

/// A [`std::process::Command`] that runs `line` through the platform shell (`sh -c` on Unix,
/// `cmd /D /S /C` on Windows), so stack files can use pipes, `&&` and native shell quoting.
/// `line` is an authorized shell command, not an escaped argument vector.
pub fn shell_command(line: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("cmd");
        // cmd.exe does not use C argv escaping: Command::arg would insert literal
        // backslashes before inner quotes. /S removes this outer pair and preserves
        // the command's own quotes; /D disables unrelated registry AutoRun commands.
        c.args(["/D", "/S", "/C"]).raw_arg(format!("\"{line}\""));
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", line]);
        c
    }
}

#[cfg(all(test, windows))]
mod windows_shell_tests {
    use super::*;

    fn detached_ready(directory: &std::path::Path) -> Option<(u32, u64, u16)> {
        let values = std::fs::read_to_string(directory.join("ready")).ok()?;
        let mut values = values.split_whitespace();
        Some((
            values.next()?.parse().ok()?,
            values.next()?.parse().ok()?,
            values.next()?.parse().ok()?,
        ))
    }

    #[test]
    #[ignore = "internal finite Windows detached TCP service fixture"]
    fn detached_listener_fixture() {
        let argv: Vec<_> = std::env::args_os().collect();
        let start = argv.iter().position(|arg| arg == "--").unwrap() + 1;
        let directory = std::path::Path::new(&argv[start]);
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let pid = std::process::id();
        let token = crate::sys::start_token(pid).unwrap();
        println!("detached standard output");
        eprintln!("detached standard error");
        std::fs::write(
            directory.join("ready"),
            format!("{pid} {token} {}", listener.local_addr().unwrap().port()),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline && !directory.join("quit").exists() {
            let _ = listener.accept();
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "internal Windows launcher with inherited output pipes"]
    fn detached_launcher_fixture() {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::Foundation::{
            GetHandleInformation, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT,
        };
        use windows_sys::Win32::System::Console::{
            AllocConsole, FreeConsole, GetConsoleMode, GetStdHandle, SetStdHandle,
            STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };
        let argv: Vec<_> = std::env::args_os().collect();
        let start = argv.iter().position(|arg| arg == "--").unwrap() + 1;
        let mode = argv[start].to_str().unwrap();
        let directory = std::path::Path::new(&argv[start + 1]);
        if mode == "console" {
            use std::os::windows::io::AsRawHandle;
            // This isolated helper first detaches any runner-provided console; freeing
            // an unattached process also succeeds. It then owns its new test console.
            assert_ne!(
                unsafe { FreeConsole() },
                0,
                "{}",
                std::io::Error::last_os_error()
            );
            assert_ne!(
                unsafe { AllocConsole() },
                0,
                "{}",
                std::io::Error::last_os_error()
            );
            let kinds = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];
            let originals = kinds.map(|kind| unsafe { GetStdHandle(kind) });
            // STARTF_USESTDHANDLES can preserve the original redirected pipes after
            // AllocConsole. Explicit console devices ensure this exercises real buffers.
            let input = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("CONIN$")
                .unwrap();
            let output = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("CONOUT$")
                .unwrap();
            for (kind, handle) in kinds.into_iter().zip([
                input.as_raw_handle(),
                output.as_raw_handle(),
                output.as_raw_handle(),
            ]) {
                assert_ne!(unsafe { SetStdHandle(kind, handle) }, 0);
                assert_ne!(
                    unsafe {
                        SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT)
                    },
                    0
                );
            }
            let handles = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].map(|kind| {
                let handle = unsafe { GetStdHandle(kind) };
                let mut mode = 0;
                assert_ne!(unsafe { GetConsoleMode(handle, &mut mode) }, 0);
                let mut flags = 0;
                assert_ne!(unsafe { GetHandleInformation(handle, &mut flags) }, 0);
                (handle, flags)
            });
            let guard = WindowsStdioInheritance::exclude().unwrap();
            drop(guard);
            for (handle, original) in handles {
                let mut flags = 0;
                assert_ne!(unsafe { GetHandleInformation(handle, &mut flags) }, 0);
                assert_eq!(flags, original);
            }
            for (kind, handle) in kinds.into_iter().zip(originals) {
                assert_ne!(unsafe { SetStdHandle(kind, handle) }, 0);
            }
            drop((input, output));
            assert_ne!(unsafe { FreeConsole() }, 0);
            std::fs::write(directory.join("console-verified"), "verified").unwrap();
            return;
        }
        struct RestoreError(HANDLE, u32);
        impl Drop for RestoreError {
            fn drop(&mut self) {
                // SAFETY: restoring the still-live original standard error handle.
                unsafe {
                    SetHandleInformation(self.0, HANDLE_FLAG_INHERIT, self.1);
                    SetStdHandle(STD_ERROR_HANDLE, self.0);
                }
            }
        }
        // Exercise the shared stdout/stderr handle case inside this isolated helper only.
        let original_error = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
        let mut original_flags = 0;
        assert_ne!(
            unsafe { GetHandleInformation(original_error, &mut original_flags) },
            0
        );
        let _error = RestoreError(original_error, original_flags);
        // Replacing a standard handle does not close it. This saved, now nonstandard
        // pipe must not be inheritable: a real shared-stream launcher has no such pipe.
        assert_ne!(
            unsafe { SetHandleInformation(original_error, HANDLE_FLAG_INHERIT, 0) },
            0
        );
        assert_ne!(
            unsafe { SetStdHandle(STD_ERROR_HANDLE, GetStdHandle(STD_OUTPUT_HANDLE)) },
            0
        );
        let flags = || {
            [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].map(|kind| {
                let mut flags = 0;
                // SAFETY: querying this process's live standard handle into writable memory.
                assert_ne!(
                    unsafe { GetHandleInformation(GetStdHandle(kind), &mut flags) },
                    0
                );
                flags
            })
        };
        let before = flags();
        let log = directory.join("service.log");
        if mode == "fixed" {
            // Another thread's ordinary inherited stdio still uses inheritable duplicates
            // while the detached-spawn guard excludes only the original handles.
            let guard = WindowsStdioInheritance::exclude().unwrap();
            assert!(std::thread::spawn(|| Command::new("cmd")
                .args(["/D", "/C", "echo concurrent normal inherited output"])
                .stdin(Stdio::null())
                .status()
                .unwrap())
            .join()
            .unwrap()
            .success());
            drop(guard);
            assert_eq!(flags(), before);
        }
        if mode == "failure" {
            let mut command = Command::new(directory.join("missing-owned-program.exe"));
            assert!(spawn_detached(&mut command, &log).is_err());
            assert_eq!(flags(), before, "failed spawn must restore original flags");
        } else {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--ignored",
                    "--exact",
                    "util::process::windows_shell_tests::detached_listener_fixture",
                    "--nocapture",
                    "--",
                ])
                .arg(directory);
            let child = if mode == "legacy" {
                // Negative control reproduces the prior spawn while redirecting the child's
                // selected stdio correctly: the original launcher pipes still get inherited.
                let output = create_private(&log).unwrap();
                let error = output.try_clone().unwrap();
                command
                    .stdin(Stdio::null())
                    .stdout(Stdio::from(output))
                    .stderr(Stdio::from(error))
                    .creation_flags(0x0800_0000 | 0x0000_0200);
                Some(command.spawn().unwrap())
            } else if mode == "quiet" {
                spawn_quiet(command).unwrap();
                assert_eq!(flags(), before, "quiet spawn must restore original flags");
                None
            } else {
                let child = spawn_detached(&mut command, &log).unwrap();
                assert_eq!(
                    flags(),
                    before,
                    "successful spawn must restore original flags"
                );
                Some(child)
            };
            let deadline = Instant::now() + Duration::from_secs(3);
            while detached_ready(directory).is_none() {
                assert!(
                    Instant::now() < deadline,
                    "owned detached child never became ready"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            drop(child);
        }
        println!("launcher returned");
        eprintln!("launcher standard error restored");
    }

    #[test]
    fn windows_detached_service_does_not_keep_launcher_output_pipes_open() {
        let directory = tempfile::tempdir().unwrap();
        struct Cleanup(Vec<std::path::PathBuf>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                for directory in &self.0 {
                    let _ = std::fs::write(directory.join("quit"), "quit");
                    if let Some((pid, token, _)) = detached_ready(directory) {
                        let deadline = Instant::now() + Duration::from_secs(2);
                        while crate::sys::start_token(pid) == Some(token)
                            && Instant::now() < deadline
                        {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        let _ = crate::sys::signal(pid, token, crate::sys::Sig::Kill);
                    }
                }
            }
        }
        let mut cleanup = Cleanup(vec![]);
        let executable = std::env::current_exe().unwrap();
        for mode in ["legacy", "fixed", "quiet", "failure", "console"] {
            let working = directory.path().join(mode);
            std::fs::create_dir(&working).unwrap();
            cleanup.0.push(working.clone());
            let budget = if mode == "legacy" {
                Duration::from_secs(1)
            } else {
                Duration::from_secs(5)
            };
            let output = run_with_timeout(
                executable.to_str().unwrap(),
                &[
                    "--ignored",
                    "--exact",
                    "util::process::windows_shell_tests::detached_launcher_fixture",
                    "--nocapture",
                    "--",
                    mode,
                    working.to_str().unwrap(),
                ],
                budget,
            );
            if mode == "legacy" {
                assert!(
                    output.is_none(),
                    "negative control must retain output pipes"
                );
            } else {
                let output = output.unwrap_or_else(|| {
                    panic!(
                        "launcher pipes must close before service exit; mode={mode}; ready={:?}; log={:?}",
                        detached_ready(&working),
                        std::fs::read_to_string(working.join("service.log"))
                    )
                });
                assert!(output.success, "{output:?}");
                if mode == "console" {
                    assert!(working.join("console-verified").exists());
                    continue;
                }
                assert!(output.stdout.contains("launcher returned"));
                assert!(output.stdout.contains("launcher standard error restored"));
                if mode == "fixed" {
                    assert!(output.stdout.contains("concurrent normal inherited output"));
                }
            }
            if mode != "failure" {
                let (pid, token, port) = detached_ready(&working).unwrap();
                assert_eq!(crate::sys::start_token(pid), Some(token));
                assert!(
                    std::net::TcpStream::connect_timeout(
                        &([127, 0, 0, 1], port).into(),
                        Duration::from_millis(500)
                    )
                    .is_ok(),
                    "detached service must remain live after launcher completion"
                );
                if mode != "quiet" {
                    let log = std::fs::read_to_string(working.join("service.log")).unwrap();
                    assert!(log.contains("detached standard output"));
                    assert!(log.contains("detached standard error"));
                }
                std::fs::write(working.join("quit"), "quit").unwrap();
            }
        }
    }

    fn bounded_status(mut command: Command) -> std::process::ExitStatus {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("owned shell fixture exceeded its deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "internal finite Windows shell subprocess fixture"]
    fn shell_fixture() {
        let marker = std::env::var_os("HOLDMAP_SHELL_TEST_MARKER").expect("parent supplied marker");
        std::fs::write(marker, "executed").unwrap();
    }

    #[test]
    fn windows_shell_preserves_quoted_executable_paths_and_shell_operators() {
        let directory = tempfile::tempdir().unwrap();
        let spaced = directory.path().join("owned shell space & literal");
        std::fs::create_dir(&spaced).unwrap();
        let exe = spaced.join("fixture.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let marker = directory.path().join("executed.txt");
        let output = directory.path().join("output.txt");
        let line = format!(
            r#""{}" --ignored --exact util::process::windows_shell_tests::shell_fixture --nocapture && echo literal ^& safe > "%HOLDMAP_SHELL_TEST_OUTPUT%""#,
            exe.display()
        );

        // Negative control: ordinary argv escaping changes native cmd syntax and cannot
        // launch this quoted executable. The production helper must preserve the text.
        let mut escaped = Command::new("cmd");
        escaped
            .args(["/D", "/S", "/C", &line])
            .env("HOLDMAP_SHELL_TEST_MARKER", &marker)
            .env("HOLDMAP_SHELL_TEST_OUTPUT", &output);
        assert!(!bounded_status(escaped).success());
        assert!(!marker.exists());
        assert!(!output.exists());

        let mut command = shell_command(&line);
        command
            .env("HOLDMAP_SHELL_TEST_MARKER", &marker)
            .env("HOLDMAP_SHELL_TEST_OUTPUT", &output);
        assert!(bounded_status(command).success());
        assert_eq!(std::fs::read_to_string(marker).unwrap(), "executed");
        assert_eq!(
            std::fs::read_to_string(output).unwrap().trim(),
            "literal & safe"
        );
    }
}

/// Spawn `cmd` detached from the caller: stdin closed, stdout and stderr written to `log`
/// (created or truncated), and on Unix in its own process group so it survives the launching
/// terminal's Ctrl-C and holdmap exiting.
pub fn spawn_detached(
    cmd: &mut std::process::Command,
    log: &std::path::Path,
) -> std::io::Result<std::process::Child> {
    use std::process::Stdio;
    if let Some(dir) = log.parent() {
        create_private_dir(dir)?;
    }
    // Logs can hold whatever the program prints (tokens, connection strings): owner-only.
    let out = create_private(log)?;
    let err = out.try_clone()?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    // Windows inherits every inheritable handle, including the launcher's original
    // output pipes even when the child's standard streams are redirected to logs.
    // Exclude those originals while std duplicates the selected null/log handles.
    #[cfg(windows)]
    let stdio_inheritance = WindowsStdioInheritance::exclude()?;
    let child = cmd.spawn()?;
    #[cfg(windows)]
    drop(stdio_inheritance);
    // A PID-only exemption would outlive this service and apply to a future reused PID.
    // Fail closed if identity cannot be captured; an unverified child remains protected.
    if let Some(token) = crate::sys::start_token(child.id()).filter(|token| *token != 0) {
        DETACHED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(child.id(), token);
    }
    Ok(child)
}

#[cfg(windows)]
pub(crate) struct WindowsStdioInheritance {
    handles: Vec<(windows_sys::Win32::Foundation::HANDLE, u32)>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(windows)]
impl WindowsStdioInheritance {
    pub(crate) fn exclude() -> std::io::Result<Self> {
        use windows_sys::Win32::Foundation::{
            GetHandleInformation, SetHandleInformation, ERROR_INVALID_HANDLE, HANDLE_FLAG_INHERIT,
            INVALID_HANDLE_VALUE,
        };
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        };
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let mut guard = Self {
            handles: Vec::with_capacity(3),
            _lock: LOCK.lock().unwrap_or_else(|error| error.into_inner()),
        };
        let mut seen = Vec::with_capacity(3);
        for kind in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            // SAFETY: querying this process's standard handles does not take ownership.
            let handle = unsafe { GetStdHandle(kind) };
            if handle.is_null() || handle == INVALID_HANDLE_VALUE || seen.contains(&handle) {
                continue;
            }
            seen.push(handle);
            let mut flags = 0;
            // SAFETY: the standard handle remains owned by this process, and flags is writable.
            if unsafe { GetHandleInformation(handle, &mut flags) } == 0 {
                let error = std::io::Error::last_os_error();
                // GetStdHandle can retain a closed handle value in console-less hosts.
                // It cannot be inherited, so it needs no exclusion or restoration.
                if error.raw_os_error() == Some(ERROR_INVALID_HANDLE as i32) {
                    continue;
                }
                return Err(error);
            }
            if flags & HANDLE_FLAG_INHERIT != 0 {
                // Save before changing so any later failure restores already changed handles.
                guard.handles.push((handle, flags));
                // Other Rust Command spawns duplicate their selected stdio with an
                // inheritable copy, independently of this original handle's bit.
                // SAFETY: only the inheritance bit changes; the handle is not closed or replaced.
                if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
        }
        Ok(guard)
    }
}

#[cfg(windows)]
impl Drop for WindowsStdioInheritance {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
        for &(handle, flags) in self.handles.iter().rev() {
            // SAFETY: these are the original live standard handles, with their original bit.
            unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, flags) };
        }
    }
}

/// Process identities started on the user's behalf with [`spawn_detached`] (a restarted dev
/// server). They're the user's services, not part of holdmap, so self-protection skips them.
pub(crate) static DETACHED: std::sync::Mutex<std::collections::BTreeMap<u32, u64>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Was `pid` started detached by this process (see [`spawn_detached`])?
pub fn started_detached(pid: u32) -> bool {
    let mut records = DETACHED.lock().unwrap_or_else(|e| e.into_inner());
    let Some(token) = records.get(&pid).copied() else {
        return false;
    };
    if token != 0 && crate::sys::start_token(pid) == Some(token) {
        true
    } else {
        records.remove(&pid);
        false
    }
}

pub(crate) fn detached_processes(
    processes: &std::collections::HashMap<u32, crate::model::ProcessInfo>,
) -> std::collections::HashSet<u32> {
    retained_detached(
        &mut DETACHED.lock().unwrap_or_else(|e| e.into_inner()),
        processes,
        crate::sys::start_token,
    )
}

pub(crate) fn retained_detached(
    records: &mut std::collections::BTreeMap<u32, u64>,
    processes: &std::collections::HashMap<u32, crate::model::ProcessInfo>,
    live_token: impl Fn(u32) -> Option<u64>,
) -> std::collections::HashSet<u32> {
    let mut detached = std::collections::HashSet::new();
    records.retain(|pid, token| {
        if *token == 0 {
            return false;
        }
        if processes
            .get(pid)
            .is_some_and(|process| process.start_token == *token)
        {
            detached.insert(*pid);
            return true;
        }
        // A service can start after this process snapshot was collected. Retain its record
        // if it is still alive, but grant no exemption to a different snapshot identity.
        live_token(*pid) == Some(*token)
    });
    detached
}

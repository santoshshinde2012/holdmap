//! Local machine power requests, isolated from process stopping and remote hosts.
//!
//! The native app origin is the authority boundary. Preview handles bind a fixed local OS
//! request; they are short-lived, one-use review handles, not authentication credentials.

use serde::Serialize;
use std::collections::VecDeque;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Manager, WebviewWindow};

const LIFETIME: Duration = Duration::from_secs(60);
const CAPACITY: usize = 8;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const STDERR_LIMIT: u64 = 8192;
const REVIEW_AGAIN: &str =
    "Shutdown confirmation expired or was already used. Review shutdown again.";

#[derive(Debug, Serialize)]
pub struct ShutdownPreview {
    confirmation_id: String,
    platform: &'static str,
    hostname: String,
    expires_in_secs: u64,
}

#[derive(Debug, Serialize)]
pub struct ShutdownRequested {
    requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandSpec {
    executable: PathBuf,
    args: &'static [&'static str],
}

struct Pending {
    id: String,
    at: Instant,
    command: CommandSpec,
}

#[derive(Default)]
struct Confirmations {
    next_id: u64,
    pending: VecDeque<Pending>,
    in_flight: bool,
    requested: bool,
}

enum RequestFailure {
    Refused(String),
    OutcomeUnknown,
}

#[derive(Default)]
pub struct PowerState {
    confirmations: Mutex<Confirmations>,
}

impl PowerState {
    fn preview(
        &self,
        command: CommandSpec,
        platform: &'static str,
        hostname: String,
        now: Instant,
    ) -> Result<ShutdownPreview, String> {
        let mut state = self.confirmations.lock().unwrap_or_else(|e| e.into_inner());
        if state.in_flight || state.requested {
            return Err("A shutdown request is already in progress.".into());
        }
        state
            .pending
            .retain(|p| now.saturating_duration_since(p.at) < LIFETIME);
        while state.pending.len() >= CAPACITY {
            state.pending.pop_front();
        }
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or("Shutdown preview unavailable. Restart Holdmap.")?;
        let id = format!("shutdown-{}", state.next_id);
        state.pending.push_back(Pending {
            id: id.clone(),
            at: now,
            command,
        });
        Ok(ShutdownPreview {
            confirmation_id: id,
            platform,
            hostname,
            expires_in_secs: LIFETIME.as_secs(),
        })
    }

    /// The executor is injected so every test runs without invoking a power command.
    fn request(
        &self,
        id: &str,
        now: Instant,
        execute: impl FnOnce(&CommandSpec) -> Result<(), RequestFailure>,
    ) -> Result<ShutdownRequested, String> {
        let command = {
            let mut state = self.confirmations.lock().unwrap_or_else(|e| e.into_inner());
            if state.in_flight || state.requested {
                return Err("A shutdown request is already in progress.".into());
            }
            state
                .pending
                .retain(|p| now.saturating_duration_since(p.at) < LIFETIME);
            let index = state
                .pending
                .iter()
                .position(|p| p.id == id)
                .ok_or(REVIEW_AGAIN)?;
            let pending = state.pending.remove(index).ok_or(REVIEW_AGAIN)?;
            // Older dialogs cannot authorize a second shutdown after this request.
            state.pending.clear();
            state.in_flight = true;
            pending.command
        };
        let result = execute(&command);
        let mut state = self.confirmations.lock().unwrap_or_else(|e| e.into_inner());
        state.in_flight = false;
        match result {
            Ok(()) => {
                state.requested = true;
                Ok(ShutdownRequested { requested: true })
            }
            Err(RequestFailure::Refused(error)) => Err(error),
            Err(RequestFailure::OutcomeUnknown) => {
                // Killing the client does not undo a shutdown already accepted by the OS.
                // Refuse another request while its outcome is uncertain.
                state.requested = true;
                Err("The shutdown request timed out and its outcome is unknown. The operating system may still shut down. Do not retry; restart Holdmap only after you have checked the machine's state.".into())
            }
        }
    }
}

fn trusted_origin(label: &str, url: &tauri::Url, dev_url: Option<&tauri::Url>) -> bool {
    if label != "main" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let packaged = matches!(
        (url.scheme(), url.host_str(), url.port()),
        ("tauri", Some("localhost"), None) | ("http" | "https", Some("tauri.localhost"), None)
    );
    packaged || dev_url.is_some_and(|dev| dev.origin() == url.origin())
}

fn authorize(window: &WebviewWindow) -> Result<(), String> {
    let url = window
        .url()
        .map_err(|_| "Couldn't verify the local app window.")?;
    // Development is restricted to the configured dev origin; release binaries never
    // authorize arbitrary localhost pages. Capability settings also allow local main only.
    let dev_url = if cfg!(debug_assertions) {
        window.app_handle().config().build.dev_url.as_ref()
    } else {
        None
    };
    if !trusted_origin(window.label(), &url, dev_url) {
        return Err("Shutdown is available only from the local Holdmap app window.".into());
    }
    Ok(())
}

fn command_for(platform: &str, windows_system_dir: Option<PathBuf>) -> Result<CommandSpec, String> {
    match platform {
        // Apple System Events' Power Suite declares this command. No shell, administrator
        // elevation, or force event is added. OS automation permission may be required.
        "macos" => Ok(CommandSpec {
            executable: "/usr/bin/osascript".into(),
            args: &[
                "-e",
                "tell application \"/System/Library/CoreServices/System Events.app\" to shut down",
            ],
        }),
        // Microsoft documents that a positive /t implies /f. Zero keeps force disabled.
        "windows" => {
            let directory = windows_system_dir
                .filter(|path| path.is_absolute())
                .ok_or("Couldn't locate the Windows system directory.")?;
            Ok(CommandSpec {
                executable: directory.join("shutdown.exe"),
                args: &["/s", "/t", "0"],
            })
        }
        // systemd >=248: explicit checking is essential for non-interactive invocations.
        // Missing systemd/unsupported flags fail closed; there is no force fallback.
        "linux" => Ok(CommandSpec {
            executable: "/usr/bin/systemctl".into(),
            args: &["--check-inhibitors=yes", "poweroff"],
        }),
        _ => Err("Shutdown is not supported on this operating system.".into()),
    }
}

fn local_command() -> Result<CommandSpec, String> {
    #[cfg(target_os = "windows")]
    let directory = Some(windows_system_directory()?);
    #[cfg(not(target_os = "windows"))]
    let directory = None;
    let spec = command_for(std::env::consts::OS, directory)?;
    if !spec.executable.is_file() {
        return Err("The operating system shutdown service is unavailable on this machine.".into());
    }
    Ok(spec)
}

#[cfg(target_os = "windows")]
fn windows_system_directory() -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
    }
    let mut buffer = [0_u16; 32768];
    // SAFETY: the OS receives a writable buffer and its exact element count.
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err("Couldn't locate the Windows system directory.".into());
    }
    Ok(std::ffi::OsString::from_wide(&buffer[..length]).into())
}

fn local_hostname() -> Result<String, String> {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn gethostname(name: *mut std::ffi::c_char, length: usize) -> std::ffi::c_int;
        }
        let mut buffer = [0_u8; 256];
        // SAFETY: gethostname receives a writable byte buffer and its length. We locate
        // the terminator inside the initialized buffer instead of reading past it.
        if unsafe { gethostname(buffer.as_mut_ptr().cast(), buffer.len()) } != 0 {
            return Err("Couldn't identify this local machine.".into());
        }
        let length = buffer
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(buffer.len());
        hostname_label(&String::from_utf8_lossy(&buffer[..length]))
    }
    #[cfg(target_os = "windows")]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetComputerNameW(buffer: *mut u16, size: *mut u32) -> i32;
        }
        let mut buffer = [0_u16; 256];
        let mut length = buffer.len() as u32;
        // SAFETY: both output pointers are valid and size describes the writable buffer.
        if unsafe { GetComputerNameW(buffer.as_mut_ptr(), &mut length) } == 0
            || length as usize > buffer.len()
        {
            return Err("Couldn't identify this local machine.".into());
        }
        hostname_label(&String::from_utf16_lossy(&buffer[..length as usize]))
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    Err("Couldn't identify this local machine.".into())
}

fn hostname_label(hostname: &str) -> Result<String, String> {
    let label = holdmap_core::util::printable(hostname).trim().to_owned();
    if label.is_empty() {
        Err("Couldn't identify this local machine.".into())
    } else {
        Ok(label)
    }
}

fn system_process(spec: &CommandSpec) -> Command {
    let mut command = Command::new(&spec.executable);
    command
        .args(spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    // Use the local OS bus, never a custom/remote address inherited from the launcher.
    command
        .env_remove("DBUS_SYSTEM_BUS_ADDRESS")
        .env_remove("SYSTEMD_BUS_ADDRESS");
    command
}

fn execute_system(spec: &CommandSpec) -> Result<(), RequestFailure> {
    let mut child = system_process(spec).stderr(Stdio::piped()).spawn().map_err(|error| {
        RequestFailure::Refused(match error.kind() {
            std::io::ErrorKind::PermissionDenied => "The operating system denied permission to shut down. Check your system permissions and review shutdown again.".into(),
            std::io::ErrorKind::NotFound => "The operating system shutdown service is unavailable on this machine.".into(),
            _ => "Couldn't request shutdown from the operating system. Review shutdown again to retry.".into(),
        })
    })?;
    let deadline = Instant::now() + REQUEST_TIMEOUT;
    let stderr = child.stderr.take().ok_or(RequestFailure::OutcomeUnknown)?;
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stderr
            .take(STDERR_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = send.send(result);
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                // This stops only the request client. It cannot cancel an accepted OS action.
                let _ = child.kill();
                let _ = child.wait();
                return Err(RequestFailure::OutcomeUnknown);
            }
        }
    };
    let bytes = match receive.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(Ok(bytes)) if bytes.len() as u64 <= STDERR_LIMIT => bytes,
        _ => return Err(RequestFailure::OutcomeUnknown),
    };
    if status.success() {
        return Ok(());
    }
    Err(RequestFailure::Refused(refusal_message(
        status.code(),
        &bytes,
    )))
}

fn refusal_message(code: Option<i32>, stderr: &[u8]) -> String {
    let error = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    if error.contains("-128") || error.contains("cancel") {
        "Shutdown was cancelled by the operating system. Review shutdown again to retry.".into()
    } else if error.contains("-1743")
        || error.contains("permission")
        || error.contains("not authorized")
        || error.contains("access is denied")
        || code == Some(5)
    {
        "The operating system denied permission to shut down. Check your system permissions and review shutdown again.".into()
    } else if error.contains("inhibit") {
        "An application or another user is blocking shutdown. Save your work and review shutdown again.".into()
    } else {
        "The operating system did not accept shutdown. It may be blocked by an application, another user, or system policy. Review shutdown again to retry.".into()
    }
}

#[tauri::command]
pub async fn shutdown_preview(window: WebviewWindow) -> Result<ShutdownPreview, String> {
    authorize(&window)?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let command = local_command()?;
        let hostname = local_hostname()?;
        app.state::<PowerState>()
            .preview(command, std::env::consts::OS, hostname, Instant::now())
    })
    .await
    .map_err(|_| "Couldn't prepare shutdown. Review shutdown again.".to_owned())?
}

#[tauri::command]
pub async fn shutdown_machine(
    window: WebviewWindow,
    confirmation_id: String,
) -> Result<ShutdownRequested, String> {
    authorize(&window)?;
    if confirmation_id.is_empty() || confirmation_id.len() > 64 {
        return Err(REVIEW_AGAIN.into());
    }
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<PowerState>()
            .request(&confirmation_id, Instant::now(), execute_system)
    })
    .await
    .map_err(|_| "Couldn't request shutdown. Review shutdown again.".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> CommandSpec {
        CommandSpec {
            executable: "/never-execute-fixture".into(),
            args: &[],
        }
    }
    fn preview(state: &PowerState, now: Instant) -> ShutdownPreview {
        state
            .preview(fixture(), "fixture", "local-fixture".into(), now)
            .unwrap()
    }

    #[test]
    fn preview_is_local_typed_and_does_not_execute() {
        let state = PowerState::default();
        let output = serde_json::to_value(preview(&state, Instant::now())).unwrap();
        assert_eq!(output["hostname"], "local-fixture");
        assert_eq!(output["platform"], "fixture");
        assert_eq!(output["expires_in_secs"], 60);
        assert!(output["confirmation_id"]
            .as_str()
            .unwrap()
            .starts_with("shutdown-"));
    }

    #[test]
    fn missing_expired_and_evicted_handles_never_reach_executor() {
        let state = PowerState::default();
        let now = Instant::now();
        let first = preview(&state, now);
        let deny = |_: &CommandSpec| -> Result<(), RequestFailure> {
            panic!("invalid preview invoked executor")
        };
        assert!(state.request("missing", now, deny).is_err());
        assert!(state
            .request(&first.confirmation_id, now + LIFETIME, deny)
            .is_err());
        let oldest = preview(&state, now);
        for _ in 0..CAPACITY {
            preview(&state, now);
        }
        assert_eq!(state.confirmations.lock().unwrap().pending.len(), CAPACITY);
        assert!(state.request(&oldest.confirmation_id, now, deny).is_err());
    }

    #[test]
    fn failure_consumes_all_previews_and_requires_new_review() {
        let state = PowerState::default();
        let now = Instant::now();
        let first = preview(&state, now);
        let second = preview(&state, now);
        assert_eq!(
            state
                .request(&first.confirmation_id, now, |spec| {
                    assert_eq!(spec, &fixture());
                    Err(RequestFailure::Refused("OS cancelled fixture".into()))
                })
                .unwrap_err(),
            "OS cancelled fixture"
        );
        for id in [&first.confirmation_id, &second.confirmation_id] {
            assert!(state
                .request(id, now, |_| panic!("replay invoked executor"))
                .is_err());
        }
        let fresh = preview(&state, now);
        assert!(
            state
                .request(&fresh.confirmation_id, now, |_| Ok(()))
                .unwrap()
                .requested
        );
        assert!(state
            .preview(fixture(), "fixture", "local-fixture".into(), now)
            .is_err());
    }

    #[test]
    fn concurrent_requests_and_new_previews_are_rejected() {
        let state = std::sync::Arc::new(PowerState::default());
        let now = Instant::now();
        let first = preview(&state, now);
        let second = preview(&state, now);
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let worker_state = state.clone();
        let worker = std::thread::spawn(move || {
            worker_state.request(&first.confirmation_id, now, |_| {
                started_tx.send(()).unwrap();
                finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            })
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(state
            .request(&second.confirmation_id, now, |_| panic!(
                "concurrent request executed"
            ))
            .is_err());
        assert!(state
            .preview(fixture(), "fixture", "local".into(), now)
            .is_err());
        finish_tx.send(()).unwrap();
        assert!(worker.join().unwrap().unwrap().requested);
    }

    #[test]
    fn only_the_main_local_app_origin_is_authorized() {
        let dev = tauri::Url::parse("http://localhost:1420").unwrap();
        for url in [
            "tauri://localhost/",
            "http://tauri.localhost/",
            "https://tauri.localhost/",
        ] {
            assert!(trusted_origin(
                "main",
                &tauri::Url::parse(url).unwrap(),
                None
            ));
        }
        for url in [
            "https://example.com/",
            "http://localhost:1420/",
            "http://tauri.localhost.evil/",
            "http://tauri.localhost:1234/",
            "http://user@tauri.localhost/",
        ] {
            assert!(
                !trusted_origin("main", &tauri::Url::parse(url).unwrap(), None),
                "{url}"
            );
        }
        assert!(trusted_origin("main", &dev, Some(&dev)));
        assert!(!trusted_origin("other", &dev, Some(&dev)));
        assert!(!trusted_origin(
            "main",
            &tauri::Url::parse("http://localhost:1421").unwrap(),
            Some(&dev)
        ));
    }

    #[test]
    fn uncertain_outcome_is_consumed_and_cannot_be_retried() {
        let state = PowerState::default();
        let now = Instant::now();
        let reviewed = preview(&state, now);
        assert!(state
            .request(&reviewed.confirmation_id, now, |_| Err(
                RequestFailure::OutcomeUnknown
            ))
            .unwrap_err()
            .contains("outcome is unknown"));
        assert!(state
            .request(&reviewed.confirmation_id, now, |_| panic!(
                "uncertain action replayed"
            ))
            .is_err());
        assert!(state
            .preview(fixture(), "fixture", "local".into(), now)
            .is_err());
    }

    #[test]
    fn local_request_drops_bus_overrides_and_does_not_expose_os_output() {
        let command = system_process(&fixture());
        let removed: Vec<_> = command
            .get_envs()
            .filter_map(|(key, value)| value.is_none().then_some(key))
            .collect();
        assert!(removed.contains(&std::ffi::OsStr::new("DBUS_SYSTEM_BUS_ADDRESS")));
        assert!(removed.contains(&std::ffi::OsStr::new("SYSTEMD_BUS_ADDRESS")));
        assert!(
            refusal_message(Some(1), b"execution error -128 private fixture").contains("cancelled")
        );
        assert!(refusal_message(Some(1), b"not authorized -1743").contains("permission"));
        assert!(refusal_message(Some(5), b"localized fixture").contains("permission"));
        assert!(refusal_message(Some(1), b"shutdown inhibited").contains("blocking"));
        assert!(!refusal_message(Some(1), b"private fixture").contains("private fixture"));
        assert_eq!(
            hostname_label("fixture\n\u{1b}machine").unwrap(),
            "fixture \u{fffd}machine"
        );
    }

    #[test]
    fn fixed_commands_preserve_non_forced_local_scope() {
        let mac = command_for("macos", None).unwrap();
        assert_eq!(mac.executable, PathBuf::from("/usr/bin/osascript"));
        assert_eq!(
            mac.args,
            [
                "-e",
                "tell application \"/System/Library/CoreServices/System Events.app\" to shut down"
            ]
        );
        let linux = command_for("linux", None).unwrap();
        assert_eq!(linux.executable, PathBuf::from("/usr/bin/systemctl"));
        assert_eq!(linux.args, ["--check-inhibitors=yes", "poweroff"]);
        let windows = command_for("windows", Some(std::env::current_dir().unwrap())).unwrap();
        assert!(windows.executable.is_absolute());
        assert_eq!(windows.executable.file_name().unwrap(), "shutdown.exe");
        assert_eq!(windows.args, ["/s", "/t", "0"]);
        assert!(command_for("windows", Some("relative".into())).is_err());
        assert!(command_for("windows", None).is_err());
        assert!(command_for("unknown", None).is_err());
    }
}

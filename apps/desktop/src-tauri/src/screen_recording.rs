//! Native capture owns one private session; renderer IPC receives opaque handles and bytes.
//! Source selection and capture are exclusively the native adapter's explicit OS picker.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(unix)]
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow};
use tempfile::TempDir;

const MAX_BYTES: u64 = 128 * 1024 * 1024;
#[cfg(target_os = "macos")]
const MAX_REPLY: usize = 64 * 1024;
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const DISCARD_TIMEOUT: Duration = Duration::from_secs(5);
const UNKNOWN_SESSION: &str = "This recording session is no longer available.";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Capabilities {
    available: bool,
    reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Choosing,
    Capturing,
    Finalizing,
    Ready,
    Cancelled,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    session_id: String,
    phase: Phase,
    elapsed_ms: u64,
    width: u32,
    height: u32,
    mp4_bytes: u64,
    gif_bytes: u64,
    warnings: Vec<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct NativeStatus {
    session_id: String,
    phase: Phase,
    #[serde(default)]
    elapsed_ms: u64,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    mp4_bytes: u64,
    #[serde(default)]
    gif_bytes: u64,
    #[serde(default)]
    warnings: Vec<String>,
    error: Option<String>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn valid_id(id: &str) -> bool {
    id.len() == 36
        && uuid::Uuid::parse_str(id).is_ok_and(|parsed| {
            parsed.hyphenated().to_string() == id && parsed.get_version_num() == 4
        })
}

fn parse_status(value: Value, id: &str) -> Result<Status, String> {
    let raw: NativeStatus = serde_json::from_value(value)
        .map_err(|_| "The native recorder returned an invalid status.".to_owned())?;
    if raw.session_id != id
        || raw.elapsed_ms > 120_000
        || raw.width > 1920
        || raw.height > 1080
        || raw.mp4_bytes > MAX_BYTES
        || raw.gif_bytes > MAX_BYTES
        || raw.warnings.len() > 8
        || raw
            .warnings
            .iter()
            .any(|warning| warning.len() > 256 || warning.chars().any(char::is_control))
        || raw
            .error
            .as_ref()
            .is_some_and(|error| error.len() > 256 || error.chars().any(char::is_control))
        || (raw.phase == Phase::Ready
            && (raw.width == 0 || raw.height == 0 || raw.mp4_bytes + raw.gif_bytes == 0))
    {
        return Err("The native recorder returned an invalid status.".into());
    }
    Ok(Status {
        session_id: id.into(),
        phase: raw.phase,
        elapsed_ms: raw.elapsed_ms,
        width: raw.width,
        height: raw.height,
        mp4_bytes: raw.mp4_bytes,
        gif_bytes: raw.gif_bytes,
        warnings: raw.warnings,
        error: raw.error,
    })
}

struct Session {
    id: String,
    directory: Mutex<Option<TempDir>>,
    busy: AtomicBool,
    cleaning: AtomicBool,
    callbacks: AtomicUsize,
    reaping: AtomicBool,
    reaper: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl Session {
    fn new() -> Result<Self, String> {
        let directory = tempfile::Builder::new()
            .prefix("holdmap-recording-")
            .tempdir()
            .map_err(|_| "Couldn't create a private recording directory.".to_owned())?;
        holdmap_core::util::create_private_dir(directory.path())
            .map_err(|_| "Couldn't protect the recording directory.".to_owned())?;
        directory
            .path()
            .to_str()
            .ok_or("The recording directory needs a valid Unicode path.")?;
        let id = uuid::Uuid::new_v4().hyphenated().to_string();
        Ok(Self {
            id,
            directory: Mutex::new(Some(directory)),
            busy: AtomicBool::new(false),
            cleaning: AtomicBool::new(false),
            callbacks: AtomicUsize::new(0),
            reaping: AtomicBool::new(false),
            reaper: Mutex::new(None),
        })
    }

    fn path(&self) -> Result<std::path::PathBuf, String> {
        lock(&self.directory)
            .as_ref()
            .map(|directory| directory.path().to_owned())
            .ok_or_else(|| UNKNOWN_SESSION.into())
    }

    fn remove_files(&self) {
        drop(lock(&self.directory).take());
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Unexpected shutdown cannot prove that native callbacks have stopped. Normal
        // discard and explicit process-exit cleanup remove files before releasing this slot.
        if let Some(directory) = lock(&self.directory).take() {
            let _ = directory.keep();
        }
    }
}

struct Operation(Arc<Session>);
impl Operation {
    fn acquire(session: Arc<Session>) -> Result<Self, String> {
        if session.cleaning.load(Ordering::Acquire)
            || session
                .busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return Err("The recording is busy or being discarded. Try again shortly.".into());
        }
        // Cleanup may have begun while acquiring the operation.
        if session.cleaning.load(Ordering::Acquire) {
            session.busy.store(false, Ordering::Release);
            return Err("The recording is being discarded.".into());
        }
        Ok(Self(session))
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}

struct Callback(Arc<Session>);
impl Drop for Callback {
    fn drop(&mut self) {
        self.0.callbacks.fetch_sub(1, Ordering::Release);
    }
}

#[cfg(test)]
type TestNative = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

#[derive(Default)]
pub(crate) struct ScreenRecordingState {
    session: Arc<Mutex<Option<Arc<Session>>>>,
    epoch: AtomicU64,
    exiting: Arc<AtomicBool>,
    #[cfg(test)]
    native: Option<TestNative>,
}

impl ScreenRecordingState {
    fn get(&self, id: &str) -> Result<Arc<Session>, String> {
        if !valid_id(id) {
            return Err(UNKNOWN_SESSION.into());
        }
        lock(&self.session)
            .as_ref()
            .filter(|session| session.id == id)
            .cloned()
            .ok_or_else(|| UNKNOWN_SESSION.into())
    }

    fn reserve(&self, epoch: u64) -> Result<Arc<Session>, String> {
        let mut slot = lock(&self.session);
        if epoch != self.epoch.load(Ordering::Acquire) {
            return Err("The app window changed. Choose a screen again.".into());
        }
        if slot.is_some() || self.exiting.load(Ordering::Acquire) {
            return Err("Discard the current recording before starting another.".into());
        }
        let session = Arc::new(Session::new()?);
        *slot = Some(Arc::clone(&session));
        Ok(session)
    }
}

#[cfg(target_os = "macos")]
fn native_command(request: Value) -> Result<Value, String> {
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;
    #[link(name = "holdmap_screen_capture", kind = "static")]
    extern "C" {
        fn hm_capture_command(request: *const c_char) -> *mut c_char;
        fn hm_capture_free_json(reply: *mut c_char);
    }
    let text = serde_json::to_vec(&request).map_err(|_| "Invalid recording request.".to_owned())?;
    if text.len() > 8192 {
        return Err("Invalid recording request.".into());
    }
    let text = CString::new(text).map_err(|_| "Invalid recording request.".to_owned())?;
    // SAFETY: callers dispatch this adapter to the app's main thread. Swift accepts
    // a valid NUL-terminated request and returns owned strdup JSON or a null pointer.
    let pointer = unsafe { hm_capture_command(text.as_ptr()) };
    if pointer.is_null() {
        return Err("The native recorder could not respond.".into());
    }
    struct Reply(*mut c_char);
    impl Drop for Reply {
        fn drop(&mut self) {
            // SAFETY: this pointer is owned by the matching Swift allocation API.
            unsafe { hm_capture_free_json(self.0) };
        }
    }
    let reply = Reply(pointer);
    // SAFETY: the Swift ABI guarantees a NUL-terminated allocation until free_json.
    let bytes = unsafe { CStr::from_ptr(reply.0) }.to_bytes();
    if bytes.len() > MAX_REPLY {
        return Err("The native recorder response is too large.".into());
    }
    serde_json::from_slice(bytes).map_err(|_| "The native recorder returned invalid JSON.".into())
}

#[cfg(not(target_os = "macos"))]
fn native_command(_: Value) -> Result<Value, String> {
    Err("Native screen recording requires macOS 14 or newer.".into())
}

async fn call_native<R: Runtime>(
    app: &AppHandle<R>,
    request: Value,
    session: Option<Arc<Session>>,
) -> Result<Value, String> {
    let state = app.state::<ScreenRecordingState>();
    #[cfg(test)]
    let native = state.native.clone();
    #[cfg(not(test))]
    let _ = state;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let pending = if let Some(session) = &session {
        session.callbacks.fetch_add(1, Ordering::AcqRel);
        Some(Callback(Arc::clone(session)))
    } else {
        None
    };
    let callback = move || {
        let result = if request["op"] == "start"
            && session
                .as_ref()
                .is_some_and(|session| session.cleaning.load(Ordering::Acquire))
        {
            Ok(json!({"session_id": request["session_id"], "phase": "cancelled"}))
        } else {
            #[cfg(test)]
            {
                if let Some(native) = native {
                    native(request)
                } else {
                    native_command(request)
                }
            }
            #[cfg(not(test))]
            {
                native_command(request)
            }
        };
        drop(pending);
        let _ = sender.send(result);
    };
    #[cfg(test)]
    let testing = app.state::<ScreenRecordingState>().native.is_some();
    #[cfg(not(test))]
    let testing = false;
    if testing {
        callback();
    } else {
        app.run_on_main_thread(callback)
            .map_err(|_| "Couldn't reach the native recorder.".to_owned())?;
    }
    tokio::time::timeout(CALL_TIMEOUT, receiver)
        .await
        .map_err(|_| "The native recorder did not respond in time.".to_owned())?
        .map_err(|_| "The native recorder could not respond.".to_owned())?
}

fn schedule_reap<R: Runtime>(app: &AppHandle<R>, session: Arc<Session>) {
    session.cleaning.store(true, Ordering::Release);
    let state = app.state::<ScreenRecordingState>();
    if state.exiting.load(Ordering::Acquire) {
        return;
    }
    if session.reaping.swap(true, Ordering::AcqRel) {
        return;
    }
    let slot = Arc::clone(&state.session);
    let exiting = Arc::clone(&state.exiting);
    let handle = app.clone();
    let job_session = Arc::clone(&session);
    let task = tauri::async_runtime::spawn(async move {
        let session = job_session;
        while !exiting.load(Ordering::Acquire) {
            if !session.busy.load(Ordering::Acquire) {
                let reply = call_native(
                    &handle,
                    json!({"op": "discard", "session_id": session.id}),
                    Some(Arc::clone(&session)),
                )
                .await;
                let cancelled = reply.as_ref().is_ok_and(|value| {
                    value["session_id"] == session.id && value["phase"] == "cancelled"
                        || value["phase"] == "error" && value["error"] == "unknown_session"
                });
                if cancelled && session.callbacks.load(Ordering::Acquire) == 0 {
                    session.remove_files();
                    let mut current = lock(&slot);
                    if current
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, &session))
                    {
                        current.take();
                    }
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        session.reaping.store(false, Ordering::Release);
    });
    *lock(&session.reaper) = Some(task);
}

/// Reload and window teardown request cancellation without blocking the event loop.
pub(crate) fn cleanup_lifecycle<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<ScreenRecordingState>();
    let session = {
        let current = lock(&state.session);
        state.epoch.fetch_add(1, Ordering::AcqRel);
        if let Some(session) = &*current {
            session.cleaning.store(true, Ordering::Release);
        }
        current.clone()
    };
    if let Some(session) = session {
        schedule_reap(app, session);
    }
}

/// Final process exit only: no native callback can outlive the terminating process.
/// Do not use this to cancel a recording while the app remains running.
pub(crate) fn cleanup_exit<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<ScreenRecordingState>();
    state.exiting.store(true, Ordering::Release);
    let session = {
        let mut current = lock(&state.session);
        state.epoch.fetch_add(1, Ordering::AcqRel);
        current.take()
    };
    if let Some(session) = session {
        if let Some(task) = lock(&session.reaper).take() {
            task.abort();
        }
        session.cleaning.store(true, Ordering::Release);
        let request = json!({"op": "discard", "session_id": session.id});
        #[cfg(test)]
        let _ = if let Some(native) = &state.native {
            native(request)
        } else {
            native_command(request)
        };
        #[cfg(not(test))]
        let _ = native_command(request);
        session.remove_files();
    }
}

#[tauri::command]
pub(crate) async fn screen_recording_capabilities<R: Runtime>(
    window: WebviewWindow<R>,
) -> Result<Capabilities, String> {
    crate::local_window::authorize(&window, "Screen recording")?;
    #[cfg(not(any(target_os = "macos", test)))]
    return Ok(Capabilities {
        available: false,
        reason: Some("This window does not offer screen sharing. Try a browser that supports it, or use your operating system's screen recorder.".into()),
    });
    #[cfg(any(target_os = "macos", test))]
    {
        let value = call_native(window.app_handle(), json!({"op": "capabilities"}), None).await?;
        let available = value["available"]
            .as_bool()
            .ok_or("The native recorder returned invalid capabilities.")?;
        Ok(Capabilities {
            available,
            reason: (!available)
                .then(|| "Native screen recording requires macOS 14 or newer.".into()),
        })
    }
}

#[tauri::command]
pub(crate) async fn start_screen_recording<R: Runtime>(
    window: WebviewWindow<R>,
) -> Result<Status, String> {
    let epoch = window
        .app_handle()
        .state::<ScreenRecordingState>()
        .epoch
        .load(Ordering::Acquire);
    crate::local_window::authorize(&window, "Screen recording")?;
    if !screen_recording_capabilities(window.clone())
        .await?
        .available
    {
        return Err("Native screen recording requires macOS 14 or newer.".into());
    }
    let app = window.app_handle().clone();
    // Detached operation ownership survives an IPC caller disappearing during a picker.
    tauri::async_runtime::spawn(async move {
        let session = app.state::<ScreenRecordingState>().reserve(epoch)?;
        let operation = Operation::acquire(Arc::clone(&session))?;
        let path = session.path()?;
        let result = call_native(
            &app,
            json!({"op": "start", "session_id": session.id, "directory": path}),
            Some(Arc::clone(&session)),
        )
        .await
        .and_then(|value| parse_status(value, &session.id));
        drop(operation);
        if result.is_err() || session.cleaning.load(Ordering::Acquire) {
            schedule_reap(&app, session);
        }
        result
    })
    .await
    .map_err(|_| "Couldn't start the native recorder.".to_owned())?
}

async fn session_action<R: Runtime>(
    window: WebviewWindow<R>,
    id: String,
    op: &'static str,
) -> Result<Status, String> {
    crate::local_window::authorize(&window, "Screen recording")?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        let session = app.state::<ScreenRecordingState>().get(&id)?;
        let _operation = Operation::acquire(Arc::clone(&session))?;
        let value = call_native(
            &app,
            json!({"op": op, "session_id": id}),
            Some(Arc::clone(&session)),
        )
        .await?;
        let status = parse_status(value, &id)?;
        Ok(status)
    })
    .await
    .map_err(|_| "Couldn't read the native recorder.".to_owned())?
}

#[tauri::command]
pub(crate) async fn screen_recording_status<R: Runtime>(
    window: WebviewWindow<R>,
    session_id: String,
) -> Result<Status, String> {
    session_action(window, session_id, "status").await
}

#[tauri::command]
pub(crate) async fn stop_screen_recording<R: Runtime>(
    window: WebviewWindow<R>,
    session_id: String,
) -> Result<Status, String> {
    session_action(window, session_id, "stop").await
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Format {
    Mp4,
    Gif,
}

impl Format {
    #[cfg(unix)]
    fn filename(self) -> &'static str {
        match self {
            Self::Mp4 => "capture.mp4",
            Self::Gif => "capture.gif",
        }
    }
    fn expected(self, status: &Status) -> u64 {
        match self {
            Self::Mp4 => status.mp4_bytes,
            Self::Gif => status.gif_bytes,
        }
    }
    #[cfg(unix)]
    fn magic(self, bytes: &[u8]) -> bool {
        match self {
            Self::Mp4 => bytes.len() >= 12 && &bytes[4..8] == b"ftyp",
            Self::Gif => bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a"),
        }
    }
}

#[cfg(unix)]
fn read_private(directory: &Path, format: Format, expected: u64) -> Result<Vec<u8>, String> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if expected == 0 || expected > MAX_BYTES {
        return Err("This recording format is unavailable or too large.".into());
    }
    let parent = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(directory)
        .map_err(|_| "Couldn't verify the private recording directory.".to_owned())?;
    let parent_meta = parent
        .metadata()
        .map_err(|_| "Couldn't verify the recording directory.".to_owned())?;
    // SAFETY: geteuid has no preconditions.
    let me = unsafe { libc::geteuid() };
    if parent_meta.uid() != me || parent_meta.mode() & 0o077 != 0 {
        return Err("The recording directory is not private.".into());
    }
    let name = std::ffi::CString::new(format.filename()).expect("fixed recording filename");
    // SAFETY: parent owns the directory descriptor; the fixed filename is NUL-terminated.
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err("Couldn't open the recording safely.".into());
    }
    // SAFETY: openat returned this newly owned descriptor.
    let file = unsafe { std::fs::File::from_raw_fd(descriptor) };
    let metadata = file
        .metadata()
        .map_err(|_| "Couldn't verify the recording file.".to_owned())?;
    if !metadata.is_file()
        || metadata.uid() != me
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
        || metadata.len() != expected
    {
        return Err("The recording file is not a private, complete regular file.".into());
    }
    let mut bytes = Vec::with_capacity(expected as usize);
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Couldn't read the recording.".to_owned())?;
    if bytes.len() as u64 != expected || !format.magic(&bytes) {
        return Err("The recording data is incomplete or invalid.".into());
    }
    Ok(bytes)
}

#[cfg(not(unix))]
fn read_private(_: &Path, _: Format, _: u64) -> Result<Vec<u8>, String> {
    Err("Native recording files are unavailable on this platform.".into())
}

#[tauri::command]
pub(crate) async fn read_screen_recording<R: Runtime>(
    window: WebviewWindow<R>,
    session_id: String,
    format: Format,
) -> Result<tauri::ipc::Response, String> {
    crate::local_window::authorize(&window, "Screen recording")?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        let session = app.state::<ScreenRecordingState>().get(&session_id)?;
        let operation = Operation::acquire(Arc::clone(&session))?;
        let value = call_native(
            &app,
            json!({"op": "status", "session_id": session_id}),
            Some(Arc::clone(&session)),
        )
        .await?;
        let status = parse_status(value, &session_id)?;
        if status.phase != Phase::Ready {
            return Err("Stop the recording and wait for its files before reading them.".into());
        }
        let path = session.path()?;
        tauri::async_runtime::spawn_blocking(move || {
            let _operation = operation;
            read_private(&path, format, format.expected(&status)).map(tauri::ipc::Response::new)
        })
        .await
        .map_err(|_| "Couldn't read the recording file.".to_owned())?
    })
    .await
    .map_err(|_| "Couldn't read the recording file.".to_owned())?
}

#[tauri::command]
pub(crate) async fn discard_screen_recording<R: Runtime>(
    window: WebviewWindow<R>,
    session_id: String,
) -> Result<(), String> {
    crate::local_window::authorize(&window, "Screen recording")?;
    let app = window.app_handle();
    let session = app.state::<ScreenRecordingState>().get(&session_id)?;
    schedule_reap(app, Arc::clone(&session));
    let wait = async {
        while app.state::<ScreenRecordingState>().get(&session_id).is_ok() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    };
    tokio::time::timeout(DISCARD_TIMEOUT, wait)
        .await
        .map_err(|_| {
            "The recorder is still stopping. Cleanup will continue in the background.".to_owned()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::path::PathBuf;
    use std::time::Instant;
    use tauri::test::{
        get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
    };

    const MP4: &[u8] = b"\0\0\0\x18ftypmp42test";
    const GIF: &[u8] = b"GIF89atest";

    fn test_app(
        native: impl Fn(Value) -> Result<Value, String> + Send + Sync + 'static,
    ) -> tauri::App<MockRuntime> {
        mock_builder()
            .manage(ScreenRecordingState {
                native: Some(Arc::new(native)),
                ..Default::default()
            })
            // Command-level authorization is exercised without the root invoke guard.
            .invoke_handler(tauri::generate_handler![
                screen_recording_capabilities,
                start_screen_recording,
                screen_recording_status,
                stop_screen_recording,
                read_screen_recording,
                discard_screen_recording
            ])
            .build(mock_context(noop_assets()))
            .unwrap()
    }

    fn window(app: &tauri::App<MockRuntime>, label: &str) -> WebviewWindow<MockRuntime> {
        tauri::WebviewWindowBuilder::new(app, label, Default::default())
            .build()
            .unwrap()
    }

    fn request(command: &str, body: Value) -> tauri::webview::InvokeRequest {
        tauri::webview::InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: if cfg!(windows) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        }
    }

    fn ipc(
        window: &WebviewWindow<MockRuntime>,
        command: &str,
        body: Value,
    ) -> Result<Value, Value> {
        get_ipc_response(window, request(command, body)).map(|body| body.deserialize().unwrap())
    }

    fn wait_until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !predicate() {
            assert!(
                Instant::now() < deadline,
                "recording cleanup did not complete"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn status(id: &str, phase: &str) -> Value {
        json!({"session_id": id, "phase": phase, "elapsed_ms": 25, "width": 320, "height": 180,
            "mp4_bytes": MP4.len(), "gif_bytes": GIF.len(), "warnings": []})
    }

    #[test]
    fn every_ipc_command_checks_the_local_main_window_before_native_effects() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let app = test_app(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Err("must not dispatch".into())
        });
        let secondary = window(&app, "secondary");
        let external = tauri::WebviewWindowBuilder::new(
            &app,
            "main",
            tauri::WebviewUrl::External("https://example.com/".parse().unwrap()),
        )
        .build()
        .unwrap();
        for view in [&secondary, &external] {
            for command in [
                "screen_recording_capabilities",
                "start_screen_recording",
                "screen_recording_status",
                "stop_screen_recording",
                "read_screen_recording",
                "discard_screen_recording",
            ] {
                let response = ipc(
                    view,
                    command,
                    json!({"sessionId": uuid::Uuid::new_v4().to_string(), "format": "mp4"}),
                );
                assert!(response
                    .unwrap_err()
                    .as_str()
                    .unwrap()
                    .contains("local Holdmap app window"));
            }
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn ipc_uses_opaque_handles_and_rejects_malformed_or_foreign_handles_and_formats() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let app = test_app(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(json!({"available": true}))
        });
        let main = window(&app, "main");
        for id in [
            "../capture.mp4".to_owned(),
            uuid::Uuid::nil().to_string(),
            uuid::Uuid::new_v4().to_string().to_uppercase(),
            uuid::Uuid::new_v4().to_string(),
        ] {
            assert!(ipc(&main, "screen_recording_status", json!({"sessionId": id})).is_err());
            assert!(ipc(&main, "discard_screen_recording", json!({"sessionId": id})).is_err());
        }
        assert!(ipc(
            &main,
            "read_screen_recording",
            json!({"sessionId": uuid::Uuid::new_v4().to_string(), "format": "../../capture.mp4"})
        )
        .is_err());
        assert!(ipc(
            &main,
            "read_screen_recording",
            json!({"sessionId": 7, "format": "mp4"})
        )
        .is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[cfg(unix)]
    #[test]
    fn ipc_lifecycle_keeps_paths_native_reads_only_ready_exact_bytes_and_removes_files_after_ack() {
        let current = Arc::new(Mutex::new((
            String::new(),
            PathBuf::new(),
            "choosing".to_string(),
        )));
        let native = Arc::clone(&current);
        let app = test_app(move |request| {
            let mut record = lock(&native);
            match request["op"].as_str().unwrap() {
                "capabilities" => return Ok(json!({"available": true})),
                "start" => {
                    record.0 = request["session_id"].as_str().unwrap().into();
                    record.1 = PathBuf::from(request["directory"].as_str().unwrap());
                    record.2 = "choosing".into();
                    for (filename, data) in [("capture.mp4", MP4), ("capture.gif", GIF)] {
                        use std::io::Write;
                        holdmap_core::util::create_private(&record.1.join(filename))
                            .unwrap()
                            .write_all(data)
                            .unwrap();
                    }
                }
                "stop" => record.2 = "ready".into(),
                "discard" => record.2 = "cancelled".into(),
                "status" => {}
                _ => unreachable!(),
            }
            Ok(status(&record.0, &record.2))
        });
        let main = window(&app, "main");
        let started = ipc(&main, "start_screen_recording", json!({})).unwrap();
        let id = started["sessionId"].as_str().unwrap();
        assert!(valid_id(id));
        assert!(started.get("directory").is_none() && started.get("session_id").is_none());
        assert!(ipc(&main, "start_screen_recording", json!({})).is_err());
        assert!(ipc(
            &main,
            "read_screen_recording",
            json!({"sessionId": id, "format": "mp4"})
        )
        .is_err());
        assert_eq!(
            ipc(&main, "stop_screen_recording", json!({"sessionId": id})).unwrap()["phase"],
            "ready"
        );
        for (format, expected) in [("mp4", MP4), ("gif", GIF)] {
            let response = get_ipc_response(
                &main,
                request(
                    "read_screen_recording",
                    json!({"sessionId": id, "format": format}),
                ),
            )
            .unwrap();
            assert_eq!(response.deserialize::<Vec<u8>>().unwrap(), expected);
        }
        let path = lock(&current).1.clone();
        assert!(path.exists());
        ipc(&main, "discard_screen_recording", json!({"sessionId": id})).unwrap();
        assert!(!path.exists());
        assert!(ipc(&main, "screen_recording_status", json!({"sessionId": id})).is_err());
        let next = ipc(&main, "start_screen_recording", json!({})).unwrap();
        assert_ne!(next["sessionId"], started["sessionId"]);
        cleanup_exit(app.handle());
    }

    #[test]
    fn discard_timeout_retains_session_until_native_ack_and_next_cleanup_is_not_lost() {
        let acknowledge = Arc::new(AtomicBool::new(false));
        let reply = Arc::clone(&acknowledge);
        let discards = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&discards);
        let app = test_app(move |request| {
            assert_eq!(request["op"], "discard");
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(
                json!({"session_id": request["session_id"], "phase": if reply.load(Ordering::SeqCst) { "cancelled" } else { "finalizing" }}),
            )
        });
        let main = window(&app, "main");
        let state = app.state::<ScreenRecordingState>();
        let session = state.reserve(state.epoch.load(Ordering::Acquire)).unwrap();
        let path = session.path().unwrap();
        let error = ipc(
            &main,
            "discard_screen_recording",
            json!({"sessionId": session.id}),
        )
        .unwrap_err();
        assert!(error.as_str().unwrap().contains("Cleanup will continue"));
        assert!(path.exists());
        assert!(state.reserve(state.epoch.load(Ordering::Acquire)).is_err());
        assert!(discards.load(Ordering::SeqCst) > 0);
        acknowledge.store(true, Ordering::SeqCst);
        wait_until(|| state.get(&session.id).is_err());
        assert!(!path.exists());
        // A new session must own its own reaping flag, even if the old task is finishing.
        let next = state.reserve(state.epoch.load(Ordering::Acquire)).unwrap();
        let next_path = next.path().unwrap();
        cleanup_lifecycle(app.handle());
        wait_until(|| state.get(&next.id).is_err());
        assert!(!next_path.exists());
    }

    #[test]
    fn cleanup_waits_for_queued_callbacks_and_suppresses_late_start_effects() {
        let starts = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&starts);
        let app = test_app(move |request| {
            if request["op"] == "start" {
                counter.fetch_add(1, Ordering::SeqCst);
            }
            Ok(json!({"session_id": request["session_id"], "phase": "cancelled"}))
        });
        let state = app.state::<ScreenRecordingState>();
        let session = state.reserve(state.epoch.load(Ordering::Acquire)).unwrap();
        let path = session.path().unwrap();
        session.callbacks.fetch_add(1, Ordering::AcqRel);
        let queued = Callback(Arc::clone(&session));
        cleanup_lifecycle(app.handle());
        std::thread::sleep(Duration::from_millis(300));
        assert!(path.exists() && state.get(&session.id).is_ok());
        let reply = tauri::async_runtime::block_on(call_native(
            app.handle(),
            json!({"op": "start", "session_id": session.id}),
            Some(Arc::clone(&session)),
        ))
        .unwrap();
        assert_eq!(reply["phase"], "cancelled");
        assert_eq!(starts.load(Ordering::SeqCst), 0);
        drop(queued);
        wait_until(|| state.get(&session.id).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn failed_start_keeps_its_private_directory_until_native_confirms_no_session() {
        let inactive = Arc::new(AtomicBool::new(false));
        let native_inactive = Arc::clone(&inactive);
        let app = test_app(move |request| match request["op"].as_str().unwrap() {
            "capabilities" => Ok(json!({"available": true})),
            "start" => Ok(json!({"phase": "error", "error": "start_failed"})),
            "discard" if native_inactive.load(Ordering::Acquire) => {
                Ok(json!({"phase": "error", "error": "unknown_session"}))
            }
            "discard" => Ok(json!({"session_id": request["session_id"], "phase": "finalizing"})),
            _ => unreachable!(),
        });
        let main = window(&app, "main");
        assert!(ipc(&main, "start_screen_recording", json!({})).is_err());
        let state = app.state::<ScreenRecordingState>();
        let session = lock(&state.session).clone().unwrap();
        let path = session.path().unwrap();
        assert!(path.exists());
        assert!(state.reserve(state.epoch.load(Ordering::Acquire)).is_err());
        inactive.store(true, Ordering::Release);
        wait_until(|| state.get(&session.id).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn reload_during_capability_check_invalidates_the_pending_start_before_allocating() {
        let (entered_sender, entered) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let released = Mutex::new(released);
        let starts = Arc::new(AtomicUsize::new(0));
        let native_starts = Arc::clone(&starts);
        let app = test_app(move |request| {
            if request["op"] == "capabilities" {
                entered_sender.send(()).unwrap();
                lock(&released)
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap();
                Ok(json!({"available": true}))
            } else {
                native_starts.fetch_add(1, Ordering::SeqCst);
                Err("No native recording operation may follow this reload.".into())
            }
        });
        let main = window(&app, "main");
        let caller = std::thread::spawn(move || ipc(&main, "start_screen_recording", json!({})));
        entered.recv_timeout(Duration::from_secs(3)).unwrap();
        cleanup_lifecycle(app.handle());
        release.send(()).unwrap();
        let error = caller.join().unwrap().unwrap_err();
        assert!(error.as_str().unwrap().contains("window changed"));
        assert_eq!(starts.load(Ordering::SeqCst), 0);
        assert!(lock(&app.state::<ScreenRecordingState>().session).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn recording_reads_refuse_links_public_files_nonregular_files_size_changes_and_bad_magic() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let directory = tempfile::tempdir().unwrap();
        holdmap_core::util::create_private_dir(directory.path()).unwrap();
        let path = directory.path().join("capture.mp4");
        let write = |bytes: &[u8]| {
            use std::io::Write;
            holdmap_core::util::create_private(&path)
                .unwrap()
                .write_all(bytes)
                .unwrap();
        };
        write(MP4);
        assert_eq!(
            read_private(directory.path(), Format::Mp4, MP4.len() as u64).unwrap(),
            MP4
        );
        assert!(read_private(directory.path(), Format::Mp4, 0).is_err());
        assert!(read_private(directory.path(), Format::Mp4, MAX_BYTES + 1).is_err());
        assert!(read_private(directory.path(), Format::Mp4, MP4.len() as u64 + 1).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_private(directory.path(), Format::Mp4, MP4.len() as u64).is_err());
        write(b"invalid-mp4-data!");
        assert!(read_private(directory.path(), Format::Mp4, 17).is_err());
        std::fs::remove_file(&path).unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), MP4).unwrap();
        symlink(outside.path(), &path).unwrap();
        assert!(read_private(directory.path(), Format::Mp4, MP4.len() as u64).is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::hard_link(outside.path(), &path).unwrap();
        assert!(read_private(directory.path(), Format::Mp4, MP4.len() as u64).is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(read_private(directory.path(), Format::Mp4, MP4.len() as u64).is_err());
        std::fs::remove_dir(&path).unwrap();
        let fifo_name = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        // SAFETY: the path is an owned temporary test destination.
        assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
        assert!(read_private(directory.path(), Format::Mp4, MP4.len() as u64).is_err());
        std::fs::remove_file(&path).unwrap();
        write(MP4);
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(read_private(directory.path(), Format::Mp4, MP4.len() as u64).is_err());
    }

    #[test]
    fn native_status_contract_rejects_mismatched_unbounded_or_incomplete_results() {
        let id = uuid::Uuid::new_v4().to_string();
        assert!(parse_status(status(&id, "ready"), &id).is_ok());
        for invalid in [
            status(&uuid::Uuid::new_v4().to_string(), "ready"),
            json!({"session_id": id, "phase": "unexpected"}),
            json!({"session_id": id, "phase": "ready"}),
            json!({"session_id": id, "phase": "capturing", "elapsed_ms": 120001}),
            json!({"session_id": id, "phase": "capturing", "mp4_bytes": MAX_BYTES + 1}),
            json!({"session_id": id, "phase": "capturing", "width": 1921}),
            json!({"session_id": id, "phase": "error", "warnings": ["\u{1b}[2J"]}),
        ] {
            assert!(parse_status(invalid, &id).is_err());
        }
    }
}

//! Selected-source still images have their own lease and never change video phases.
//! The renderer receives only opaque UUIDs, bounded status, and checked PNG bytes.

use crate::capture_lease::{lock, Operation, Session};
use crate::{capture_bridge, snapshot_image};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

const UNKNOWN: &str = "This snapshot is no longer available.";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Choosing,
    Capturing,
    Ready,
    Cancelled,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    snapshot_id: String,
    phase: Phase,
    width: u32,
    height: u32,
    png_bytes: u64,
    error: Option<String>,
}

#[derive(Deserialize)]
struct NativeStatus {
    session_id: String,
    phase: Phase,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    png_bytes: u64,
    error: Option<String>,
}

fn valid_id(id: &str) -> bool {
    id.len() == 36
        && uuid::Uuid::parse_str(id)
            .is_ok_and(|value| value.get_version_num() == 4 && value.hyphenated().to_string() == id)
}

fn parse_status(value: Value, id: &str) -> Result<Status, String> {
    let raw: NativeStatus = serde_json::from_value(value)
        .map_err(|_| "The native snapshot returned an invalid status.".to_owned())?;
    if raw.session_id != id
        || raw.width > snapshot_image::MAX_WIDTH
        || raw.height > snapshot_image::MAX_HEIGHT
        || raw.png_bytes > snapshot_image::MAX_BYTES as u64
        || raw
            .error
            .as_ref()
            .is_some_and(|error| error.len() > 256 || error.chars().any(char::is_control))
        || raw.phase == Phase::Ready && (raw.width == 0 || raw.height == 0 || raw.png_bytes == 0)
    {
        return Err("The native snapshot returned an invalid status.".into());
    }
    Ok(Status {
        snapshot_id: id.into(),
        phase: raw.phase,
        width: raw.width,
        height: raw.height,
        png_bytes: raw.png_bytes,
        error: raw.error,
    })
}

#[derive(Default)]
pub(crate) struct ScreenSnapshotState {
    session: Arc<Mutex<Option<Arc<Session>>>>,
    epoch: AtomicU64,
    exiting: Arc<AtomicBool>,
    #[cfg(test)]
    native: Option<capture_bridge::Native>,
}

impl ScreenSnapshotState {
    fn get(&self, id: &str) -> Result<Arc<Session>, String> {
        if !valid_id(id) {
            return Err(UNKNOWN.into());
        }
        lock(&self.session)
            .as_ref()
            .filter(|session| session.id == id)
            .cloned()
            .ok_or_else(|| UNKNOWN.into())
    }
    fn reserve(&self, epoch: u64) -> Result<Arc<Session>, String> {
        let mut current = lock(&self.session);
        if self.exiting.load(Ordering::Acquire) || epoch != self.epoch.load(Ordering::Acquire) {
            return Err("The app window changed. Take a snapshot again.".into());
        }
        if current.is_some() {
            return Err("Finish or discard the current snapshot first.".into());
        }
        let next = Arc::new(Session::new("holdmap-snapshot-")?);
        *current = Some(Arc::clone(&next));
        Ok(next)
    }
}

async fn call_native<R: Runtime>(
    app: &AppHandle<R>,
    request: Value,
    session: Option<Arc<Session>>,
) -> Result<Value, String> {
    #[cfg(test)]
    let native = app.state::<ScreenSnapshotState>().native.clone();
    #[cfg(not(test))]
    let native = None;
    capture_bridge::call(app, request, session, native).await
}

fn schedule_reap<R: Runtime>(app: &AppHandle<R>, session: Arc<Session>) {
    session.cleaning.store(true, Ordering::Release);
    let state = app.state::<ScreenSnapshotState>();
    if state.exiting.load(Ordering::Acquire) || session.reaping.swap(true, Ordering::AcqRel) {
        return;
    }
    let slot = Arc::clone(&state.session);
    let exiting = Arc::clone(&state.exiting);
    let handle = app.clone();
    let owner = Arc::clone(&session);
    let task = tauri::async_runtime::spawn(async move {
        while !exiting.load(Ordering::Acquire) {
            std::sync::atomic::fence(Ordering::SeqCst);
            if !owner.busy.load(Ordering::Acquire) {
                let reply = call_native(
                    &handle,
                    json!({"op":"discard_snapshot", "session_id":owner.id}),
                    Some(Arc::clone(&owner)),
                )
                .await;
                let cancelled = reply.as_ref().is_ok_and(|value| {
                    value["session_id"] == owner.id && value["phase"] == "cancelled"
                        || value["phase"] == "error" && value["error"] == "unknown_snapshot"
                });
                if cancelled && owner.callbacks.load(Ordering::Acquire) == 0 {
                    owner.remove_files();
                    let mut current = lock(&slot);
                    if current
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, &owner))
                    {
                        current.take();
                    }
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        owner.reaping.store(false, Ordering::Release);
    });
    *lock(&session.reaper) = Some(task);
}

pub(crate) fn cleanup_lifecycle<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<ScreenSnapshotState>();
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

pub(crate) fn cleanup_exit<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<ScreenSnapshotState>();
    state.exiting.store(true, Ordering::Release);
    state.epoch.fetch_add(1, Ordering::AcqRel);
    let current = lock(&state.session).take();
    if let Some(session) = current {
        if let Some(task) = lock(&session.reaper).take() {
            task.abort();
        }
        session.cleaning.store(true, Ordering::Release);
        let request = json!({"op":"discard_snapshot", "session_id":session.id});
        #[cfg(test)]
        let _ = if let Some(native) = &state.native {
            native(request)
        } else {
            capture_bridge::native_command(request)
        };
        #[cfg(not(test))]
        let _ = capture_bridge::native_command(request);
        session.remove_files();
    }
}

#[tauri::command]
pub(crate) async fn start_screen_snapshot<R: Runtime>(
    window: WebviewWindow<R>,
    session_id: Option<String>,
) -> Result<Status, String> {
    crate::local_window::authorize(&window, "Screen snapshots")?;
    if session_id.as_ref().is_some_and(|id| !valid_id(id)) {
        return Err("The recording source is no longer available.".into());
    }
    let app = window.app_handle().clone();
    let epoch = app
        .state::<ScreenSnapshotState>()
        .epoch
        .load(Ordering::Acquire);
    tauri::async_runtime::spawn(async move {
        let capabilities = call_native(&app, json!({"op":"capabilities"}), None).await?;
        if capabilities["available"] != true {
            return Err("Native screen snapshots require macOS 14 or newer.".into());
        }
        let session = app.state::<ScreenSnapshotState>().reserve(epoch)?;
        let operation = Operation::acquire(Arc::clone(&session))?;
        let result = call_native(
            &app,
            json!({"op":"start_snapshot", "session_id":session.id,
            "recording_id":session_id, "directory":session.path()?}),
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
    .map_err(|_| "Couldn't start the snapshot.".to_owned())?
}

#[tauri::command]
pub(crate) async fn screen_snapshot_status<R: Runtime>(
    window: WebviewWindow<R>,
    snapshot_id: String,
) -> Result<Status, String> {
    crate::local_window::authorize(&window, "Screen snapshots")?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        let session = app.state::<ScreenSnapshotState>().get(&snapshot_id)?;
        let _operation = Operation::acquire(Arc::clone(&session))?;
        parse_status(
            call_native(
                &app,
                json!({"op":"snapshot_status", "session_id":snapshot_id}),
                Some(Arc::clone(&session)),
            )
            .await?,
            &snapshot_id,
        )
    })
    .await
    .map_err(|_| "Couldn't read the snapshot status.".to_owned())?
}

#[tauri::command]
pub(crate) async fn read_screen_snapshot<R: Runtime>(
    window: WebviewWindow<R>,
    snapshot_id: String,
) -> Result<tauri::ipc::Response, String> {
    crate::local_window::authorize(&window, "Screen snapshots")?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn(async move {
        let session = app.state::<ScreenSnapshotState>().get(&snapshot_id)?;
        let operation = Operation::acquire(Arc::clone(&session))?;
        let status = parse_status(
            call_native(
                &app,
                json!({"op":"snapshot_status", "session_id":snapshot_id}),
                Some(Arc::clone(&session)),
            )
            .await?,
            &snapshot_id,
        )?;
        if status.phase != Phase::Ready {
            return Err("Wait for the snapshot to finish before reading it.".into());
        }
        let path = session.path()?;
        tauri::async_runtime::spawn_blocking(move || {
            let _operation = operation;
            let bytes = crate::capture_file::read_private(
                &path,
                "capture.png",
                status.png_bytes,
                snapshot_image::MAX_BYTES as u64,
            )?;
            if snapshot_image::validate(&bytes)? != (status.width, status.height) {
                return Err("The snapshot dimensions changed.".into());
            }
            Ok(tauri::ipc::Response::new(bytes))
        })
        .await
        .map_err(|_| "Couldn't read the PNG snapshot.".to_owned())?
    })
    .await
    .map_err(|_| "Couldn't read the PNG snapshot.".to_owned())?
}

#[tauri::command]
pub(crate) async fn discard_screen_snapshot<R: Runtime>(
    window: WebviewWindow<R>,
    snapshot_id: String,
) -> Result<(), String> {
    crate::local_window::authorize(&window, "Screen snapshots")?;
    let app = window.app_handle();
    let session = app.state::<ScreenSnapshotState>().get(&snapshot_id)?;
    schedule_reap(app, Arc::clone(&session));
    let wait = async {
        while app.state::<ScreenSnapshotState>().get(&snapshot_id).is_ok() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), wait)
        .await
        .map_err(|_| {
            "Snapshot cleanup will continue until the native operation stops.".to_owned()
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tauri::test::{
        get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
    };

    const PNG: &[u8] = include_bytes!("../icons/32x32.png");

    fn app(
        native: impl Fn(Value) -> Result<Value, String> + Send + Sync + 'static,
    ) -> tauri::App<MockRuntime> {
        mock_builder()
            .manage(ScreenSnapshotState {
                native: Some(Arc::new(native)),
                ..Default::default()
            })
            .invoke_handler(tauri::generate_handler![
                start_screen_snapshot,
                screen_snapshot_status,
                read_screen_snapshot,
                discard_screen_snapshot
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
    fn ready(id: &str) -> Value {
        json!({"session_id":id, "phase":"ready", "width":32, "height":32, "png_bytes":PNG.len()})
    }
    fn wait_until(mut ready: impl FnMut() -> bool) {
        let until = std::time::Instant::now() + Duration::from_secs(3);
        while !ready() && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(ready(), "snapshot cleanup did not acknowledge completion");
    }

    #[test]
    fn all_snapshot_commands_reject_untrusted_windows_before_native_effects() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let app = app(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            Err("forbidden".into())
        });
        let foreign = window(&app, "other");
        let id = uuid::Uuid::new_v4().to_string();
        for command in [
            "start_screen_snapshot",
            "screen_snapshot_status",
            "read_screen_snapshot",
            "discard_screen_snapshot",
        ] {
            assert!(
                ipc(&foreign, command, json!({"snapshotId":id})).is_err(),
                "{command}"
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn standalone_source_chooses_explicitly_and_cannot_read_before_ready() {
        let app = app(|request| match request["op"].as_str().unwrap() {
            "capabilities" => Ok(json!({"available":true})),
            "start_snapshot" => {
                assert!(request["recording_id"].is_null());
                Ok(json!({"session_id":request["session_id"],"phase":"choosing"}))
            }
            "snapshot_status" => Ok(json!({"session_id":request["session_id"],"phase":"choosing"})),
            "discard_snapshot" => {
                Ok(json!({"session_id":request["session_id"],"phase":"cancelled"}))
            }
            other => panic!("unexpected snapshot operation: {other}"),
        });
        let main = window(&app, "main");
        assert!(ipc(
            &main,
            "start_screen_snapshot",
            json!({"sessionId":"../../foreign"})
        )
        .is_err());
        let started = ipc(&main, "start_screen_snapshot", json!({})).unwrap();
        let id = started["snapshotId"].as_str().unwrap();
        assert_eq!(started["phase"], "choosing");
        assert_eq!(started["pngBytes"], 0);
        assert!(ipc(&main, "read_screen_snapshot", json!({"snapshotId":id})).is_err());
        ipc(&main, "discard_screen_snapshot", json!({"snapshotId":id})).unwrap();
        assert!(lock(&app.state::<ScreenSnapshotState>().session).is_none());
    }

    #[test]
    fn active_source_uses_opaque_recording_handle_and_reads_only_complete_private_png() {
        let paths = Arc::new(Mutex::new(Vec::new()));
        let saved_paths = Arc::clone(&paths);
        let operations = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&operations);
        let recording = uuid::Uuid::new_v4().to_string();
        let expected = recording.clone();
        let app = app(move |request| {
            let op = request["op"].as_str().unwrap();
            lock(&seen).push(op.to_owned());
            match op {
                "capabilities" => Ok(json!({"available":true})),
                "start_snapshot" => {
                    assert_eq!(request["recording_id"], expected);
                    let path = std::path::PathBuf::from(request["directory"].as_str().unwrap());
                    let file = path.join("capture.png");
                    std::fs::write(&file, PNG).unwrap();
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))
                            .unwrap();
                    }
                    lock(&saved_paths).push(path);
                    Ok(ready(request["session_id"].as_str().unwrap()))
                }
                "snapshot_status" => Ok(ready(request["session_id"].as_str().unwrap())),
                "discard_snapshot" => {
                    Ok(json!({"session_id":request["session_id"],"phase":"cancelled"}))
                }
                _ => panic!("snapshot must not stop, discard or reconfigure the video: {op}"),
            }
        });
        let main = window(&app, "main");
        let started = ipc(
            &main,
            "start_screen_snapshot",
            json!({"sessionId":recording}),
        )
        .unwrap();
        let id = started["snapshotId"].as_str().unwrap();
        assert!(valid_id(id));
        assert_eq!(started["phase"], "ready");
        assert!(started.get("directory").is_none());
        let foreign = uuid::Uuid::new_v4().to_string();
        assert!(ipc(&main, "read_screen_snapshot", json!({"snapshotId":foreign})).is_err());
        assert!(ipc(&main, "start_screen_snapshot", json!({})).is_err());
        #[cfg(unix)]
        {
            let bytes = get_ipc_response(
                &main,
                request("read_screen_snapshot", json!({"snapshotId":id})),
            )
            .unwrap()
            .deserialize::<Vec<u8>>()
            .unwrap();
            assert_eq!(bytes, PNG);
        }
        ipc(&main, "discard_screen_snapshot", json!({"snapshotId":id})).unwrap();
        assert!(!lock(&paths)[0].exists());
        assert!(ipc(&main, "screen_snapshot_status", json!({"snapshotId":id})).is_err());
        assert!(lock(&operations)
            .iter()
            .all(|op| !matches!(op.as_str(), "start" | "stop" | "discard")));
    }

    #[test]
    fn discarding_waits_for_pending_native_ack_and_queued_callbacks() {
        let acknowledged = Arc::new(AtomicBool::new(false));
        let ack = Arc::clone(&acknowledged);
        let app = app(move |request| {
            assert_eq!(request["op"], "discard_snapshot");
            Ok(
                json!({"session_id":request["session_id"], "phase":if ack.load(Ordering::Acquire) {"cancelled"} else {"capturing"}}),
            )
        });
        let state = app.state::<ScreenSnapshotState>();
        let lease = state.reserve(0).unwrap();
        let path = lease.path().unwrap();
        lease.callbacks.fetch_add(1, Ordering::AcqRel);
        schedule_reap(app.handle(), Arc::clone(&lease));
        std::thread::sleep(Duration::from_millis(120));
        assert!(path.exists());
        assert!(state.reserve(0).is_err());
        assert!(Operation::acquire(Arc::clone(&lease)).is_err());
        acknowledged.store(true, Ordering::Release);
        std::thread::sleep(Duration::from_millis(120));
        assert!(
            path.exists(),
            "queued callbacks must retain the directory after native ack"
        );
        lease.callbacks.fetch_sub(1, Ordering::Release);
        wait_until(|| state.get(&lease.id).is_err());
        assert!(!path.exists());
        let next = state.reserve(state.epoch.load(Ordering::Acquire)).unwrap();
        cleanup_exit(app.handle());
        assert!(next.path().is_err());
    }

    #[test]
    fn reload_revokes_a_pending_snapshot_start_without_allocating_or_invoking_a_picker() {
        let (started, enter) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let released = Mutex::new(released);
        let app = app(move |request| {
            assert_eq!(request["op"], "capabilities");
            started.send(()).unwrap();
            lock(&released)
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
            Ok(json!({"available":true}))
        });
        let main = window(&app, "main");
        let caller = std::thread::spawn(move || ipc(&main, "start_screen_snapshot", json!({})));
        enter.recv_timeout(Duration::from_secs(3)).unwrap();
        cleanup_lifecycle(app.handle());
        release.send(()).unwrap();
        assert!(caller.join().unwrap().is_err());
        assert!(lock(&app.state::<ScreenSnapshotState>().session).is_none());
    }

    #[test]
    fn rejects_unbounded_foreign_or_incomplete_native_status() {
        let id = uuid::Uuid::new_v4().to_string();
        for value in [
            json!({"session_id":uuid::Uuid::new_v4(),"phase":"choosing"}),
            json!({"session_id":id,"phase":"ready","width":1,"height":1,"png_bytes":0}),
            json!({"session_id":id,"phase":"ready","width":3841,"height":1,"png_bytes":1}),
            json!({"session_id":id,"phase":"capturing","png_bytes":snapshot_image::MAX_BYTES+1}),
            json!({"session_id":id,"phase":"error","error":"unsafe\ncode"}),
        ] {
            assert!(parse_status(value, &id).is_err());
        }
        assert!(parse_status(ready(&id), &id).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn png_reads_refuse_links_shared_files_public_permissions_and_size_changes() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let directory = tempfile::tempdir().unwrap();
        holdmap_core::util::create_private_dir(directory.path()).unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), PNG).unwrap();
        let output = directory.path().join("capture.png");
        let read = || {
            crate::capture_file::read_private(
                directory.path(),
                "capture.png",
                PNG.len() as u64,
                snapshot_image::MAX_BYTES as u64,
            )
        };
        symlink(outside.path(), &output).unwrap();
        assert!(read().is_err());
        std::fs::remove_file(&output).unwrap();
        std::fs::hard_link(outside.path(), &output).unwrap();
        assert!(read().is_err());
        std::fs::remove_file(&output).unwrap();
        std::fs::write(&output, PNG).unwrap();
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read().is_err());
        std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read().unwrap(), PNG);
        std::fs::write(&output, &PNG[..PNG.len() - 1]).unwrap();
        assert!(read().is_err());
        assert_eq!(std::fs::read(outside.path()).unwrap(), PNG);
    }
}

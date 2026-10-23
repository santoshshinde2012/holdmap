//! Save a bounded recording only to a destination selected in the native save dialog.
//! No path or filesystem permission is accepted from JavaScript.

use std::{
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::{ipc::InvokeBody, AppHandle, Runtime, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

const MAX_BYTES: usize = 128 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct RecordingSaveState {
    busy: Arc<AtomicBool>,
    // Test only: substitute the interactive OS boundary while exercising the real
    // generated command, binary IPC decoding, lease and atomic filesystem writer.
    #[cfg(test)]
    picker: Option<TestSavePicker>,
}

#[cfg(test)]
type TestSavePicker =
    Arc<dyn Fn(&str, &str) -> Result<Option<std::path::PathBuf>, String> + Send + Sync + 'static>;

struct SaveLease(Arc<AtomicBool>);

impl SaveLease {
    fn acquire(state: &RecordingSaveState) -> Result<Self, String> {
        state
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Finish or cancel the open save dialog first.".to_string())?;
        Ok(Self(Arc::clone(&state.busy)))
    }
}

impl Drop for SaveLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// A report is a UTF-8 JSON object of at most 8 MiB; the picker, not JavaScript, chooses where.
fn is_report(bytes: &[u8]) -> bool {
    bytes.len() <= 8 * 1024 * 1024
        && bytes.first() == Some(&b'{')
        && std::str::from_utf8(bytes).is_ok()
        && serde_json::from_slice::<serde_json::Value>(bytes).is_ok()
}

fn validate<'a>(filename: &'a str, bytes: &[u8]) -> Result<&'a str, String> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err("Recording must contain data and be smaller than 128 MiB.".into());
    }
    if !filename.starts_with("holdmap-")
        || filename.len() > 64
        || !filename
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.'))
    {
        return Err("Invalid recording filename.".into());
    }
    let format = filename.rsplit_once('.').map(|(_, ext)| ext);
    match format {
        Some("png") => {
            crate::snapshot_image::validate(bytes)?;
            Ok("png")
        }
        Some("gif") if bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a") => Ok("gif"),
        Some("mp4") if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" => Ok("mp4"),
        // Exported reports are plain text, small, and chosen in the same native dialog.
        Some(ext @ ("json" | "sarif")) if is_report(bytes) => Ok(ext),
        _ => Err(
            "Only PNG snapshots, MP4 video and GIF recordings, and JSON or SARIF reports can be saved."
                .into(),
        ),
    }
}

fn persist_recording(path: &Path, extension: &str, bytes: &[u8]) -> Result<(), String> {
    if !path.is_absolute()
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
    {
        return Err(format!("Choose a filename ending in .{extension}."));
    }
    let parent = path
        .parent()
        .ok_or("Choose a local folder for the recording.")?;
    // Same-directory replacement is atomic. A failed save leaves an existing file intact,
    // and tempfile gives the unpublished recording private permissions on Unix.
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("Couldn't create the recording in this folder: {e}"))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|e| format!("Couldn't write the recording: {e}"))?;
    temporary
        .persist(path)
        .map_err(|e| format!("Couldn't save the recording: {}", e.error))?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn save_screen_recording<R: Runtime>(
    window: WebviewWindow<R>,
    app: AppHandle<R>,
    state: tauri::State<'_, RecordingSaveState>,
    request: tauri::ipc::Request<'_>,
) -> Result<Option<String>, String> {
    crate::local_window::authorize(&window, "Saving recordings")?;
    let filename = request
        .headers()
        .get("x-holdmap-recording-name")
        .and_then(|value| value.to_str().ok())
        .ok_or("Missing recording filename.")?;
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("Send the recording as binary data.".into());
    };
    let extension = validate(filename, bytes)?.to_owned();
    let lease = SaveLease::acquire(&state)?;
    let filename = filename.to_owned();
    let bytes = bytes.clone();
    #[cfg(test)]
    let picker = state.picker.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let native_picker = || {
            app.dialog()
                .file()
                .set_parent(&window)
                .set_title(match extension.as_str() {
                    "png" => "Save Holdmap snapshot",
                    "json" | "sarif" => "Save Holdmap report",
                    _ => "Save Holdmap recording",
                })
                .set_file_name(&filename)
                .add_filter(
                    match extension.as_str() {
                        "png" => "PNG snapshot",
                        "mp4" => "MP4 video",
                        "json" => "JSON report",
                        "sarif" => "SARIF report",
                        _ => "GIF animation",
                    },
                    &[&extension],
                )
                .blocking_save_file()
                .map(|selected| {
                    selected
                        .into_path()
                        .map_err(|_| "Choose a local file for the recording.".to_owned())
                })
                .transpose()
        };
        #[cfg(test)]
        let selected = match picker {
            Some(picker) => picker(&filename, &extension),
            None => native_picker(),
        };
        #[cfg(not(test))]
        let selected = native_picker();
        let Some(path) = selected? else {
            return Ok(None);
        };
        persist_recording(&path, &extension, &bytes)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|_| "The save dialog could not finish. Try saving again.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tauri::test::{
        get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
    };

    fn test_app(
        picker: impl Fn(&str, &str) -> Result<Option<std::path::PathBuf>, String>
            + Send
            + Sync
            + 'static,
    ) -> tauri::App<MockRuntime> {
        mock_builder()
            .manage(RecordingSaveState {
                busy: Arc::default(),
                picker: Some(Arc::new(picker)),
            })
            // Use the actual generated production handler. Do not replace the command
            // or its argument decoding with a fixture sink or an outer origin guard.
            .invoke_handler(tauri::generate_handler![save_screen_recording])
            .build(mock_context(noop_assets()))
            .unwrap()
    }

    fn request(filename: Option<&str>, body: InvokeBody) -> tauri::webview::InvokeRequest {
        let mut headers = tauri::http::HeaderMap::new();
        if let Some(filename) = filename {
            headers.insert("x-holdmap-recording-name", filename.parse().unwrap());
        }
        tauri::webview::InvokeRequest {
            cmd: "save_screen_recording".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body,
            headers,
            invoke_key: INVOKE_KEY.into(),
        }
    }

    fn raw_request(filename: &str, bytes: &[u8]) -> tauri::webview::InvokeRequest {
        request(Some(filename), InvokeBody::Raw(bytes.to_vec()))
    }

    fn valid_gif() -> Vec<u8> {
        use base64::Engine;
        // A real one-pixel red GIF89a; no screen access or external encoder required.
        base64::engine::general_purpose::STANDARD
            .decode("R0lGODlhAQABAIAAAP8AAAAAACH5BAAAAAAALAAAAAABAAEAAAICRAEAOw==")
            .unwrap()
    }

    #[test]
    fn ipc_saves_checked_png_bytes_through_the_same_atomic_save_boundary() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("snapshot.png");
        std::fs::write(&output, b"previous snapshot").unwrap();
        let chosen = output.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let app = test_app(move |name, extension| {
            assert_eq!(name, "holdmap-snapshot.png");
            assert_eq!(extension, "png");
            seen.fetch_add(1, Ordering::SeqCst);
            Ok(Some(chosen.clone()))
        });
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let png = include_bytes!("../icons/32x32.png");
        assert!(
            get_ipc_response(&window, raw_request("holdmap-snapshot.png", &png[..24])).is_err()
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "invalid PNG must not open a save chooser"
        );
        let saved = get_ipc_response(&window, raw_request("holdmap-snapshot.png", png)).unwrap();
        assert_eq!(
            saved.deserialize::<Option<String>>().unwrap(),
            Some(output.to_string_lossy().into_owned())
        );
        assert_eq!(std::fs::read(&output).unwrap(), png);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn ipc_saves_json_and_sarif_reports_only_when_they_are_json() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("report.sarif");
        let chosen = output.clone();
        let app = test_app(move |name, extension| {
            assert_eq!(name, "holdmap-security-2026-10-23.sarif");
            assert_eq!(extension, "sarif");
            Ok(Some(chosen.clone()))
        });
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        for bad in [&b"not json"[..], b"{\"unterminated\":", b"[1]"] {
            assert!(get_ipc_response(
                &window,
                raw_request("holdmap-security-2026-10-23.sarif", bad)
            )
            .is_err());
        }
        let report = br#"{"version":"2.1.0","runs":[]}"#;
        let saved = get_ipc_response(
            &window,
            raw_request("holdmap-security-2026-10-23.sarif", report),
        )
        .unwrap();
        assert_eq!(
            saved.deserialize::<Option<String>>().unwrap(),
            Some(output.to_string_lossy().into_owned())
        );
        assert_eq!(std::fs::read(&output).unwrap(), report);
    }

    #[test]
    fn ipc_saves_exact_binary_bytes_and_releases_the_lease_after_cancellation() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("chosen.gif");
        std::fs::write(&output, b"previous recording").unwrap();
        let chosen = output.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let selections = Arc::clone(&calls);
        let app = test_app(move |filename, extension| {
            assert_eq!(filename, "holdmap-test.gif");
            assert_eq!(extension, "gif");
            Ok((selections.fetch_add(1, Ordering::SeqCst) > 0).then(|| chosen.clone()))
        });
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let bytes = valid_gif();
        let cancelled = get_ipc_response(&window, raw_request("holdmap-test.gif", &bytes)).unwrap();
        assert_eq!(cancelled.deserialize::<Option<String>>().unwrap(), None);
        assert_eq!(std::fs::read(&output).unwrap(), b"previous recording");
        let saved = get_ipc_response(&window, raw_request("holdmap-test.gif", &bytes)).unwrap();
        assert_eq!(
            saved.deserialize::<Option<String>>().unwrap(),
            Some(output.to_string_lossy().into_owned())
        );
        assert_eq!(std::fs::read(&output).unwrap(), bytes);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn ipc_rejects_invalid_payloads_and_untrusted_views_before_the_chooser() {
        let calls = Arc::new(AtomicUsize::new(0));
        let selections = Arc::clone(&calls);
        let app = test_app(move |_, _| {
            selections.fetch_add(1, Ordering::SeqCst);
            Err("The chooser must not be reached".into())
        });
        let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let secondary = tauri::WebviewWindowBuilder::new(&app, "secondary", Default::default())
            .build()
            .unwrap();
        let bytes = valid_gif();
        for (invalid, expected) in [
            (
                request(None, InvokeBody::Raw(bytes.clone())),
                "Missing recording filename",
            ),
            (
                request(
                    Some("holdmap-test.gif"),
                    InvokeBody::Json(serde_json::json!({ "bytes": bytes })),
                ),
                "binary data",
            ),
            (
                raw_request("../holdmap-test.gif", &bytes),
                "Invalid recording filename",
            ),
            (
                raw_request("holdmap-test.mp4", &bytes),
                "Only PNG snapshots, MP4 video and GIF",
            ),
            (raw_request("holdmap-test.gif", b""), "must contain data"),
        ] {
            let error = get_ipc_response(&main, invalid).unwrap_err();
            assert!(error.as_str().unwrap().contains(expected), "{error}");
        }
        let error =
            get_ipc_response(&secondary, raw_request("holdmap-test.gif", &bytes)).unwrap_err();
        assert!(error.as_str().unwrap().contains("local Holdmap app window"));

        // A main-labelled view navigated to a remote origin must also fail the
        // production command's own guard, independent of Builder's outer guard.
        let remote_app = test_app(|_, _| panic!("remote view reached the chooser"));
        let remote = tauri::WebviewWindowBuilder::new(
            &remote_app,
            "main",
            tauri::WebviewUrl::External("https://example.com/".parse().unwrap()),
        )
        .build()
        .unwrap();
        let error = get_ipc_response(&remote, raw_request("holdmap-test.gif", &bytes)).unwrap_err();
        assert!(error.as_str().unwrap().contains("local Holdmap app window"));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn ipc_write_failure_preserves_the_destination_and_allows_retry() {
        let directory = tempfile::tempdir().unwrap();
        let obstructed = directory.path().join("existing.gif");
        std::fs::create_dir(&obstructed).unwrap();
        let sentinel = obstructed.join("existing-content");
        std::fs::write(&sentinel, b"keep this").unwrap();
        let output = directory.path().join("retry.gif");
        let chosen = output.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let selections = Arc::clone(&calls);
        let app = test_app(move |_, _| {
            Ok(Some(if selections.fetch_add(1, Ordering::SeqCst) == 0 {
                obstructed.clone()
            } else {
                chosen.clone()
            }))
        });
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let bytes = valid_gif();
        let error = get_ipc_response(&window, raw_request("holdmap-test.gif", &bytes)).unwrap_err();
        assert!(error
            .as_str()
            .unwrap()
            .contains("Couldn't save the recording"));
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep this");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        let saved = get_ipc_response(&window, raw_request("holdmap-test.gif", &bytes)).unwrap();
        assert_eq!(
            saved.deserialize::<Option<String>>().unwrap(),
            Some(output.to_string_lossy().into_owned())
        );
        assert_eq!(std::fs::read(output).unwrap(), bytes);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn ipc_rejects_overlapping_saves_until_the_native_selection_finishes() {
        let (entered, ready) = std::sync::mpsc::sync_channel(1);
        let (release, released) = std::sync::mpsc::sync_channel(1);
        let released = std::sync::Mutex::new(released);
        let app = test_app(move |_, _| {
            entered
                .send(())
                .map_err(|_| "fixture caller disconnected".to_owned())?;
            released
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|_| "fixture selection timed out".to_owned())?;
            Ok(None)
        });
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let first_window = window.clone();
        let (first_result, first_response) = std::sync::mpsc::sync_channel(1);
        let first = std::thread::spawn(move || {
            let response =
                get_ipc_response(&first_window, raw_request("holdmap-test.gif", &valid_gif()));
            let _ = first_result.send(response);
        });
        let entered = ready.recv_timeout(std::time::Duration::from_secs(5));
        let (second_result, second_response) = std::sync::mpsc::sync_channel(1);
        let second = std::thread::spawn(move || {
            let response = get_ipc_response(&window, raw_request("holdmap-test.gif", &valid_gif()));
            let _ = second_result.send(response);
        });
        // A serialization regression must fail within a deadline, not deadlock
        // this test waiting for a second dialog while the first stays open.
        let overlap = second_response.recv_timeout(std::time::Duration::from_secs(2));
        let _ = release.send(());
        let first_response = first_response.recv_timeout(std::time::Duration::from_secs(5));
        if first.is_finished() {
            first.join().unwrap();
        }
        if second.is_finished() {
            second.join().unwrap();
        }
        // Assertions come after releasing the chooser and bounded response waits.
        entered.unwrap();
        assert_eq!(
            first_response
                .unwrap()
                .unwrap()
                .deserialize::<Option<String>>()
                .unwrap(),
            None
        );
        assert!(overlap
            .unwrap()
            .unwrap_err()
            .as_str()
            .unwrap()
            .contains("Finish or cancel the open save dialog"));
    }

    #[test]
    fn accepts_only_bounded_binary_recordings_with_safe_names() {
        assert_eq!(
            validate("holdmap-20261014-130809.gif", b"GIF89a..."),
            Ok("gif")
        );
        assert_eq!(
            validate("holdmap-test.mp4", b"\0\0\0\x18ftypisom"),
            Ok("mp4")
        );
        for filename in [
            "../holdmap-test.gif",
            "holdmap-/test.gif",
            "holdmap-test.exe",
            "holdmap-test.gif\0",
        ] {
            assert!(validate(filename, b"GIF89a...").is_err());
        }
        assert!(validate("holdmap-test.mp4", b"GIF89a...").is_err());
        assert!(validate("holdmap-test.gif", b"").is_err());
        assert!(validate("holdmap-test.gif", &vec![0; MAX_BYTES + 1]).is_err());
    }

    #[test]
    fn saves_atomically_and_keeps_existing_files_on_invalid_destination() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("recording.gif");
        std::fs::write(&file, "old").unwrap();
        persist_recording(&file, "gif", b"GIF89a...").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"GIF89a...");
        assert!(persist_recording(&file, "mp4", b"new").is_err());
        assert_eq!(std::fs::read(&file).unwrap(), b"GIF89a...");
        assert!(persist_recording(Path::new("relative.gif"), "gif", b"new").is_err());
        assert!(persist_recording(
            &directory.path().join("missing/recording.gif"),
            "gif",
            b"new"
        )
        .is_err());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn allows_only_one_save_dialog_and_releases_after_cancellation() {
        let state = RecordingSaveState::default();
        let lease = SaveLease::acquire(&state).unwrap();
        assert!(SaveLease::acquire(&state).is_err());
        drop(lease);
        assert!(SaveLease::acquire(&state).is_ok());
    }
}

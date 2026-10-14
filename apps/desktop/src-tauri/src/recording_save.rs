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
use tauri::{ipc::InvokeBody, AppHandle, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

const MAX_BYTES: usize = 128 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct RecordingSaveState(Arc<AtomicBool>);

struct SaveLease(Arc<AtomicBool>);

impl SaveLease {
    fn acquire(state: &RecordingSaveState) -> Result<Self, String> {
        state
            .0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Finish or cancel the open save dialog first.".to_string())?;
        Ok(Self(Arc::clone(&state.0)))
    }
}

impl Drop for SaveLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
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
        Some("gif") if bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a") => Ok("gif"),
        Some("mp4") if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" => Ok("mp4"),
        _ => Err("Only MP4 video and GIF recordings can be saved.".into()),
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
pub(crate) async fn save_screen_recording(
    window: WebviewWindow,
    app: AppHandle,
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
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let selected = app
            .dialog()
            .file()
            .set_parent(&window)
            .set_title("Save Holdmap recording")
            .set_file_name(filename)
            .add_filter(
                if extension == "mp4" {
                    "MP4 video"
                } else {
                    "GIF animation"
                },
                &[&extension],
            )
            .blocking_save_file();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let path = selected
            .into_path()
            .map_err(|_| "Choose a local file for the recording.")?;
        persist_recording(&path, &extension, &bytes)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|_| "The save dialog could not finish. Try saving again.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

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

//! Self-update through the Tauri updater.
//!
//! Only release builds made with an update signing key carry a `plugins.updater` section (the
//! release workflow adds it with `--config`), so local and unsigned builds never register the
//! plugin and never contact the network. When an update is found the UI is told with an
//! `update-available` event and installs it only when the user asks.

use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::state::AppState;

/// Whether this build was configured with an updater public key and endpoint.
pub fn configured<R: tauri::Runtime>(ctx: &tauri::Context<R>) -> bool {
    ctx.config().plugins.0.contains_key("updater")
}

/// A newer version the user can install.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

fn enabled(app: &AppHandle) -> bool {
    app.state::<AppState>().updater.load(Ordering::Relaxed)
}

/// Check once, a little after start-up so it never competes with the first scan.
pub fn spawn_check(app: AppHandle) {
    if !enabled(&app) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_secs(20)))
            .await
            .ok();
        let Ok(updater) = app.updater() else { return };
        if let Ok(Some(u)) = updater.check().await {
            let _ = app.emit(
                "update-available",
                UpdateInfo {
                    version: u.version.clone(),
                    notes: u.body.clone(),
                },
            );
        }
    });
}

/// Download, verify (the updater checks the minisign signature) and install the update, then
/// restart. Does nothing in builds without an updater.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    if !enabled(&app) {
        return Err("this build doesn't update itself; download the new version instead".into());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let Some(update) = update else {
        return Err("holdmap is up to date".into());
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart()
}

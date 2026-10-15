//! Self-update through the Tauri updater.
//!
//! Only release builds made with an update signing key carry a `plugins.updater` section (the
//! release workflow adds it with `--config`), so local and unsigned builds never register the
//! plugin or check automatically. When an update is found the UI is told with an
//! `update-available` event and installs it only when the user asks. Explicit manual-update
//! actions open only the official desktop download or installation page.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::state::AppState;

const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

/// Named help destinations keep native browser access restricted to official pages.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopHelp {
    Downloads,
    Installation,
}

impl DesktopHelp {
    fn url(self) -> &'static str {
        match self {
            Self::Downloads => "https://santoshshinde2012.github.io/holdmap/#install",
            Self::Installation => {
                "https://santoshshinde2012.github.io/holdmap/docs/desktop/#first-open"
            }
        }
    }
}

/// A user-requested route to manual desktop updates. Never downloads or installs a binary.
#[tauri::command]
pub async fn open_desktop_help(destination: DesktopHelp) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        holdmap_core::util::open_url(destination.url()).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Couldn't open the desktop guide: {error}"))?
}

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

struct InstallGuard(Arc<AtomicBool>);

impl InstallGuard {
    fn claim(installing: &Arc<AtomicBool>) -> Result<Self, String> {
        installing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "An update installation is already in progress.".to_owned())?;
        Ok(Self(Arc::clone(installing)))
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
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
        let Ok(updater) = app.updater_builder().timeout(CHECK_TIMEOUT).build() else {
            return;
        };
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
    // Hold across checks, download and installation; early errors and cancelled async
    // tasks release the latch so a later explicit attempt can retry.
    let installation = InstallGuard::claim(&app.state::<AppState>().installing_update)?;
    let update = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let Some(mut update) = update else {
        return Err("holdmap is up to date".into());
    };
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    let (_installation, result) = tauri::async_runtime::spawn_blocking(move || {
        // Installation performs synchronous archive and filesystem work. Retain the
        // guard in this worker so dropping the awaiting caller cannot authorize a race.
        let result = update.install(bytes).map_err(|error| error.to_string());
        (installation, result)
    })
    .await
    .map_err(|error| format!("Update installation task failed: {error}"))?;
    result?;
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_update_help_accepts_only_named_official_https_destinations() {
        for name in ["downloads", "installation"] {
            let destination: DesktopHelp = serde_json::from_value(serde_json::json!(name)).unwrap();
            let url = tauri::Url::parse(destination.url()).unwrap();
            assert_eq!(url.scheme(), "https");
            assert_eq!(url.host_str(), Some("santoshshinde2012.github.io"));
            assert!(url.path().starts_with("/holdmap/"));
            assert!(url.username().is_empty());
            assert!(url.password().is_none());
            assert!(url.query().is_none());
        }
        for value in [
            serde_json::json!("https://example.com/"),
            serde_json::json!("file:///Applications/holdmap.app"),
            serde_json::json!("javascript:alert(1)"),
            serde_json::json!({"downloads": "https://example.com/"}),
            serde_json::Value::Null,
        ] {
            assert!(serde_json::from_value::<DesktopHelp>(value).is_err());
        }
    }

    #[test]
    fn installs_are_serialized_and_failure_or_cancellation_allows_retry() {
        let installing = Arc::new(AtomicBool::new(false));
        let attempt = InstallGuard::claim(&installing).unwrap();
        assert!(InstallGuard::claim(&installing).is_err());
        drop(attempt);
        assert!(!installing.load(Ordering::Acquire));
        let retry = InstallGuard::claim(&installing).unwrap();
        assert!(InstallGuard::claim(&installing).is_err());
        drop(retry);
        assert!(!installing.load(Ordering::Acquire));
    }

    #[test]
    fn worker_retains_install_authority_until_it_finishes() {
        let installing = Arc::new(AtomicBool::new(false));
        let attempt = InstallGuard::claim(&installing).unwrap();
        let (started, running) = std::sync::mpsc::channel();
        let (finish, finished) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _attempt = attempt;
            started.send(()).unwrap();
            finished.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(InstallGuard::claim(&installing).is_err());
        finish.send(()).unwrap();
        worker.join().unwrap();
        assert!(InstallGuard::claim(&installing).is_ok());
    }
}

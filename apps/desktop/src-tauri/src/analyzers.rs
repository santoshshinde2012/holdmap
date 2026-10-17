//! IPC adapters for opt-in analyzers. Consent, pins, runs and normalisation live in
//! `holdmap-analyzers`; this only builds the inventory from a fresh scan and keeps runs from
//! overlapping. Approval is only possible from the main window's permissions sheet.

use crate::state::AppState;
use holdmap_analyzers::service::{AnalyzerView, Preview};
use holdmap_analyzers::{
    AnalyzerResult, AnalyzerService, FileStore, FindingState, Insights, Stores,
};
use holdmap_core::agents::AgentsReport;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub(crate) struct AnalyzersState {
    service: OnceLock<Arc<AnalyzerService>>,
    /// Held for the length of a run, so Run now and run-on-change never overlap.
    running: Arc<Mutex<()>>,
}

fn service(app: &AppHandle) -> Arc<AnalyzerService> {
    let state = app.state::<AnalyzersState>();
    state
        .service
        .get_or_init(|| {
            let dir = app.state::<AppState>().store.dir().join("analyzers");
            Arc::new(AnalyzerService::new(Stores::of(FileStore::new(dir))))
        })
        .clone()
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("background task failed: {e}"))?
}

fn inventory(app: &AppHandle) -> Result<AgentsReport, String> {
    app.state::<AppState>()
        .with_engine(Duration::from_millis(1000), |e| {
            e.agents_with_capabilities()
        })
}

/// Every analyzer and its consent state. Never runs anything.
#[tauri::command]
pub async fn analyzers_list(app: AppHandle) -> Result<Vec<AnalyzerView>, String> {
    blocking(move || Ok(service(&app).list())).await
}

/// The exact data an analyzer would get, for the permissions sheet.
#[tauri::command]
pub async fn analyzer_preview(app: AppHandle, id: String) -> Result<Preview, String> {
    blocking(move || {
        service(&app)
            .preview(&id, &inventory(&app)?)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Approve exactly the manifest and program the sheet showed.
#[tauri::command]
pub async fn analyzer_approve(app: AppHandle, id: String) -> Result<AnalyzerView, String> {
    blocking(move || service(&app).approve(&id).map_err(|e| e.to_string())).await
}

/// Turn an analyzer off and delete its results.
#[tauri::command]
pub async fn analyzer_revoke(app: AppHandle, id: String) -> Result<bool, String> {
    blocking(move || {
        let running = app.state::<AnalyzersState>().running.clone();
        let _guard = running.lock().map_err(|e| e.to_string())?;
        service(&app).revoke(&id).map_err(|e| e.to_string())
    })
    .await
}

/// Run one approved analyzer now.
#[tauri::command]
pub async fn analyzer_run(app: AppHandle, id: String) -> Result<AnalyzerResult, String> {
    blocking(move || {
        let running = app.state::<AnalyzersState>().running.clone();
        let _guard = running.lock().map_err(|e| e.to_string())?;
        service(&app)
            .run(&id, &inventory(&app)?)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Run approved analyzers whose input changed; skipped while another run is in progress.
#[tauri::command]
pub async fn analyzers_run_changed(app: AppHandle) -> Result<Vec<String>, String> {
    blocking(move || {
        let running = app.state::<AnalyzersState>().running.clone();
        let Ok(_guard) = running.try_lock() else {
            return Ok(Vec::new());
        };
        let svc = service(&app);
        if !svc
            .list()
            .iter()
            .any(|a| a.state == holdmap_analyzers::AnalyzerState::Approved)
        {
            return Ok(Vec::new());
        }
        Ok(svc.run_changed(&inventory(&app)?))
    })
    .await
}

/// Latest findings of approved analyzers.
#[tauri::command]
pub async fn insights(app: AppHandle) -> Result<Insights, String> {
    blocking(move || Ok(service(&app).insights())).await
}

/// Mark a finding reviewed, snooze it for `days`, or reopen it.
#[tauri::command]
pub async fn review_finding(
    app: AppHandle,
    fingerprint: String,
    state: FindingState,
    days: Option<u64>,
) -> Result<(), String> {
    blocking(move || {
        service(&app)
            .review(
                &fingerprint,
                state,
                days.map(|d| d.min(365).saturating_mul(86_400_000)),
            )
            .map_err(|e| e.to_string())
    })
    .await
}

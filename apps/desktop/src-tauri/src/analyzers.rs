//! IPC adapters for opt-in analyzers. Consent, pins, runs and normalisation live in
//! `holdmap-analyzers`; this only builds the inventory from a fresh scan and keeps runs from
//! overlapping. Approval is only possible from the main window's permissions sheet.

use crate::state::AppState;
use holdmap_analyzers::catalog::{CatalogDiff, CatalogStatus};
use holdmap_analyzers::finding::RunRecord;
use holdmap_analyzers::install::{InstallEvent, InstallPlan, InstallRecord};
use holdmap_analyzers::manifest::ToolSpec;
use holdmap_analyzers::mcp_map::OutputMapping;
use holdmap_analyzers::presets::PresetView;
use holdmap_analyzers::probe::SystemProbe;
use holdmap_analyzers::runner::{ProgressSink, RunEvent};
use holdmap_analyzers::service::{AnalyzerView, ConfiguredServer, McpDraft, Preview, ToolReport};
use holdmap_analyzers::{
    AnalyzerResult, AnalyzerService, FileStore, FindingState, Insights, Stores,
};
use holdmap_core::agents::AgentsReport;
use serde::Serialize;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub(crate) struct AnalyzersState {
    service: OnceLock<Arc<AnalyzerService>>,
    /// Held for the length of a run, so Run now and run-on-change never overlap.
    running: Arc<Mutex<()>>,
    /// Held for the length of an install; one at a time.
    installing: Arc<Mutex<()>>,
}

#[derive(Clone, Serialize)]
struct RunProgress<'a> {
    id: &'a str,
    event: &'a RunEvent,
}

#[derive(Clone, Serialize)]
struct InstallProgress<'a> {
    id: &'a str,
    event: &'a InstallEvent,
}

fn service(app: &AppHandle) -> Arc<AnalyzerService> {
    let state = app.state::<AnalyzersState>();
    state
        .service
        .get_or_init(|| {
            let dir = app.state::<AppState>().store.dir().join("analyzers");
            let tools = dir.join("tools");
            // Production wiring includes the OS keychain for API keys. In-app installs go to
            // holdmap's own folder, never system-wide.
            Arc::new(
                AnalyzerService::new(Stores::of(FileStore::new(dir)))
                    .with_tools_dir(tools, SystemProbe::from_env()),
            )
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

/// Approve exactly the manifest, program and tool list the sheet showed. A remote MCP
/// server also needs the sheet's explicit opt-in.
#[tauri::command]
pub async fn analyzer_approve(
    app: AppHandle,
    id: String,
    allow_remote: Option<bool>,
) -> Result<AnalyzerView, String> {
    blocking(move || {
        service(&app)
            .approve_with(&id, allow_remote.unwrap_or(false))
            .map_err(|e| e.to_string())
    })
    .await
}

/// Stop the current run: the MCP call in progress is cancelled and CLI analyzers are killed.
#[tauri::command]
pub async fn analyzer_cancel(app: AppHandle) -> Result<(), String> {
    service(&app).cancel_runs();
    Ok(())
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
        let emitter = app.clone();
        let run_id = id.clone();
        let progress = ProgressSink::new(move |event| {
            let _ = emitter.emit(
                "analyzer-run-progress",
                RunProgress {
                    id: &run_id,
                    event: &event,
                },
            );
        });
        service(&app)
            .run_with_progress(&id, &inventory(&app)?, progress)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Past runs of an analyzer (newest first): when, which version, how many findings.
#[tauri::command]
pub async fn analyzer_history(app: AppHandle, id: String) -> Result<Vec<RunRecord>, String> {
    blocking(move || service(&app).history(&id).map_err(|e| e.to_string())).await
}

/// Show a scanned skill's folder. Only folders holdmap recorded for the latest run can be
/// revealed; the renderer names a subject, never a path.
#[tauri::command]
pub async fn analyzer_reveal_subject(
    window: tauri::WebviewWindow,
    app: AppHandle,
    id: String,
    subject: String,
) -> Result<(), String> {
    crate::local_window::authorize(&window, "Showing folders")?;
    blocking(move || {
        let dir = service(&app)
            .subject_dir(&id, &subject)
            .map_err(|e| e.to_string())?;
        holdmap_core::util::reveal(&dir).map_err(|e| format!("Couldn't show the folder: {e}"))
    })
    .await
}

/// What installing an analyzer's program would do: package, version, folder, network. Runs
/// nothing.
#[tauri::command]
pub async fn analyzer_install_plan(app: AppHandle, id: String) -> Result<InstallPlan, String> {
    blocking(move || service(&app).install_plan(&id).map_err(|e| e.to_string())).await
}

/// Install an analyzer's program after the user agreed to its plan in the main window.
/// Progress arrives as `analyzer-install` events. The analyzer still needs approval.
#[tauri::command]
pub async fn analyzer_install(
    window: tauri::WebviewWindow,
    app: AppHandle,
    id: String,
) -> Result<InstallRecord, String> {
    crate::local_window::authorize(&window, "Installing analyzers")?;
    blocking(move || {
        let installing = app.state::<AnalyzersState>().installing.clone();
        let Ok(_guard) = installing.try_lock() else {
            return Err("Another install is in progress.".into());
        };
        let emitter = app.clone();
        let on = |event: InstallEvent| {
            let _ = emitter.emit(
                "analyzer-install",
                InstallProgress {
                    id: &id,
                    event: &event,
                },
            );
        };
        service(&app).install(&id, &on).map_err(|e| e.to_string())
    })
    .await
}

/// Stop the install in progress.
#[tauri::command]
pub async fn analyzer_install_cancel(app: AppHandle) -> Result<(), String> {
    service(&app).cancel_install();
    Ok(())
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

/// Mark a finding reviewed, snooze it for `days`, or reopen it, with an optional reason.
#[tauri::command]
pub async fn review_finding(
    app: AppHandle,
    fingerprint: String,
    state: FindingState,
    days: Option<u64>,
    reason: Option<String>,
) -> Result<(), String> {
    blocking(move || {
        service(&app)
            .review_with_reason(
                &fingerprint,
                state,
                days.map(|d| d.min(365).saturating_mul(86_400_000)),
                reason.as_deref(),
            )
            .map_err(|e| e.to_string())
    })
    .await
}

/// Store an API key the analyzer declares, in the OS keychain. The value is never returned.
#[tauri::command]
pub async fn analyzer_secret_set(
    app: AppHandle,
    id: String,
    name: String,
    value: String,
) -> Result<(), String> {
    blocking(move || {
        service(&app)
            .set_secret(&id, &name, &holdmap_analyzers::secrets::Secret::new(value))
            .map_err(|e| e.to_string())
    })
    .await
}

/// Delete a stored API key.
#[tauri::command]
pub async fn analyzer_secret_delete(
    app: AppHandle,
    id: String,
    name: String,
) -> Result<bool, String> {
    blocking(move || {
        service(&app)
            .delete_secret(&id, &name)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Allow or stop automatic runs, when items change, for an approved analyzer that uses the
/// network.
#[tauri::command]
pub async fn analyzer_background(
    app: AppHandle,
    id: String,
    allow: bool,
) -> Result<AnalyzerView, String> {
    blocking(move || {
        service(&app)
            .set_background(&id, allow)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Which signed catalog is in use and any update waiting for review.
#[tauri::command]
pub async fn analyzer_catalog(app: AppHandle) -> Result<CatalogStatus, String> {
    blocking(move || Ok(service(&app).catalog_status())).await
}

/// Download the newest signed catalog (only when the user clicks) and stage it for review.
#[tauri::command]
pub async fn analyzer_catalog_fetch(app: AppHandle) -> Result<CatalogDiff, String> {
    blocking(move || service(&app).fetch_catalog().map_err(|e| e.to_string())).await
}

/// Apply the staged catalog update.
#[tauri::command]
pub async fn analyzer_catalog_accept(app: AppHandle) -> Result<CatalogStatus, String> {
    blocking(move || service(&app).accept_catalog().map_err(|e| e.to_string())).await
}

/// Discard the staged catalog update.
#[tauri::command]
pub async fn analyzer_catalog_reject(app: AppHandle) -> Result<(), String> {
    blocking(move || service(&app).reject_catalog().map_err(|e| e.to_string())).await
}

/// Register an analyzer from manifest text the user pasted. It stays off until approved.
#[tauri::command]
pub async fn analyzer_add(app: AppHandle, manifest: String) -> Result<AnalyzerView, String> {
    blocking(move || {
        service(&app)
            .add_manifest(&manifest)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Ready-made manifests for well-known local analyzers.
#[tauri::command]
pub async fn analyzer_presets(app: AppHandle) -> Result<Vec<PresetView>, String> {
    blocking(move || Ok(service(&app).presets())).await
}

/// Register a preset as the user's own analyzer; it stays off until approved.
#[tauri::command]
pub async fn analyzer_add_preset(app: AppHandle, id: String) -> Result<AnalyzerView, String> {
    blocking(move || service(&app).add_preset(&id).map_err(|e| e.to_string())).await
}

/// Remove an analyzer the user registered.
#[tauri::command]
pub async fn analyzer_remove(app: AppHandle, id: String) -> Result<bool, String> {
    blocking(move || {
        service(&app)
            .remove_manifest(&id)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Register any MCP server (a command, a URL or one of the agents' servers). It stays off.
#[tauri::command]
pub async fn analyzer_add_mcp(app: AppHandle, draft: McpDraft) -> Result<AnalyzerView, String> {
    blocking(move || service(&app).add_mcp(&draft).map_err(|e| e.to_string())).await
}

/// The agents' configured MCP servers, as drafts where holdmap knows enough. Starts nothing.
#[tauri::command]
pub async fn analyzer_configured_servers(app: AppHandle) -> Result<Vec<ConfiguredServer>, String> {
    blocking(move || Ok(service(&app).configured_servers(&inventory(&app)?))).await
}

/// Start (or contact) the server once to list its tools, after the user clicked List tools.
/// No tool is called.
#[tauri::command]
pub async fn analyzer_list_tools(app: AppHandle, id: String) -> Result<ToolReport, String> {
    blocking(move || {
        let running = app.state::<AnalyzersState>().running.clone();
        let _guard = running.lock().map_err(|e| e.to_string())?;
        service(&app).list_tools(&id).map_err(|e| e.to_string())
    })
    .await
}

/// The saved tool listing. Starts nothing.
#[tauri::command]
pub async fn analyzer_tools(app: AppHandle, id: String) -> Result<ToolReport, String> {
    blocking(move || service(&app).tools(&id).map_err(|e| e.to_string())).await
}

/// Save the chosen tools and their input mapping. Approval is needed again.
#[tauri::command]
pub async fn analyzer_map_tools(
    app: AppHandle,
    id: String,
    tools: Vec<ToolSpec>,
) -> Result<AnalyzerView, String> {
    blocking(move || {
        service(&app)
            .map_tools(&id, &tools)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Save where findings sit in the tools' output. Approval is needed again.
#[tauri::command]
pub async fn analyzer_map_output(
    app: AppHandle,
    id: String,
    output: OutputMapping,
) -> Result<AnalyzerView, String> {
    blocking(move || {
        service(&app)
            .map_output(&id, &output)
            .map_err(|e| e.to_string())
    })
    .await
}

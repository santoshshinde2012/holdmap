//! Tauri commands: thin adapters from the Svelte UI onto `portwise-core`. No business logic
//! lives here — scanning, planning, topology and history are all core abstractions.

use crate::state::{scan_now, AppState};
use crate::tray::refresh_tray;
use portwise_core::history::{self, HistoryEntry};
use portwise_core::store::Config;
use portwise_core::topology::Graph;
use portwise_core::{
    execute, ActionPlan, Engine, Explanation, Snapshot, StopOptions, StopReport, Target,
};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    platform: &'static str,
    tray: bool,
    /// Global shortcut that shows the window, when it registered.
    shortcut: Option<&'static str>,
}

#[derive(Clone, Serialize)]
struct Progress {
    target: String,
    line: String,
}

/// Result of restarting a stopped command.
#[derive(Serialize)]
pub struct Restarted {
    pid: u32,
    command: String,
    log: String,
}

fn stop_options(force: bool, allow_protected: bool) -> StopOptions {
    StopOptions {
        force,
        allow_protected,
        ..StopOptions::default()
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("background task failed: {e}"))?
}

async fn with_engine<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(&Engine) -> T + Send + 'static,
) -> Result<T, String> {
    let app = app.clone();
    blocking(move || {
        let state = app.state::<AppState>();
        let mut guard = state.engine.lock().unwrap();
        if guard.is_none() {
            *guard = Some(scan_now(false, state.docker())?);
        }
        Ok(f(guard.as_ref().unwrap()))
    })
    .await
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        tray: state.tray_ok.load(Ordering::Relaxed),
        shortcut: crate::shortcuts::registered(),
    }
}

/// Scan the machine and return a fresh snapshot.
#[tauri::command]
pub async fn scan(app: AppHandle, all: bool, docker: Option<bool>) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    if let Some(d) = docker {
        state.docker.store(d, Ordering::Relaxed);
    }
    let docker = state.docker();
    let engine = blocking(move || scan_now(all, docker)).await?;
    let snapshot = engine.snapshot().clone();
    *state.engine.lock().unwrap() = Some(engine);
    refresh_tray(&app, &snapshot);
    Ok(snapshot)
}

/// The service graph of the most recent scan. Dev services (and their peers) unless `all`.
#[tauri::command]
pub async fn topology(app: AppHandle, all: bool) -> Result<Graph, String> {
    with_engine(&app, move |e| {
        let mut g = e.topology();
        if !all {
            g.retain_dev();
        }
        g
    })
    .await
}

/// Explain why `port` is busy, using the most recent scan (or a new one).
#[tauri::command]
pub async fn explain(app: AppHandle, port: u16) -> Result<Explanation, String> {
    with_engine(&app, move |e| e.explain(port, &StopOptions::default())).await
}

/// Build (but don't run) the stop plan for a target such as `3000`, `pid:123`, `node` or
/// `cluster:acme-shop` (dependency-ordered).
#[tauri::command]
pub async fn plan(
    app: AppHandle,
    target: String,
    force: bool,
    allow_protected: bool,
) -> Result<ActionPlan, String> {
    let opts = stop_options(force, allow_protected);
    with_engine(&app, move |e| e.plan(&Target::parse(&target), &opts)).await
}

/// Re-scan, re-plan and execute a stop. The plan is rebuilt from a fresh scan so a PID that was
/// reused since the UI last refreshed can never be signalled; `execute` re-checks protection.
/// What was stopped is recorded in the history so it can be restarted.
#[tauri::command]
pub async fn stop(
    app: AppHandle,
    target: String,
    force: bool,
    allow_protected: bool,
) -> Result<StopReport, String> {
    let docker = app.state::<AppState>().docker();
    let emitter = app.clone();
    blocking(move || {
        let engine = scan_now(false, docker)?;
        let plan = engine.plan(
            &Target::parse(&target),
            &stop_options(force, allow_protected),
        );
        if let Some(b) = &plan.blocked {
            return Err(b.message.clone());
        }
        let report = execute(&plan, &mut |line| {
            let _ = emitter.emit(
                "stop-progress",
                Progress {
                    target: target.clone(),
                    line: line.to_string(),
                },
            );
        });
        let entries = history::entries_from_plan(&engine.scan, &plan, &report);
        let _ = emitter.state::<AppState>().store.record(&entries);
        Ok(report)
    })
    .await
}

/// First free TCP port at or after `near` (checked with real bind probes).
#[tauri::command]
pub async fn free_port(near: u16) -> Result<Option<u16>, String> {
    blocking(move || {
        Ok((near.max(1)..=u16::MAX).take(2000).find(|p| {
            portwise_core::probe::probe_tcp(*p) == portwise_core::probe::ProbeResult::Free
        }))
    })
    .await
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Config {
    state.store.config()
}

/// Pin or unpin a port; returns the updated config.
#[tauri::command]
pub fn toggle_pin(
    state: State<'_, AppState>,
    port: u16,
    label: Option<String>,
) -> Result<Config, String> {
    state
        .store
        .update_config(|c| c.toggle_pin(port, label))
        .map(|(c, _)| c)
        .map_err(|e| e.to_string())
}

/// Notification preferences.
#[tauri::command]
pub fn set_notify(
    state: State<'_, AppState>,
    notify: bool,
    dev_only: bool,
) -> Result<Config, String> {
    state
        .store
        .update_config(|c| {
            c.notify = notify;
            c.notify_dev_only = dev_only;
        })
        .map(|(c, _)| c)
        .map_err(|e| e.to_string())
}

/// Recently stopped services, newest first.
#[tauri::command]
pub fn history(state: State<'_, AppState>, limit: Option<usize>) -> Vec<HistoryEntry> {
    state.store.history(limit.unwrap_or(50))
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    state.store.clear_history().map_err(|e| e.to_string())
}

/// Start a previously stopped command again in its original directory.
#[tauri::command]
pub async fn restart(app: AppHandle, entry: HistoryEntry) -> Result<Restarted, String> {
    blocking(move || {
        if !entry.restartable() {
            return Err(format!(
                "portwise didn't record a command for :{} — start it the usual way",
                entry.port
            ));
        }
        let logs = app.state::<AppState>().store.logs_dir();
        let (pid, log) = history::restart(&entry, &logs).map_err(|e| e.to_string())?;
        Ok(Restarted {
            pid,
            command: entry.command_line(),
            log: log.display().to_string(),
        })
    })
    .await
}

/// Whether portwise launches at login.
#[tauri::command]
pub fn autostart(app: AppHandle, enable: Option<bool>) -> Result<bool, String> {
    crate::shortcuts::autostart(&app, enable)
}

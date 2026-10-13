//! Tauri adapters from the Svelte UI onto `holdmap-core`, with native confirmation and folder
//! authorization boundaries. Scanning, planning, topology and history are core abstractions.

use crate::confirmation::{validate_fresh, Preview};
use crate::state::{scan_now, AppState};
use crate::tray::refresh_tray;
use holdmap_core::agents::AgentsReport;
use holdmap_core::history::{self, HistoryEntry};
use holdmap_core::store::{Config, Store};
use holdmap_core::topology::Graph;
use holdmap_core::{execute, Engine, Explanation, Snapshot, StopOptions, StopReport, Target};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    platform: &'static str,
    tray: bool,
    /// Global shortcut that shows the window, when it registered.
    shortcut: Option<String>,
    /// Where config and history live.
    config_dir: String,
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

/// Keep the outcome of process control distinct from persistence of its restart recording.
#[derive(Serialize)]
pub struct Stopped {
    #[serde(flatten)]
    pub(crate) report: StopReport,
    pub(crate) history_error: Option<String>,
}

fn record_stop(store: &Store, entries: &[HistoryEntry], report: StopReport) -> Stopped {
    let history_error = store
        .record(entries)
        .err()
        .map(|error| holdmap_core::redact::line(&error.to_string()));
    Stopped {
        report,
        history_error,
    }
}

fn stop_options(force: bool, allow_protected: bool) -> StopOptions {
    StopOptions {
        force,
        allow_protected,
        ..StopOptions::default()
    }
}

/// Desktop rows distinguish TCP and UDP listeners. A protocol suffix must never fall
/// through to a process-name target (a process can itself be named `5353/udp`).
pub(crate) fn stop_request(
    target: &str,
    force: bool,
    allow_protected: bool,
) -> (Target, StopOptions) {
    let mut opts = stop_options(force, allow_protected);
    if let Some((port, protocol)) = target.trim().split_once('/') {
        let protocol = match protocol {
            "tcp" => Some(holdmap_core::Protocol::Tcp),
            "udp" => Some(holdmap_core::Protocol::Udp),
            _ => None,
        };
        if let (Ok(port), Some(protocol)) = (port.parse::<u16>(), protocol) {
            opts.protocol = Some(protocol);
            return (Target::Port(port), opts);
        }
    }
    (Target::parse(target), opts)
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("background task failed: {e}"))?
}

/// File locks, private-file validation, parsing and fsync must not block the native UI.
async fn with_store<T: Send + 'static>(
    app: AppHandle,
    operation: impl FnOnce(&Store) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let store = app.state::<AppState>().store.clone();
    blocking(move || operation(&store)).await
}

async fn with_engine<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(&Engine) -> T + Send + 'static,
) -> Result<T, String> {
    let app = app.clone();
    blocking(move || {
        let state = app.state::<AppState>();
        state.with_engine(FRESH, f)
    })
    .await
}

#[tauri::command]
pub async fn app_info(app: AppHandle) -> Result<AppInfo, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        Ok(AppInfo {
            version: env!("CARGO_PKG_VERSION"),
            platform: std::env::consts::OS,
            tray: state.tray_ok.load(Ordering::Relaxed),
            shortcut: crate::shortcuts::registered(),
            config_dir: state.store.dir().display().to_string(),
        })
    })
    .await
}

/// How old a scan the UI's poll may share (the watcher scans on the same cadence).
const FRESH: Duration = Duration::from_millis(1000);

/// Scan the machine and return a snapshot no older than `max_age_ms` (default 1 s; 0 forces
/// a new scan). Scans never overlap: one in flight is waited for and shared.
#[tauri::command]
pub async fn scan(
    app: AppHandle,
    all: bool,
    docker: Option<bool>,
    max_age_ms: Option<u64>,
) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    if let Some(d) = docker {
        state.docker.store(d, Ordering::Relaxed);
    }
    let max_age = max_age_ms.map_or(FRESH, Duration::from_millis);
    let handle = app.clone();
    let snapshot = blocking(move || handle.state::<AppState>().snapshot(all, max_age)).await?;
    // The tray menu is updated on the main thread; don't make the scan wait for it.
    let (handle, snap) = (app.clone(), snapshot.clone());
    let _ = app.run_on_main_thread(move || refresh_tray(&handle, &snap));
    Ok(snapshot)
}

/// AI coding agents in a scan no older than one second: folders, ports, connections and access.
#[tauri::command]
pub async fn agents(app: AppHandle) -> Result<AgentsReport, String> {
    with_engine(&app, |e| e.agents()).await
}

/// Read-only process and socket relationships from one consistent local scan.
#[tauri::command]
pub async fn agent_flow(
    app: AppHandle,
) -> Result<holdmap_core::agents::flow::AgentFlowReport, String> {
    with_engine(&app, |engine| {
        let agents = engine.agents();
        holdmap_core::agents::flow::flow(&engine.scan, &agents)
    })
    .await
}

/// Service graph from a scan no older than one second. Dev services (and peers) unless `all`.
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

/// Ask a local port what it serves over HTTP (status, page title, server), with a short
/// `GET /`. `None` when it doesn't answer HTTP.
#[tauri::command]
pub async fn http_info(port: u16) -> Result<Option<holdmap_core::http::HttpInfo>, String> {
    blocking(move || {
        Ok(holdmap_core::http::probe(
            port,
            "/",
            std::time::Duration::from_millis(1200),
        ))
    })
    .await
}

/// Explain why `port` is busy, using a scan no older than one second.
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
) -> Result<Preview, String> {
    let (parsed, opts) = stop_request(&target, force, allow_protected);
    let plan = with_engine(&app, move |e| e.plan(&parsed, &opts)).await?;
    Ok(app
        .state::<AppState>()
        .confirmations
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .preview(target, force, allow_protected, plan))
}

/// Consume the reviewed plan, then verify a fresh scan still has exactly those effects.
/// A changed target requires a new confirmation; execution also re-checks PID identity and
/// protection. What was stopped is recorded in the history so it can be restarted.
#[tauri::command]
pub async fn stop(
    app: AppHandle,
    target: String,
    force: bool,
    allow_protected: bool,
    confirmation_id: String,
) -> Result<Stopped, String> {
    let docker = app.state::<AppState>().docker();
    let emitter = app.clone();
    blocking(move || {
        let confirmed = emitter
            .state::<AppState>()
            .confirmations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take(&confirmation_id, &target, force, allow_protected)?;
        let engine = scan_now(false, docker)?;
        let (parsed, opts) = stop_request(&target, force, allow_protected);
        let plan = engine.plan(&parsed, &opts);
        validate_fresh(&confirmed, &plan)?;
        let report = execute(&confirmed, &mut |line| {
            let _ = emitter.emit(
                "stop-progress",
                Progress {
                    target: target.clone(),
                    line: line.to_string(),
                },
            );
        });
        let entries = history::entries_from_plan(&engine.scan, &confirmed, &report);
        Ok(record_stop(
            &emitter.state::<AppState>().store,
            &entries,
            report,
        ))
    })
    .await
}

/// Consume every reviewed listener handle and preflight one fresh agent scan before any
/// effect. Retained plans allow a shared child to release several ports with one exit.
#[tauri::command]
pub async fn stop_many(
    app: AppHandle,
    agent_id: String,
    requests: Vec<crate::scheduler::ScheduledStopRequest>,
) -> Result<Vec<Stopped>, String> {
    crate::scheduler::validate_batch_requests(&requests)?;
    if agent_id.is_empty() || agent_id.len() > 64 {
        return Err("Invalid agent identity.".into());
    }
    blocking(move || {
        let state = app.state::<AppState>();
        let plans = {
            let mut confirmations = state
                .confirmations
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            crate::stop_batch::take(&mut confirmations, &requests)?
        };
        let engine = scan_now(false, state.docker())?;
        crate::stop_batch::run(&engine, &agent_id, &plans, |pinned, execution| {
            let report = execute(execution, &mut |line| {
                let _ = app.emit(
                    "stop-progress",
                    Progress {
                        target: pinned.target.clone(),
                        line: line.to_string(),
                    },
                );
            });
            let entries = history::entries_from_plan(&engine.scan, &pinned.plan, &report);
            record_stop(&state.store, &entries, report)
        })
    })
    .await
}

/// First free TCP port at or after `near` (checked with real bind probes).
#[tauri::command]
pub async fn free_port(near: u16) -> Result<Option<u16>, String> {
    blocking(move || {
        Ok((near.max(1)..=u16::MAX)
            .take(2000)
            .find(|p| holdmap_core::probe::probe_tcp(*p) == holdmap_core::probe::ProbeResult::Free))
    })
    .await
}

#[tauri::command]
pub async fn get_config(app: AppHandle) -> Result<Config, String> {
    with_store(app, |store| {
        store
            .try_config()
            .map_err(|error| format!("Couldn't read settings: {error}"))
    })
    .await
}

/// Pin or unpin a port; returns the updated config.
#[tauri::command]
pub async fn toggle_pin(
    app: AppHandle,
    port: u16,
    label: Option<String>,
) -> Result<Config, String> {
    // Quick pin uses an observed project title; keep its automatic label within the same
    // bound as the editable label without rejecting a service with a long project name.
    let label = label.map(|label| label.chars().take(40).collect::<String>());
    validate_pin(port, label.as_deref())?;
    with_store(app, move |store| {
        save(store, |c| {
            c.toggle_pin(port, label);
        })
    })
    .await
}

/// Notification preferences.
#[tauri::command]
pub async fn set_notify(app: AppHandle, notify: bool, dev_only: bool) -> Result<Config, String> {
    with_store(app, move |store| {
        save(store, |c| {
            c.notify = notify;
            c.notify_dev_only = dev_only;
        })
    })
    .await
}

/// Recently stopped services, newest first. Secrets in their commands are hidden: the real
/// command never leaves the backend.
#[tauri::command]
pub async fn history(app: AppHandle, limit: Option<usize>) -> Result<Vec<HistoryEntry>, String> {
    with_store(app, move |store| {
        Ok(store
            .try_history(limit.unwrap_or(50))
            .map_err(|error| format!("Couldn't read stop history: {error}"))?
            .iter()
            .map(HistoryEntry::redacted)
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn clear_history(app: AppHandle) -> Result<(), String> {
    with_store(app, |store| {
        store.clear_history().map_err(|e| e.to_string())
    })
    .await
}

/// Start a previously stopped command again in its original directory. The webview names the
/// history entry (when it was stopped, which port); the command itself is read from the
/// history file, so the UI can never make the app run a command of its choosing.
#[tauri::command]
pub async fn restart(
    app: AppHandle,
    at_ms: u64,
    port: u16,
    protocol: Option<holdmap_core::Protocol>,
    pid: Option<u32>,
) -> Result<Restarted, String> {
    blocking(move || {
        let entry = restart_recording(
            &app.state::<AppState>().store,
            Some(at_ms),
            port,
            protocol,
            pid,
        )?;
        if !entry.restartable() {
            return Err(format!(
                "holdmap can't safely restart this recording for :{} — start it the usual way",
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

/// After the UI stopped `port` (through the normal confirm-and-stop flow), start what it just
/// recorded again: "Restart" for a running dev server. Only an entry recorded in the last
/// two minutes counts, so this never resurrects something stopped long ago.
#[tauri::command]
pub async fn restart_stopped(
    app: AppHandle,
    port: u16,
    protocol: Option<holdmap_core::Protocol>,
    pid: Option<u32>,
) -> Result<Restarted, String> {
    blocking(move || {
        let store = &app.state::<AppState>().store;
        let entry = restart_recording(store, None, port, protocol, pid)?;
        let (pid, log) = history::restart(&entry, &store.logs_dir()).map_err(|e| e.to_string())?;
        Ok(Restarted {
            pid,
            command: entry.command_line(),
            log: log.display().to_string(),
        })
    })
    .await
}

fn restart_recording(
    store: &holdmap_core::store::Store,
    at_ms: Option<u64>,
    port: u16,
    protocol: Option<holdmap_core::Protocol>,
    pid: Option<u32>,
) -> Result<HistoryEntry, String> {
    let entry = match at_ms {
        Some(at_ms) => store.entry_matching(at_ms, port, protocol, pid),
        None => store.last_matching(port, protocol, pid),
    }
    .map_err(|error| error.to_string())?;
    entry
        .filter(|entry| {
            at_ms.is_some() || holdmap_core::util::now_ms().saturating_sub(entry.at_ms) < 120_000
        })
        .filter(HistoryEntry::restartable)
        .ok_or_else(|| {
            format!(
                "holdmap has no safely restartable recording for :{port} — start it the usual way"
            )
        })
}

/// Connections, process tree, uptime and bind risk for the listeners on `port`. Loaded
/// lazily for the selected port only, from a scan that includes connected sockets.
#[tauri::command]
pub async fn port_details(port: u16) -> Result<Vec<holdmap_core::details::PortDetails>, String> {
    blocking(move || {
        let engine = scan_now(true, false)?;
        Ok(holdmap_core::details::for_port(&engine.scan, port))
    })
    .await
}

/// The folder the service on `port` runs in: its project root, else the process's directory.
/// Looked up from the latest scan; the webview only ever names a port.
fn folder_for(engine: &Engine, port: u16) -> Result<std::path::PathBuf, String> {
    let e = engine
        .snapshot()
        .entries
        .iter()
        .find(|x| {
            x.port == port
                && x.state.is_listening()
                && (x.project.is_some() || x.process.as_ref().is_some_and(|p| p.cwd.is_some()))
        })
        .cloned()
        .ok_or_else(|| format!("No project folder is known for :{port}"))?;
    e.project
        .map(|p| p.root)
        .or_else(|| e.process.and_then(|p| p.cwd))
        .ok_or_else(|| format!("No project folder is known for :{port}"))
}

/// Open `http://localhost:<port>` in the default browser.
#[tauri::command]
pub async fn open_port(port: u16) -> Result<(), String> {
    if port == 0 {
        return Err("Port must be between 1 and 65535".into());
    }
    blocking(move || {
        holdmap_core::util::open_url(&format!("http://localhost:{port}")).map_err(|e| e.to_string())
    })
    .await
}

/// Open the project folder of the service on `port` in Finder / Explorer.
#[tauri::command]
pub async fn reveal_project(app: AppHandle, port: u16) -> Result<(), String> {
    blocking(move || {
        let dir = app
            .state::<AppState>()
            .with_engine(Duration::ZERO, |e| folder_for(e, port))??;
        holdmap_core::util::reveal(&dir).map_err(|e| e.to_string())
    })
    .await
}

/// Open the project folder of the service on `port` in the user's editor; returns its name.
#[tauri::command]
pub async fn open_in_editor(app: AppHandle, port: u16) -> Result<String, String> {
    blocking(move || {
        let dir = app
            .state::<AppState>()
            .with_engine(Duration::ZERO, |e| folder_for(e, port))??;
        holdmap_core::util::open_in_editor(&dir).map_err(|e| e.to_string())
    })
    .await
}

/// Resolve a folder the Agents view named: it must belong to `agent_id` in the current report,
/// so the webview never opens an arbitrary path.
fn agent_folder(
    report: &AgentsReport,
    agent_id: &str,
    path: &str,
) -> Result<std::path::PathBuf, String> {
    let want = std::path::PathBuf::from(path);
    if !want.is_absolute() {
        return Err("Folder path must be absolute".into());
    }
    let agent = report
        .agents
        .iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| format!("No agent `{agent_id}` in the current scan"))?;
    if !agent.known_folders().any(|p| p == &want) {
        return Err("That folder isn't part of this agent's footprint".into());
    }
    Ok(want)
}

/// Open an agent's folder in Finder / Explorer (path must appear on that agent).
#[tauri::command]
pub async fn reveal_agent_folder(
    app: AppHandle,
    agent_id: String,
    path: String,
) -> Result<(), String> {
    blocking(move || {
        let dir = app.state::<AppState>().with_engine(Duration::ZERO, |e| {
            agent_folder(&e.agents(), &agent_id, &path)
        })??;
        holdmap_core::util::reveal(&dir).map_err(|e| e.to_string())
    })
    .await
}

/// Open an agent's folder in the user's editor; returns the editor's name.
#[tauri::command]
pub async fn open_agent_folder(
    app: AppHandle,
    agent_id: String,
    path: String,
) -> Result<String, String> {
    blocking(move || {
        let dir = app.state::<AppState>().with_engine(Duration::ZERO, |e| {
            agent_folder(&e.agents(), &agent_id, &path)
        })??;
        holdmap_core::util::open_in_editor(&dir).map_err(|e| e.to_string())
    })
    .await
}

/// Whether holdmap launches at login.
#[tauri::command]
pub async fn autostart(app: AppHandle, enable: Option<bool>) -> Result<bool, String> {
    blocking(move || crate::shortcuts::autostart(&app, enable)).await
}

fn save(store: &Store, f: impl FnOnce(&mut Config)) -> Result<Config, String> {
    store
        .update_config(f)
        .map(|(c, _)| c)
        .map_err(|e| format!("Couldn't save settings: {e}"))
}

fn validate_pin(port: u16, label: Option<&str>) -> Result<(), String> {
    if port == 0 {
        return Err("Port must be between 1 and 65535".into());
    }
    if label.is_some_and(|label| label.chars().count() > 40) {
        return Err("Keep the label to 40 characters or fewer".into());
    }
    Ok(())
}

/// Pin a port (or update its label).
#[tauri::command]
pub async fn set_pin(app: AppHandle, port: u16, label: Option<String>) -> Result<Config, String> {
    validate_pin(port, label.as_deref())?;
    with_store(app, move |store| save(store, |c| c.set_pin(port, label))).await
}

#[tauri::command]
pub async fn unpin(app: AppHandle, port: u16) -> Result<Config, String> {
    with_store(app, move |store| {
        save(store, |c| {
            c.unpin(port);
        })
    })
    .await
}

/// Scan interval and history length (validated against the core's allowed ranges).
#[tauri::command]
pub async fn set_preferences(
    app: AppHandle,
    scan_interval_secs: Option<u64>,
    history_limit: Option<usize>,
) -> Result<Config, String> {
    if let Some(s) = scan_interval_secs {
        if !Config::SCAN_INTERVAL.contains(&s) {
            return Err(format!(
                "Scan interval must be {}–{} seconds",
                Config::SCAN_INTERVAL.start(),
                Config::SCAN_INTERVAL.end()
            ));
        }
    }
    if let Some(h) = history_limit {
        if !Config::HISTORY_LIMIT.contains(&h) {
            return Err(format!(
                "History must keep {}–{} entries",
                Config::HISTORY_LIMIT.start(),
                Config::HISTORY_LIMIT.end()
            ));
        }
    }
    with_store(app, move |store| {
        save(store, |c| {
            if let Some(s) = scan_interval_secs {
                c.scan_interval_secs = s;
            }
            if let Some(h) = history_limit {
                c.history_limit = h;
            }
        })
    })
    .await
}

#[derive(Serialize)]
pub struct HotkeyPreset {
    id: &'static str,
    label: &'static str,
}

/// The global-shortcut presets, labelled for this OS.
#[tauri::command]
pub fn hotkeys() -> Vec<HotkeyPreset> {
    crate::shortcuts::PRESETS
        .iter()
        .filter_map(|(id, ..)| crate::shortcuts::label(id).map(|label| HotkeyPreset { id, label }))
        .collect()
}

/// Change the global shortcut; persists only when registration succeeded.
#[tauri::command]
pub async fn set_hotkey(app: AppHandle, preset: String) -> Result<Option<String>, String> {
    blocking(move || {
        crate::shortcuts::apply_and_save(&app, &preset, || {
            save(&app.state::<AppState>().store, |c| {
                c.hotkey = preset.clone()
            })
            .map(|_| ())
        })
    })
    .await
}

/// Read-only scan of another machine over SSH (agentless: `ss` + `ps`). `local` reads this
/// machine through `sh`, which is handy without an SSH server.
#[tauri::command]
pub async fn remote_scan(app: AppHandle, host: String) -> Result<Snapshot, String> {
    use holdmap_core::remote::{scan_remote, validate_host, LocalShell, RemoteRunner, SshRunner};
    let host = host.trim().to_string();
    validate_host(&host)?;
    blocking(move || {
        let runner: Box<dyn RemoteRunner> = if host == "local" {
            Box::new(LocalShell)
        } else {
            Box::new(SshRunner::new(host.clone()))
        };
        let scan = scan_remote(runner.as_ref(), &host).map_err(|e| e.to_string())?;
        let _ = app
            .state::<AppState>()
            .store
            .update_config(|c| c.remember_host(&host));
        Ok(scan.snapshot)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_core::agents::AgentsBuilder;
    use holdmap_core::{ProcessInfo, ProcessTable, Protocol, Scan};

    fn recorded(port: u16, at_ms: u64, protocol: Protocol, pid: u32) -> HistoryEntry {
        HistoryEntry {
            at_ms,
            port,
            protocol,
            pid,
            label: format!("fixture {pid}"),
            command: vec![format!("fixture-owner-{pid}")],
            cwd: None,
            project: None,
            framework: None,
            windows_restart: None,
            restartable: None,
            copyable: None,
        }
    }

    fn completed_report() -> StopReport {
        StopReport {
            target: "3000/tcp".into(),
            success: true,
            freed: true,
            ports_still_busy: vec![],
            signalled: vec![100],
            escalated: false,
            survivors: vec![],
            elapsed_ms: 17,
            log: vec!["Port 3000 is free".into()],
            error: None,
        }
    }

    #[test]
    fn stop_outcome_and_history_failure_are_reported_independently() {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("state");
        std::fs::write(&state, b"preserve existing non-directory").unwrap();
        let report = record_stop(
            &Store::new(&state),
            &[recorded(3000, 7, Protocol::Tcp, 100)],
            completed_report(),
        );
        assert!(report.report.success && report.report.freed);
        assert!(report.history_error.is_some());
        assert_eq!(report.report.signalled, [100]);
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["freed"], true);
        assert!(json["history_error"].is_string());
        assert!(
            json.get("report").is_none(),
            "original IPC result fields stay flat"
        );
        assert_eq!(
            std::fs::read(&state).unwrap(),
            b"preserve existing non-directory"
        );
    }

    #[test]
    fn successful_stop_recording_is_available_for_the_exact_restart_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let entry = recorded(3000, 7, Protocol::Tcp, 100);
        let report = record_stop(&store, std::slice::from_ref(&entry), completed_report());
        assert!(report.history_error.is_none());
        assert!(report.report.success && report.report.freed);
        assert_eq!(
            store
                .entry_matching(7, 3000, Some(Protocol::Tcp), Some(100))
                .unwrap(),
            Some(entry)
        );
    }

    #[test]
    fn both_pin_commands_share_the_same_native_validation() {
        assert!(validate_pin(0, None).is_err());
        assert!(validate_pin(3000, Some(&"x".repeat(41))).is_err());
        assert!(validate_pin(3000, Some(&"界".repeat(40))).is_ok());
    }

    #[test]
    fn history_restart_selects_exact_protocol_and_owner_and_refuses_ambiguity() {
        let dir = tempfile::tempdir().unwrap();
        let store = holdmap_core::store::Store::new(dir.path());
        let at_ms = holdmap_core::util::now_ms();
        let tcp = recorded(3000, at_ms, Protocol::Tcp, 100);
        let udp = recorded(3000, at_ms, Protocol::Udp, 200);
        store.record(&[tcp.clone(), udp.clone()]).unwrap();
        assert!(restart_recording(&store, Some(at_ms), 3000, None, None).is_err());
        assert_eq!(
            restart_recording(&store, Some(at_ms), 3000, Some(Protocol::Tcp), Some(100)).unwrap(),
            tcp
        );
        assert_eq!(
            restart_recording(&store, Some(at_ms), 3000, Some(Protocol::Udp), Some(200)).unwrap(),
            udp
        );
        assert!(
            restart_recording(&store, Some(at_ms), 3000, Some(Protocol::Udp), Some(100)).is_err()
        );
        store
            .record(&[recorded(3001, at_ms, Protocol::Tcp, 100)])
            .unwrap();
        assert!(
            restart_recording(&store, Some(at_ms), 3001, None, None).is_ok(),
            "unique older callers remain compatible"
        );
        store
            .record(&[recorded(3000, at_ms, Protocol::Tcp, 100)])
            .unwrap();
        assert!(
            restart_recording(&store, Some(at_ms), 3000, Some(Protocol::Tcp), Some(100)).is_err()
        );
    }

    #[test]
    fn running_restart_keeps_selected_root_and_protocol_and_refuses_stale_or_unsafe_record() {
        let dir = tempfile::tempdir().unwrap();
        let store = holdmap_core::store::Store::new(dir.path());
        let now = holdmap_core::util::now_ms();
        let selected = recorded(3000, now - 2, Protocol::Tcp, 100);
        let sibling = recorded(3000, now - 1, Protocol::Tcp, 200);
        let udp = recorded(3000, now, Protocol::Udp, 300);
        store.record(&[selected.clone(), sibling, udp]).unwrap();
        assert_eq!(
            restart_recording(&store, None, 3000, Some(Protocol::Tcp), Some(100)).unwrap(),
            selected
        );
        assert!(restart_recording(&store, None, 3000, Some(Protocol::Tcp), Some(300)).is_err());
        let stale = recorded(3001, now - 120_000, Protocol::Tcp, 100);
        store.record(&[stale]).unwrap();
        assert!(restart_recording(&store, None, 3001, Some(Protocol::Tcp), Some(100)).is_err());
        let unsafe_record = HistoryEntry {
            at_ms: now,
            command: vec![],
            ..selected
        };
        store.record(&[unsafe_record]).unwrap();
        assert!(
            restart_recording(&store, None, 3000, Some(Protocol::Tcp), Some(100)).is_err(),
            "must not fall back to an older launch"
        );
    }

    fn folders(path: &str) -> AgentsReport {
        let process = ProcessInfo {
            pid: 100,
            ppid: None,
            name: "codex".into(),
            exe: None,
            cmdline: vec!["codex".into()],
            cwd: Some(path.into()),
            uid: Some(1000),
            user: Some("fixture".into()),
            start_time: 1,
            start_token: 1,
            memory_bytes: 0,
            cpu_percent: 0.0,
        };
        let scan = Scan {
            snapshot: Snapshot {
                entries: vec![],
                hidden_sockets: 0,
                platform: "fixture".into(),
                taken_at_ms: 1,
                scan_ms: 0,
                docker_available: false,
                warnings: vec![],
            },
            table: ProcessTable::from_processes([(100, process)].into(), 999),
            published: vec![],
            raw: vec![],
        };
        AgentsBuilder::new(&scan)
            .with_home(None)
            .with_project_detection(false)
            .build()
    }

    #[test]
    fn folder_authorization_requires_current_agent_and_exact_known_path() {
        let root = std::env::current_dir().unwrap();
        let project = root
            .join("holdmap-fixture-project")
            .to_string_lossy()
            .into_owned();
        let other = root
            .join("holdmap-fixture-project-other")
            .to_string_lossy()
            .into_owned();
        let new_project = root
            .join("holdmap-fixture-new-project")
            .to_string_lossy()
            .into_owned();
        assert!(std::path::Path::new(&project).is_absolute());
        let original = folders(&project);
        let id = &original.agents[0].id;
        assert!(agent_folder(&original, id, &project).is_ok());
        assert!(agent_folder(&original, id, "relative/project").is_err());
        assert!(agent_folder(&original, id, &other).is_err());
        assert!(agent_folder(&original, "agent:unknown", &project).is_err());
        let current = folders(&new_project);
        assert!(agent_folder(&current, id, &project).is_err());
    }

    #[test]
    fn protocol_row_targets_cannot_be_interpreted_as_process_names() {
        for (target, protocol) in [("5353/udp", Protocol::Udp), ("3000/tcp", Protocol::Tcp)] {
            let (parsed, opts) = stop_request(target, true, false);
            assert!(matches!(parsed, Target::Port(_)));
            assert_eq!(opts.protocol, Some(protocol));
            assert!(opts.force);
            assert!(!opts.allow_protected);
        }
        let (parsed, opts) = stop_request("pid:100", false, true);
        assert_eq!(parsed, Target::Pid(100));
        assert_eq!(opts.protocol, None);
        assert!(opts.allow_protected);
    }
}

//! portwise desktop: a Tauri v2 shell around `portwise-core` with a tray menu.
//!
//! All the real work (scanning, explaining, planning, stopping) happens in `portwise-core`; this
//! crate only exposes it to the Svelte UI as async commands and keeps a small tray menu of
//! running dev servers.

use portwise_core::{
    execute, ActionPlan, Engine, Explanation, PortEntry, ScanOptions, Snapshot, StopOptions,
    StopReport, Target,
};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

const TRAY_ID: &str = "portwise-tray";

#[derive(Default)]
struct AppState {
    engine: Mutex<Option<Engine>>,
    docker: AtomicBool,
    tray_ok: AtomicBool,
}

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    platform: &'static str,
    tray: bool,
}

#[derive(Clone, Serialize)]
struct Progress {
    target: String,
    line: String,
}

fn scan_now(all: bool, docker: bool) -> Result<Engine, String> {
    Engine::new(&ScanOptions {
        all_states: all,
        docker,
    })
    .map_err(|e| format!("Couldn't read the socket table: {e}"))
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

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        tray: state.tray_ok.load(Ordering::Relaxed),
    }
}

/// Scan the machine and return a fresh snapshot.
#[tauri::command]
async fn scan(app: AppHandle, all: bool, docker: Option<bool>) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    if let Some(d) = docker {
        state.docker.store(d, Ordering::Relaxed);
    }
    let docker = state.docker.load(Ordering::Relaxed);
    let engine = blocking(move || scan_now(all, docker)).await?;
    let snapshot = engine.snapshot().clone();
    *state.engine.lock().unwrap() = Some(engine);
    refresh_tray(&app, &snapshot);
    Ok(snapshot)
}

/// Explain why `port` is busy, using the most recent scan (or a new one).
#[tauri::command]
async fn explain(app: AppHandle, port: u16) -> Result<Explanation, String> {
    with_engine(&app, move |e| e.explain(port, &StopOptions::default())).await
}

/// Build (but don't run) the stop plan for a target such as `3000`, `pid:123` or `node`.
#[tauri::command]
async fn plan(
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
#[tauri::command]
async fn stop(
    app: AppHandle,
    target: String,
    force: bool,
    allow_protected: bool,
) -> Result<StopReport, String> {
    let docker = app.state::<AppState>().docker.load(Ordering::Relaxed);
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
        Ok(report)
    })
    .await
}

/// First free TCP port at or after `near` (checked with real bind probes).
#[tauri::command]
async fn free_port(near: u16) -> Result<Option<u16>, String> {
    blocking(move || {
        Ok((near.max(1)..=u16::MAX).take(2000).find(|p| {
            portwise_core::probe::probe_tcp(*p) == portwise_core::probe::ProbeResult::Free
        }))
    })
    .await
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
            *guard = Some(scan_now(false, state.docker.load(Ordering::Relaxed))?);
        }
        Ok(f(guard.as_ref().unwrap()))
    })
    .await
}

fn tray_label(e: &PortEntry) -> String {
    let what = match (&e.framework, &e.project) {
        (Some(f), Some(p)) => format!("{} · {}", f.name, p.name),
        (Some(f), None) => f.name.clone(),
        (None, Some(p)) => p.name.clone(),
        _ => e.label.clone(),
    };
    format!("● :{}   {}", e.port, what)
}

fn build_tray_menu(
    app: &AppHandle,
    snapshot: Option<&Snapshot>,
) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "show",
        "Open portwise",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let dev: Vec<&PortEntry> = snapshot
        .map(|s| {
            s.entries
                .iter()
                .filter(|e| e.is_dev && e.is_mine)
                .take(10)
                .collect()
        })
        .unwrap_or_default();
    if dev.is_empty() {
        menu.append(&MenuItem::with_id(
            app,
            "none",
            "No dev servers running",
            false,
            None::<&str>,
        )?)?;
    } else {
        menu.append(&MenuItem::with_id(
            app,
            "hdr",
            format!(
                "{} dev server{} running",
                dev.len(),
                if dev.len() == 1 { "" } else { "s" }
            ),
            false,
            None::<&str>,
        )?)?;
        for e in dev {
            menu.append(&MenuItem::with_id(
                app,
                format!("focus:{}", e.port),
                tray_label(e),
                true,
                None::<&str>,
            )?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    if let Some(s) = snapshot {
        let exposed = s
            .entries
            .iter()
            .filter(|e| e.exposure != portwise_core::model::Exposure::Loopback)
            .count();
        menu.append(&MenuItem::with_id(
            app,
            "summary",
            format!(
                "{} ports in use · {exposed} network-exposed",
                s.entries.len()
            ),
            false,
            None::<&str>,
        )?)?;
    }
    menu.append(&MenuItem::with_id(
        app,
        "refresh",
        "Refresh",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Quit portwise",
        true,
        Some("CmdOrCtrl+Q"),
    )?)?;
    Ok(menu)
}

fn refresh_tray(app: &AppHandle, snapshot: &Snapshot) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(menu) = build_tray_menu(app, Some(snapshot)) {
            let _ = tray.set_menu(Some(menu));
        }
        let dev = snapshot.entries.iter().filter(|e| e.is_dev).count();
        let _ = tray.set_tooltip(Some(format!(
            "portwise — {} ports in use, {dev} dev servers",
            snapshot.entries.len()
        )));
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("portwise")
        .menu(&build_tray_menu(app, None)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            "refresh" => {
                let _ = app.emit("refresh", ());
                let app = app.clone();
                std::thread::spawn(move || tray_tick(&app));
            }
            id => {
                if let Some(port) = id
                    .strip_prefix("focus:")
                    .and_then(|p| p.parse::<u16>().ok())
                {
                    show_main(app);
                    let _ = app.emit("focus-port", port);
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Background refresh of the tray menu so it stays useful while the window is hidden.
fn tray_tick(app: &AppHandle) {
    let docker = app.state::<AppState>().docker.load(Ordering::Relaxed);
    if let Ok(engine) = scan_now(false, docker) {
        refresh_tray(app, engine.snapshot());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::default();
    state.docker.store(true, Ordering::Relaxed);
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            app_info, scan, explain, plan, stop, free_port
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            match setup_tray(&handle) {
                Ok(()) => app
                    .state::<AppState>()
                    .tray_ok
                    .store(true, Ordering::Relaxed),
                Err(e) => eprintln!("portwise: tray unavailable ({e}); closing the window quits"),
            }
            let bg = handle.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(20));
                let hidden = bg
                    .get_webview_window("main")
                    .and_then(|w| w.is_visible().ok())
                    .map(|v| !v)
                    .unwrap_or(true);
                if hidden {
                    tray_tick(&bg);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // With a tray, closing the window keeps portwise running in the menu bar.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window
                    .app_handle()
                    .state::<AppState>()
                    .tray_ok
                    .load(Ordering::Relaxed)
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running portwise");
}

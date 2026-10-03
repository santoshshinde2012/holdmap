//! portwise desktop: a Tauri v2 shell around `portwise-core` with a tray menu.
//!
//! All the real work (scanning, explaining, planning, topology, stopping, history) happens in
//! `portwise-core`; this crate only adapts it to the Svelte UI:
//!
//! - [`commands`]: async Tauri commands (thin adapters, no business logic)
//! - [`tray`]: the tray / menu-bar icon listing running dev servers
//! - [`watch`]: background re-scan → `port-events` + desktop notifications
//! - [`shortcuts`]: global show-window shortcut and launch at login
//! - [`state`]: shared state (last engine, config store)
//! - [`update`]: optional self-update (release builds signed with an update key)

mod commands;
mod shortcuts;
mod state;
mod tray;
mod update;
mod watch;

use state::AppState;
use std::sync::atomic::Ordering;
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    let updater = update::configured(&context);
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init());
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![shortcuts::HIDDEN_ARG]),
        ));
        if updater {
            builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
        }
    }
    builder
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::scan,
            commands::topology,
            commands::explain,
            commands::http_info,
            commands::plan,
            commands::stop,
            commands::free_port,
            commands::get_config,
            commands::toggle_pin,
            commands::set_notify,
            commands::history,
            commands::clear_history,
            commands::restart,
            commands::autostart,
            commands::set_pin,
            commands::unpin,
            commands::set_preferences,
            commands::set_hotkey,
            commands::hotkeys,
            commands::remote_scan,
            update::install_update,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let tray_ok = match tray::setup_tray(&handle) {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("portwise: tray unavailable ({e}); closing the window quits");
                    false
                }
            };
            app.state::<AppState>()
                .tray_ok
                .store(tray_ok, Ordering::Relaxed);
            app.state::<AppState>()
                .updater
                .store(updater && cfg!(desktop), Ordering::Relaxed);
            let hotkey = app.state::<AppState>().store.config().hotkey;
            shortcuts::setup(app, &hotkey);
            if tray_ok && shortcuts::started_hidden() {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            update::spawn_check(handle.clone());
            watch::spawn(handle);
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
        .run(context)
        .expect("error while running portwise");
}

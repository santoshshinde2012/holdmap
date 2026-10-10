//! holdmap desktop: a Tauri v2 shell around `holdmap-core` with a tray menu.
//!
//! All the real work (scanning, explaining, planning, topology, stopping, history) happens in
//! `holdmap-core`; this crate only adapts it to the Svelte UI:
//!
//! - [`commands`]: async Tauri adapters with target and folder authorization checks
//! - [`confirmation`]: bounded one-use handles for the exact reviewed stop plans
//! - [`tray`]: the tray / menu-bar icon listing running dev servers
//! - [`watch`]: background re-scan → `port-events` + desktop notifications
//! - [`shortcuts`]: global show-window shortcut and launch at login
//! - [`state`]: shared state (last engine, config store)
//! - [`update`]: optional self-update (release builds signed with an update key)

mod commands;
mod confirmation;
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
    // Project lookups read files in the user's folders; on macOS the first read under
    // ~/Documents waits for the privacy prompt. Keep scans live: wait only a few ms for them
    // and fill project fields in on a later scan.
    holdmap_core::project::set_scan_budget(std::time::Duration::from_millis(15));
    let context = tauri::generate_context!();
    let updater = update::configured(&context);
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_notification::init());
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
            commands::agents,
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
            commands::restart_stopped,
            commands::port_details,
            commands::open_port,
            commands::reveal_project,
            commands::open_in_editor,
            commands::reveal_agent_folder,
            commands::open_agent_folder,
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
                    eprintln!("holdmap: tray unavailable ({e}); closing the window quits");
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
            // With a tray, closing the window keeps holdmap running in the menu bar.
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
        .expect("error while running holdmap");
}

#[cfg(test)]
mod security_tests {
    //! The webview's privileges are part of the security model: fail loudly if they grow.

    fn json(text: &str) -> serde_json::Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn the_window_gets_only_the_permissions_it_uses() {
        let cap = json(include_str!("../capabilities/default.json"));
        let perms: Vec<&str> = cap["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        assert_eq!(
            perms,
            [
                "core:event:allow-listen",
                "core:event:allow-unlisten",
                "core:window:allow-start-dragging"
            ]
        );
        assert_eq!(cap["windows"], serde_json::json!(["main"]));
        assert_eq!(cap["local"], true);
        assert!(
            cap["remote"].is_null(),
            "remote pages must have no native permissions"
        );
    }

    #[test]
    fn the_csp_is_strict() {
        let conf = json(include_str!("../tauri.conf.json"));
        let sec = &conf["app"]["security"];
        let csp = sec["csp"].as_str().unwrap();
        for d in [
            "default-src 'self'",
            "script-src 'self'",
            "object-src 'none'",
            "base-uri 'none'",
            "frame-ancestors 'none'",
            "form-action 'none'",
        ] {
            assert!(csp.contains(d), "CSP lacks {d}");
        }
        assert!(!csp.contains("unsafe-eval"));
        assert_eq!(
            csp.split(';')
                .map(str::trim)
                .find(|d| d.starts_with("connect-src")),
            Some("connect-src ipc: http://ipc.localhost"),
            "webview network connections must remain limited to native IPC"
        );
        assert!(!csp
            .split(';')
            .any(|d| d.trim().starts_with("script-src") && d.contains("unsafe-inline")));
        assert!(!csp.contains("http:") || csp.contains("http://ipc.localhost"));
        // `freezePrototype` stays off: the bundle assigns `constructor` on plain objects and
        // renders a blank window with a frozen prototype (caught by the end-to-end check).
        assert!(
            conf["app"]["withGlobalTauri"].is_null() || conf["app"]["withGlobalTauri"] == false
        );
    }

    #[test]
    fn the_info_plist_explains_folder_access() {
        let plist = include_str!("../Info.plist");
        for key in [
            "NSDocumentsFolderUsageDescription",
            "NSDesktopFolderUsageDescription",
            "NSDownloadsFolderUsageDescription",
        ] {
            let at = plist
                .find(key)
                .unwrap_or_else(|| panic!("Info.plist lacks {key}"));
            let rest = &plist[at..];
            let s = rest.find("<string>").unwrap() + 8;
            let e = rest.find("</string>").unwrap();
            let why = &rest[s..e];
            assert!(why.len() > 20 && why.len() < 200, "{key}: {why}");
        }
    }
}

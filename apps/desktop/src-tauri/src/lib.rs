//! holdmap desktop: a Tauri v2 shell around `holdmap-core` with a tray menu.
//!
//! All the real work (scanning, explaining, planning, topology, stopping, history) happens in
//! `holdmap-core`; this crate only adapts it to the Svelte UI:
//!
//! - [`commands`]: async Tauri adapters with target and folder authorization checks
//! - [`confirmation`]: bounded one-use handles for the exact reviewed stop plans
//! - [`scheduler`]: native timing, retained authority, and cancellation for delayed actions
//! - [`tray`]: the tray / menu-bar icon listing running dev servers
//! - [`watch`]: background re-scan → `port-events` + desktop notifications
//! - [`shortcuts`]: global show-window shortcut and launch at login
//! - [`state`]: shared state (last engine, config store)
//! - [`update`]: optional self-update (release builds signed with an update key)

mod activity;
mod capture_bridge;
mod capture_file;
mod capture_lease;
mod commands;
mod confirmation;
mod local_window;
mod power;
mod recording_save;
mod scheduler;
mod screen_recording;
mod screen_snapshot;
mod shortcuts;
mod snapshot_image;
mod state;
mod stop_batch;
mod tray;
mod update;
mod watch;
mod window_lifecycle;

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
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init());
    #[cfg(target_os = "linux")]
    {
        // WebKitGTK denies display requests unless the embedder handles them. Its
        // display backend still requires the ScreenCast portal's source picker.
        builder = builder.on_permission_request(|webview, kind| {
            local_window::screen_capture_permission(&webview, kind)
        });
    }
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
        .manage(activity::ActivityState::default())
        .manage(power::PowerState::default())
        .manage(scheduler::SchedulerState::default())
        .manage(recording_save::RecordingSaveState::default())
        .manage(screen_recording::ScreenRecordingState::default())
        .manage(screen_snapshot::ScreenSnapshotState::default())
        .invoke_handler(local_window::guard(tauri::generate_handler![
            commands::app_info,
            commands::scan,
            commands::topology,
            commands::agents,
            commands::agent_flow,
            activity::activity_status,
            activity::activity_snapshot,
            activity::start_activity,
            activity::stop_activity,
            activity::clear_activity,
            activity::activity_setup,
            commands::explain,
            commands::http_info,
            commands::plan,
            commands::stop,
            commands::stop_many,
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
            power::shutdown_preview,
            power::shutdown_machine,
            scheduler::schedule_shutdown,
            scheduler::schedule_stops,
            scheduler::list_schedules,
            scheduler::cancel_schedule,
            update::install_update,
            update::open_desktop_help,
            recording_save::save_screen_recording,
            screen_recording::screen_recording_capabilities,
            screen_recording::start_screen_recording,
            screen_recording::screen_recording_status,
            screen_recording::stop_screen_recording,
            screen_recording::read_screen_recording,
            screen_recording::discard_screen_recording,
            screen_snapshot::start_screen_snapshot,
            screen_snapshot::screen_snapshot_status,
            screen_snapshot::read_screen_snapshot,
            screen_snapshot::discard_screen_snapshot,
        ]))
        .on_page_load(|webview, payload| {
            if webview.label() == "main"
                && payload.event() == tauri::webview::PageLoadEvent::Started
            {
                screen_recording::cleanup_lifecycle(webview.app_handle());
                screen_snapshot::cleanup_lifecycle(webview.app_handle());
            }
        })
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
                if let Err(error) = window_lifecycle::hide_main(&handle) {
                    eprintln!("holdmap: couldn't start in the tray ({error})");
                }
            }
            update::spawn_check(handle.clone());
            scheduler::spawn(handle.clone());
            watch::spawn(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, WindowEvent::Destroyed) {
                screen_recording::cleanup_lifecycle(window.app_handle());
                screen_snapshot::cleanup_lifecycle(window.app_handle());
            }
            // With a tray, closing the window keeps holdmap running in the menu bar.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let tray_available = window
                    .app_handle()
                    .state::<AppState>()
                    .tray_ok
                    .load(Ordering::Relaxed);
                window_lifecycle::close_requested(window, api, tray_available);
            }
        })
        .build(context)
        .expect("error while building holdmap")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                screen_recording::cleanup_exit(app);
                screen_snapshot::cleanup_exit(app);
            }
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                // Dock clicks also restore minimized windows, which macOS counts as visible.
                tray::show_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
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
        assert!(
            csp.contains("media-src 'self' blob: mediastream:"),
            "screen recordings use blob and mediastream URLs"
        );
        assert!(
            csp.contains("img-src 'self' data: blob:"),
            "GIF previews use blob URLs"
        );
        assert!(
            csp.contains("worker-src 'self' blob:"),
            "GIF and video encoding use local workers"
        );
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
            "NSScreenCaptureUsageDescription",
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

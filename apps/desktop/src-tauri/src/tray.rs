//! Tray / menu-bar icon with the running dev servers.

use crate::state::{scan_now, AppState};
use portwise_core::{PortEntry, Snapshot};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

const TRAY_ID: &str = "portwise-tray";

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

pub fn refresh_tray(app: &AppHandle, snapshot: &Snapshot) {
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

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
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
pub fn tray_tick(app: &AppHandle) {
    let docker = app.state::<AppState>().docker();
    if let Ok(engine) = scan_now(false, docker) {
        refresh_tray(app, engine.snapshot());
    }
}

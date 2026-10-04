//! Tray / menu-bar icon with the running dev servers.

use crate::state::AppState;
use portwise_core::{PortEntry, Snapshot};
use std::sync::Mutex;
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

/// One line of the tray menu. Kept as plain data so a refresh can tell whether anything
/// changed: swapping the native menu while it's open closes it (and costs a rebuild), so
/// [`refresh_tray`] only does that when the content differs.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TrayItem {
    Item {
        id: String,
        label: String,
        enabled: bool,
        accel: Option<&'static str>,
    },
    Separator,
}

fn item(id: impl Into<String>, label: impl Into<String>, enabled: bool) -> TrayItem {
    TrayItem::Item {
        id: id.into(),
        label: label.into(),
        enabled,
        accel: None,
    }
}

fn tray_items(snapshot: Option<&Snapshot>) -> Vec<TrayItem> {
    let mut out = vec![item("show", "Open portwise", true), TrayItem::Separator];
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
        out.push(item("none", "No dev servers running", false));
    } else {
        out.push(item(
            "hdr",
            format!(
                "{} dev server{} running",
                dev.len(),
                if dev.len() == 1 { "" } else { "s" }
            ),
            false,
        ));
        for e in dev {
            out.push(item(format!("focus:{}", e.port), tray_label(e), true));
        }
    }
    if snapshot.is_some_and(|s| {
        s.entries
            .iter()
            .any(|e| e.is_dev && e.is_mine && !e.protected)
    }) {
        out.push(item("stop-all-dev", "Stop all dev servers…", true));
    }
    out.push(TrayItem::Separator);
    if let Some(s) = snapshot {
        let exposed = s
            .entries
            .iter()
            .filter(|e| e.exposure != portwise_core::model::Exposure::Loopback)
            .count();
        out.push(item(
            "summary",
            format!(
                "{} in use · {exposed} network-exposed",
                portwise_core::util::count(s.entries.len(), "port", "ports")
            ),
            false,
        ));
    }
    out.push(item("refresh", "Refresh", true));
    out.push(TrayItem::Item {
        id: "quit".into(),
        label: "Quit portwise".into(),
        enabled: true,
        accel: Some("CmdOrCtrl+Q"),
    });
    out
}

fn tray_tooltip(snapshot: &Snapshot) -> String {
    let dev = snapshot.entries.iter().filter(|e| e.is_dev).count();
    format!(
        "portwise — {} ports in use, {dev} dev servers",
        snapshot.entries.len()
    )
}

fn build_tray_menu(app: &AppHandle, items: &[TrayItem]) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    for i in items {
        match i {
            TrayItem::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            TrayItem::Item {
                id,
                label,
                enabled,
                accel,
            } => menu.append(&MenuItem::with_id(
                app,
                id.as_str(),
                label,
                *enabled,
                *accel,
            )?)?,
        }
    }
    Ok(menu)
}

/// What the tray shows now; a refresh with the same content is a no-op.
static SHOWN: Mutex<Option<(Vec<TrayItem>, String)>> = Mutex::new(None);

/// True when `next` differs from what's shown (and records it as shown).
fn tray_changed(
    shown: &Mutex<Option<(Vec<TrayItem>, String)>>,
    next: (Vec<TrayItem>, String),
) -> bool {
    let mut g = shown.lock().unwrap_or_else(|e| e.into_inner());
    if g.as_ref() == Some(&next) {
        return false;
    }
    *g = Some(next);
    true
}

pub fn refresh_tray(app: &AppHandle, snapshot: &Snapshot) {
    let items = tray_items(Some(snapshot));
    let tooltip = tray_tooltip(snapshot);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if !tray_changed(&SHOWN, (items.clone(), tooltip.clone())) {
            return;
        }
        if let Ok(menu) = build_tray_menu(app, &items) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_tooltip(Some(tooltip));
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
        .menu(&build_tray_menu(app, &tray_items(None))?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            // Confirmation happens in the window, with the full plan.
            "stop-all-dev" => {
                show_main(app);
                let _ = app.emit("stop-all-dev", ());
            }
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
    if let Ok(snapshot) = app
        .state::<AppState>()
        .snapshot(false, std::time::Duration::ZERO)
    {
        refresh_tray(app, &snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(ports: &[(u16, bool)]) -> Snapshot {
        let entries = ports
            .iter()
            .map(|&(port, dev)| {
                serde_json::from_value(serde_json::json!({
                    "id": format!("tcp:{port}"), "port": port, "protocol": "tcp", "state": "listen",
                    "addresses": ["127.0.0.1"], "families": [], "remote": null,
                    "exposure": "loopback", "pid": 1, "pids": [1], "uid": null, "user": null,
                    "process": null, "project": null, "framework": null, "container": null,
                    "label": format!("svc{port}"), "is_dev": dev, "is_mine": true, "protected": false, "tunnel": null
                }))
                .expect("fixture entry")
            })
            .collect();
        Snapshot {
            entries,
            hidden_sockets: 0,
            platform: "linux".into(),
            taken_at_ms: 1,
            scan_ms: 1,
            docker_available: false,
            warnings: vec![],
        }
    }

    #[test]
    fn same_content_does_not_rebuild_the_menu() {
        let shown = Mutex::new(None);
        let a = snapshot(&[(3000, true), (5432, false)]);
        let mut b = a.clone();
        b.taken_at_ms = 99; // a later scan with the same ports
        let model = |s: &Snapshot| (tray_items(Some(s)), tray_tooltip(s));
        assert!(tray_changed(&shown, model(&a)));
        assert!(!tray_changed(&shown, model(&b)));
        assert!(tray_changed(
            &shown,
            model(&snapshot(&[(3000, true), (3001, true)]))
        ));
    }

    #[test]
    fn lists_dev_servers_and_the_stop_all_action() {
        let items = tray_items(Some(&snapshot(&[(3000, true)])));
        let ids: Vec<&str> = items
            .iter()
            .filter_map(|i| match i {
                TrayItem::Item { id, .. } => Some(id.as_str()),
                TrayItem::Separator => None,
            })
            .collect();
        assert_eq!(
            ids,
            [
                "show",
                "hdr",
                "focus:3000",
                "stop-all-dev",
                "summary",
                "refresh",
                "quit"
            ]
        );
        assert!(tray_items(None).contains(&item("none", "No dev servers running", false)));
    }
}

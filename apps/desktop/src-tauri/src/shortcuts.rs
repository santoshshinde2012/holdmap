//! OS integration: the global "show portwise" shortcut and launch-at-login.

use std::sync::OnceLock;
use tauri::AppHandle;

static REGISTERED: OnceLock<&'static str> = OnceLock::new();

/// Human label of the global shortcut, once registration succeeded.
pub fn registered() -> Option<&'static str> {
    REGISTERED.get().copied()
}

#[cfg(desktop)]
pub fn setup(app: &tauri::App) {
    use tauri_plugin_global_shortcut::{
        Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
    };
    let (mods, label) = if cfg!(target_os = "macos") {
        (Modifiers::SUPER | Modifiers::ALT, "⌘⌥P")
    } else {
        (Modifiers::CONTROL | Modifiers::ALT, "Ctrl+Alt+P")
    };
    let show = Shortcut::new(Some(mods), Code::KeyP);
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |app, sc, ev| {
            if *sc == show && ev.state() == ShortcutState::Pressed {
                crate::tray::show_main(app);
            }
        })
        .build();
    if let Err(e) = app.handle().plugin(plugin) {
        eprintln!("portwise: global shortcut unavailable ({e})");
        return;
    }
    match app.global_shortcut().register(show) {
        Ok(()) => {
            let _ = REGISTERED.set(label);
        }
        Err(e) => eprintln!("portwise: couldn't register {label} ({e})"),
    }
}

#[cfg(not(desktop))]
pub fn setup(_app: &tauri::App) {}

/// Query (and optionally change) launch-at-login.
#[cfg(desktop)]
pub fn autostart(app: &AppHandle, enable: Option<bool>) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let l = app.autolaunch();
    match enable {
        Some(true) => l.enable().map_err(|e| e.to_string())?,
        Some(false) => l.disable().map_err(|e| e.to_string())?,
        None => {}
    }
    l.is_enabled().map_err(|e| e.to_string())
}

#[cfg(not(desktop))]
pub fn autostart(_app: &AppHandle, _enable: Option<bool>) -> Result<bool, String> {
    Err("launch at login isn't supported on this platform".into())
}

/// Launched by the autostart entry: start in the tray instead of opening the window.
pub fn started_hidden() -> bool {
    std::env::args().any(|a| a == HIDDEN_ARG)
}

pub const HIDDEN_ARG: &str = "--hidden";

//! OS integration: the global "show portwise" shortcut and launch-at-login.

use std::sync::Mutex;
use tauri::AppHandle;

static REGISTERED: Mutex<Option<String>> = Mutex::new(None);

/// Selectable global-shortcut presets: (id, macOS label, other-OS label).
pub const PRESETS: &[(&str, &str, &str)] = &[
    ("alt-p", "⌘⌥P", "Ctrl+Alt+P"),
    ("alt-space", "⌥Space", "Ctrl+Alt+Space"),
    ("alt-k", "⌘⌥K", "Ctrl+Alt+K"),
    ("off", "Off", "Off"),
];

/// Human label of a preset on this OS, or `None` for unknown ids.
pub fn label(preset: &str) -> Option<&'static str> {
    PRESETS
        .iter()
        .find(|p| p.0 == preset)
        .map(|p| if cfg!(target_os = "macos") { p.1 } else { p.2 })
}

/// Human label of the global shortcut, once registration succeeded.
pub fn registered() -> Option<String> {
    REGISTERED.lock().ok().and_then(|g| g.clone())
}

#[cfg(desktop)]
fn shortcut(preset: &str) -> Option<tauri_plugin_global_shortcut::Shortcut> {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
    let cmd = if cfg!(target_os = "macos") {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    };
    match preset {
        "alt-p" => Some(Shortcut::new(Some(cmd | Modifiers::ALT), Code::KeyP)),
        "alt-k" => Some(Shortcut::new(Some(cmd | Modifiers::ALT), Code::KeyK)),
        "alt-space" if cfg!(target_os = "macos") => {
            Some(Shortcut::new(Some(Modifiers::ALT), Code::Space))
        }
        "alt-space" => Some(Shortcut::new(
            Some(Modifiers::CONTROL | Modifiers::ALT),
            Code::Space,
        )),
        _ => None,
    }
}

#[cfg(desktop)]
pub fn setup(app: &tauri::App, preset: &str) {
    use tauri_plugin_global_shortcut::ShortcutState;
    // Only portwise's own shortcut is ever registered, so any press shows the window.
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |app, _sc, ev| {
            if ev.state() == ShortcutState::Pressed {
                crate::tray::show_main(app);
            }
        })
        .build();
    if let Err(e) = app.handle().plugin(plugin) {
        eprintln!("portwise: global shortcut unavailable ({e})");
        return;
    }
    if let Err(e) = apply(app.handle(), preset) {
        eprintln!("portwise: {e}");
    }
}

/// Switch the global shortcut to `preset` (`off` unregisters it). Returns the active label.
#[cfg(desktop)]
pub fn apply(app: &AppHandle, preset: &str) -> Result<Option<String>, String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let label = label(preset).ok_or_else(|| format!("unknown shortcut preset {preset:?}"))?;
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    *REGISTERED.lock().unwrap() = None;
    let Some(sc) = shortcut(preset) else {
        return Ok(None);
    };
    gs.register(sc)
        .map_err(|e| format!("couldn't register {label}: another app may be using it ({e})"))?;
    *REGISTERED.lock().unwrap() = Some(label.to_string());
    Ok(Some(label.to_string()))
}

#[cfg(not(desktop))]
pub fn setup(_app: &tauri::App, _preset: &str) {}

#[cfg(not(desktop))]
pub fn apply(_app: &AppHandle, _preset: &str) -> Result<Option<String>, String> {
    Err("global shortcuts aren't supported on this platform".into())
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_has_a_label_and_unknown_ids_dont() {
        for (id, ..) in PRESETS {
            assert!(label(id).is_some(), "{id}");
            #[cfg(desktop)]
            assert_eq!(shortcut(id).is_none(), *id == "off", "{id}");
        }
        assert_eq!(label("nope"), None);
    }
}

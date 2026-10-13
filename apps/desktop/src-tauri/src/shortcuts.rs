//! OS integration: the global "show holdmap" shortcut and launch-at-login.

use std::sync::Mutex;
use tauri::AppHandle;

static REGISTERED: Mutex<Option<&'static str>> = Mutex::new(None);

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
    REGISTERED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .and_then(label)
        .map(str::to_owned)
}

/// Register the replacement before releasing the working shortcut. A collision must
/// leave both the existing registration and the displayed/persisted setting intact.
#[cfg(any(desktop, test))]
fn replace_registration(
    active: &mut Option<&'static str>,
    next: Option<&'static str>,
    register: impl FnOnce(&str) -> Result<(), String>,
    mut unregister: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    if *active == next {
        return Ok(());
    }
    if let Some(next) = next {
        register(next)?;
    }
    if let Some(previous) = *active {
        if let Err(error) = unregister(previous) {
            if let Some(next) = next {
                if let Err(rollback) = unregister(next) {
                    return Err(format!(
                        "{error}; couldn't remove the replacement shortcut: {rollback}"
                    ));
                }
            }
            return Err(error);
        }
    }
    *active = next;
    Ok(())
}

/// Persistence can fail after the OS accepts a replacement. Restore the actual
/// registration, including an inactive shortcut, rather than the saved preference.
#[cfg(any(desktop, test))]
fn replace_saved_registration(
    active: &mut Option<&'static str>,
    next: Option<&'static str>,
    mut register: impl FnMut(&str) -> Result<(), String>,
    mut unregister: impl FnMut(&str) -> Result<(), String>,
    save: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let previous = *active;
    replace_registration(active, next, &mut register, &mut unregister)?;
    if let Err(error) = save() {
        if let Err(rollback) = replace_registration(active, previous, register, unregister) {
            return Err(format!(
                "{error}; couldn't restore the previous shortcut: {rollback}"
            ));
        }
        return Err(error);
    }
    Ok(())
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
    // Only holdmap's own shortcut is ever registered, so any press shows the window.
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |app, _sc, ev| {
            if ev.state() == ShortcutState::Pressed {
                crate::tray::show_main(app);
            }
        })
        .build();
    if let Err(e) = app.handle().plugin(plugin) {
        eprintln!("holdmap: global shortcut unavailable ({e})");
        return;
    }
    if let Err(e) = apply(app.handle(), preset) {
        eprintln!("holdmap: {e}");
    }
}

/// Switch the global shortcut to `preset` (`off` unregisters it). Returns the active label.
#[cfg(desktop)]
pub fn apply(app: &AppHandle, preset: &str) -> Result<Option<String>, String> {
    apply_and_save(app, preset, || Ok(()))
}

/// Keep registration and saving serialized so a failed save cannot roll back a
/// later successful request. `REGISTERED` always describes the actual OS state.
#[cfg(desktop)]
pub fn apply_and_save(
    app: &AppHandle,
    preset: &str,
    save: impl FnOnce() -> Result<(), String>,
) -> Result<Option<String>, String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let preset = PRESETS
        .iter()
        .find(|entry| entry.0 == preset)
        .map(|entry| entry.0)
        .ok_or_else(|| format!("unknown shortcut preset {preset:?}"))?;
    let gs = app.global_shortcut();
    let mut active = REGISTERED.lock().unwrap_or_else(|error| error.into_inner());
    replace_saved_registration(
        &mut active,
        (preset != "off").then_some(preset),
        |next| {
            gs.register(shortcut(next).expect("validated shortcut preset"))
                .map_err(|error| {
                    format!(
                        "couldn't register {}: another app may be using it ({error})",
                        label(next).unwrap()
                    )
                })
        },
        |previous| {
            gs.unregister(shortcut(previous).expect("registered shortcut preset"))
                .map_err(|error| {
                    format!("couldn't unregister {} ({error})", label(previous).unwrap())
                })
        },
        save,
    )?;
    Ok(active.and_then(label).map(str::to_owned))
}

#[cfg(not(desktop))]
pub fn setup(_app: &tauri::App, _preset: &str) {}

#[cfg(not(desktop))]
pub fn apply(_app: &AppHandle, _preset: &str) -> Result<Option<String>, String> {
    Err("global shortcuts aren't supported on this platform".into())
}

#[cfg(not(desktop))]
pub fn apply_and_save(
    _app: &AppHandle,
    _preset: &str,
    _save: impl FnOnce() -> Result<(), String>,
) -> Result<Option<String>, String> {
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

    #[test]
    fn collision_preserves_the_previous_working_shortcut() {
        let mut active = Some("alt-p");
        let result = replace_registration(
            &mut active,
            Some("alt-k"),
            |_| Err("shortcut already in use".into()),
            |_| panic!("a collision must not unregister the working shortcut"),
        );
        assert_eq!(result.unwrap_err(), "shortcut already in use");
        assert_eq!(active, Some("alt-p"));
    }

    #[test]
    fn replacements_are_ordered_and_failed_removal_rolls_back() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        let mut active = Some("alt-p");
        replace_registration(
            &mut active,
            Some("alt-k"),
            |next| {
                calls.borrow_mut().push(format!("register {next}"));
                Ok(())
            },
            |previous| {
                calls.borrow_mut().push(format!("unregister {previous}"));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(active, Some("alt-k"));
        assert_eq!(*calls.borrow(), ["register alt-k", "unregister alt-p"]);
        calls.borrow_mut().clear();
        let result = replace_registration(
            &mut active,
            Some("alt-space"),
            |next| {
                calls.borrow_mut().push(format!("register {next}"));
                Ok(())
            },
            |previous| {
                calls.borrow_mut().push(format!("unregister {previous}"));
                if previous == "alt-k" {
                    Err("removal failed".into())
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(result.unwrap_err(), "removal failed");
        assert_eq!(active, Some("alt-k"));
        assert_eq!(
            *calls.borrow(),
            [
                "register alt-space",
                "unregister alt-k",
                "unregister alt-space"
            ]
        );
    }

    #[test]
    fn reselecting_is_a_noop_and_disabling_updates_state_only_after_success() {
        let mut active = Some("alt-p");
        replace_registration(
            &mut active,
            Some("alt-p"),
            |_| panic!("already registered"),
            |_| panic!("still active"),
        )
        .unwrap();
        assert!(replace_registration(
            &mut active,
            None,
            |_| panic!("off registers nothing"),
            |_| Err("removal failed".into())
        )
        .is_err());
        assert_eq!(active, Some("alt-p"));
        replace_registration(
            &mut active,
            None,
            |_| panic!("off registers nothing"),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(active, None);
    }

    #[test]
    fn rejected_config_write_restores_actual_registration_and_preserves_saved_bytes() {
        use std::cell::RefCell;
        use std::collections::HashSet;

        let directory = tempfile::tempdir().unwrap();
        let store = holdmap_core::store::Store::new(directory.path());
        store.save_config(&Default::default()).unwrap();
        let saved = std::fs::read(directory.path().join("config.json")).unwrap();
        // The active shortcut can differ from config after startup registration fails
        // or a previous OS/storage error. It is the state that must be restored.
        let mut active = Some("alt-k");
        let registered = RefCell::new(HashSet::from(["alt-k".to_owned()]));
        let result = replace_saved_registration(
            &mut active,
            Some("alt-space"),
            |preset| {
                assert!(registered.borrow_mut().insert(preset.to_owned()));
                Ok(())
            },
            |preset| {
                assert!(registered.borrow_mut().remove(preset));
                Ok(())
            },
            || {
                store
                    .update_config(|config| {
                        config.hotkey = "alt-space".into();
                        config.set_pin(3000, Some("x".repeat(1024 * 1024)));
                    })
                    .map(|_| ())
                    .map_err(|error| format!("Couldn't save settings: {error}"))
            },
        );
        assert!(result.unwrap_err().contains("Couldn't save settings"));
        assert_eq!(active, Some("alt-k"));
        assert_eq!(*registered.borrow(), HashSet::from(["alt-k".to_owned()]));
        assert_eq!(
            std::fs::read(directory.path().join("config.json")).unwrap(),
            saved
        );
        assert_eq!(store.config().hotkey, "alt-p");
    }

    #[test]
    fn failed_saves_restore_inactive_and_disabled_shortcuts() {
        use std::cell::RefCell;
        use std::collections::HashSet;

        for (previous, next) in [
            (None, Some("alt-space")),
            (Some("alt-k"), None),
            (Some("alt-k"), Some("alt-k")),
        ] {
            let mut active = previous;
            let expected: HashSet<_> = previous.into_iter().map(str::to_owned).collect();
            let registered = RefCell::new(expected.clone());
            let result = replace_saved_registration(
                &mut active,
                next,
                |preset| {
                    assert!(registered.borrow_mut().insert(preset.to_owned()));
                    Ok(())
                },
                |preset| {
                    assert!(registered.borrow_mut().remove(preset));
                    Ok(())
                },
                || Err("settings are read-only".into()),
            );
            assert_eq!(result.unwrap_err(), "settings are read-only");
            assert_eq!(active, previous);
            assert_eq!(*registered.borrow(), expected);
        }
    }

    #[test]
    fn unsuccessful_restore_reports_both_failures_and_retains_known_active_state() {
        let mut active = Some("alt-k");
        let error = replace_saved_registration(
            &mut active,
            Some("alt-space"),
            |preset| {
                if preset == "alt-k" {
                    Err("previous shortcut was acquired by another app".into())
                } else {
                    Ok(())
                }
            },
            |_| Ok(()),
            || Err("settings write failed".into()),
        )
        .unwrap_err();
        assert!(error.contains("settings write failed"));
        assert!(error.contains("couldn't restore the previous shortcut"));
        assert!(error.contains("acquired by another app"));
        assert_eq!(active, Some("alt-space"));
    }
}

//! Background watcher: re-scans every few seconds, emits `port-events` to the UI, sends desktop
//! notifications for new / conflicting listeners, and keeps the tray menu fresh while hidden.

use crate::state::{scan_now, AppState};
use portwise_core::events::{PortEvent, Watcher};
use portwise_core::store::Config;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// Re-scan cadence while the window is visible comes from the user's *Scan every* setting;
/// while hidden it slows to [`Config::background_interval`] (≥ 10 s; idle CPU ≈ 0).
fn cadence(cfg: &Config, hidden: bool) -> Duration {
    if hidden {
        cfg.background_interval()
    } else {
        cfg.scan_interval()
    }
}

/// Should this event pop a notification? Openings and conflicts by default (dev-only unless
/// configured otherwise); closings only for pinned ports, which you explicitly care about.
pub fn should_notify(ev: &PortEvent, cfg: &Config) -> bool {
    if !cfg.notify {
        return false;
    }
    let pins: Vec<u16> = cfg.pins.iter().map(|p| p.port).collect();
    match ev {
        PortEvent::Closed { entry } => pins.contains(&entry.port),
        _ => ev.notable(cfg.notify_dev_only, &pins),
    }
}

fn title(ev: &PortEvent) -> String {
    match ev {
        PortEvent::Opened { entry } => format!(":{} is now in use", entry.port),
        PortEvent::Closed { entry } => format!(":{} was freed", entry.port),
        PortEvent::Conflict { port, .. } => format!("Port conflict on :{port}"),
    }
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut watcher = Watcher::new();
        loop {
            let state = app.state::<AppState>();
            if let Ok(engine) = scan_now(false, state.docker()) {
                let events = watcher.observe(engine.snapshot().clone());
                if !events.is_empty() {
                    let _ = app.emit("port-events", &events);
                    let cfg = state.store.config();
                    for ev in events.iter().filter(|e| should_notify(e, &cfg)).take(3) {
                        let _ = app
                            .notification()
                            .builder()
                            .title(title(ev))
                            .body(ev.summary())
                            .show();
                    }
                }
                let hidden = app
                    .get_webview_window("main")
                    .and_then(|w| w.is_visible().ok())
                    .map(|v| !v)
                    .unwrap_or(true);
                if hidden {
                    crate::tray::refresh_tray(&app, engine.snapshot());
                }
            }
            let hidden = app
                .get_webview_window("main")
                .and_then(|w| w.is_visible().ok())
                .is_some_and(|v| !v);
            std::thread::sleep(cadence(&state.store.config(), hidden));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use portwise_core::store::Pin;
    use portwise_core::PortEntry;

    fn entry(port: u16, dev: bool) -> Box<PortEntry> {
        Box::new(
            serde_json::from_value(serde_json::json!({
                "id": format!("tcp:{port}"), "port": port, "protocol": "tcp", "state": "listen",
                "addresses": ["127.0.0.1"], "families": [], "remote": null,
                "exposure": "loopback", "pid": 1, "pids": [1], "uid": null, "user": null,
                "process": null, "project": null, "framework": null, "container": null,
                "label": "x", "is_dev": dev, "is_mine": true, "protected": false, "tunnel": null
            }))
            .expect("fixture entry"),
        )
    }

    #[test]
    fn cadence_follows_the_setting() {
        let cfg = Config {
            scan_interval_secs: 2,
            ..Config::default()
        };
        assert_eq!(cadence(&cfg, false), Duration::from_secs(2));
        assert_eq!(cadence(&cfg, true), Duration::from_secs(10));
    }

    #[test]
    fn notification_policy() {
        let mut cfg = Config::default();
        assert!(cfg.notify && cfg.notify_dev_only, "defaults");
        let opened = |p, dev| PortEvent::Opened {
            entry: entry(p, dev),
        };
        assert!(should_notify(&opened(3000, true), &cfg));
        assert!(!should_notify(&opened(631, false), &cfg));
        assert!(!should_notify(
            &PortEvent::Closed {
                entry: entry(3000, true)
            },
            &cfg
        ));
        cfg.pins.push(Pin {
            port: 3000,
            label: None,
        });
        assert!(should_notify(
            &PortEvent::Closed {
                entry: entry(3000, true)
            },
            &cfg
        ));
        cfg.notify = false;
        assert!(!should_notify(&opened(3000, true), &cfg));
    }
}

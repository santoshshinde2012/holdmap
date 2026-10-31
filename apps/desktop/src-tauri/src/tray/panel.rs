//! The popup panel: a small window under the tray icon with live readings, the agents' CPU
//! history, an attention history and lists, like a system monitor's popover. Its page
//! (`tray.html`) can call only [`tray_panel`] (read the data) and [`tray_action`] (an action id
//! the tray menu also offers); `local_window::guard` enforces both the window and the commands.
//! Nothing is stopped from here: "Stop all dev servers…" opens the window's reviewed plan.

use super::model::{Action, PanelData};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

/// The panel's window and page; `local_window` owns them with the commands the page may call.
pub(crate) use crate::local_window::{TRAY_PANEL as LABEL, TRAY_PANEL_PAGE as PAGE};
const WIDTH: f64 = 360.0;
const HEIGHT: f64 = 600.0;
/// Space between the panel and the icon or the screen edge, in points.
const MARGIN: f64 = 6.0;
/// A click on the icon first takes focus from an open panel, which hides it; a click this soon
/// after closes the panel instead of opening it again.
const REOPEN_GRACE: Duration = Duration::from_millis(350);

/// Where the tray icon was last clicked (Linux trays don't say).
static ANCHOR: Mutex<Option<Rect>> = Mutex::new(None);
static HIDDEN_AT: Mutex<Option<Instant>> = Mutex::new(None);

fn hidden_just_now() -> bool {
    HIDDEN_AT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some_and(|at| at.elapsed() < REOPEN_GRACE)
}

/// Show the panel under the icon at `anchor` (or where it was last), or hide it if it's open.
/// Blocking: call it off the main thread, since creating a window from an event handler can
/// deadlock on Windows.
pub fn toggle(app: &AppHandle, anchor: Option<Rect>) {
    if anchor.is_some() {
        *ANCHOR.lock().unwrap_or_else(|e| e.into_inner()) = anchor;
    }
    let window = match app.get_webview_window(LABEL) {
        Some(window) => window,
        None => match create(app) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("holdmap: couldn't open the tray panel ({error})");
                return;
            }
        },
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    if anchor.is_some() && hidden_just_now() {
        return;
    }
    keep_main_hidden(app);
    place(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(PAGE.into()))
        .title("holdmap")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .minimizable(false)
        .maximizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .visible(false)
        // Creating a focused window activates the app, and on macOS that unhides it with the
        // main window; focus comes after `keep_main_hidden` instead.
        .focused(false);
    // macOS: a titled window without its title or buttons keeps the system's rounded corners
    // and shadow without private APIs. Elsewhere, a plain undecorated window.
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    let window = builder.build()?;
    #[cfg(target_os = "macos")]
    macos::panel_chrome(&window);
    let handle = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            *HIDDEN_AT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
            let _ = handle.hide();
        }
    });
    Ok(window)
}

/// Under a menu-bar icon, above a taskbar icon, centred on it and kept on its screen. Without a
/// known icon position (Linux), the top-right corner.
fn place(app: &AppHandle, window: &WebviewWindow) {
    let anchor = *ANCHOR.lock().unwrap_or_else(|e| e.into_inner());
    let monitor = anchor
        .and_then(|r| {
            let p = r.position.to_physical::<f64>(1.0);
            app.monitor_from_point(p.x, p.y).ok().flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let (mx, my) = (monitor.position().x as f64, monitor.position().y as f64);
    let (mw, mh) = (monitor.size().width as f64, monitor.size().height as f64);
    let (w, h, margin) = (WIDTH * scale, HEIGHT * scale, MARGIN * scale);
    let (x, y) = match anchor {
        Some(r) => {
            let p = r.position.to_physical::<f64>(scale);
            let size = r.size.to_physical::<f64>(scale);
            let below = p.y < my + mh / 2.0;
            (
                p.x + size.width / 2.0 - w / 2.0,
                if below {
                    p.y + size.height + margin
                } else {
                    p.y - h - margin
                },
            )
        }
        None => (mx + mw - w - margin * 2.0, my + 32.0 * scale),
    };
    let x = x.clamp(mx + margin, (mx + mw - w - margin).max(mx + margin));
    let y = y.clamp(my, (my + mh - h).max(my));
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// Closing the main window hides the whole app on macOS (`window_lifecycle`), and focusing the
/// panel unhides it, which would bring the main window back with it (AppKit restores the windows
/// it hid, even ones ordered out since). So unhide first, with the main window out of sight, and
/// order it out. A fullscreen main window stays (ordering it out leaves its Space blank).
fn keep_main_hidden(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let main = app
            .get_webview_window("main")
            .filter(|w| !w.is_fullscreen().unwrap_or(true))
            .and_then(|w| w.ns_window().ok())
            .map(|w| w as usize);
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = app.run_on_main_thread(move || {
            macos::unhide_without(main);
            let _ = tx.send(());
        });
        let _ = rx.recv_timeout(Duration::from_secs(2));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSWindow, NSWindowButton};

    /// If macOS has hidden the whole app, unhide it without activating it and without showing
    /// `window` again: it is fully transparent while AppKit restores it, then ordered out.
    /// Main thread only.
    pub fn unhide_without(window: Option<usize>) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        if !app.isHidden() {
            return;
        }
        // SAFETY: Tauri's NSWindow for the live main window, used on the main thread.
        let window = window.map(|w| unsafe { &*(w as *const NSWindow) });
        if let Some(w) = window {
            w.setAlphaValue(0.0);
        }
        app.unhideWithoutActivation();
        if let Some(w) = window {
            w.orderOut(None);
            w.setAlphaValue(1.0);
        }
    }

    /// No close, minimise or zoom buttons, and the panel stays where it opened.
    pub fn panel_chrome(window: &tauri::WebviewWindow) {
        let Ok(ns_window) = window.ns_window() else {
            return;
        };
        let ns_window = ns_window as usize;
        let _ = window.run_on_main_thread(move || {
            // SAFETY: Tauri's NSWindow for this live window, used on the main thread.
            let ns_window = unsafe { &*(ns_window as *const NSWindow) };
            for kind in [
                NSWindowButton::CloseButton,
                NSWindowButton::MiniaturizeButton,
                NSWindowButton::ZoomButton,
            ] {
                if let Some(button) = ns_window.standardWindowButton(kind) {
                    button.setHidden(true);
                }
            }
            ns_window.setMovable(false);
        });
    }
}

/// The panel's data. Only the panel page may call it (`local_window::guard`).
#[tauri::command]
pub async fn tray_panel(app: AppHandle) -> PanelData {
    super::panel_data(&app)
}

/// Run a panel row's action: the same ids, and the same effects, as the tray menu's rows.
#[tauri::command]
pub async fn tray_action(app: AppHandle, id: String) -> Result<(), String> {
    let action = Action::parse(&id).ok_or("Unknown tray action")?;
    if !matches!(action, Action::Style(_) | Action::Refresh) {
        hide(&app);
    }
    super::run(&app, action);
    Ok(())
}

//! Closing the main window keeps a working tray session available. macOS application
//! hiding also deactivates the app; ordering out a fullscreen window leaves its Space blank.

use tauri::{AppHandle, CloseRequestApi, Manager, Runtime, Window};

/// Retain the same window and webview so selection, reviews and live data survive hiding.
pub(crate) fn hide_main<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let window = app
        .get_webview_window("main")
        .ok_or(tauri::Error::WindowNotFound)?;
    #[cfg(target_os = "macos")]
    {
        // NSApplication.hide handles activation and Spaces as one native operation.
        // Do not order out the fullscreen NSWindow or race an asynchronous fullscreen exit.
        let _ = window;
        app.hide()
    }
    #[cfg(not(target_os = "macos"))]
    window.hide()
}

/// Tray, shortcut and Dock requests share one restore path, including minimized windows.
pub(crate) fn show_main<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let window = app
        .get_webview_window("main")
        .ok_or(tauri::Error::WindowNotFound)?;
    #[cfg(target_os = "macos")]
    {
        app.show()?;
        // Restore before making the window key: AppKit ignores focus while miniaturized.
        // Leave fullscreen windows alone rather than sending an unnecessary deminiaturize.
        if window.is_minimized()? {
            window.unminimize()?;
        }
    }
    window.show()?;
    #[cfg(not(target_os = "macos"))]
    window.unminimize()?;
    window.set_focus()
}

/// A missing tray must preserve ordinary close-to-quit behavior. Secondary windows close
/// normally; only the main window owns the background session.
pub(crate) fn close_requested<R: Runtime>(
    window: &Window<R>,
    api: &CloseRequestApi,
    tray_available: bool,
) {
    if window.label() != "main" || !tray_available {
        return;
    }
    api.prevent_close();
    if let Err(error) = hide_main(window.app_handle()) {
        eprintln!("holdmap: couldn't hide the app window ({error})");
    }
}

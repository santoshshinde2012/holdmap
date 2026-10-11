//! Shared origin policy for local desktop services that expose sensitive capabilities.

use tauri::{Manager, WebviewWindow};

pub(crate) fn trusted_origin(label: &str, url: &tauri::Url, dev_url: Option<&tauri::Url>) -> bool {
    if label != "main" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let packaged = matches!(
        (url.scheme(), url.host_str(), url.port()),
        ("tauri", Some("localhost"), None) | ("http" | "https", Some("tauri.localhost"), None)
    );
    packaged || dev_url.is_some_and(|dev| dev.origin() == url.origin())
}

pub(crate) fn authorize(window: &WebviewWindow, action: &str) -> Result<(), String> {
    let url = window
        .url()
        .map_err(|_| "Couldn't verify the local app window.")?;
    let dev_url = if cfg!(debug_assertions) {
        window.app_handle().config().build.dev_url.as_ref()
    } else {
        None
    };
    if !trusted_origin(window.label(), &url, dev_url) {
        return Err(format!(
            "{action} is available only from the local Holdmap app window."
        ));
    }
    Ok(())
}

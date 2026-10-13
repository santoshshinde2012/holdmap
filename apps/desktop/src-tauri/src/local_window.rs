//! Shared origin policy for local desktop services that expose sensitive capabilities.

use tauri::{ipc::Invoke, Manager, Runtime, Webview, WebviewWindow};

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
    authorize_webview(window.as_ref(), action)
}

fn authorize_webview<R: Runtime>(webview: &Webview<R>, action: &str) -> Result<(), String> {
    let url = webview
        .url()
        .map_err(|_| "Couldn't verify the local app window.")?;
    let dev_url = if cfg!(debug_assertions) {
        webview.app_handle().config().build.dev_url.as_ref()
    } else {
        None
    };
    if webview.window().label() != "main" || !trusted_origin(webview.label(), &url, dev_url) {
        return Err(format!(
            "{action} is available only from the local Holdmap app window."
        ));
    }
    Ok(())
}

/// Apply the same boundary to every first-party command before argument decoding or effects.
/// Tauri's capability policy still governs plugin commands and the requesting frame's origin.
pub(crate) fn guard<R: Runtime>(
    dispatch: impl Fn(Invoke<R>) -> bool + Send + Sync + 'static,
) -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    move |invoke| {
        if let Err(error) = authorize_webview(invoke.message.webview_ref(), "Native commands") {
            invoke.resolver.reject(error);
            return true;
        }
        dispatch(invoke)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicUsize, atomic::Ordering, Arc};
    use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};

    #[test]
    fn origins_require_the_main_view_and_exact_local_authority() {
        let dev = tauri::Url::parse("http://localhost:1420").unwrap();
        for url in [
            "tauri://localhost/index.html",
            "http://tauri.localhost/",
            "https://tauri.localhost/",
            "http://localhost:1420/settings",
        ] {
            let url = tauri::Url::parse(url).unwrap();
            assert!(trusted_origin("main", &url, Some(&dev)), "{url}");
            assert!(!trusted_origin("secondary", &url, Some(&dev)), "{url}");
        }
        for url in [
            "https://example.com/",
            "tauri://example.com/",
            "http://tauri.localhost.example.com/",
            "http://tauri.localhost:1420/",
            "http://user@tauri.localhost/",
            "http://localhost:1421/",
            "http://127.0.0.1:1420/",
            "data:text/html,test",
            "file:///tmp/index.html",
        ] {
            let url = tauri::Url::parse(url).unwrap();
            assert!(!trusted_origin("main", &url, Some(&dev)), "{url}");
        }
        assert!(!trusted_origin("main", &dev, None));
    }

    #[test]
    fn ipc_rejects_secondary_local_views_before_any_command_dispatch() {
        let calls = Arc::new(AtomicUsize::new(0));
        let dispatched = Arc::clone(&calls);
        let app = mock_builder()
            .invoke_handler(guard(move |invoke| {
                dispatched.fetch_add(1, Ordering::SeqCst);
                invoke.resolver.resolve("accepted");
                true
            }))
            .build(mock_context(noop_assets()))
            .unwrap();
        let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let secondary = tauri::WebviewWindowBuilder::new(&app, "secondary", Default::default())
            .build()
            .unwrap();
        // Custom app commands can otherwise be reachable by any local view when no app
        // ACL manifest is defined. The guard is independent of command names and payloads.
        for command in [
            "restart",
            "set_preferences",
            "scan",
            "install_update",
            "future_command",
        ] {
            let request = || tauri::webview::InvokeRequest {
                cmd: command.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: if cfg!(any(windows, target_os = "android")) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .unwrap(),
                body: tauri::ipc::InvokeBody::default(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.into(),
            };
            let before = calls.load(Ordering::SeqCst);
            let error = get_ipc_response(&secondary, request()).unwrap_err();
            assert!(error.as_str().unwrap().contains("local Holdmap app window"));
            assert_eq!(calls.load(Ordering::SeqCst), before);
            let response = get_ipc_response(&main, request()).unwrap();
            assert_eq!(response.deserialize::<String>().unwrap(), "accepted");
            assert_eq!(calls.load(Ordering::SeqCst), before + 1);
        }
    }
}

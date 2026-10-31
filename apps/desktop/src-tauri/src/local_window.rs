//! Shared origin policy for local desktop services that expose sensitive capabilities.

use tauri::{ipc::Invoke, Manager, Runtime, Webview, WebviewWindow};

pub(crate) fn trusted_origin(label: &str, url: &tauri::Url, dev_url: Option<&tauri::Url>) -> bool {
    label == "main" && local_origin(url, dev_url)
}

/// The bundled app assets (or the dev server in debug builds), never another authority.
fn local_origin(url: &tauri::Url, dev_url: Option<&tauri::Url>) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let packaged = matches!(
        (url.scheme(), url.host_str(), url.port()),
        ("tauri", Some("localhost"), None) | ("http" | "https", Some("tauri.localhost"), None)
    );
    packaged || dev_url.is_some_and(|dev| dev.origin() == url.origin())
}

fn dev_url<R: Runtime>(webview: &Webview<R>) -> Option<tauri::Url> {
    if cfg!(debug_assertions) {
        webview.app_handle().config().build.dev_url.clone()
    } else {
        None
    }
}

/// The tray popup panel's window label, its bundled page, and the only commands it may call.
pub(crate) const TRAY_PANEL: &str = "tray";
pub(crate) const TRAY_PANEL_PAGE: &str = "tray.html";
pub(crate) const TRAY_PANEL_COMMANDS: [&str; 2] = ["tray_panel", "tray_action"];

/// The tray panel: its own window showing the bundled panel page, which may call only the
/// panel's two commands (read its data; run a row's tray action).
fn authorize_tray_panel<R: Runtime>(webview: &Webview<R>, command: &str) -> Result<(), String> {
    let url = webview
        .url()
        .map_err(|_| "Couldn't verify the tray panel.")?;
    let page = url.path().strip_prefix('/') == Some(TRAY_PANEL_PAGE);
    if webview.window().label() != TRAY_PANEL
        || !page
        || !local_origin(&url, dev_url(webview).as_ref())
        || !TRAY_PANEL_COMMANDS.contains(&command)
    {
        return Err("The tray panel can only read its readings and run its own rows.".into());
    }
    Ok(())
}

pub(crate) fn authorize<R: Runtime>(window: &WebviewWindow<R>, action: &str) -> Result<(), String> {
    authorize_webview(window.as_ref(), action)
}

/// Linux's host gate only: WebKitGTK separately requires OS portal selection for
/// every display request. Camera, microphone and other permissions keep defaults.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn screen_capture_permission<R: Runtime>(
    webview: &Webview<R>,
    kind: tauri::webview::PermissionKind,
) -> tauri::webview::PermissionResponse {
    use tauri::webview::{PermissionKind, PermissionResponse};
    if !matches!(kind, PermissionKind::DisplayCapture) {
        return PermissionResponse::Default;
    }
    if authorize_webview(webview, "Screen capture").is_ok() {
        PermissionResponse::Allow
    } else {
        PermissionResponse::Deny
    }
}

fn authorize_webview<R: Runtime>(webview: &Webview<R>, action: &str) -> Result<(), String> {
    let url = webview
        .url()
        .map_err(|_| "Couldn't verify the local app window.")?;
    if webview.window().label() != "main"
        || !trusted_origin(webview.label(), &url, dev_url(webview).as_ref())
    {
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
        let webview = invoke.message.webview_ref();
        let allowed = if webview.label() == TRAY_PANEL {
            authorize_tray_panel(webview, invoke.message.command())
        } else {
            authorize_webview(webview, "Native commands")
        };
        if let Err(error) = allowed {
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
    fn display_host_permission_requires_the_main_local_view_and_never_grants_other_devices() {
        use tauri::webview::{PermissionKind, PermissionResponse};
        for (label, external, expected) in [
            ("main", false, PermissionResponse::Allow),
            ("secondary", false, PermissionResponse::Deny),
            ("main", true, PermissionResponse::Deny),
        ] {
            let app = mock_builder().build(mock_context(noop_assets())).unwrap();
            let url = if external {
                tauri::WebviewUrl::External("https://example.com/".parse().unwrap())
            } else {
                Default::default()
            };
            let window = tauri::WebviewWindowBuilder::new(&app, label, url)
                .build()
                .unwrap();
            assert_eq!(
                screen_capture_permission(window.as_ref(), PermissionKind::DisplayCapture),
                expected
            );
            for kind in [
                PermissionKind::Camera,
                PermissionKind::Microphone,
                PermissionKind::Geolocation,
                PermissionKind::Other,
            ] {
                assert_eq!(
                    screen_capture_permission(window.as_ref(), kind),
                    PermissionResponse::Default
                );
            }
        }
    }

    #[test]
    fn the_tray_panel_page_may_call_only_its_own_commands() {
        let local = if cfg!(any(windows, target_os = "android")) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        };
        for (label, page, command, allowed) in [
            ("tray", "tray.html", "tray_panel", true),
            ("tray", "tray.html", "tray_action", true),
            ("tray", "tray.html", "scan", false),
            ("tray", "tray.html", "stop", false),
            ("tray", "tray.html", "set_preferences", false),
            ("tray", "tray.html", "future_command", false),
            // The panel's label alone grants nothing: it must show the panel page.
            ("tray", "index.html", "tray_panel", false),
            // Other windows still need to be the main window, panel commands included.
            ("secondary", "tray.html", "tray_panel", false),
        ] {
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
            let window =
                tauri::WebviewWindowBuilder::new(&app, label, tauri::WebviewUrl::App(page.into()))
                    .build()
                    .unwrap();
            let request = tauri::webview::InvokeRequest {
                cmd: command.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: format!("{local}/{page}").parse().unwrap(),
                body: tauri::ipc::InvokeBody::default(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.into(),
            };
            let response = get_ipc_response(&window, request);
            assert_eq!(response.is_ok(), allowed, "{label} {page} {command}");
            assert_eq!(calls.load(Ordering::SeqCst), usize::from(allowed));
        }
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
            "open_desktop_help",
            "save_screen_recording",
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

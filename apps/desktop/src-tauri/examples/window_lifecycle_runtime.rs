//! Check native fullscreen close and retained window identity in an owned macOS app.
//! Run only through scripts/check-window-lifecycle-runtime.py; no production state is loaded.

#[cfg(target_os = "macos")]
#[path = "../src/window_lifecycle.rs"]
mod window_lifecycle;

#[cfg(target_os = "macos")]
mod fixture {
    use serde::{Deserialize, Serialize};
    use std::ffi::{c_char, c_void};
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Manager, WebviewWindow};

    #[derive(Deserialize)]
    struct Fixture {
        root: PathBuf,
        app: PathBuf,
        result: PathBuf,
        identifier: String,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    struct State {
        fullscreen: bool,
        visible: bool,
        focused: bool,
        minimized: bool,
        application_active: bool,
        application_hidden: bool,
        on_active_space: bool,
        width: u32,
        height: u32,
        native_window: usize,
        native_view: usize,
        native_webview: usize,
    }

    #[derive(Serialize)]
    struct Stage {
        name: &'static str,
        state: State,
    }

    struct Controls {
        ready: AtomicBool,
        closed: AtomicUsize,
        dock_reopened: AtomicUsize,
        tray_available: AtomicBool,
        events: Mutex<Vec<String>>,
        started: Instant,
        dock_request: PathBuf,
    }

    impl Controls {
        fn mark(&self, event: &str) {
            self.events.lock().unwrap().push(format!(
                "{:08}µs {event}",
                self.started.elapsed().as_micros()
            ));
        }
    }

    fn wait_condition(name: &str, matches: impl Fn() -> bool) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches() {
            if Instant::now() >= deadline {
                return Err(format!("{name} did not complete"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    // These read-only Cocoa queries use the system runtime already linked by Tauri.
    // No Accessibility or Screen Recording APIs, observers or permissions are involved.
    #[link(name = "objc")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn objc_msgSend();
    }

    fn cocoa_state(window: &WebviewWindow) -> Result<(bool, bool, bool, bool), String> {
        // Called only by on_main while Tauri's event loop owns the application/window.
        unsafe {
            let send_object: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let send_bool: unsafe extern "C" fn(*mut c_void, *mut c_void) -> i8 =
                std::mem::transmute(objc_msgSend as *const ());
            let send_unsigned: unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize =
                std::mem::transmute(objc_msgSend as *const ());
            let class = objc_getClass(c"NSApplication".as_ptr());
            let application = send_object(class, sel_registerName(c"sharedApplication".as_ptr()));
            if application.is_null() {
                return Err("NSApplication is unavailable".into());
            }
            let native = window.ns_window().map_err(|error| error.to_string())?;
            Ok((
                send_bool(application, sel_registerName(c"isActive".as_ptr())) != 0,
                send_bool(application, sel_registerName(c"isHidden".as_ptr())) != 0,
                send_bool(native, sel_registerName(c"isOnActiveSpace".as_ptr())) != 0,
                // NSWindowStyleMaskFullScreen is 1 << 14 in AppKit. Read the native
                // mode, not Tao's requested fullscreen cache, which changes early.
                send_unsigned(native, sel_registerName(c"styleMask".as_ptr())) & (1 << 14) != 0,
            ))
        }
    }

    fn on_main<T: Send + 'static>(
        app: &AppHandle,
        action: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        app.run_on_main_thread(move || {
            let _ = sender.send(action());
        })
        .map_err(|error| error.to_string())?;
        receiver
            .recv_timeout(Duration::from_secs(3))
            .map_err(|error| format!("main-thread operation did not complete: {error}"))?
    }

    fn sample(app: &AppHandle, window: &WebviewWindow) -> Result<State, String> {
        // with_webview dispatches to the main thread; wait only on this owned worker.
        let (sender, receiver) = mpsc::sync_channel(1);
        window
            .with_webview(move |platform| {
                let _ = sender.send(platform.inner() as usize);
            })
            .map_err(|error| error.to_string())?;
        let native_webview = receiver
            .recv_timeout(Duration::from_secs(3))
            .map_err(|error| format!("native webview query did not complete: {error}"))?;
        let window = window.clone();
        on_main(app, move || {
            let (application_active, application_hidden, on_active_space, fullscreen) =
                cocoa_state(&window)?;
            let size = window.inner_size().map_err(|error| error.to_string())?;
            Ok(State {
                fullscreen,
                visible: window.is_visible().map_err(|error| error.to_string())?,
                focused: window.is_focused().map_err(|error| error.to_string())?,
                minimized: window.is_minimized().map_err(|error| error.to_string())?,
                application_active,
                application_hidden,
                on_active_space,
                width: size.width,
                height: size.height,
                native_window: window.ns_window().map_err(|error| error.to_string())? as usize,
                native_view: window.ns_view().map_err(|error| error.to_string())? as usize,
                native_webview,
            })
        })
    }

    fn settle(
        app: &AppHandle,
        window: &WebviewWindow,
        stages: &mut Vec<Stage>,
        name: &'static str,
        matches: impl Fn(&State) -> bool,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut previous = None;
        let mut stable_since = Instant::now();
        loop {
            let state = sample(app, window)?;
            if previous.as_ref() != Some(&state) || !matches(&state) {
                stable_since = Instant::now();
            }
            if matches(&state) && stable_since.elapsed() >= Duration::from_secs(1) {
                stages.push(Stage { name, state });
                return Ok(());
            }
            if Instant::now() >= deadline {
                stages.push(Stage {
                    name,
                    state: state.clone(),
                });
                return Err(format!("{name} did not settle: {state:?}"));
            }
            previous = Some(state);
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn reopen(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
        let handle = app.clone();
        let window = window.clone();
        on_main(app, move || {
            handle.show().map_err(|error| error.to_string())?;
            window.show().map_err(|error| error.to_string())?;
            window.unminimize().map_err(|error| error.to_string())?;
            window.set_focus().map_err(|error| error.to_string())
        })
    }

    fn native_close(
        app: &AppHandle,
        window: &WebviewWindow,
        controls: &Arc<Controls>,
    ) -> Result<(), String> {
        let window = window.clone();
        let observed = controls.clone();
        on_main(app, move || {
            observed.mark("owned NSWindow.performClose:nil entered main thread");
            let native = window.ns_window().map_err(|error| error.to_string())?;
            if native.is_null() {
                return Err("owned native window is unavailable".into());
            }
            // The standard traffic-light close action, on this fixture's own NSWindow.
            unsafe {
                let send_close: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                    std::mem::transmute(objc_msgSend as *const ());
                send_close(
                    native,
                    sel_registerName(c"performClose:".as_ptr()),
                    std::ptr::null_mut(),
                );
            }
            observed.mark("owned NSWindow.performClose:nil completed main thread");
            Ok(())
        })
    }

    /// Returns true when cooperative background activation prevents a valid fullscreen test.
    fn exercise(
        app: &AppHandle,
        window: &WebviewWindow,
        stages: &mut Vec<Stage>,
        controls: &Arc<Controls>,
    ) -> Result<bool, String> {
        wait_condition("owned Tauri Ready event", || {
            controls.ready.load(Ordering::Acquire)
        })?;
        let before = controls.dock_reopened.load(Ordering::Acquire);
        std::fs::write(
            controls.dock_request.with_file_name("bootstrap-open"),
            "activate owned bundle",
        )
        .map_err(|error| error.to_string())?;
        wait_condition("owned bundle activation", || {
            controls.dock_reopened.load(Ordering::Acquire) > before
        })?;
        reopen(app, window)?;
        if let Err(error) = settle(app, window, stages, "initial", |state| {
            state.visible
                && state.focused
                && state.application_active
                && state.on_active_space
                && !state.fullscreen
        }) {
            let observed = sample(app, window)?;
            if !observed.visible
                || observed.minimized
                || (observed.application_active && observed.focused && observed.on_active_space)
            {
                return Err(error);
            }
            controls.mark(&format!(
                "cooperative background activation unavailable: {error}"
            ));
            return Ok(true);
        }
        window
            .set_fullscreen(true)
            .map_err(|error| error.to_string())?;
        if let Err(error) = settle(app, window, stages, "fullscreen", |state| {
            state.visible
                && state.focused
                && state.application_active
                && state.on_active_space
                && state.fullscreen
        }) {
            let observed = sample(app, window)?;
            if !observed.fullscreen
                || !observed.visible
                || observed.minimized
                || (observed.application_active && observed.focused && observed.on_active_space)
            {
                return Err(error);
            }
            controls.mark(&format!(
                "foreground fullscreen precondition unavailable: {error}"
            ));
            return Ok(true);
        }

        // Exercise the actual native traffic-light action and production close interceptor.
        let before = controls.closed.load(Ordering::Acquire);
        native_close(app, window, controls)?;
        wait_condition("native close callback completion", || {
            controls.closed.load(Ordering::Acquire) > before
        })?;
        settle(app, window, stages, "fullscreen_closed", |state| {
            !state.application_active && !state.on_active_space && state.fullscreen
        })?;

        let handle = app.clone();
        on_main(app, move || {
            crate::window_lifecycle::show_main(&handle).map_err(|error| error.to_string())
        })?;
        // Visibility and identity are testable here. Foreground activation is an observation:
        // macOS 14+ may refuse a background caller's activation without user cooperation.
        settle(app, window, stages, "reopen_observation", |state| {
            state.visible && !state.minimized
        })?;
        Ok(false)
    }

    fn normal_closes(
        app: &AppHandle,
        window: &WebviewWindow,
        controls: &Controls,
    ) -> Result<serde_json::Value, String> {
        let handle = app.clone();
        let secondary = on_main(app, move || {
            tauri::WebviewWindowBuilder::new(
                &handle,
                "owned-secondary",
                tauri::WebviewUrl::External("about:blank".parse().unwrap()),
            )
            .visible(false)
            .build()
            .map_err(|error| error.to_string())
        })?;
        secondary.close().map_err(|error| error.to_string())?;
        wait_condition("secondary window destruction", || {
            app.get_webview_window("owned-secondary").is_none()
        })?;
        controls.tray_available.store(false, Ordering::Release);
        window.close().map_err(|error| error.to_string())?;
        wait_condition("main window destruction without tray", || {
            app.get_webview_window("main").is_none()
        })?;
        Ok(serde_json::json!({ "secondary_destroyed": true, "main_destroyed_without_tray": true }))
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let arguments: Vec<_> = std::env::args_os().skip(1).collect();
        let [flag, path] = arguments.as_slice() else {
            return Err("use the owned scratch runner with --fixture".into());
        };
        if flag != "--fixture" {
            return Err("expected --fixture".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(64 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 {
            return Err("fixture metadata exceeds its limit".into());
        }
        let fixture: Fixture = serde_json::from_slice(&bytes)?;
        let root = fixture.root.canonicalize()?;
        let app_path = fixture.app.canonicalize()?;
        let executable = std::env::current_exe()?.canonicalize()?;
        if !root.starts_with(std::env::temp_dir().canonicalize()?)
            || !root.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .starts_with("holdmap-window-runtime-")
            })
            || root.metadata()?.permissions().mode() & 0o077 != 0
            || !app_path.starts_with(&root)
            || executable != app_path.join("Contents/MacOS/window_lifecycle_runtime")
            || std::fs::read_to_string(root.join("owned-fixture"))? != "holdmap window fixture"
            || !fixture
                .identifier
                .strip_prefix("dev.holdmap.windowfixture.")
                .is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
                })
        {
            return Err("refusing to run outside the private owned scratch application".into());
        }
        let result_parent = fixture
            .result
            .parent()
            .ok_or("missing result parent")?
            .canonicalize()?;
        if !result_parent.starts_with(&root) {
            return Err("result directory escaped the private scratch root".into());
        }
        let result = result_parent.join(
            fixture
                .result
                .file_name()
                .ok_or("missing result filename")?,
        );
        if std::fs::symlink_metadata(&result)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err("result file must not be a symlink".into());
        }

        let mut context = tauri::generate_context!();
        context.config_mut().app.windows.clear();
        context.config_mut().identifier = fixture.identifier.clone();
        let controls = Arc::new(Controls {
            ready: AtomicBool::new(false),
            closed: AtomicUsize::new(0),
            dock_reopened: AtomicUsize::new(0),
            tray_available: AtomicBool::new(true),
            events: Mutex::new(Vec::new()),
            started: Instant::now(),
            dock_request: root.join("dock-request"),
        });
        let close_controls = controls.clone();
        let runtime_controls = controls.clone();
        tauri::Builder::default()
            .on_window_event(move |window, event| {
                if let tauri::WindowEvent::Focused(focused) = event {
                    close_controls.mark(&format!("focused:{}={focused}", window.label()));
                }
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    close_controls.mark(&format!("close_requested:{} entered main thread", window.label()));
                    crate::window_lifecycle::close_requested(window, api, close_controls.tray_available.load(Ordering::Acquire));
                    close_controls.mark(&format!("close_requested:{} completed main thread", window.label()));
                    if window.label() == "main" {
                        close_controls.closed.fetch_add(1, Ordering::Release);
                    }
                }
            })
            .setup(move |app| {
                let window = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::External("about:blank".parse().unwrap()))
                    .title("Owned Holdmap window lifecycle fixture")
                    .title_bar_style(tauri::TitleBarStyle::Overlay)
                    .hidden_title(true)
                    .inner_size(640.0, 420.0)
                    .center()
                    .build()?;
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let mut stages = Vec::new();
                    let measured = exercise(&handle, &window, &mut stages, &controls);
                    let activation_unavailable = matches!(measured, Ok(true));
                    let error = measured.err();
                    // Always return the owned window to a normal Space before exiting.
                    let cleanup = reopen(&handle, &window)
                        .and_then(|()| window.set_fullscreen(false).map_err(|error| error.to_string()))
                        .and_then(|()| settle(&handle, &window, &mut stages, "restored", |state| !state.fullscreen && state.visible));
                    let cleanup_error = cleanup.err();
                    let close_checks = if cleanup_error.is_none() {
                        normal_closes(&handle, &window, &controls)
                    } else {
                        Err("not attempted after an earlier fixture error".into())
                    };
                    let output = serde_json::json!({ "error": error, "cleanup_error": cleanup_error, "stages": stages, "activation_unavailable": activation_unavailable, "events": controls.events.lock().unwrap().clone(), "normal_closes": close_checks.as_ref().ok(), "normal_closes_error": close_checks.err(), "dock_reopen_count": controls.dock_reopened.load(Ordering::Acquire) });
                    let written = holdmap_core::util::create_private(&result)
                        .and_then(|mut file| file.write_all(&serde_json::to_vec_pretty(&output).unwrap()));
                    handle.exit(if written.is_ok() { 0 } else { 2 });
                });
                Ok(())
            })
            .build(context)?
            .run(move |handle, event| match event {
                tauri::RunEvent::Ready => {
                    runtime_controls.mark("owned Tauri Ready event");
                    runtime_controls.ready.store(true, Ordering::Release);
                }
                tauri::RunEvent::Reopen { .. } => {
                    runtime_controls.mark("Dock Reopen entered main thread");
                    if let Err(error) = crate::window_lifecycle::show_main(handle) {
                        runtime_controls.mark(&format!("Dock Reopen failed: {error}"));
                    } else {
                        runtime_controls.dock_reopened.fetch_add(1, Ordering::Release);
                        runtime_controls.mark("Dock Reopen completed main thread");
                    }
                }
                // Keep the fixture alive only long enough to prove real no-tray destruction
                // and write its result; explicit handle.exit below is still honored.
                tauri::RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
                _ => {}
            });
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    fixture::run()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("The owned window lifecycle runtime proof currently targets macOS.");
}

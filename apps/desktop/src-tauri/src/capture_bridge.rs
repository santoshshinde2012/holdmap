//! Bounded main-thread transport to the trusted native capture adapter.
use crate::capture_lease::{Callback, Session};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Runtime};

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(target_os = "macos")]
const MAX_REPLY: usize = 64 * 1024;
pub(crate) type Native = Arc<dyn Fn(Value) -> Result<Value, String> + Send + Sync>;

#[cfg(target_os = "macos")]
pub(crate) fn native_command(request: Value) -> Result<Value, String> {
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;
    #[link(name = "holdmap_screen_capture", kind = "static")]
    extern "C" {
        fn hm_capture_command(request: *const c_char) -> *mut c_char;
        fn hm_capture_free_json(reply: *mut c_char);
    }
    let text = serde_json::to_vec(&request).map_err(|_| "Invalid recording request.".to_owned())?;
    if text.len() > 8192 {
        return Err("Invalid recording request.".into());
    }
    let text = CString::new(text).map_err(|_| "Invalid recording request.".to_owned())?;
    // SAFETY: callers dispatch this adapter to the app's main thread. Swift accepts
    // a valid NUL-terminated request and returns owned strdup JSON or a null pointer.
    let pointer = unsafe { hm_capture_command(text.as_ptr()) };
    if pointer.is_null() {
        return Err("The native recorder could not respond.".into());
    }
    struct Reply(*mut c_char);
    impl Drop for Reply {
        fn drop(&mut self) {
            // SAFETY: this pointer is owned by the matching Swift allocation API.
            unsafe { hm_capture_free_json(self.0) };
        }
    }
    let reply = Reply(pointer);
    // SAFETY: the Swift ABI guarantees a NUL-terminated allocation until free_json.
    let bytes = unsafe { CStr::from_ptr(reply.0) }.to_bytes();
    if bytes.len() > MAX_REPLY {
        return Err("The native recorder response is too large.".into());
    }
    serde_json::from_slice(bytes).map_err(|_| "The native recorder returned invalid JSON.".into())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn native_command(_: Value) -> Result<Value, String> {
    Err("Native screen recording requires macOS 14 or newer.".into())
}

pub(crate) async fn call<R: Runtime>(
    app: &AppHandle<R>,
    request: Value,
    session: Option<Arc<Session>>,
    native: Option<Native>,
) -> Result<Value, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let pending = session.as_ref().map(|session| {
        session.callbacks.fetch_add(1, Ordering::AcqRel);
        Callback(Arc::clone(session))
    });
    let testing = native.is_some();
    let callback = move || {
        let result = if matches!(request["op"].as_str(), Some("start" | "start_snapshot"))
            && session
                .as_ref()
                .is_some_and(|session| session.cleaning.load(Ordering::Acquire))
        {
            Ok(json!({"session_id": request["session_id"], "phase": "cancelled"}))
        } else if let Some(native) = native {
            native(request)
        } else {
            native_command(request)
        };
        drop(pending);
        let _ = sender.send(result);
    };
    if testing {
        callback();
    } else {
        app.run_on_main_thread(callback)
            .map_err(|_| "Couldn't reach the native capture adapter.".to_owned())?;
    }
    tokio::time::timeout(CALL_TIMEOUT, receiver)
        .await
        .map_err(|_| "The native capture adapter did not respond in time.".to_owned())?
        .map_err(|_| "The native capture adapter could not respond.".to_owned())?
}

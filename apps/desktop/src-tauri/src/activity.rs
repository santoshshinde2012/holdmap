//! Trusted IPC adapters for an explicit, session-only metadata collector.
//!
//! Collection and normalization live in `holdmap-activity`. Credentials are only exposed
//! when starting or copying an integration; status and graph data never contain the token.

use holdmap_activity::{
    ActivityCollector, ActivitySnapshot, CollectorCredentials, CollectorStatus, Provider,
};
use std::sync::Mutex;
use tauri::{Manager, WebviewWindow};

#[derive(Default)]
pub(crate) struct ActivityState {
    session: Mutex<Session>,
}

#[derive(Default)]
struct Session {
    collector: ActivityCollector,
    credentials: Option<CollectorCredentials>,
}

impl Session {
    fn start(&mut self) -> Result<CollectorCredentials, String> {
        let credentials = self.collector.start().map_err(|error| error.to_string())?;
        self.credentials = Some(credentials.clone());
        Ok(credentials)
    }

    fn stop(&mut self) -> Result<CollectorStatus, String> {
        let status = self.collector.stop().map_err(|error| error.to_string())?;
        self.credentials = None;
        Ok(status)
    }

    fn setup(&self, provider: Provider) -> Result<String, String> {
        let credentials = self
            .credentials
            .as_ref()
            .filter(|_| self.collector.status().active)
            .ok_or("Start local recording before copying its setup.")?;
        holdmap_activity::integrations::setup(provider, credentials, cfg!(windows))
            .map_err(|error| error.to_string())
    }
}

/// Keep authorization, blocking work and session serialization on one IPC boundary.
async fn with_session<T: Send + 'static>(
    window: WebviewWindow,
    action: &str,
    join_error: &'static str,
    operation: impl FnOnce(&mut Session) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    crate::local_window::authorize(&window, action)?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<ActivityState>();
        let mut session = state
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        operation(&mut session)
    })
    .await
    .map_err(|_| join_error.to_owned())?
}

#[tauri::command]
pub async fn activity_status(window: WebviewWindow) -> Result<CollectorStatus, String> {
    with_session(
        window,
        "Agent activity",
        "Couldn't read recording status.",
        |session| Ok(session.collector.status()),
    )
    .await
}

#[tauri::command]
pub async fn activity_snapshot(window: WebviewWindow) -> Result<ActivitySnapshot, String> {
    with_session(
        window,
        "Agent activity",
        "Couldn't read recorded activity.",
        |session| Ok(session.collector.snapshot()),
    )
    .await
}

#[tauri::command]
pub async fn start_activity(window: WebviewWindow) -> Result<CollectorCredentials, String> {
    with_session(
        window,
        "Agent activity",
        "Couldn't start local recording.",
        Session::start,
    )
    .await
}

#[tauri::command]
pub async fn stop_activity(window: WebviewWindow) -> Result<CollectorStatus, String> {
    with_session(
        window,
        "Agent activity",
        "Couldn't stop local recording.",
        Session::stop,
    )
    .await
}

#[tauri::command]
pub async fn clear_activity(window: WebviewWindow) -> Result<(), String> {
    with_session(
        window,
        "Agent activity",
        "Couldn't clear recorded activity.",
        |session| session.collector.clear().map_err(|error| error.to_string()),
    )
    .await
}

#[tauri::command]
pub async fn activity_setup(window: WebviewWindow, provider: Provider) -> Result<String, String> {
    with_session(
        window,
        "Agent activity setup",
        "Couldn't prepare recording setup.",
        move |session| session.setup(provider),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_requires_active_recording_and_status_omits_credentials() {
        let mut session = Session::default();
        assert!(session.setup(Provider::Codex).is_err());
        let credentials = session.start().unwrap();
        assert!(session.start().is_err());
        let setup = session.setup(Provider::Codex).unwrap();
        assert!(setup.contains(&credentials.endpoint));
        assert!(setup.contains(&credentials.token));
        assert!(!serde_json::to_string(&session.collector.status())
            .unwrap()
            .contains(&credentials.token));
        session.stop().unwrap();
        assert!(session.credentials.is_none());
        assert!(session.setup(Provider::Codex).is_err());
        let next = session.start().unwrap();
        assert_ne!(credentials.token, next.token);
        session.stop().unwrap();
    }
}

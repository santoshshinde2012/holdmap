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

#[tauri::command]
pub async fn activity_status(window: WebviewWindow) -> Result<CollectorStatus, String> {
    crate::local_window::authorize(&window, "Agent activity")?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(app
            .state::<ActivityState>()
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .collector
            .status())
    })
    .await
    .map_err(|_| "Couldn't read recording status.")?
}

#[tauri::command]
pub async fn activity_snapshot(window: WebviewWindow) -> Result<ActivitySnapshot, String> {
    crate::local_window::authorize(&window, "Agent activity")?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(app
            .state::<ActivityState>()
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .collector
            .snapshot())
    })
    .await
    .map_err(|_| "Couldn't read recorded activity.")?
}

#[tauri::command]
pub async fn start_activity(window: WebviewWindow) -> Result<CollectorCredentials, String> {
    crate::local_window::authorize(&window, "Agent activity")?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<ActivityState>()
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .start()
    })
    .await
    .map_err(|_| "Couldn't start local recording.")?
}

#[tauri::command]
pub async fn stop_activity(window: WebviewWindow) -> Result<CollectorStatus, String> {
    crate::local_window::authorize(&window, "Agent activity")?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<ActivityState>()
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .stop()
    })
    .await
    .map_err(|_| "Couldn't stop local recording.")?
}

#[tauri::command]
pub async fn clear_activity(window: WebviewWindow) -> Result<(), String> {
    crate::local_window::authorize(&window, "Agent activity")?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<ActivityState>()
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .collector
            .clear()
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "Couldn't clear recorded activity.")?
}

#[tauri::command]
pub async fn activity_setup(window: WebviewWindow, provider: Provider) -> Result<String, String> {
    crate::local_window::authorize(&window, "Agent activity setup")?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<ActivityState>()
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .setup(provider)
    })
    .await
    .map_err(|_| "Couldn't prepare recording setup.")?
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

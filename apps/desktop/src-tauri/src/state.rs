//! Shared app state and the scan helper every command goes through.

use portwise_core::store::Store;
use portwise_core::{Engine, ScanOptions};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub struct AppState {
    pub engine: Mutex<Option<Engine>>,
    pub docker: AtomicBool,
    pub tray_ok: AtomicBool,
    /// Config, pins and stop history (`$PORTWISE_HOME` or the platform config dir).
    pub store: Store,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            engine: Mutex::new(None),
            docker: AtomicBool::new(true),
            tray_ok: AtomicBool::new(false),
            store: Store::open_default(),
        }
    }
}

impl AppState {
    pub fn docker(&self) -> bool {
        self.docker.load(Ordering::Relaxed)
    }
}

pub fn scan_now(all: bool, docker: bool) -> Result<Engine, String> {
    Engine::new(&ScanOptions {
        all_states: all,
        docker,
    })
    .map_err(|e| format!("Couldn't read the socket table: {e}"))
}

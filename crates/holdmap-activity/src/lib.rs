//! Opt-in activity collection with a bounded metadata projection and local HTTP transport.
//!
//! The collector performs no process control, filesystem reads or outbound requests. It
//! starts only through [`ActivityCollector::start`], binds IPv4 loopback and requires fresh
//! bearer credentials. [`client`] sends metadata only to an explicitly supplied loopback
//! endpoint, and [`integrations`] generates configuration without writing agent settings.

mod dto;
mod normalize;
mod server;
mod store;

#[cfg(test)]
mod tests;

pub mod client;
pub mod integrations;

pub use dto::*;
pub use normalize::{normalize_hook, pseudonymize_id};

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// A fixed-message error that never includes payloads, paths or credentials.
#[derive(Debug, Error)]
pub enum ActivityError {
    /// A collector start was requested while one is already owned.
    #[error("Activity collection is already running")]
    AlreadyActive,
    /// Listener, randomness or worker creation failed.
    #[error("Activity collection could not start")]
    StartFailed,
    /// Internal synchronization is unavailable.
    #[error("Activity collection is unavailable")]
    Unavailable,
    /// A source record does not satisfy the metadata schema.
    #[error("Invalid activity metadata")]
    InvalidMetadata,
    /// A bounded projection or request limit was exceeded.
    #[error("Activity metadata exceeds collector limits")]
    LimitExceeded,
}

/// Owns one local listener and a bounded in-memory store. Inactive by default.
pub struct ActivityCollector {
    store: Arc<Mutex<store::ActivityStore>>,
    runtime: Mutex<Option<server::Runtime>>,
}

impl Default for ActivityCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityCollector {
    /// Create an inactive collector with an empty in-memory store.
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(store::ActivityStore::new())),
            runtime: Mutex::new(None),
        }
    }

    /// Explicitly start recording and return private credentials for integration setup.
    /// Existing retained metadata remains available until explicitly cleared.
    pub fn start(&self) -> Result<CollectorCredentials, ActivityError> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| ActivityError::Unavailable)?;
        if runtime.is_some() {
            return Err(ActivityError::AlreadyActive);
        }
        let (started, credentials) = server::Runtime::start(Arc::clone(&self.store))?;
        *runtime = Some(started);
        Ok(credentials)
    }

    /// Stop and join owned workers, retain captured metadata, and mark unfinished starts unknown.
    pub fn stop(&self) -> Result<CollectorStatus, ActivityError> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| ActivityError::Unavailable)?;
        if let Some(mut running) = runtime.take() {
            running.stop();
        }
        self.store
            .lock()
            .map_err(|_| ActivityError::Unavailable)?
            .finish_pending();
        drop(runtime);
        Ok(self.status())
    }

    /// Clear captured metadata and counters without changing whether recording is active.
    pub fn clear(&self) -> Result<(), ActivityError> {
        self.store
            .lock()
            .map_err(|_| ActivityError::Unavailable)?
            .clear();
        Ok(())
    }

    /// Read a bounded metadata snapshot without credentials or source payloads.
    pub fn snapshot(&self) -> ActivitySnapshot {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot()
    }

    /// Read recording state and store counters without private credentials.
    pub fn status(&self) -> CollectorStatus {
        let runtime = self
            .runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let store = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        CollectorStatus {
            active: runtime.as_ref().is_some_and(server::Runtime::active),
            endpoint: runtime.as_ref().map(|running| running.endpoint.clone()),
            started_at_ms: runtime.as_ref().map(|running| running.started_at_ms),
            received: store.received,
            dropped: store.dropped,
        }
    }
}

impl Drop for ActivityCollector {
    fn drop(&mut self) {
        if let Some(mut running) = self
            .runtime
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            running.stop();
        }
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

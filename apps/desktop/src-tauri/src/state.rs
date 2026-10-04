//! Shared app state and the scan helper every command goes through.

use portwise_core::store::Store;
use portwise_core::{Engine, ScanOptions, Snapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct AppState {
    pub engine: Mutex<Option<Engine>>,
    /// Held for the length of a scan, so the UI poll, the watcher and the tray never scan at
    /// the same time (they wait and share the result instead).
    scan_lock: Mutex<()>,
    /// When the engine's scan finished, and whether it included non-listening sockets.
    last_scan: Mutex<Option<(Instant, bool)>>,
    pub docker: AtomicBool,
    pub tray_ok: AtomicBool,
    /// Whether the updater plugin is registered (release builds with an update key).
    pub updater: AtomicBool,
    /// Config, pins and stop history (`$PORTWISE_HOME` or the platform config dir).
    pub store: Store,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            engine: Mutex::new(None),
            scan_lock: Mutex::new(()),
            last_scan: Mutex::new(None),
            docker: AtomicBool::new(true),
            tray_ok: AtomicBool::new(false),
            updater: AtomicBool::new(false),
            store: Store::open_default(),
        }
    }
}

impl AppState {
    pub fn docker(&self) -> bool {
        self.docker.load(Ordering::Relaxed)
    }

    /// The current snapshot, scanning only when the last scan is older than `max_age` (or of
    /// the wrong kind). One scan runs at a time; a caller that arrives mid-scan waits for it
    /// and, being fresh, reuses it. Blocking: call it off the main thread.
    pub fn snapshot(&self, all: bool, max_age: Duration) -> Result<Snapshot, String> {
        let _one_at_a_time = self.scan_lock.lock().unwrap_or_else(|e| e.into_inner());
        let last = *self.last_scan.lock().unwrap_or_else(|e| e.into_inner());
        if reusable(last, all, max_age, Instant::now()) {
            if let Some(e) = self
                .engine
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
            {
                return Ok(e.snapshot().clone());
            }
        }
        let engine = scan_now(all, self.docker())?;
        let snapshot = engine.snapshot().clone();
        *self.engine.lock().unwrap_or_else(|e| e.into_inner()) = Some(engine);
        *self.last_scan.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), all));
        Ok(snapshot)
    }
}

/// Can a scan that finished at `last` (with `last_all` sockets) answer a request for `all`
/// sockets no older than `max_age`? Only a scan of the same kind will do.
fn reusable(last: Option<(Instant, bool)>, all: bool, max_age: Duration, now: Instant) -> bool {
    last.is_some_and(|(at, last_all)| {
        last_all == all && now.saturating_duration_since(at) <= max_age
    })
}

pub fn scan_now(all: bool, docker: bool) -> Result<Engine, String> {
    Engine::new(&ScanOptions {
        all_states: all,
        docker,
    })
    .map_err(|e| format!("Couldn't read the socket table: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recent_scan_is_shared() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let at = |d: u64| t0 + ms(d);
        assert!(!reusable(None, false, ms(1000), t0));
        assert!(reusable(Some((t0, false)), false, ms(1000), at(400)));
        assert!(
            !reusable(Some((t0, false)), false, ms(1000), at(1500)),
            "too old"
        );
        assert!(
            !reusable(Some((t0, false)), false, Duration::ZERO, at(1)),
            "forced fresh"
        );
        assert!(
            !reusable(Some((t0, false)), true, ms(1000), at(10)),
            "needs every socket"
        );
        assert!(
            !reusable(Some((t0, true)), false, ms(1000), at(10)),
            "would add non-listeners"
        );
        assert!(reusable(Some((t0, true)), true, ms(1000), at(10)));
    }
}

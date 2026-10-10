//! Shared app state and the scan helper every command goes through.

use holdmap_core::store::Store;
use holdmap_core::{Engine, ScanOptions, Snapshot};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct AppState {
    pub engine: Mutex<Option<Engine>>,
    /// Held for the length of a scan, so the UI poll, the watcher and the tray never scan at
    /// the same time (they wait and share the result instead).
    scan_lock: Mutex<()>,
    /// When the engine's scan finished and the options used to collect it.
    last_scan: Mutex<Option<CachedScan>>,
    pub docker: AtomicBool,
    pub tray_ok: AtomicBool,
    /// Whether the updater plugin is registered (release builds with an update key).
    pub updater: AtomicBool,
    /// Config, pins and stop history (`$HOLDMAP_HOME` or the platform config dir).
    pub store: Store,
}

#[derive(Clone, Copy)]
struct CachedScan {
    at: Instant,
    all: bool,
    docker: bool,
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
        self.read_engine(Some(all), max_age, scan_now, |engine| {
            engine.snapshot().clone()
        })
    }

    /// Read a derived report from a fresh engine, retaining the latest socket-view setting.
    /// Shares the scan lock with snapshot requests so a report cannot race another scan.
    /// Blocking: call it off the main thread.
    pub fn with_engine<T>(
        &self,
        max_age: Duration,
        read: impl FnOnce(&Engine) -> T,
    ) -> Result<T, String> {
        self.read_engine(None, max_age, scan_now, read)
    }

    fn read_engine<T>(
        &self,
        all: Option<bool>,
        max_age: Duration,
        collect: impl FnOnce(bool, bool) -> Result<Engine, String>,
        read: impl FnOnce(&Engine) -> T,
    ) -> Result<T, String> {
        let _one_at_a_time = self.scan_lock.lock().unwrap_or_else(|e| e.into_inner());
        let last = *self.last_scan.lock().unwrap_or_else(|e| e.into_inner());
        let all = all.unwrap_or_else(|| last.is_some_and(|scan| scan.all));
        let docker = self.docker();
        if reusable(last, all, docker, max_age, Instant::now()) {
            let engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(engine) = engine.as_ref() {
                return Ok(read(engine));
            }
        }
        // Collect under the scan lock alone: synchronous folder actions may keep reading the
        // previous engine while a slow OS/container query runs on this background thread.
        // A failed refresh returns the error and leaves the previous cache due for a retry.
        let fresh = collect(all, docker)?;
        let at = Instant::now();
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        *engine = Some(fresh);
        *self.last_scan.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(CachedScan { at, all, docker });
        Ok(read(engine.as_ref().ok_or("no scan yet")?))
    }
}

/// Can the cached scan answer this request without changing the socket view or container
/// setting? A zero age always requests a new scan.
fn reusable(
    last: Option<CachedScan>,
    all: bool,
    docker: bool,
    max_age: Duration,
    now: Instant,
) -> bool {
    !max_age.is_zero()
        && last.is_some_and(|scan| {
            scan.all == all
                && scan.docker == docker
                && now.saturating_duration_since(scan.at) <= max_age
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
    use holdmap_core::agents::{AgentsBuilder, AgentsReport};
    use holdmap_core::{ProcessInfo, ProcessTable, Scan};

    fn fixture_engine(name: &str, taken_at_ms: u64, warnings: Vec<String>) -> Engine {
        let process = ProcessInfo {
            pid: 100,
            ppid: None,
            name: name.into(),
            exe: None,
            cmdline: vec![name.into()],
            cwd: None,
            uid: Some(1000),
            user: Some("fixture".into()),
            start_time: 1,
            start_token: taken_at_ms,
            memory_bytes: 0,
            cpu_percent: 0.0,
        };
        Engine::from_scan(Scan {
            snapshot: Snapshot {
                entries: vec![],
                hidden_sockets: 0,
                platform: "fixture".into(),
                taken_at_ms,
                scan_ms: 0,
                docker_available: false,
                warnings,
            },
            table: ProcessTable::from_processes([(100, process)].into(), 999),
            published: vec![],
            raw: vec![],
        })
    }

    fn report(engine: &Engine) -> AgentsReport {
        AgentsBuilder::new(&engine.scan)
            .with_home(None)
            .with_project_detection(false)
            .build()
    }

    fn seed(state: &AppState, engine: Engine, age: Duration, all: bool, docker: bool) {
        *state.engine.lock().unwrap() = Some(engine);
        *state.last_scan.lock().unwrap() = Some(CachedScan {
            at: Instant::now() - age,
            all,
            docker,
        });
    }

    #[test]
    fn a_recent_scan_is_shared() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let at = |d: u64| t0 + ms(d);
        let cached = |all| {
            Some(CachedScan {
                at: t0,
                all,
                docker: true,
            })
        };
        assert!(!reusable(None, false, true, ms(1000), t0));
        assert!(reusable(cached(false), false, true, ms(1000), at(400)));
        assert!(
            !reusable(cached(false), false, true, ms(1000), at(1500)),
            "too old"
        );
        assert!(
            !reusable(cached(false), false, true, Duration::ZERO, t0),
            "forced fresh"
        );
        assert!(
            !reusable(cached(false), true, true, ms(1000), at(10)),
            "needs every socket"
        );
        assert!(
            !reusable(cached(true), false, true, ms(1000), at(10)),
            "would add non-listeners"
        );
        assert!(reusable(cached(true), true, true, ms(1000), at(10)));
        assert!(
            !reusable(cached(false), false, false, ms(1000), at(10)),
            "container option changed"
        );
    }

    #[test]
    fn derived_reports_refresh_stale_processes_and_share_the_new_scan() {
        let state = AppState::default();
        seed(
            &state,
            fixture_engine("codex", 1, vec![]),
            Duration::from_secs(2),
            true,
            true,
        );
        let current = state
            .read_engine(
                None,
                Duration::from_secs(1),
                |all, docker| {
                    assert!(
                        state.engine.try_lock().is_ok(),
                        "collection must not block synchronous cache readers"
                    );
                    assert!(
                        all && docker,
                        "derived reads retain the current socket-view options"
                    );
                    Ok(fixture_engine(
                        "claude",
                        2,
                        vec!["partial socket fixture".into()],
                    ))
                },
                report,
            )
            .unwrap();
        assert_eq!(current.taken_at_ms, 2);
        assert_eq!(
            current.agents[0].product, "claude-code",
            "the exited agent is not retained"
        );
        assert!(current
            .limits
            .iter()
            .any(|limit| limit.contains("partial socket fixture")));
        let shared = state
            .read_engine(
                None,
                Duration::from_secs(1),
                |_, _| panic!("fresh scan should be shared"),
                report,
            )
            .unwrap();
        assert_eq!(shared.taken_at_ms, current.taken_at_ms);
        assert_eq!(shared.agents[0].product, current.agents[0].product);
    }

    #[test]
    fn changing_container_collection_invalidates_a_fresh_scan() {
        let state = AppState::default();
        seed(
            &state,
            fixture_engine("codex", 1, vec![]),
            Duration::ZERO,
            false,
            true,
        );
        state.docker.store(false, Ordering::Relaxed);
        let snapshot = state
            .read_engine(
                Some(false),
                Duration::from_secs(60),
                |all, docker| {
                    assert!(!all && !docker);
                    Ok(fixture_engine("codex", 2, vec![]))
                },
                |engine| engine.snapshot().clone(),
            )
            .unwrap();
        assert_eq!(snapshot.taken_at_ms, 2);
        assert!(!state.last_scan.lock().unwrap().unwrap().docker);
    }

    #[test]
    fn failed_refresh_returns_an_error_and_remains_due_for_a_retry() {
        let state = AppState::default();
        seed(
            &state,
            fixture_engine("codex", 1, vec![]),
            Duration::from_secs(2),
            false,
            true,
        );
        let at = state.last_scan.lock().unwrap().unwrap().at;
        let failed = state.read_engine(
            None,
            Duration::from_secs(1),
            |_, _| Err("socket fixture failed".into()),
            report,
        );
        assert_eq!(failed.unwrap_err(), "socket fixture failed");
        assert_eq!(state.last_scan.lock().unwrap().unwrap().at, at);
        let retried = state
            .read_engine(
                None,
                Duration::from_secs(1),
                |_, _| Ok(fixture_engine("claude", 2, vec![])),
                report,
            )
            .unwrap();
        assert_eq!(retried.taken_at_ms, 2);
    }
}

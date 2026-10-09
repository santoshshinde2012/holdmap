//! Project lookups off the scan path.
//!
//! Detecting a project reads files under the process's working directory (manifests, `.git`).
//! Those reads can block for a long time: on macOS the first read under `~/Documents`,
//! `~/Desktop` or `~/Downloads` waits until the user answers the privacy prompt, and a stale
//! network mount can hang. A scan must never wait on that, so lookups run on background
//! threads and a scan waits at most a short, shared budget for them.
//!
//! Results are cached per directory. A stale result is served while it is refreshed in the
//! background, so project fields never blink out between scans. A lookup that hangs or is
//! denied just leaves the project empty.

use super::{detect_project, ProjectDetails};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Resolves a project for `(cwd, home)`. Runs on a background thread and may block.
pub type Resolver = Arc<dyn Fn(&Path, Option<&Path>) -> Option<ProjectDetails> + Send + Sync>;

type Key = (PathBuf, Option<PathBuf>);

/// How long a scan waits, in total, for project lookups that aren't cached yet.
const DEFAULT_BUDGET: Duration = Duration::from_millis(100);
/// How long a result is fresh; after that it is re-read in the background (git branch changes).
const DEFAULT_TTL: Duration = Duration::from_secs(10);
/// Lookups that may run at once. Hung lookups hold a slot, so this also caps hung threads.
const MAX_IN_FLIGHT: usize = 8;
/// Entries not asked for in this long are dropped once the cache grows.
const IDLE_DROP: Duration = Duration::from_secs(120);
const PRUNE_ABOVE: usize = 256;

static BUDGET_MS: AtomicU64 = AtomicU64::new(DEFAULT_BUDGET.as_millis() as u64);

/// Set how long a scan may wait for uncached project lookups (the desktop app uses a few ms).
pub fn set_scan_budget(d: Duration) {
    BUDGET_MS.store(d.as_millis() as u64, Ordering::Relaxed);
}

/// The current scan budget.
pub fn scan_budget() -> Duration {
    Duration::from_millis(BUDGET_MS.load(Ordering::Relaxed))
}

#[derive(Default)]
struct Slot {
    /// `None` until the first lookup finishes; then the result (which may be "no project").
    value: Option<Option<ProjectDetails>>,
    /// When `value` was produced.
    at: Option<Instant>,
    /// A lookup is running for this key.
    pending: bool,
    /// Last time a scan asked for it.
    used: Option<Instant>,
}

#[derive(Default)]
struct Inner {
    slots: HashMap<Key, Slot>,
    in_flight: usize,
}

/// A shared, thread-safe cache of project lookups.
pub struct ProjectCache {
    resolve: Resolver,
    ttl: Duration,
    inner: Mutex<Inner>,
    ready: Condvar,
}

impl std::fmt::Debug for ProjectCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectCache")
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl ProjectCache {
    /// A cache that resolves with `resolve` and refreshes results older than `ttl`.
    pub fn new(resolve: Resolver, ttl: Duration) -> Arc<Self> {
        Arc::new(Self {
            resolve,
            ttl,
            inner: Mutex::new(Inner::default()),
            ready: Condvar::new(),
        })
    }

    /// The process-wide cache used by scans.
    pub fn global() -> Arc<Self> {
        static G: OnceLock<Arc<ProjectCache>> = OnceLock::new();
        G.get_or_init(|| Self::new(Arc::new(detect_project), DEFAULT_TTL))
            .clone()
    }

    /// The project for `cwd`, waiting no later than `deadline` for a lookup that isn't cached
    /// yet. Never blocks past the deadline; returns `None` when the answer isn't ready.
    pub fn get(
        self: &Arc<Self>,
        cwd: &Path,
        home: Option<&Path>,
        deadline: Instant,
    ) -> Option<ProjectDetails> {
        let key: Key = (cwd.to_path_buf(), home.map(Path::to_path_buf));
        let mut inner = self.lock();
        let now = Instant::now();
        if inner.slots.len() > PRUNE_ABOVE {
            inner.slots.retain(|_, s| {
                s.pending || s.used.is_some_and(|u| now.duration_since(u) < IDLE_DROP)
            });
        }
        let slot = inner.slots.entry(key.clone()).or_default();
        slot.used = Some(now);
        let stale = slot.at.is_none_or(|at| now.duration_since(at) >= self.ttl);
        if stale && !slot.pending {
            self.start(&mut inner, key.clone());
        }
        loop {
            if let Some(v) = inner.slots.get(&key).and_then(|s| s.value.clone()) {
                return v;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            inner = self
                .ready
                .wait_timeout(inner, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn start(self: &Arc<Self>, inner: &mut Inner, key: Key) {
        if inner.in_flight >= MAX_IN_FLIGHT {
            return; // try again on a later scan
        }
        if let Some(s) = inner.slots.get_mut(&key) {
            s.pending = true;
        }
        inner.in_flight += 1;
        let me = Arc::clone(self);
        let k = key.clone();
        let spawned = std::thread::Builder::new()
            .name("holdmap-project".into())
            .spawn(move || {
                let v = (me.resolve)(&k.0, k.1.as_deref());
                let mut inner = me.lock();
                inner.in_flight -= 1;
                if let Some(s) = inner.slots.get_mut(&k) {
                    s.value = Some(v);
                    s.at = Some(Instant::now());
                    s.pending = false;
                }
                drop(inner);
                me.ready.notify_all();
            });
        if spawned.is_err() {
            inner.in_flight -= 1;
            if let Some(s) = inner.slots.get_mut(&key) {
                s.pending = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProjectInfo;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;

    fn details(name: &str) -> ProjectDetails {
        ProjectDetails {
            info: ProjectInfo {
                name: name.into(),
                root: PathBuf::from("/p"),
                kind: "package.json".into(),
                git_branch: None,
                workspace: None,
                git_root: None,
            },
            deps: vec![],
        }
    }

    fn soon(ms: u64) -> Instant {
        Instant::now() + Duration::from_millis(ms)
    }

    #[test]
    fn a_hanging_lookup_returns_nothing_by_the_deadline_then_fills_in() {
        let (tx, rx) = mpsc::channel::<()>();
        let rx = Mutex::new(rx);
        let cache = ProjectCache::new(
            Arc::new(move |_, _| {
                let _ = rx.lock().unwrap().recv(); // blocks like an unanswered privacy prompt
                Some(details("web"))
            }),
            Duration::from_secs(60),
        );
        let t = Instant::now();
        assert_eq!(cache.get(Path::new("/p"), None, soon(30)), None);
        assert!(
            t.elapsed() < Duration::from_millis(500),
            "waited {:?}",
            t.elapsed()
        );
        // Asking again doesn't start a second lookup or wait past its deadline.
        assert_eq!(cache.get(Path::new("/p"), None, Instant::now()), None);
        tx.send(()).unwrap();
        let v = cache.get(Path::new("/p"), None, soon(5_000));
        assert_eq!(v.map(|d| d.info.name), Some("web".into()));
    }

    #[test]
    fn a_stale_result_is_served_while_it_refreshes() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let (tx, rx) = mpsc::channel::<()>();
        let rx = Mutex::new(rx);
        let cache = ProjectCache::new(
            Arc::new(move |_, _| {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n > 0 {
                    let _ = rx.lock().unwrap().recv(); // the refresh hangs
                }
                Some(details(&format!("v{n}")))
            }),
            Duration::ZERO,
        );
        let first = cache.get(Path::new("/p"), None, soon(5_000));
        assert_eq!(first.map(|d| d.info.name), Some("v0".into()));
        // Stale now: a refresh starts, but the old value comes back at once (no blink).
        let t = Instant::now();
        for _ in 0..3 {
            let v = cache.get(Path::new("/p"), None, soon(1_000));
            assert_eq!(v.map(|d| d.info.name), Some("v0".into()));
        }
        assert!(t.elapsed() < Duration::from_millis(500));
        let until = soon(2_000);
        while calls.load(Ordering::SeqCst) < 2 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(calls.load(Ordering::SeqCst), 2, "one refresh at a time");
        tx.send(()).unwrap();
    }

    #[test]
    fn no_project_is_cached_too() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let cache = ProjectCache::new(
            Arc::new(move |_, _| {
                c.fetch_add(1, Ordering::SeqCst);
                None
            }),
            Duration::from_secs(60),
        );
        for _ in 0..3 {
            assert_eq!(cache.get(Path::new("/x"), None, soon(2_000)), None);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn hung_lookups_are_capped() {
        let (tx, rx) = mpsc::channel::<()>();
        let rx = Arc::new(Mutex::new(rx));
        let started = Arc::new(AtomicUsize::new(0));
        let s = started.clone();
        let cache = ProjectCache::new(
            Arc::new(move |_, _| {
                s.fetch_add(1, Ordering::SeqCst);
                let _ = rx.lock().unwrap().recv();
                None
            }),
            Duration::from_secs(60),
        );
        for i in 0..(MAX_IN_FLIGHT + 5) {
            let _ = cache.get(Path::new(&format!("/d{i}")), None, Instant::now());
        }
        std::thread::sleep(Duration::from_millis(100));
        assert!(started.load(Ordering::SeqCst) <= MAX_IN_FLIGHT);
        drop(tx);
    }
}

//! Shared private lifetime ownership for native video and still-image jobs.
//! Cleanup waits for admitted IO operations and queued native callbacks.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tempfile::TempDir;

const UNKNOWN_SESSION: &str = "This capture session is no longer available.";

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

pub(crate) struct Session {
    pub(crate) id: String,
    directory: Mutex<Option<TempDir>>,
    pub(crate) busy: AtomicBool,
    pub(crate) cleaning: AtomicBool,
    pub(crate) callbacks: AtomicUsize,
    pub(crate) reaping: AtomicBool,
    pub(crate) reaper: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl Session {
    pub(crate) fn new(prefix: &str) -> Result<Self, String> {
        let directory = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .map_err(|_| "Couldn't create a private recording directory.".to_owned())?;
        holdmap_core::util::create_private_dir(directory.path())
            .map_err(|_| "Couldn't protect the recording directory.".to_owned())?;
        directory
            .path()
            .to_str()
            .ok_or("The recording directory needs a valid Unicode path.")?;
        let id = uuid::Uuid::new_v4().hyphenated().to_string();
        Ok(Self {
            id,
            directory: Mutex::new(Some(directory)),
            busy: AtomicBool::new(false),
            cleaning: AtomicBool::new(false),
            callbacks: AtomicUsize::new(0),
            reaping: AtomicBool::new(false),
            reaper: Mutex::new(None),
        })
    }

    pub(crate) fn path(&self) -> Result<std::path::PathBuf, String> {
        lock(&self.directory)
            .as_ref()
            .map(|directory| directory.path().to_owned())
            .ok_or_else(|| UNKNOWN_SESSION.into())
    }

    pub(crate) fn remove_files(&self) {
        drop(lock(&self.directory).take());
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Unexpected shutdown cannot prove that native callbacks have stopped. Normal
        // discard and explicit process-exit cleanup remove files before releasing this slot.
        if let Some(directory) = lock(&self.directory).take() {
            let _ = directory.keep();
        }
    }
}

pub(crate) struct Operation(Arc<Session>);
impl Operation {
    pub(crate) fn acquire(session: Arc<Session>) -> Result<Self, String> {
        if session.cleaning.load(Ordering::Acquire)
            || session
                .busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return Err("The recording is busy or being discarded. Try again shortly.".into());
        }
        // Pair with the reaper's fence before reading busy: either this check
        // observes cleanup or cleanup observes this guard. Acquire/Release alone
        // allows both threads to miss the other's store to a different atomic.
        std::sync::atomic::fence(Ordering::SeqCst);
        // Cleanup may have begun while acquiring the operation.
        if session.cleaning.load(Ordering::Acquire) {
            session.busy.store(false, Ordering::Release);
            return Err("The recording is being discarded.".into());
        }
        Ok(Self(session))
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}

pub(crate) struct Callback(pub(crate) Arc<Session>);
impl Drop for Callback {
    fn drop(&mut self) {
        self.0.callbacks.fetch_sub(1, Ordering::Release);
    }
}

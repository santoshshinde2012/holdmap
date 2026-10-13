//! Persistent user state: config (pins, notification settings) and the stopped-port history.
//!
//! Everything lives in one directory (see [`Store::default_dir`]) as small JSON / JSON-lines
//! files written atomically. Frontends depend on [`Store`], tests point it at a temp dir.

use crate::history::HistoryEntry;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const MAX_CONFIG_BYTES: usize = 1024 * 1024;
const MAX_HISTORY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_HISTORY_ENTRY_BYTES: usize = 1024 * 1024;

// Hold the process lock as well as the file lock: separate Store instances and
// concurrent frontends must not lose each other's read/change/write transactions.
static TRANSACTIONS: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Transaction {
    _local: std::sync::MutexGuard<'static, ()>,
    _file: fs::File,
}

struct BoundedBuffer {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "serialized state exceeds its size limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A pinned ("favourite") port, shown first and watched even while it's free.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    /// Pinned port.
    pub port: u16,
    /// Optional note such as "shop web".
    #[serde(default)]
    pub label: Option<String>,
}

/// User configuration (`config.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Pinned ports.
    pub pins: Vec<Pin>,
    /// Desktop/watch notifications for new listeners, closed listeners and conflicts.
    pub notify: bool,
    /// Only notify about dev servers (and pinned ports).
    pub notify_dev_only: bool,
    /// How many stopped ports to remember.
    pub history_limit: usize,
    /// Foreground re-scan interval for the desktop app and its watcher, in seconds
    /// (the background cadence is derived from it). Clamped to [`Config::SCAN_INTERVAL`].
    pub scan_interval_secs: u64,
    /// Global "show holdmap" shortcut preset: `alt-p` (⌘⌥P / Ctrl+Alt+P), `alt-space`
    /// (⌥Space / Ctrl+Alt+Space), `alt-k` (⌘⌥K / Ctrl+Alt+K) or `off`.
    pub hotkey: String,
    /// Recently used SSH hosts for the desktop remote view, newest first.
    pub recent_hosts: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pins: Vec::new(),
            notify: true,
            notify_dev_only: true,
            history_limit: 200,
            scan_interval_secs: 4,
            hotkey: "alt-p".into(),
            recent_hosts: Vec::new(),
        }
    }
}

impl Config {
    /// Allowed foreground scan interval, in seconds.
    pub const SCAN_INTERVAL: std::ops::RangeInclusive<u64> = 1..=60;
    /// Allowed history length.
    pub const HISTORY_LIMIT: std::ops::RangeInclusive<usize> = 10..=5000;
    /// How many recent SSH hosts are remembered.
    pub const RECENT_HOSTS: usize = 6;

    /// Foreground scan interval, clamped to the allowed range.
    pub fn scan_interval(&self) -> std::time::Duration {
        let (lo, hi) = (*Self::SCAN_INTERVAL.start(), *Self::SCAN_INTERVAL.end());
        std::time::Duration::from_secs(self.scan_interval_secs.clamp(lo, hi))
    }

    /// Background (window hidden) interval: 2.5× the foreground one, at least 10 s.
    pub fn background_interval(&self) -> std::time::Duration {
        (self.scan_interval() * 5 / 2).max(std::time::Duration::from_secs(10))
    }

    /// Pin `port` (or update its label). An empty label is stored as `None`.
    pub fn set_pin(&mut self, port: u16, label: Option<String>) {
        let label = label
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty());
        match self.pins.iter_mut().find(|p| p.port == port) {
            Some(p) => p.label = label,
            None => {
                self.pins.push(Pin { port, label });
                self.pins.sort_by_key(|p| p.port);
            }
        }
    }

    /// Remove a pin; returns whether it existed.
    pub fn unpin(&mut self, port: u16) -> bool {
        let before = self.pins.len();
        self.pins.retain(|p| p.port != port);
        before != self.pins.len()
    }

    /// Move `host` to the front of the recent-hosts list.
    pub fn remember_host(&mut self, host: &str) {
        self.recent_hosts.retain(|h| h != host);
        self.recent_hosts.insert(0, host.to_string());
        self.recent_hosts.truncate(Self::RECENT_HOSTS);
    }

    /// True when `port` is pinned.
    pub fn is_pinned(&self, port: u16) -> bool {
        self.pins.iter().any(|p| p.port == port)
    }

    /// Pin or unpin `port`; returns the new pinned state.
    pub fn toggle_pin(&mut self, port: u16, label: Option<String>) -> bool {
        if self.is_pinned(port) {
            self.pins.retain(|p| p.port != port);
            false
        } else {
            self.pins.push(Pin { port, label });
            self.pins.sort_by_key(|p| p.port);
            true
        }
    }
}

/// File-backed store.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// A store rooted at `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// `$HOLDMAP_HOME` (or legacy `$PORTWISE_HOME`), else the platform config dir
    /// (`~/.config/holdmap`, `~/Library/Application Support/holdmap`, `%APPDATA%\holdmap`).
    /// If only a legacy `portwise` directory exists, it is renamed to `holdmap` once.
    pub fn default_dir() -> PathBuf {
        if let Some(d) = std::env::var_os("HOLDMAP_HOME")
            .or_else(|| std::env::var_os("PORTWISE_HOME"))
            .filter(|d| !d.is_empty())
        {
            return PathBuf::from(d);
        }
        let dir = Self::platform_config_dir("holdmap");
        let legacy = Self::platform_config_dir("portwise");
        if !dir.exists() && legacy.exists() {
            let _ = fs::rename(&legacy, &dir);
        }
        dir
    }

    fn platform_config_dir(name: &str) -> PathBuf {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        if cfg!(windows) {
            if let Some(a) = std::env::var_os("APPDATA") {
                return PathBuf::from(a).join(name);
            }
        }
        if cfg!(target_os = "macos") {
            return home.join("Library/Application Support").join(name);
        }
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join(name)
    }

    /// The store in the platform config directory (`HOLDMAP_HOME` overrides it).
    pub fn open_default() -> Self {
        Self::new(Self::default_dir())
    }

    /// Directory holding `config.json` and `history.jsonl`.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn config_path(&self) -> PathBuf {
        self.dir.join("config.json")
    }

    fn history_path(&self) -> PathBuf {
        self.dir.join("history.jsonl")
    }

    /// Directory for logs of restarted commands.
    pub fn logs_dir(&self) -> PathBuf {
        self.dir.join("logs")
    }

    /// Load the config; a missing or unreadable file yields defaults.
    pub fn config(&self) -> Config {
        self.read_file("config.json")
            .ok()
            .and_then(|f| {
                let mut text = String::new();
                f.take(MAX_CONFIG_BYTES as u64 + 1)
                    .read_to_string(&mut text)
                    .ok()?;
                if text.len() > MAX_CONFIG_BYTES {
                    return None;
                }
                serde_json::from_str(&text).ok()
            })
            .unwrap_or_default()
    }

    /// Write `config.json` atomically.
    pub fn save_config(&self, c: &Config) -> io::Result<()> {
        let _transaction = self.transaction()?;
        self.save_config_unlocked(c)
    }

    fn save_config_unlocked(&self, c: &Config) -> io::Result<()> {
        let mut text = BoundedBuffer {
            bytes: Vec::new(),
            limit: MAX_CONFIG_BYTES,
        };
        serde_json::to_writer_pretty(&mut text, c)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        self.write_atomic(&self.config_path(), &text.bytes)
    }

    /// Load, change and save the config.
    pub fn update_config<R>(&self, f: impl FnOnce(&mut Config) -> R) -> io::Result<(Config, R)> {
        let _transaction = self.transaction()?;
        let mut c = self.config();
        let r = f(&mut c);
        self.save_config_unlocked(&c)?;
        Ok((c, r))
    }

    fn transaction(&self) -> io::Result<Transaction> {
        let local = TRANSACTIONS.lock().unwrap_or_else(|e| e.into_inner());
        crate::util::create_private_dir(&self.dir)?;
        #[cfg(unix)]
        let file = {
            use std::os::fd::{AsRawFd, FromRawFd};
            use std::os::unix::fs::MetadataExt;
            let dir = self.open_dir()?;
            // SAFETY: the directory is held open and the fixed filename is valid. Never
            // follow a pre-existing link, or inherit the lock descriptor into children.
            let fd = unsafe {
                libc::openat(
                    dir.as_raw_fd(),
                    c"state.lock".as_ptr(),
                    libc::O_RDWR
                        | libc::O_CREAT
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC
                        | libc::O_NONBLOCK,
                    0o600,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: openat returned a new owned descriptor.
            let file = unsafe { fs::File::from_raw_fd(fd) };
            crate::util::check_owned_file(&file)?;
            if file.metadata()?.mode() & 0o022 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "state lock must not be writable by other accounts",
                ));
            }
            file
        };
        #[cfg(not(unix))]
        let file = {
            self.check_dir()?;
            let mut options = fs::OpenOptions::new();
            options.read(true).write(true).create(true).truncate(false);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.custom_flags(
                    windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
                );
            }
            let file = options.open(self.dir.join("state.lock"))?;
            #[cfg(windows)]
            crate::util::check_windows_private_file(&file)?;
            file
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(fs::TryLockError::WouldBlock) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "another holdmap process is updating state",
                    ));
                }
                Err(fs::TryLockError::Error(error)) => return Err(error),
            }
        }
        Ok(Transaction {
            _local: local,
            _file: file,
        })
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        crate::util::create_private_dir(&self.dir)?;
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            let dir = self.open_dir()?;
            let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid state filename")
            })?;
            let dest = std::ffi::CString::new(name).map_err(io::Error::other)?;
            let (tmp, mut file) = loop {
                let tmp = std::ffi::CString::new(format!(
                    ".{name}.{}.{}.tmp",
                    std::process::id(),
                    next_temp_id()
                ))
                .map_err(io::Error::other)?;
                // SAFETY: the directory handle and NUL-terminated name are valid. Exclusive
                // creation prevents a pre-existing link from being opened or truncated.
                let fd = unsafe {
                    libc::openat(
                        dir.as_raw_fd(),
                        tmp.as_ptr(),
                        libc::O_WRONLY
                            | libc::O_CREAT
                            | libc::O_EXCL
                            | libc::O_NOFOLLOW
                            | libc::O_CLOEXEC,
                        0o600,
                    )
                };
                if fd >= 0 {
                    // SAFETY: openat returned a new owned descriptor.
                    break (tmp, unsafe { fs::File::from_raw_fd(fd) });
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::AlreadyExists {
                    return Err(error);
                }
            };
            let result = (|| {
                file.write_all(bytes)?;
                file.sync_all()?;
                // SAFETY: both names and the held directory descriptor are valid. Relative
                // operations remain attached to that directory if a parent path changes.
                if unsafe {
                    libc::renameat(
                        dir.as_raw_fd(),
                        tmp.as_ptr(),
                        dir.as_raw_fd(),
                        dest.as_ptr(),
                    )
                } != 0
                {
                    return Err(io::Error::last_os_error());
                }
                dir.sync_all()?;
                Ok(())
            })();
            if result.is_err() {
                // SAFETY: remove only the exclusive temporary file in the held directory.
                unsafe { libc::unlinkat(dir.as_raw_fd(), tmp.as_ptr(), 0) };
            }
            result
        }
        #[cfg(not(unix))]
        {
            // Windows ACLs are inherited from the account-controlled directory.
            let tmp = self.dir.join(format!(
                ".{}.{}.{}.tmp",
                path.file_name().unwrap_or_default().to_string_lossy(),
                std::process::id(),
                next_temp_id()
            ));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            let result = (|| {
                file.write_all(bytes)?;
                file.sync_all()?;
                drop(file);
                fs::rename(&tmp, path)
            })();
            if result.is_err() {
                let _ = fs::remove_file(&tmp);
            }
            result
        }
    }

    /// Append stopped ports to the history, trimming it to `history_limit`.
    pub fn record(&self, entries: &[HistoryEntry]) -> io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let _transaction = self.transaction()?;
        let limit = self
            .config()
            .history_limit
            .clamp(1, *Config::HISTORY_LIMIT.end());
        let mut all = self.history(usize::MAX);
        all.reverse(); // oldest first
        let incoming = &entries[entries.len().saturating_sub(limit)..];
        let skip = all
            .len()
            .saturating_add(incoming.len())
            .saturating_sub(limit);
        let mut lines = VecDeque::new();
        let mut total = 0;
        for e in all.iter().chain(incoming).skip(skip) {
            let mut line = BoundedBuffer {
                bytes: Vec::new(),
                limit: MAX_HISTORY_ENTRY_BYTES - 1,
            };
            serde_json::to_writer(&mut line, e).map_err(io::Error::other)?;
            line.bytes.push(b'\n');
            while total + line.bytes.len() > MAX_HISTORY_BYTES as usize {
                total -= lines.pop_front().map_or(0, |line: Vec<u8>| line.len());
            }
            total += line.bytes.len();
            lines.push_back(line.bytes);
        }
        let mut out = Vec::with_capacity(total);
        for line in lines {
            out.extend_from_slice(&line);
        }
        self.write_atomic(&self.history_path(), &out)
    }

    /// Most recent first.
    pub fn history(&self, limit: usize) -> Vec<HistoryEntry> {
        let limit = limit.min(*Config::HISTORY_LIMIT.end());
        if limit == 0 {
            return Vec::new();
        }
        let Ok(mut f) = self.read_file("history.jsonl") else {
            return Vec::new();
        };
        let start = f
            .metadata()
            .map_or(0, |m| m.len().saturating_sub(MAX_HISTORY_BYTES));
        if f.seek(SeekFrom::Start(start)).is_err() {
            return Vec::new();
        }
        let mut reader = io::BufReader::new(f.take(MAX_HISTORY_BYTES));
        if start > 0 && reader.skip_until(b'\n').is_err() {
            return Vec::new();
        }
        let mut entries = VecDeque::with_capacity(limit);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader
                .by_ref()
                .take(MAX_HISTORY_ENTRY_BYTES as u64 + 1)
                .read_until(b'\n', &mut line)
            {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if line.len() > MAX_HISTORY_ENTRY_BYTES {
                if line.last() != Some(&b'\n') && reader.skip_until(b'\n').is_err() {
                    break;
                }
                continue;
            }
            if let Ok(entry) = serde_json::from_slice(&line) {
                if entries.len() == limit {
                    entries.pop_front();
                }
                entries.push_back(entry);
            }
        }
        entries.into_iter().rev().collect()
    }

    #[cfg(unix)]
    fn open_dir(&self) -> io::Result<fs::File> {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let dir = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&self.dir)?;
        let meta = dir.metadata()?;
        // SAFETY: geteuid has no preconditions.
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "state directory must be private and owned by the current account",
            ));
        }
        Ok(dir)
    }

    fn read_file(&self, name: &str) -> io::Result<fs::File> {
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            use std::os::unix::fs::MetadataExt;
            let dir = self.open_dir()?;
            let name = std::ffi::CString::new(name).map_err(io::Error::other)?;
            // SAFETY: directory and name are valid; returned descriptor is independently owned.
            let fd = unsafe {
                libc::openat(
                    dir.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: openat returned a new descriptor, now owned by File.
            let file = unsafe { fs::File::from_raw_fd(fd) };
            crate::util::check_owned_file(&file)?;
            if file.metadata()?.mode() & 0o022 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "state files must not be writable by other accounts",
                ));
            }
            Ok(file)
        }
        #[cfg(not(unix))]
        {
            self.check_dir()?;
            crate::util::open_private_read(&self.dir.join(name))
        }
    }

    #[cfg(not(unix))]
    fn check_dir(&self) -> io::Result<()> {
        let meta = fs::symlink_metadata(&self.dir)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "state directory must not be a link",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "state directory must not be a reparse point",
                ));
            }
        }
        Ok(())
    }

    /// The most recent history entry for `port`.
    pub fn last_for_port(&self, port: u16) -> Option<HistoryEntry> {
        self.history(usize::MAX)
            .into_iter()
            .find(|h| h.port == port)
    }

    /// The history entry stopped at `at_ms` on `port` (how a frontend names one to restart,
    /// so the command that runs always comes from this file, never from the caller).
    pub fn entry(&self, at_ms: u64, port: u16) -> Option<HistoryEntry> {
        self.entry_matching(at_ms, port, None, None).ok().flatten()
    }

    /// Resolve a caller's history identity without accepting executable data from it.
    /// Old callers may omit protocol/PID, but ambiguous recordings always fail closed.
    pub fn entry_matching(
        &self,
        at_ms: u64,
        port: u16,
        protocol: Option<crate::model::Protocol>,
        pid: Option<u32>,
    ) -> io::Result<Option<HistoryEntry>> {
        let mut matches = self
            .history(usize::MAX)
            .into_iter()
            .filter(|entry| entry.at_ms == at_ms && history_matches(entry, port, protocol, pid));
        let first = matches.next();
        if matches.next().is_some() {
            return Err(ambiguous_history());
        }
        Ok(first)
    }

    /// The newest recording for this owner/protocol; equal-time duplicates are ambiguous.
    pub fn last_matching(
        &self,
        port: u16,
        protocol: Option<crate::model::Protocol>,
        pid: Option<u32>,
    ) -> io::Result<Option<HistoryEntry>> {
        let mut matches = self
            .history(usize::MAX)
            .into_iter()
            .filter(|entry| history_matches(entry, port, protocol, pid));
        let first = matches.next();
        if first
            .as_ref()
            .is_some_and(|first| matches.any(|next| next.at_ms == first.at_ms))
        {
            return Err(ambiguous_history());
        }
        Ok(first)
    }

    /// Forget the stop history.
    pub fn clear_history(&self) -> io::Result<()> {
        if !self.dir.exists() {
            return Ok(());
        }
        let _transaction = self.transaction()?;
        let result = (|| {
            #[cfg(unix)]
            {
                use std::os::fd::AsRawFd;
                let dir = self.open_dir()?;
                // SAFETY: the checked directory is held open; unlink only its named entry.
                if unsafe { libc::unlinkat(dir.as_raw_fd(), c"history.jsonl".as_ptr(), 0) } != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            }
            #[cfg(not(unix))]
            {
                self.check_dir()?;
                fs::remove_file(self.history_path())
            }
        })();
        match result {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

fn history_matches(
    entry: &HistoryEntry,
    port: u16,
    protocol: Option<crate::model::Protocol>,
    pid: Option<u32>,
) -> bool {
    entry.port == port
        && protocol.is_none_or(|protocol| entry.protocol == protocol)
        && pid.is_none_or(|pid| entry.pid == pid)
}

fn ambiguous_history() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "ambiguous stopped service recording; start it using its normal launcher",
    )
}

fn next_temp_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Protocol;

    fn h(port: u16, at: u64) -> HistoryEntry {
        HistoryEntry {
            at_ms: at,
            port,
            protocol: Protocol::Tcp,
            label: format!("svc {port}"),
            command: vec!["npm".into(), "run".into(), "dev".into()],
            cwd: Some("/tmp".into()),
            project: None,
            framework: None,
            pid: 1,
            windows_restart: None,
            restartable: None,
            copyable: None,
        }
    }

    #[cfg(unix)]
    #[test]
    fn security_history_rejects_replaceable_files_and_shared_directories() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("state"));
        store.record(&[h(3000, 7)]).unwrap();
        let history = store.history_path();
        fs::set_permissions(&history, fs::Permissions::from_mode(0o660)).unwrap();
        assert!(
            store.entry(7, 3000).is_none(),
            "other-account writable restart commands must not load"
        );
        fs::set_permissions(&history, fs::Permissions::from_mode(0o600)).unwrap();
        let target = dir.path().join("target");
        fs::rename(&history, &target).unwrap();
        symlink(&target, &history).unwrap();
        assert!(store.history(10).is_empty());
        fs::remove_file(&history).unwrap();
        fs::hard_link(&target, &history).unwrap();
        assert!(store.history(10).is_empty());
        fs::remove_file(&target).unwrap();
        assert!(store.entry(7, 3000).is_some());
        fs::set_permissions(store.dir(), fs::Permissions::from_mode(0o770)).unwrap();
        assert!(store.history(10).is_empty());
        store.save_config(&Config::default()).unwrap();
        assert_eq!(
            fs::metadata(store.dir()).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[cfg(unix)]
    #[test]
    fn security_atomic_save_does_not_follow_predictable_temp_links() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("state"));
        crate::util::create_private_dir(store.dir()).unwrap();
        let target = dir.path().join("target");
        fs::write(&target, "fixture must survive").unwrap();
        symlink(&target, store.dir().join("config.tmp")).unwrap();
        let mut config = Config::default();
        config.set_pin(3000, None);
        store.save_config(&config).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "fixture must survive");
        assert!(store.config().is_pinned(3000));
    }

    #[cfg(unix)]
    #[test]
    fn security_clear_history_refuses_a_redirected_state_directory() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("unrelated");
        crate::util::create_private_dir(&target).unwrap();
        fs::write(target.join("history.jsonl"), "fixture must survive").unwrap();
        let redirect = dir.path().join("state");
        symlink(&target, &redirect).unwrap();
        assert!(Store::new(redirect).clear_history().is_err());
        assert_eq!(
            fs::read_to_string(target.join("history.jsonl")).unwrap(),
            "fixture must survive"
        );
        Store::new(dir.path().join("missing"))
            .clear_history()
            .unwrap();
    }

    #[test]
    fn config_round_trip_and_pins() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Store::new(tmp.path().join("hm"));
        assert_eq!(s.config(), Config::default());
        let (c, pinned) = s
            .update_config(|c| c.toggle_pin(3000, Some("web".into())))
            .unwrap();
        assert!(pinned && c.is_pinned(3000));
        s.update_config(|c| c.toggle_pin(1234, None)).unwrap();
        assert_eq!(
            s.config().pins.iter().map(|p| p.port).collect::<Vec<_>>(),
            [1234, 3000]
        );
        let (_, pinned) = s.update_config(|c| c.toggle_pin(3000, None)).unwrap();
        assert!(!pinned);
        assert!(!s.config().is_pinned(3000));
        // Corrupt config falls back to defaults instead of failing.
        fs::write(tmp.path().join("hm/config.json"), "{nope").unwrap();
        assert_eq!(s.config(), Config::default());
    }

    #[test]
    fn oversized_config_saves_and_updates_preserve_existing_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let mut original = Config {
            notify: false,
            ..Default::default()
        };
        original.set_pin(3000, Some("web".into()));
        store.save_config(&original).unwrap();
        let prior_bytes = fs::read(store.config_path()).unwrap();
        let mut oversized = original.clone();
        oversized.set_pin(4000, Some("x".repeat(MAX_CONFIG_BYTES)));
        assert_eq!(
            store.save_config(&oversized).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(store.config_path()).unwrap(), prior_bytes);
        assert_eq!(store.config(), original);
        assert_eq!(
            store
                .update_config(|config| {
                    config.toggle_pin(4000, Some("x".repeat(MAX_CONFIG_BYTES)));
                })
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(store.config_path()).unwrap(), prior_bytes);
        assert_eq!(store.config(), original);
    }

    #[test]
    fn config_size_cap_accepts_exact_boundary_and_rejects_oversized_valid_json() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let mut config = Config {
            notify: false,
            ..Default::default()
        };
        config.set_pin(3000, Some("x".into()));
        let overhead = serde_json::to_vec_pretty(&config).unwrap().len() - 1;
        config.pins[0].label = Some("x".repeat(MAX_CONFIG_BYTES - overhead));
        store.save_config(&config).unwrap();
        assert_eq!(
            fs::metadata(store.config_path()).unwrap().len(),
            MAX_CONFIG_BYTES as u64
        );
        assert_eq!(store.config(), config);
        // This remains valid JSON, but a reader must check the byte beyond the cap rather
        // than silently accepting a complete JSON prefix from an oversized file.
        fs::OpenOptions::new()
            .append(true)
            .open(store.config_path())
            .unwrap()
            .write_all(b" ")
            .unwrap();
        assert_eq!(store.config(), Config::default());
    }

    #[test]
    fn history_is_private_and_keeps_the_real_command() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Store::new(tmp.path().join("hm"));
        let mut e = h(3000, 7);
        e.command = vec!["node".into(), "server.js".into(), "--token=abc".into()];
        s.record(&[e]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &str| {
                fs::metadata(tmp.path().join(p))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777
            };
            assert_eq!(mode("hm/history.jsonl"), 0o600);
            assert_eq!(mode("hm"), 0o700);
        }
        let got = s.entry(7, 3000).unwrap();
        assert_eq!(
            got.command[2], "--token=abc",
            "restart needs the real command"
        );
        assert_eq!(got.redacted().command[2], "--token=••••");
        assert_eq!(got.command_line(), "node server.js --token=••••");
        assert!(s.entry(7, 3001).is_none());
    }

    #[test]
    fn history_is_recent_first_and_trimmed() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Store::new(tmp.path());
        s.update_config(|c| c.history_limit = 3).unwrap();
        s.record(&[h(1, 1), h(2, 2)]).unwrap();
        s.record(&[h(3, 3), h(1, 4)]).unwrap();
        let all = s.history(10);
        assert_eq!(all.iter().map(|e| e.at_ms).collect::<Vec<_>>(), [4, 3, 2]);
        assert_eq!(s.last_for_port(1).unwrap().at_ms, 4);
        assert!(s.last_for_port(9).is_none());
        s.clear_history().unwrap();
        assert!(s.history(10).is_empty());
        s.clear_history().unwrap();
    }

    #[test]
    fn history_lookup_binds_protocol_and_owner_and_rejects_ambiguous_records() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let tcp = h(3000, 100);
        let udp = HistoryEntry {
            protocol: Protocol::Udp,
            pid: 2,
            command: vec!["udp-owner".into()],
            ..tcp.clone()
        };
        store.record(&[tcp.clone(), udp.clone()]).unwrap();
        assert!(store.entry(100, 3000).is_none());
        assert_eq!(
            store
                .entry_matching(100, 3000, None, None)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            store
                .entry_matching(100, 3000, Some(Protocol::Tcp), Some(1))
                .unwrap(),
            Some(tcp.clone())
        );
        assert_eq!(
            store
                .entry_matching(100, 3000, Some(Protocol::Udp), Some(2))
                .unwrap(),
            Some(udp)
        );
        assert!(store
            .entry_matching(100, 3000, Some(Protocol::Udp), Some(1))
            .unwrap()
            .is_none());
        assert!(store.last_matching(3000, None, None).is_err());
        let newer_udp = HistoryEntry {
            at_ms: 200,
            protocol: Protocol::Udp,
            pid: 3,
            ..tcp.clone()
        };
        store.record(std::slice::from_ref(&newer_udp)).unwrap();
        assert_eq!(
            store
                .last_matching(3000, Some(Protocol::Tcp), None)
                .unwrap(),
            Some(tcp.clone())
        );
        assert_eq!(
            store.last_matching(3000, None, None).unwrap(),
            Some(newer_udp)
        );
        // Even the complete identity cannot disambiguate duplicate persisted records.
        // File order is authoritative even when the clock moves backwards; an intervening
        // timestamp must not hide another record carrying the same identity.
        store.record(&[h(3000, 90), tcp]).unwrap();
        assert!(store
            .entry_matching(100, 3000, Some(Protocol::Tcp), Some(1))
            .is_err());
        assert!(store
            .last_matching(3000, Some(Protocol::Tcp), Some(1))
            .is_err());
    }

    #[test]
    fn newest_matching_history_keeps_nonrestartable_record_without_older_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let older = h(3000, 100);
        let latest = HistoryEntry {
            at_ms: 200,
            command: vec![],
            ..older.clone()
        };
        store.record(&[older, latest.clone()]).unwrap();
        assert_eq!(
            store
                .last_matching(3000, Some(Protocol::Tcp), None)
                .unwrap(),
            Some(latest.clone())
        );
        assert!(!latest.restartable());
    }

    #[test]
    fn oversized_history_lines_are_skipped_and_recent_valid_entries_survive() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        store.record(&[h(3000, 1)]).unwrap();
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(store.history_path())
            .unwrap();
        file.write_all(&vec![b'x'; MAX_HISTORY_ENTRY_BYTES + 1])
            .unwrap();
        file.write_all(b"\n").unwrap();
        serde_json::to_writer(&mut file, &h(3001, 2)).unwrap();
        file.write_all(b"\n").unwrap();
        assert_eq!(
            store
                .history(10)
                .iter()
                .map(|e| e.at_ms)
                .collect::<Vec<_>>(),
            [2, 1]
        );
        assert!(store.history(0).is_empty());
        let too_large = HistoryEntry {
            command: vec!["x".repeat(MAX_HISTORY_ENTRY_BYTES)],
            ..h(3002, 3)
        };
        assert!(store.record(&[too_large]).is_err());
        assert_eq!(
            store
                .history(10)
                .iter()
                .map(|e| e.at_ms)
                .collect::<Vec<_>>(),
            [2, 1]
        );
    }

    #[test]
    fn oversized_history_files_keep_the_newest_complete_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        store.record(&[h(3000, 1)]).unwrap();
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(store.history_path())
            .unwrap();
        file.seek(SeekFrom::End(0)).unwrap();
        // A sparse oversized line avoids allocating the entire file in the test.
        file.set_len(MAX_HISTORY_BYTES + 100).unwrap();
        file.seek(SeekFrom::End(0)).unwrap();
        file.write_all(b"\n").unwrap();
        serde_json::to_writer(&mut file, &h(3001, 2)).unwrap();
        file.write_all(b"\n").unwrap();
        assert_eq!(
            store
                .history(10)
                .iter()
                .map(|e| e.at_ms)
                .collect::<Vec<_>>(),
            [2]
        );
    }

    #[test]
    fn history_limit_cannot_disable_the_retention_bound() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        store
            .update_config(|c| c.history_limit = usize::MAX)
            .unwrap();
        let entries: Vec<_> = (0..=*Config::HISTORY_LIMIT.end())
            .map(|i| h(3000, i as u64))
            .collect();
        store.record(&entries).unwrap();
        let history = store.history(usize::MAX);
        assert_eq!(history.len(), *Config::HISTORY_LIMIT.end());
        assert_eq!(history.first().unwrap().at_ms, 5000);
        assert_eq!(history.last().unwrap().at_ms, 1);
    }

    #[test]
    fn concurrent_store_instances_preserve_config_and_history_updates() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..24)
                .map(|i| {
                    let store = store.clone();
                    scope.spawn(move || {
                        store.update_config(|c| c.set_pin(3000 + i, None)).unwrap();
                        store.record(&[h(3000 + i, u64::from(i))]).unwrap();
                    })
                })
                .collect();
            for worker in workers {
                worker.join().unwrap();
            }
        });
        assert_eq!(store.config().pins.len(), 24);
        assert_eq!(store.history(usize::MAX).len(), 24);
    }

    #[cfg(unix)]
    #[test]
    fn state_transactions_honor_an_existing_advisory_lock() {
        use std::os::fd::AsRawFd;
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::new(tmp.path());
        let file = crate::util::create_private(&tmp.path().join("state.lock")).unwrap();
        // SAFETY: file owns the descriptor for the entire test.
        assert_eq!(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) }, 0);
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            sender
                .send(store.update_config(|c| c.set_pin(3000, None)))
                .unwrap();
        });
        assert!(receiver
            .recv_timeout(std::time::Duration::from_millis(40))
            .is_err());
        // SAFETY: release only the lock this test took on the same live descriptor.
        assert_eq!(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) }, 0);
        assert!(receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap()
            .is_ok());
        worker.join().unwrap();
    }

    #[test]
    fn preferences_are_clamped_and_old_configs_load() {
        let old: Config = serde_json::from_str(r#"{"pins":[],"notify":false}"#).unwrap();
        assert_eq!(
            old.scan_interval_secs, 4,
            "missing fields fall back to defaults"
        );
        assert_eq!(old.hotkey, "alt-p");
        assert!(!old.notify);
        let mut c = Config {
            scan_interval_secs: 0,
            ..Config::default()
        };
        assert_eq!(c.scan_interval().as_secs(), 1);
        assert_eq!(c.background_interval().as_secs(), 10);
        c.scan_interval_secs = 600;
        assert_eq!(c.scan_interval().as_secs(), 60);
        assert_eq!(c.background_interval().as_secs(), 150);
    }

    #[test]
    fn set_pin_upserts_and_unpin_removes() {
        let mut c = Config::default();
        c.set_pin(5173, Some("  docs ".into()));
        c.set_pin(3000, None);
        assert_eq!(
            c.pins.iter().map(|p| p.port).collect::<Vec<_>>(),
            [3000, 5173]
        );
        assert_eq!(c.pins[1].label.as_deref(), Some("docs"));
        c.set_pin(5173, Some("   ".into()));
        assert_eq!(c.pins.len(), 2, "updating doesn't duplicate");
        assert_eq!(c.pins[1].label, None, "blank label clears it");
        assert!(c.unpin(3000));
        assert!(!c.unpin(3000));
    }

    #[test]
    fn recent_hosts_are_deduplicated_and_capped() {
        let mut c = Config::default();
        for h in ["a", "b", "c", "a", "d", "e", "f", "g"] {
            c.remember_host(h);
        }
        assert_eq!(c.recent_hosts, ["g", "f", "e", "d", "a", "c"]);
    }
}

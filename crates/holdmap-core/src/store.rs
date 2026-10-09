//! Persistent user state: config (pins, notification settings) and the stopped-port history.
//!
//! Everything lives in one directory (see [`Store::default_dir`]) as small JSON / JSON-lines
//! files written atomically. Frontends depend on [`Store`], tests point it at a temp dir.

use crate::history::HistoryEntry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

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
        fs::read_to_string(self.config_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Write `config.json` atomically.
    pub fn save_config(&self, c: &Config) -> io::Result<()> {
        let text = serde_json::to_string_pretty(c).map_err(io::Error::other)?;
        self.write_atomic(&self.config_path(), text.as_bytes())
    }

    /// Load, change and save the config.
    pub fn update_config<R>(&self, f: impl FnOnce(&mut Config) -> R) -> io::Result<(Config, R)> {
        let mut c = self.config();
        let r = f(&mut c);
        self.save_config(&c)?;
        Ok((c, r))
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        crate::util::create_private_dir(&self.dir)?;
        let tmp = path.with_extension("tmp");
        {
            // Owner-only: the history keeps the real command lines of stopped services.
            let mut f = crate::util::create_private(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        fs::rename(tmp, path)
    }

    /// Append stopped ports to the history, trimming it to `history_limit`.
    pub fn record(&self, entries: &[HistoryEntry]) -> io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut all = self.history(usize::MAX);
        all.reverse(); // oldest first
        all.extend(entries.iter().cloned());
        let limit = self.config().history_limit.max(1);
        let skip = all.len().saturating_sub(limit);
        let mut out = Vec::new();
        for e in &all[skip..] {
            serde_json::to_writer(&mut out, e).map_err(io::Error::other)?;
            out.push(b'\n');
        }
        self.write_atomic(&self.history_path(), &out)
    }

    /// Most recent first.
    pub fn history(&self, limit: usize) -> Vec<HistoryEntry> {
        let Ok(f) = fs::File::open(self.history_path()) else {
            return Vec::new();
        };
        let mut v: Vec<HistoryEntry> = io::BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| serde_json::from_str(&l).ok())
            .collect();
        v.reverse();
        v.truncate(limit);
        v
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
        self.history(usize::MAX)
            .into_iter()
            .find(|e| e.at_ms == at_ms && e.port == port)
    }

    /// Forget the stop history.
    pub fn clear_history(&self) -> io::Result<()> {
        match fs::remove_file(self.history_path()) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
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
        }
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

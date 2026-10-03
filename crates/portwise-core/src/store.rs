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
    pub port: u16,
    /// Optional note such as "shop web".
    #[serde(default)]
    pub label: Option<String>,
}

/// User configuration (`config.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub pins: Vec<Pin>,
    /// Desktop/watch notifications for new listeners, closed listeners and conflicts.
    pub notify: bool,
    /// Only notify about dev servers (and pinned ports).
    pub notify_dev_only: bool,
    /// How many stopped ports to remember.
    pub history_limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pins: Vec::new(),
            notify: true,
            notify_dev_only: true,
            history_limit: 200,
        }
    }
}

impl Config {
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
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// `$PORTWISE_HOME`, else the platform config dir (`~/.config/portwise`,
    /// `~/Library/Application Support/portwise`, `%APPDATA%\portwise`).
    pub fn default_dir() -> PathBuf {
        if let Some(d) = std::env::var_os("PORTWISE_HOME").filter(|d| !d.is_empty()) {
            return PathBuf::from(d);
        }
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        if cfg!(windows) {
            if let Some(a) = std::env::var_os("APPDATA") {
                return PathBuf::from(a).join("portwise");
            }
        }
        if cfg!(target_os = "macos") {
            return home.join("Library/Application Support/portwise");
        }
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("portwise")
    }

    pub fn open_default() -> Self {
        Self::new(Self::default_dir())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn config_path(&self) -> PathBuf {
        self.dir.join("config.json")
    }

    fn history_path(&self) -> PathBuf {
        self.dir.join("history.jsonl")
    }

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
        fs::create_dir_all(&self.dir)?;
        let tmp = path.with_extension("tmp");
        {
            let mut f = fs::File::create(&tmp)?;
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
        let s = Store::new(tmp.path().join("pw"));
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
        fs::write(tmp.path().join("pw/config.json"), "{nope").unwrap();
        assert_eq!(s.config(), Config::default());
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
}

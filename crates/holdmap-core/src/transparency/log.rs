//! A small on-disk series of agent headline samples.
//!
//! Each series is keyed by a stable agent identity: the product id plus a hash of the agent's
//! first folder. Restarts keep adding to the same series. Names, command lines and paths are
//! not stored (the folder is hashed). The file is owner-only.
//!
//! Every scan writes once: one locked load, all agents recorded, one save. The save goes to a
//! private temporary file that is renamed over the log, so a crash never leaves half a file.
//! A lock file keeps the CLI, TUI, MCP server and desktop app from losing each other's writes.
//! A file that does not parse is moved aside (`memory-samples.corrupt.json`) and reported,
//! never silently wiped.

use super::report::MemorySample;
use crate::store::Store;
use crate::util::{create_private_dir, open_private};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Samples kept per agent identity.
pub const MAX_PER_AGENT: usize = 120;
/// Agent identities kept; the least recently seen are dropped first.
pub const MAX_AGENTS: usize = 64;
const LOCK_WAIT: Duration = Duration::from_millis(500);
const VERSION: u32 = 2;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct LogFile {
    version: u32,
    /// Identity → samples, oldest first.
    series: BTreeMap<String, Vec<MemorySample>>,
}

/// What one batched write produced.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LogOutcome {
    /// Identity → retained samples, oldest first (including this scan's sample).
    pub series: HashMap<String, Vec<MemorySample>>,
    /// A plain sentence when the log could not be used normally (corrupt, busy, unwritable).
    pub note: Option<String>,
}

/// Retains headline samples so a later report can show a direction.
pub trait MemoryLog: Send + Sync + std::fmt::Debug {
    /// Record one sample per identity taken at `at_ms`, and return every identity's series.
    fn record_batch(&self, at_ms: u64, entries: &[(String, u64)]) -> LogOutcome;
}

/// No retained samples.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoMemoryLog;

impl MemoryLog for NoMemoryLog {
    fn record_batch(&self, _: u64, _: &[(String, u64)]) -> LogOutcome {
        LogOutcome::default()
    }
}

/// `memory-samples.json` under a private directory.
#[derive(Debug, Clone)]
pub struct FileMemoryLog {
    path: PathBuf,
}

enum Loaded {
    Fresh(LogFile),
    Corrupt,
}

impl FileMemoryLog {
    /// The log in holdmap's config directory. `None` when that directory cannot be created.
    pub fn open_default() -> Option<Self> {
        let dir = Store::default_dir();
        create_private_dir(&dir).ok()?;
        Some(Self {
            path: dir.join("memory-samples.json"),
        })
    }

    /// A log at `path`'s file. The parent directory must already exist.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn sibling(&self, suffix: &str) -> PathBuf {
        let name = self
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("memory-samples");
        self.path.with_file_name(format!("{name}{suffix}"))
    }

    fn load(&self) -> Loaded {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Loaded::Fresh(LogFile::default());
        };
        match serde_json::from_str::<LogFile>(&text) {
            Ok(file) if file.version == VERSION => Loaded::Fresh(file),
            // Version 1 kept per-process samples; they cannot be keyed by identity. Starting
            // over is expected, not corruption.
            Ok(_) => Loaded::Fresh(LogFile::default()),
            Err(_)
                if serde_json::from_str::<serde_json::Value>(&text)
                    .is_ok_and(|value| value.get("samples").is_some()) =>
            {
                Loaded::Fresh(LogFile::default())
            }
            Err(_) => Loaded::Corrupt,
        }
    }

    fn lock(&self) -> Option<std::fs::File> {
        let file = open_private(&self.sibling(".lock")).ok()?;
        let deadline = Instant::now() + LOCK_WAIT;
        loop {
            match file.try_lock() {
                Ok(()) => return Some(file),
                Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return None,
            }
        }
    }

    fn save(&self, file: &LogFile) -> bool {
        let Ok(body) = serde_json::to_vec(file) else {
            return false;
        };
        write_atomic(&self.path, &body).is_ok()
    }
}

/// Write `bytes` to a new private temporary file beside `path`, then rename it over `path`.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("memory-samples.json");
    let tmp = path.with_file_name(format!(".{name}.{}.{id}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut out = options.open(&tmp)?;
    let result = (|| {
        out.write_all(bytes)?;
        out.sync_all()?;
        drop(out);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

impl MemoryLog for FileMemoryLog {
    fn record_batch(&self, at_ms: u64, entries: &[(String, u64)]) -> LogOutcome {
        let mut outcome = LogOutcome::default();
        let Some(_lock) = self.lock() else {
            outcome.note = Some(
                "Another holdmap window was saving memory samples, so this scan's sample was not kept.".into(),
            );
            return outcome;
        };
        let mut file = match self.load() {
            Loaded::Fresh(file) => file,
            Loaded::Corrupt => {
                let kept = self.sibling(".corrupt.json");
                let moved = std::fs::rename(&self.path, &kept).is_ok();
                outcome.note = Some(if moved {
                    "The saved memory trend file could not be read. It was kept as memory-samples.corrupt.json and a new one was started.".into()
                } else {
                    "The saved memory trend file could not be read or moved aside, so trends start again from this scan.".into()
                });
                LogFile::default()
            }
        };
        file.version = VERSION;
        for (key, bytes) in entries {
            let series = file.series.entry(key.clone()).or_default();
            series.retain(|sample| sample.at_ms != at_ms);
            series.push(MemorySample {
                at_ms,
                bytes: *bytes,
            });
            series.sort_by_key(|sample| sample.at_ms);
            let excess = series.len().saturating_sub(MAX_PER_AGENT);
            series.drain(..excess);
        }
        if file.series.len() > MAX_AGENTS {
            let mut newest: Vec<(String, u64)> = file
                .series
                .iter()
                .map(|(key, samples)| (key.clone(), samples.last().map_or(0, |s| s.at_ms)))
                .collect();
            newest.sort_by_key(|(_, at)| *at);
            for (key, _) in newest.iter().take(file.series.len() - MAX_AGENTS) {
                file.series.remove(key);
            }
        }
        if !self.save(&file) && outcome.note.is_none() {
            outcome.note =
                Some("Memory samples could not be saved; trends show this scan only.".into());
        }
        for (key, _) in entries {
            if let Some(series) = file.series.get(key) {
                outcome.series.insert(key.clone(), series.clone());
            }
        }
        outcome
    }
}

/// The stable identity a trend series is kept under: product id plus a hash of the first
/// folder, so two projects of the same product do not share a series. No path is stored.
pub fn trend_key(agent: &crate::agents::Agent) -> String {
    match agent.folders.first() {
        Some(folder) => format!(
            "{}#{:016x}",
            agent.product,
            fnv1a(folder.path.to_string_lossy().as_bytes())
        ),
        None => agent.product.clone(),
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Record every agent's headline in one write and replace its trend with the retained series.
pub fn persist_trends(report: &mut crate::agents::AgentsReport, log: &dyn MemoryLog) {
    let keys: Vec<(usize, String)> = report
        .memory
        .iter()
        .enumerate()
        .map(|(index, memory)| {
            let key = report
                .agents
                .iter()
                .find(|agent| agent.id == memory.agent_id)
                .map(trend_key)
                .unwrap_or_else(|| memory.agent_id.clone());
            (index, key)
        })
        .collect();
    let entries: Vec<(String, u64)> = keys
        .iter()
        .map(|(index, key)| (key.clone(), report.memory[*index].headline_bytes))
        .collect();
    if entries.is_empty() {
        return;
    }
    let taken = report.taken_at_ms;
    let outcome = log.record_batch(taken, &entries);
    for (index, key) in keys {
        let memory = &mut report.memory[index];
        let mut samples = outcome.series.get(&key).cloned().unwrap_or_default();
        if !samples.iter().any(|sample| sample.at_ms == taken) {
            samples.push(MemorySample {
                at_ms: taken,
                bytes: memory.headline_bytes,
            });
        }
        samples.sort_by_key(|sample| sample.at_ms);
        super::report::apply_trend(memory, samples);
        if let Some(note) = &outcome.note {
            memory.limits.push(note.clone());
        }
    }
}

/// Record into the default log when it can be opened. Reports stay valid when it cannot.
pub fn persist_trends_default(report: &mut crate::agents::AgentsReport) {
    if let Some(log) = FileMemoryLog::open_default() {
        persist_trends(report, &log);
    }
}

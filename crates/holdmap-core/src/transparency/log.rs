//! A small on-disk series of agent headline samples.
//!
//! Each record is a process id, start time, timestamp and byte count. Names, command lines
//! and paths are not stored. The file is owner-only. A corrupt file is replaced on the next
//! successful write rather than parsed into numbers.

use super::report::MemorySample;
use crate::store::Store;
use crate::util::{create_private, create_private_dir};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;

const MAX_PER_PROCESS: usize = 8;
const MAX_PROCESSES: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSample {
    pid: u32,
    started_at: u64,
    at_ms: u64,
    bytes: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct LogFile {
    samples: Vec<StoredSample>,
}

/// Retains headline samples so a later report can show a direction.
pub trait MemoryLog: Send + Sync + std::fmt::Debug {
    /// Samples for this process incarnation, oldest first.
    fn recent(&self, pid: u32, started_at: u64) -> Vec<MemorySample>;
    /// Remember one headline sample. Failures are ignored by the caller; the report still
    /// shows the current sample.
    fn record(&self, pid: u32, started_at: u64, at_ms: u64, bytes: u64) -> bool;
}

/// No retained samples.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoMemoryLog;

impl MemoryLog for NoMemoryLog {
    fn recent(&self, _: u32, _: u64) -> Vec<MemorySample> {
        Vec::new()
    }

    fn record(&self, _: u32, _: u64, _: u64, _: u64) -> bool {
        false
    }
}

/// `memory-samples.json` under a private directory.
#[derive(Debug, Clone)]
pub struct FileMemoryLog {
    path: PathBuf,
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

    fn load(&self) -> LogFile {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return LogFile::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    fn save(&self, file: &LogFile) -> bool {
        let Ok(body) = serde_json::to_vec(file) else {
            return false;
        };
        let Ok(mut out) = create_private(&self.path) else {
            return false;
        };
        std::io::Write::write_all(&mut out, &body).is_ok()
    }
}

impl MemoryLog for FileMemoryLog {
    fn recent(&self, pid: u32, started_at: u64) -> Vec<MemorySample> {
        self.load()
            .samples
            .into_iter()
            .filter(|sample| sample.pid == pid && sample.started_at == started_at)
            .map(|sample| MemorySample {
                at_ms: sample.at_ms,
                bytes: sample.bytes,
            })
            .collect()
    }

    fn record(&self, pid: u32, started_at: u64, at_ms: u64, bytes: u64) -> bool {
        let mut file = self.load();
        file.samples.retain(|sample| {
            !(sample.pid == pid && sample.started_at == started_at && sample.at_ms == at_ms)
        });
        file.samples.push(StoredSample {
            pid,
            started_at,
            at_ms,
            bytes,
        });
        let mut grouped: Vec<(u32, u64, VecDeque<StoredSample>)> = Vec::new();
        for sample in file.samples.drain(..) {
            if let Some(group) = grouped
                .iter_mut()
                .find(|group| group.0 == sample.pid && group.1 == sample.started_at)
            {
                group.2.push_back(sample);
            } else {
                grouped.push((sample.pid, sample.started_at, VecDeque::from([sample])));
            }
        }
        grouped
            .sort_by_key(|(_, _, samples)| samples.back().map(|sample| sample.at_ms).unwrap_or(0));
        if grouped.len() > MAX_PROCESSES {
            grouped.drain(0..grouped.len() - MAX_PROCESSES);
        }
        for (_, _, samples) in &mut grouped {
            while samples.len() > MAX_PER_PROCESS {
                samples.pop_front();
            }
            file.samples.extend(samples.drain(..));
        }
        self.save(&file)
    }
}

/// Record each agent's headline and replace its trend with the retained series.
pub fn persist_trends(report: &mut crate::agents::AgentsReport, log: &dyn MemoryLog) {
    for memory in &mut report.memory {
        log.record(
            memory.pid,
            memory.started_at,
            report.taken_at_ms,
            memory.headline_bytes,
        );
        let mut samples = log.recent(memory.pid, memory.started_at);
        if !samples
            .iter()
            .any(|sample| sample.at_ms == report.taken_at_ms)
        {
            samples.push(MemorySample {
                at_ms: report.taken_at_ms,
                bytes: memory.headline_bytes,
            });
        }
        samples.sort_by_key(|sample| sample.at_ms);
        super::report::apply_trend(memory, samples);
    }
}

/// Record into the default log when it can be opened. Reports stay valid when it cannot.
pub fn persist_trends_default(report: &mut crate::agents::AgentsReport) {
    if let Some(log) = FileMemoryLog::open_default() {
        persist_trends(report, &log);
    }
}

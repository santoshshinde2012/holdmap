//! Per-skill trust: a deterministic fingerprint of each skill folder, when holdmap first saw it,
//! whether it changed since the last scan or since you trusted it, and a names-and-sizes summary
//! of what changed.
//!
//! Privacy and safety rules:
//! - Only the skill's own folder is read. Symlinks inside it are never followed: a link counts
//!   as an entry (its target string is hashed, never stored), so retargeting it shows up as a
//!   change without reading what it points to.
//! - Reads are bounded ([`HashLimits`]); a folder over the limits is marked `partial`.
//! - Only hashes and short metadata (relative file names, sizes, times) are stored. File
//!   contents are never stored or shown.

use crate::finding::Subject;
use holdmap_core::agents::AgentsReport;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;

/// How much of a skill folder is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashLimits {
    /// Entries (files and links) per skill.
    pub max_files: usize,
    /// Bytes per file; a larger file is hashed by name and size only.
    pub max_file_bytes: u64,
    /// Bytes per skill.
    pub max_total_bytes: u64,
    /// Folder depth below the skill folder.
    pub max_depth: usize,
}

impl Default for HashLimits {
    fn default() -> Self {
        Self {
            max_files: 2_000,
            max_file_bytes: 16 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_depth: 16,
        }
    }
}

/// What a folder entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A symbolic link (never followed).
    Link,
}

/// One entry of a fingerprint. `path` is relative, with `/` separators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Relative path.
    pub path: String,
    /// Kind.
    pub kind: EntryKind,
    /// Size in bytes (links: 0).
    pub size: u64,
    /// SHA-256 of the content (links: of the link target text), hex.
    pub sha256: String,
}

/// A skill folder's fingerprint at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillFingerprint {
    /// SHA-256 over the sorted entries (`kind path size sha256` per line), hex.
    pub sha256: String,
    /// Entries, sorted by path.
    pub files: Vec<FileEntry>,
    /// Total bytes hashed.
    pub total_bytes: u64,
    /// Newest modification time among the entries (ms since the epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_ms: Option<u64>,
    /// True when a limit was reached; the hash then covers only what was read.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
}

/// Fingerprint the folder at `root`. The folder itself may be a link (skills are often linked
/// in); nothing below it is followed.
pub fn fingerprint(root: &Path, limits: HashLimits) -> io::Result<SkillFingerprint> {
    let root = fs::canonicalize(root)?;
    if !fs::metadata(&root)?.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a folder"));
    }
    let mut walk = Walk {
        root: &root,
        limits,
        files: Vec::new(),
        total: 0,
        modified: None,
        partial: false,
    };
    walk.dir(&root, 0)?;
    let Walk {
        mut files,
        total,
        modified,
        partial,
        ..
    } = walk;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut hasher = Sha256::new();
    for f in &files {
        let kind = match f.kind {
            EntryKind::File => "file",
            EntryKind::Link => "link",
        };
        hasher.update(format!("{kind} {} {} {}\n", f.path, f.size, f.sha256));
    }
    Ok(SkillFingerprint {
        sha256: hex::encode(hasher.finalize()),
        files,
        total_bytes: total,
        modified_ms: modified,
        partial,
    })
}

struct Walk<'a> {
    root: &'a Path,
    limits: HashLimits,
    files: Vec<FileEntry>,
    total: u64,
    modified: Option<u64>,
    partial: bool,
}

impl Walk<'_> {
    fn dir(&mut self, dir: &Path, depth: usize) -> io::Result<()> {
        if depth > self.limits.max_depth {
            self.partial = true;
            return Ok(());
        }
        let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if self.files.len() >= self.limits.max_files {
                self.partial = true;
                return Ok(());
            }
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            let rel = relative(self.root, &path);
            self.note_modified(&meta);
            if meta.file_type().is_symlink() {
                let target = fs::read_link(&path)?;
                self.files.push(FileEntry {
                    path: rel,
                    kind: EntryKind::Link,
                    size: 0,
                    sha256: hex::encode(Sha256::digest(target.to_string_lossy().as_bytes())),
                });
            } else if meta.is_dir() {
                self.dir(&path, depth + 1)?;
            } else if meta.is_file() {
                let size = meta.len();
                let within = size <= self.limits.max_file_bytes
                    && self.total.saturating_add(size) <= self.limits.max_total_bytes;
                let sha256 = if within {
                    self.total += size;
                    hash_file(&path)?
                } else {
                    self.partial = true;
                    String::new()
                };
                self.files.push(FileEntry {
                    path: rel,
                    kind: EntryKind::File,
                    size,
                    sha256,
                });
            }
        }
        Ok(())
    }

    fn note_modified(&mut self, meta: &fs::Metadata) {
        let ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);
        if let Some(ms) = ms {
            self.modified = Some(self.modified.map_or(ms, |m| m.max(ms)));
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Hash a regular file without following a link swapped in after the check.
fn hash_file(path: &Path) -> io::Result<String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// A file that differs between two fingerprints. Names and sizes only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    /// Relative path.
    pub path: String,
    /// Size before (absent when added).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_before: Option<u64>,
    /// Size after (absent when removed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_after: Option<u64>,
}

/// Most changed files listed per kind; the totals stay exact.
pub const MAX_CHANGES_LISTED: usize = 20;

/// What changed between two fingerprints, by name and size. Never contents.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSummary {
    /// Added (first [`MAX_CHANGES_LISTED`]).
    pub added: Vec<FileChange>,
    /// Removed.
    pub removed: Vec<FileChange>,
    /// Modified.
    pub modified: Vec<FileChange>,
    /// Exact counts.
    pub added_total: usize,
    /// Exact counts.
    pub removed_total: usize,
    /// Exact counts.
    pub modified_total: usize,
}

impl ChangeSummary {
    /// Compare `before` with `after`.
    pub fn between(before: &[FileEntry], after: &[FileEntry]) -> Self {
        let old: BTreeMap<&str, &FileEntry> = before.iter().map(|f| (f.path.as_str(), f)).collect();
        let new: BTreeMap<&str, &FileEntry> = after.iter().map(|f| (f.path.as_str(), f)).collect();
        let mut out = Self::default();
        for (path, a) in &new {
            match old.get(path) {
                None => push(
                    &mut out.added,
                    &mut out.added_total,
                    path,
                    None,
                    Some(a.size),
                ),
                Some(b) if b.sha256 != a.sha256 || b.size != a.size || b.kind != a.kind => push(
                    &mut out.modified,
                    &mut out.modified_total,
                    path,
                    Some(b.size),
                    Some(a.size),
                ),
                Some(_) => {}
            }
        }
        for (path, b) in &old {
            if !new.contains_key(path) {
                push(
                    &mut out.removed,
                    &mut out.removed_total,
                    path,
                    Some(b.size),
                    None,
                );
            }
        }
        out
    }

    /// Nothing changed.
    pub fn is_empty(&self) -> bool {
        self.added_total + self.removed_total + self.modified_total == 0
    }
}

fn push(
    list: &mut Vec<FileChange>,
    total: &mut usize,
    path: &str,
    before: Option<u64>,
    after: Option<u64>,
) {
    *total += 1;
    if list.len() < MAX_CHANGES_LISTED {
        list.push(FileChange {
            path: path.to_string(),
            size_before: before,
            size_after: after,
        });
    }
}

/// The version you trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedVersion {
    /// Its fingerprint hash.
    pub sha256: String,
    /// Its entries (names, sizes, hashes), to say what changed later.
    pub files: Vec<FileEntry>,
    /// When.
    pub at_ms: u64,
    /// Why (capped, secret-masked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// What holdmap remembers about one skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustRecord {
    /// The skill (id, name, agents, folder, scope).
    pub subject: Subject,
    /// First scan that saw it.
    pub first_seen_ms: u64,
    /// Latest scan.
    pub seen_ms: u64,
    /// Latest fingerprint (absent when the folder couldn't be read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<SkillFingerprint>,
    /// The fingerprint hash of the scan before the latest one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_sha256: Option<String>,
    /// Why the latest scan couldn't read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The version you trusted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted: Option<TrustedVersion>,
}

/// Where a skill stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustState {
    /// Changed since you trusted it.
    Changed,
    /// Seen, never trusted.
    New,
    /// Couldn't be read.
    Unreadable,
    /// Matches the version you trusted.
    Trusted,
}

impl TrustState {
    /// Needs your attention.
    pub fn alerts(self) -> bool {
        matches!(self, Self::Changed | Self::New)
    }
    /// Plain label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Changed => "Changed since trusted",
            Self::New => "Not trusted yet",
            Self::Unreadable => "Couldn't read",
            Self::Trusted => "Trusted",
        }
    }
}

/// One skill as shown everywhere (UI, CLI, MCP). File contents never appear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillTrustView {
    /// The skill.
    pub subject: Subject,
    /// State.
    pub state: TrustState,
    /// Current fingerprint hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Entries in the current fingerprint.
    pub file_count: usize,
    /// Bytes hashed.
    pub total_bytes: u64,
    /// Newest file time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_ms: Option<u64>,
    /// A limit was reached.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
    /// First seen.
    pub first_seen_ms: u64,
    /// Last scanned.
    pub seen_ms: u64,
    /// Different from the scan before.
    pub changed_since_last_scan: bool,
    /// When you trusted it, and why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_ms: Option<u64>,
    /// Your reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// What changed since you trusted it (Changed only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<ChangeSummary>,
    /// Why it couldn't be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SkillTrustView {
    /// The view of a stored record.
    pub fn of(record: &TrustRecord) -> Self {
        let current = record.current.as_ref();
        let state = match (current, &record.trusted) {
            (None, _) => TrustState::Unreadable,
            (Some(_), None) => TrustState::New,
            (Some(c), Some(t)) if c.sha256 == t.sha256 => TrustState::Trusted,
            (Some(_), Some(_)) => TrustState::Changed,
        };
        let changes = match (state, current, &record.trusted) {
            (TrustState::Changed, Some(c), Some(t)) => {
                Some(ChangeSummary::between(&t.files, &c.files))
            }
            _ => None,
        };
        Self {
            subject: record.subject.clone(),
            state,
            sha256: current.map(|c| c.sha256.clone()),
            file_count: current.map_or(0, |c| c.files.len()),
            total_bytes: current.map_or(0, |c| c.total_bytes),
            modified_ms: current.and_then(|c| c.modified_ms),
            partial: current.is_some_and(|c| c.partial),
            first_seen_ms: record.first_seen_ms,
            seen_ms: record.seen_ms,
            changed_since_last_scan: match (current, &record.previous_sha256) {
                (Some(c), Some(p)) => &c.sha256 != p,
                _ => false,
            },
            trusted_ms: record.trusted.as_ref().map(|t| t.at_ms),
            reason: record.trusted.as_ref().and_then(|t| t.reason.clone()),
            changes,
            error: record.error.clone(),
        }
    }
}

/// Every skill's trust, most urgent first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustReport {
    /// Skills.
    pub skills: Vec<SkillTrustView>,
    /// Changed since trusted.
    pub changed: usize,
    /// Never trusted.
    pub untrusted: usize,
    /// Latest scan (absent before the first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanned_ms: Option<u64>,
}

impl TrustReport {
    fn of(records: &BTreeMap<String, TrustRecord>) -> Self {
        let mut skills: Vec<SkillTrustView> = records.values().map(SkillTrustView::of).collect();
        skills.sort_by(|a, b| {
            a.state
                .cmp(&b.state)
                .then_with(|| a.subject.name.cmp(&b.subject.name))
        });
        Self {
            changed: skills
                .iter()
                .filter(|s| s.state == TrustState::Changed)
                .count(),
            untrusted: skills.iter().filter(|s| s.state == TrustState::New).count(),
            scanned_ms: records.values().map(|r| r.seen_ms).max(),
            skills,
        }
    }

    /// Skills that need your attention.
    pub fn alerts(&self) -> impl Iterator<Item = &SkillTrustView> {
        self.skills.iter().filter(|s| s.state.alerts())
    }
}

/// Where trust records live.
pub trait TrustStore: Send + Sync {
    /// All records, by skill id.
    fn trust_records(&self) -> BTreeMap<String, TrustRecord>;
    /// Replace them.
    fn set_trust_records(&self, records: &BTreeMap<String, TrustRecord>) -> io::Result<()>;
}

/// Why trusting failed.
#[derive(Debug, thiserror::Error)]
pub enum TrustError {
    /// Not a skill holdmap has scanned.
    #[error("no skill called {0:?}; run a trust scan first")]
    Unknown(String),
    /// The folder couldn't be read, so there is no version to trust.
    #[error("{0} couldn't be read, so there is no version to trust")]
    Unreadable(String),
    /// A save failed.
    #[error("could not save: {0}")]
    Io(#[from] io::Error),
}

/// Scans skills and records trust.
pub struct SkillTrust {
    store: Arc<dyn TrustStore>,
    now: fn() -> u64,
    limits: HashLimits,
}

/// Reasons are capped like review reasons.
pub const MAX_REASON: usize = crate::finding::MAX_NOTE;

impl SkillTrust {
    /// Trust backed by `store`.
    pub fn new(store: Arc<dyn TrustStore>) -> Self {
        Self {
            store,
            now: now_ms,
            limits: HashLimits::default(),
        }
    }

    /// Use `now` as the clock (tests).
    pub fn with_clock(mut self, now: fn() -> u64) -> Self {
        self.now = now;
        self
    }

    /// Use other limits (tests, benchmarks).
    pub fn with_limits(mut self, limits: HashLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Fingerprint every skill in the inventory and remember it. Skills no longer installed
    /// are dropped (with their trust), so a reinstalled skill asks again.
    pub fn scan(&self, report: &AgentsReport) -> Result<TrustReport, TrustError> {
        let now = (self.now)();
        let old = self.store.trust_records();
        let mut records = BTreeMap::new();
        for input in crate::input::skill_subjects(report).subjects {
            let Some(dir) = input.dir.as_deref() else {
                continue;
            };
            let id = input.subject.id.clone();
            let prior = old.get(&id);
            let (current, error) = match fingerprint(dir, self.limits) {
                Ok(f) => (Some(f), None),
                Err(e) => (
                    None,
                    Some(crate::finding::clean_text(&e.to_string(), MAX_REASON)),
                ),
            };
            records.insert(
                id,
                TrustRecord {
                    subject: input.subject,
                    first_seen_ms: prior.map_or(now, |p| p.first_seen_ms),
                    seen_ms: now,
                    previous_sha256: prior
                        .and_then(|p| p.current.as_ref())
                        .map(|c| c.sha256.clone()),
                    current,
                    error,
                    trusted: prior.and_then(|p| p.trusted.clone()),
                },
            );
        }
        self.store.set_trust_records(&records)?;
        Ok(TrustReport::of(&records))
    }

    /// The last scan, without reading any folder.
    pub fn report(&self) -> TrustReport {
        TrustReport::of(&self.store.trust_records())
    }

    /// Trust the version the last scan saw, with an optional reason.
    pub fn trust(&self, skill: &str, reason: Option<&str>) -> Result<SkillTrustView, TrustError> {
        let mut records = self.store.trust_records();
        let record = records
            .get_mut(skill)
            .ok_or_else(|| TrustError::Unknown(skill.to_string()))?;
        let current = record
            .current
            .clone()
            .ok_or_else(|| TrustError::Unreadable(record.subject.name.clone()))?;
        record.trusted = Some(TrustedVersion {
            sha256: current.sha256,
            files: current.files,
            at_ms: (self.now)(),
            reason: reason
                .map(|r| crate::finding::clean_text(r, MAX_REASON))
                .filter(|r| !r.is_empty()),
        });
        let view = SkillTrustView::of(record);
        self.store.set_trust_records(&records)?;
        Ok(view)
    }

    /// Forget that you trusted a skill.
    pub fn untrust(&self, skill: &str) -> Result<SkillTrustView, TrustError> {
        let mut records = self.store.trust_records();
        let record = records
            .get_mut(skill)
            .ok_or_else(|| TrustError::Unknown(skill.to_string()))?;
        record.trusted = None;
        let view = SkillTrustView::of(record);
        self.store.set_trust_records(&records)?;
        Ok(view)
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

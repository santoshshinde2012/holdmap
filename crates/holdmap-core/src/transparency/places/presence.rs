//! Bounded looks at session data, auto memory and nested rule folders.
//!
//! Session data keeps only totals: no file names, no entries, no candidates. Auto memory keeps
//! a line count of its index; the bytes are read through a no-follow handle into a fixed buffer
//! and discarded. Nested rule folders keep rule file names, never bodies.

use super::stat::{entry_kind, has_any_suffix, is_link, modified, strip_suffix};
use super::{CapabilityPlace, PlaceStatus, MAX_CANDIDATES};
use crate::agents::capabilities::{reader, safe_name};
use std::fs::{self, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Files visited before a session-data walk gives up on an exact total.
pub(crate) const MAX_SESSION_FILES: usize = 20_000;
const MAX_SESSION_DEPTH: usize = 6;
const SESSION_TIME: Duration = Duration::from_millis(150);
/// Lines Claude Code loads from `MEMORY.md` at the start of a session.
pub(crate) const MEMORY_LOAD_LINES: u64 = 200;
/// Bytes Claude Code loads from `MEMORY.md` at the start of a session.
pub(crate) const MEMORY_LOAD_BYTES: u64 = 25 * 1024;
const MAX_MEMORY_SCAN: u64 = 1024 * 1024;
const MAX_RULE_DEPTH: usize = 4;

/// Total size, file count and newest time of transcripts, history or logs. Never names.
pub(super) fn session_data(place: &mut CapabilityPlace, path: &Path, metadata: &Metadata) {
    place.modified_unix_ms = modified(metadata);
    if metadata.is_file() {
        place.file_count = Some(1);
        place.byte_size = Some(metadata.len());
        return;
    }
    if !metadata.is_dir() {
        place.status_reason = Some("The path is not a regular file or directory.".into());
        return;
    }
    let start = Instant::now();
    let mut stack: Vec<(PathBuf, usize)> = vec![(path.to_path_buf(), 0)];
    let (mut files, mut bytes, mut newest) = (0u64, 0u64, place.modified_unix_ms.unwrap_or(0));
    let mut visited = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        let Ok(read) = fs::read_dir(&dir) else {
            place.status_reason =
                Some("Some folders could not be listed, so totals are unknown.".into());
            return;
        };
        for entry in read {
            visited += 1;
            if visited > MAX_SESSION_FILES || start.elapsed() > SESSION_TIME {
                place.status_reason = Some(format!(
                    "More than {MAX_SESSION_FILES} entries or too slow to total; size and count are not reported."
                ));
                return;
            }
            let Ok(entry) = entry else { continue };
            // DirEntry metadata does not follow links.
            let Ok(meta) = entry.metadata() else { continue };
            if is_link(&meta) {
                continue;
            }
            if meta.is_dir() {
                if depth + 1 < MAX_SESSION_DEPTH {
                    stack.push((entry.path(), depth + 1));
                }
                continue;
            }
            if meta.is_file() {
                files += 1;
                bytes = bytes.saturating_add(meta.len());
                newest = newest.max(modified(&meta).unwrap_or(0));
            }
        }
    }
    place.file_count = Some(files);
    place.byte_size = Some(bytes);
    place.modified_unix_ms = (newest > 0).then_some(newest);
}

/// Claude auto memory: count and size of the folder, and the line count of `MEMORY.md`.
pub(super) fn auto_memory(place: &mut CapabilityPlace, path: &Path, metadata: &Metadata) {
    place.modified_unix_ms = modified(metadata);
    place.load_limit_lines = Some(MEMORY_LOAD_LINES);
    if !metadata.is_dir() {
        place.status_reason = Some("Auto memory is expected to be a folder.".into());
        return;
    }
    let Ok(read) = fs::read_dir(path) else {
        place.status = PlaceStatus::Unreadable;
        place.present = false;
        place.status_reason = Some("The folder could not be listed.".into());
        return;
    };
    let (mut files, mut bytes) = (0u64, 0u64);
    for entry in read.take(super::MAX_ENTRIES).flatten() {
        if let Ok(meta) = entry.metadata() {
            if entry_kind(&meta) == "file" {
                files += 1;
                bytes = bytes.saturating_add(meta.len());
            }
        }
    }
    place.file_count = Some(files);
    place.byte_size = Some(bytes);
    let index = path.join("MEMORY.md");
    let Ok(meta) = fs::symlink_metadata(&index) else {
        place.status_reason = Some("No MEMORY.md index yet.".into());
        return;
    };
    if is_link(&meta) || !meta.is_file() {
        place.status_reason =
            Some("MEMORY.md is not a regular file, so its lines were not counted.".into());
        return;
    }
    match count_lines(&index, meta.len()) {
        Some(lines) => {
            place.line_count = Some(lines);
            if meta.len() > MEMORY_LOAD_BYTES {
                place.status_reason =
                    Some("MEMORY.md is over 25 KB; only the first 25 KB load each session.".into());
            } else if lines > MEMORY_LOAD_LINES {
                place.status_reason = Some(format!(
                    "MEMORY.md has {lines} lines; only the first {MEMORY_LOAD_LINES} load each session."
                ));
            }
        }
        None => {
            place.status_reason =
                Some("MEMORY.md is too large or unreadable, so its lines were not counted.".into());
        }
    }
}

/// Newlines in a file, read through a no-follow handle into a fixed buffer and discarded.
pub(crate) fn count_lines(path: &Path, len: u64) -> Option<u64> {
    if len > MAX_MEMORY_SCAN {
        return None;
    }
    let mut file = reader::open(path).ok()?;
    let mut buffer = [0u8; 8192];
    let (mut lines, mut last, mut total) = (0u64, b'\n', 0u64);
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_MEMORY_SCAN {
            return None;
        }
        lines += buffer[..read].iter().filter(|byte| **byte == b'\n').count() as u64;
        last = buffer[read - 1];
    }
    buffer.fill(0);
    Some(if last == b'\n' { lines } else { lines + 1 })
}

/// A nested rules folder: names of loaded rule files (relative, without suffix), and a count
/// of files the product documents as ignored.
pub(super) fn deep(
    place: &mut CapabilityPlace,
    path: &Path,
    metadata: &Metadata,
    loads: &[&str],
    ignored: &[&str],
) {
    place.modified_unix_ms = modified(metadata);
    if metadata.is_file() {
        place.file_count = Some(1);
        place.byte_size = Some(metadata.len());
        return;
    }
    if !metadata.is_dir() {
        place.status_reason = Some("The path is not a regular file or directory.".into());
        return;
    }
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(path.to_path_buf(), String::new(), 0)];
    let (mut loaded, mut skipped, mut bytes, mut visited) = (0u64, 0u64, 0u64, 0usize);
    while let Some((dir, prefix, depth)) = stack.pop() {
        let Ok(read) = fs::read_dir(&dir) else {
            place.status_reason =
                Some("Some folders could not be listed, so totals are unknown.".into());
            return;
        };
        let mut children: Vec<_> = read.flatten().collect();
        children.sort_by_key(|entry| entry.file_name());
        for entry in children {
            visited += 1;
            if visited > super::MAX_ENTRIES {
                place.status_reason = Some(format!(
                    "More than {} entries; the count would be incomplete, so it is not reported.",
                    super::MAX_ENTRIES
                ));
                place.candidates.clear();
                return;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !safe_name(&name) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            match entry_kind(&meta) {
                "directory" if depth + 1 < MAX_RULE_DEPTH => {
                    stack.push((entry.path(), format!("{prefix}{name}/"), depth + 1));
                }
                "file" if has_any_suffix(&name, loads) => {
                    loaded += 1;
                    bytes = bytes.saturating_add(meta.len());
                    if place.candidates.len() < MAX_CANDIDATES {
                        place
                            .candidates
                            .push(format!("{prefix}{}", strip_suffix(&name, loads)));
                    }
                }
                "file" if has_any_suffix(&name, ignored) => skipped += 1,
                _ => {}
            }
        }
    }
    place.candidates.sort();
    place.file_count = Some(loaded);
    place.byte_size = Some(bytes);
    if !ignored.is_empty() {
        place.ignored_count = Some(skipped);
        if skipped > 0 {
            let noun = if skipped == 1 { "file is" } else { "files are" };
            place.status_reason = Some(format!(
                "{skipped} .{} {noun} ignored here; only .{} rule files load.",
                ignored.join("/."),
                loads.join("/.")
            ));
        }
    }
}

//! Stat one documented path without following links or reading bodies.

use super::{
    blank, presence, CapabilityPlace, PlaceEntry, PlaceStatus, MAX_CANDIDATES, MAX_ENTRIES,
};
use crate::agents::access::privacy_area;
use crate::transparency::locations::{Form, LocationSpec};
use std::fs::{self, Metadata};
use std::path::{Component, Path};
use std::time::UNIX_EPOCH;

/// Stat `path` for `spec` and fill what its form allows.
pub(super) fn stat_place(
    product: &str,
    spec: &LocationSpec,
    path: &Path,
    home: &Path,
    platform: &str,
) -> CapabilityPlace {
    let mut place = blank(product, spec, path.display().to_string());
    if let Some(reason) = refuse(path, home, platform) {
        place.status = PlaceStatus::Skipped;
        place.status_reason = Some(reason);
        return place;
    }
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => place,
        Err(error) => {
            place.status = PlaceStatus::Unreadable;
            place.status_reason = Some(format!(
                "Could not inspect the path ({})",
                io_reason(&error)
            ));
            place
        }
        Ok(metadata) if is_link(&metadata) => {
            place.present = true;
            place.status = PlaceStatus::Present;
            if spec.form != Form::SecretStore {
                place.symlink_target = fs::read_link(path)
                    .ok()
                    .map(|target| target.display().to_string());
            }
            place.status_reason = Some(
                "The path is a symbolic link. holdmap does not follow it, so size and children are unknown.".into(),
            );
            place
        }
        Ok(metadata) => {
            place.present = true;
            place.status = PlaceStatus::Present;
            match spec.form {
                Form::SecretStore => {
                    place.status_reason =
                        Some("Secrets store. Present; never opened, listed or sized.".into());
                }
                Form::SessionData => presence::session_data(&mut place, path, &metadata),
                Form::AutoMemory => presence::auto_memory(&mut place, path, &metadata),
                Form::Deep { loads, ignored } => {
                    presence::deep(&mut place, path, &metadata, loads, ignored)
                }
                form => fill_listing(&mut place, path, &metadata, form),
            }
            place
        }
    }
}

/// One-level listing for files, skill roots, named files and folders.
fn fill_listing(place: &mut CapabilityPlace, path: &Path, metadata: &Metadata, form: Form) {
    place.modified_unix_ms = modified(metadata);
    if metadata.is_file() {
        place.file_count = Some(1);
        place.byte_size = Some(metadata.len());
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            place.candidates.push(name.to_owned());
        }
        return;
    }
    if !metadata.is_dir() {
        place.status_reason = Some(
            "The path is not a regular file or directory, so its children were not listed.".into(),
        );
        return;
    }
    let Ok(read) = fs::read_dir(path) else {
        place.status = PlaceStatus::Unreadable;
        place.present = false;
        place.status_reason = Some("The directory could not be listed.".into());
        return;
    };
    let mut children: Vec<_> = read.take(MAX_ENTRIES + 1).collect();
    if children.len() > MAX_ENTRIES {
        place.status_reason = Some(format!(
            "The directory has more than {MAX_ENTRIES} entries. The count would be incomplete, so it is not reported."
        ));
        return;
    }
    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut sized = true;
    children.sort_by_key(|entry| {
        entry
            .as_ref()
            .ok()
            .map(|entry| entry.file_name())
            .unwrap_or_default()
    });
    for entry in children {
        let Ok(entry) = entry else {
            sized = false;
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !crate::agents::capabilities::safe_name(&name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            sized = false;
            place.entries.push(PlaceEntry {
                name,
                kind: "unreadable".into(),
                bytes: None,
                modified_unix_ms: None,
            });
            continue;
        };
        let kind = entry_kind(&meta);
        let file_bytes = (kind == "file").then_some(meta.len());
        if kind == "file" {
            files += 1;
            bytes = bytes.saturating_add(meta.len());
        } else if kind == "directory" {
            files += 1;
            // Skill folders keep their text one level down. Stat that file only; do not read it.
            if form == Form::SkillRoot {
                if let Some(len) = regular_file_len(&entry.path().join("SKILL.md")) {
                    bytes = bytes.saturating_add(len);
                }
            }
        }
        if place.entries.len() < MAX_CANDIDATES {
            place.entries.push(PlaceEntry {
                name: name.clone(),
                kind: kind.into(),
                bytes: file_bytes,
                modified_unix_ms: modified(&meta),
            });
        }
        if place.candidates.len() < MAX_CANDIDATES && is_candidate(&entry.path(), kind, form) {
            place.candidates.push(candidate_name(&name, form));
        }
    }
    if sized {
        place.file_count = Some(files);
        place.byte_size = Some(bytes);
    } else {
        place.status_reason = Some(
            "Some directory entries could not be inspected, so the count and byte size are unknown."
                .into(),
        );
    }
}

pub(super) fn entry_kind(meta: &Metadata) -> &'static str {
    if is_link(meta) {
        "symlink"
    } else if meta.is_dir() {
        "directory"
    } else if meta.is_file() {
        "file"
    } else {
        "other"
    }
}

fn is_candidate(path: &Path, kind: &str, form: Form) -> bool {
    match form {
        Form::File => kind == "file",
        Form::SkillRoot => {
            kind == "directory" && regular_file_len(&path.join("SKILL.md")).is_some()
        }
        Form::Named(suffixes) => {
            kind == "file"
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| has_any_suffix(name, suffixes))
        }
        _ => false,
    }
}

pub(super) fn has_any_suffix(name: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|suffix| name_has_suffix(name, suffix))
}

fn name_has_suffix(name: &str, suffix: &str) -> bool {
    name.len() > suffix.len() + 1 && name.ends_with(&format!(".{suffix}"))
}

pub(super) fn strip_suffix<'a>(name: &'a str, suffixes: &[&str]) -> &'a str {
    suffixes
        .iter()
        .find_map(|suffix| name.strip_suffix(&format!(".{suffix}")))
        .unwrap_or(name)
}

fn candidate_name(name: &str, form: Form) -> String {
    match form {
        Form::Named(suffixes) => strip_suffix(name, suffixes).to_owned(),
        _ => name.to_owned(),
    }
}

fn regular_file_len(path: &Path) -> Option<u64> {
    let metadata = fs::symlink_metadata(path).ok()?;
    (!is_link(&metadata) && metadata.is_file()).then_some(metadata.len())
}

fn refuse(path: &Path, home: &Path, platform: &str) -> Option<String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Some("Relative and traversing paths are not inspected.".into());
    }
    if path
        .to_str()
        .is_none_or(|text| text.len() > 4096 || text.chars().any(char::is_control))
    {
        return Some("Non-Unicode, control-bearing and oversized paths are not inspected.".into());
    }
    if let Some(area) = privacy_area(path, Some(home), platform) {
        return Some(format!(
            "Not inspected: inside {area}, which the operating system asks permission for."
        ));
    }
    for ancestor in path.ancestors().skip(1) {
        if ancestor.parent().is_none() {
            break;
        }
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if is_link(&metadata) => {
                return Some(
                    "A parent of this path is a symbolic link. holdmap does not follow it.".into(),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(_) => return Some("A parent of this path could not be inspected.".into()),
        }
    }
    None
}

pub(super) fn modified(metadata: &Metadata) -> Option<u64> {
    metadata.modified().ok().and_then(|time| {
        time.duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| u64::try_from(d.as_millis()).ok())
    })
}

pub(super) fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn io_reason(error: &std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => "access denied",
        std::io::ErrorKind::NotFound => "not found",
        _ => "metadata read failed",
    }
}

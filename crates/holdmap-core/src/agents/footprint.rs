//! Collects observed working directories and their project labels.

use super::{access, AgentFolder, Evidence, FolderSource, MAX_FOLDERS};
use crate::model::{ProcessInfo, ProjectInfo};
use crate::project::ProjectDetector;
use crate::scan::Scan;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// Paths seen on the machine: working directories, project roots and their parents.
pub(super) fn known_paths(scan: &Scan) -> HashSet<PathBuf> {
    let mut out = HashSet::new();
    let mut add = |p: &Path| {
        for a in p.ancestors().take(6) {
            if a.parent().is_none() {
                break;
            }
            out.insert(a.to_path_buf());
        }
    };
    for p in scan.table.iter() {
        if let Some(c) = &p.cwd {
            add(c);
        }
    }
    for e in &scan.snapshot.entries {
        if let Some(pr) = &e.project {
            add(&pr.root);
        }
    }
    out
}

pub(super) fn folders(
    home: Option<&Path>,
    detect_projects: bool,
    root: &ProcessInfo,
    procs: &[&ProcessInfo],
    known_projects: &[&ProjectInfo],
    detector: &mut ProjectDetector,
    platform: &str,
) -> (Vec<AgentFolder>, usize, HashSet<PathBuf>) {
    // Working directory → processes there, agent's own first.
    let mut at: BTreeMap<PathBuf, Vec<u32>> = BTreeMap::new();
    for p in procs {
        let Some(cwd) = &p.cwd else { continue };
        if cwd.parent().is_none() || cwd.to_string_lossy().contains(".app/Contents") {
            continue; // "/" and app bundles say nothing about the work
        }
        at.entry(cwd.clone()).or_default().push(p.pid);
    }
    let mut list: Vec<(PathBuf, Vec<u32>)> = at.into_iter().collect();
    list.sort_by_key(|(path, pids)| {
        (
            root.cwd.as_ref() != Some(path),
            std::cmp::Reverse(pids.len()),
            path.clone(),
        )
    });
    let all_paths = list.iter().map(|(path, _)| path.clone()).collect();
    let more_folders = list.len().saturating_sub(MAX_FOLDERS);
    list.truncate(MAX_FOLDERS);
    let folders = list
        .into_iter()
        .map(|(path, pids)| {
            let area = access::privacy_area(&path, home, platform);
            let known = known_projects
                .iter()
                .filter(|p| path.starts_with(&p.root))
                .max_by_key(|p| p.root.components().count())
                .map(|p| (*p).clone());
            let (project, note) = match (known, area) {
                (Some(p), _) => (Some(p), None),
                (None, Some(a)) => (
                    None,
                    Some(format!("Not inspected: inside {a}, which macOS guards.")),
                ),
                (None, None) if detect_projects => (detector.detect(&path).map(|d| d.info), None),
                (None, None) => (None, None),
            };
            AgentFolder {
                label: project
                    .as_ref()
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| folder_label(&path, home)),
                source: if root.cwd.as_ref() == Some(&path) {
                    FolderSource::Agent
                } else {
                    FolderSource::Child
                },
                evidence: Evidence::Observed,
                privacy_area: area.map(str::to_string),
                project,
                pids,
                note,
                path,
            }
        })
        .collect();
    (folders, more_folders, all_paths)
}

/// A folder's display name: its own name, or `~` for the home directory.
pub(super) fn folder_label(path: &Path, home: Option<&Path>) -> String {
    if home == Some(path) {
        return "~".into();
    }
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

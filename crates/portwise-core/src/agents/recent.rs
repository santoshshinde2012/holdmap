//! Recent and open projects from the agents' own state, where that is safe to read.
//!
//! Only two sources, both outside macOS privacy-protected folders and neither holding chats or
//! credentials:
//!
//! * **Claude Code** keeps one folder per project under `~/.claude/projects`, named after the
//!   project path. Only the folder *names* are listed; the transcripts inside are never opened.
//! * **VS Code-based editors** (Cursor, Windsurf, Kiro) record their open windows in
//!   `User/globalStorage/storage.json`. Only the folder URIs are taken from it.
//!
//! Settings files that may also hold tokens (`~/.claude.json`, `~/.codex/config.toml`) are not
//! read at all.

use super::model::Evidence;
use std::collections::HashSet;
use std::fmt::Debug;
use std::path::{Path, PathBuf};

/// A project folder a product lists as recent or open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentFolder {
    /// Absolute path.
    pub path: PathBuf,
    /// Exact (observed) or decoded (inferred).
    pub evidence: Evidence,
    /// Where it came from.
    pub note: String,
}

/// A source of recent projects per product (injected so tests stay off the real home folder).
pub trait RecentProjects: Send + Sync + Debug {
    /// Recent or open project folders of `product`, newest first. `known` holds paths seen on
    /// the machine (working directories, project roots), used to resolve ambiguous names.
    fn recent(&self, product: &str, known: &HashSet<PathBuf>) -> Vec<RecentFolder>;
}

/// No recent projects (tests, and surfaces that only want live data).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoRecent;

impl RecentProjects for NoRecent {
    fn recent(&self, _: &str, _: &HashSet<PathBuf>) -> Vec<RecentFolder> {
        Vec::new()
    }
}

/// At most this many recent folders per product.
pub const MAX_RECENT: usize = 8;
/// State files larger than this are skipped.
const MAX_STATE_BYTES: u64 = 4 * 1024 * 1024;

/// The agents' state under the user's home directory.
#[derive(Debug, Clone)]
pub struct HomeRecent {
    home: PathBuf,
    /// Per-user application data: `~/Library/Application Support`, `~/.config`, `%APPDATA%`.
    app_data: PathBuf,
}

impl HomeRecent {
    /// Sources under `home`, with the platform's application-data folder.
    pub fn new(home: PathBuf) -> Self {
        let app_data = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else if cfg!(windows) {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData/Roaming"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
        };
        Self { home, app_data }
    }

    /// Sources with an explicit application-data folder (tests).
    pub fn with_app_data(home: PathBuf, app_data: PathBuf) -> Self {
        Self { home, app_data }
    }

    /// For the current user, when `HOME` / `USERPROFILE` is set.
    pub fn current() -> Option<Self> {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(|h| Self::new(PathBuf::from(h)))
    }
}

impl RecentProjects for HomeRecent {
    fn recent(&self, product: &str, known: &HashSet<PathBuf>) -> Vec<RecentFolder> {
        let mut out = match product {
            "claude-code" => claude_projects(&self.home.join(".claude/projects"), known),
            "cursor" => editor_windows(&self.app_data.join("Cursor"), "Cursor"),
            "windsurf" => editor_windows(&self.app_data.join("Windsurf"), "Windsurf"),
            "kiro" => editor_windows(&self.app_data.join("Kiro"), "Kiro"),
            _ => Vec::new(),
        };
        out.truncate(MAX_RECENT);
        out
    }
}

/// How Claude Code names a project folder: every character that isn't a letter or digit
/// becomes `-` (`/Users/me/shop-web` → `-Users-me-shop-web`).
pub fn claude_key(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// `~/.claude/projects/<key>` folder names, newest first, resolved against `known` paths.
/// A name that matches no known path is decoded best-effort and marked inferred.
fn claude_projects(dir: &Path, known: &HashSet<PathBuf>) -> Vec<RecentFolder> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<(std::time::SystemTime, String)> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let at = e.metadata().and_then(|m| m.modified()).ok()?;
            Some((at, name))
        })
        .collect();
    dirs.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let by_key: std::collections::HashMap<String, &PathBuf> =
        known.iter().map(|p| (claude_key(p), p)).collect();
    dirs.into_iter()
        .filter_map(|(_, name)| {
            if let Some(p) = by_key.get(&name) {
                return Some(RecentFolder {
                    path: (*p).clone(),
                    evidence: Evidence::Observed,
                    note: "Claude Code project".into(),
                });
            }
            // Only Unix-style keys decode to a path; `-` may have been `/`, `.`, `_` or `-`.
            let rest = name.strip_prefix('-')?;
            Some(RecentFolder {
                path: PathBuf::from(format!("/{}", rest.replace('-', "/"))),
                evidence: Evidence::Inferred,
                note: format!(
                    "Claude Code project, path decoded from the folder name `{name}` (dashes are ambiguous)"
                ),
            })
        })
        .take(MAX_RECENT)
        .collect()
}

/// Folders of the open windows recorded in a VS Code-based editor's `storage.json`.
fn editor_windows(app_dir: &Path, app: &str) -> Vec<RecentFolder> {
    let file = app_dir.join("User/globalStorage/storage.json");
    let Ok(meta) = std::fs::metadata(&file) else {
        return Vec::new();
    };
    if meta.len() > MAX_STATE_BYTES {
        return Vec::new();
    }
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Vec::new();
    };
    folders_from_storage(&text, app)
}

/// Pull the window folders out of `storage.json`: the last active window first, then the other
/// open windows, then hot-exit backups. Nothing else in the file is kept.
pub fn folders_from_storage(text: &str, app: &str) -> Vec<RecentFolder> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let w = &v["windowsState"];
    let mut uris: Vec<&str> = Vec::new();
    uris.extend(w["lastActiveWindow"]["folder"].as_str());
    for win in w["openedWindows"].as_array().into_iter().flatten() {
        uris.extend(win["folder"].as_str());
    }
    for f in v["backupWorkspaces"]["folders"]
        .as_array()
        .into_iter()
        .flatten()
    {
        uris.extend(f["folderUri"].as_str());
    }
    let mut seen = HashSet::new();
    uris.into_iter()
        .filter_map(file_uri_path)
        .filter(|p| seen.insert(p.clone()))
        .map(|path| RecentFolder {
            path,
            evidence: Evidence::Observed,
            note: format!("open in {app}"),
        })
        .collect()
}

/// `file:///Users/me/shop%20web` → `/Users/me/shop web`; other schemes (remote) → `None`.
pub fn file_uri_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        if let (b'%', Some(b)) = (bytes[i], hex) {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    let s = String::from_utf8(out).ok()?;
    // Windows: file:///c%3A/Users/me → c:/Users/me
    let s = match s.strip_prefix('/') {
        Some(win) if win.get(1..2) == Some(":") => win.to_string(),
        _ => s,
    };
    (!s.is_empty()).then(|| PathBuf::from(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_keys_match_its_folder_names() {
        assert_eq!(
            claude_key(Path::new("/Users/me/shop-web")),
            "-Users-me-shop-web"
        );
        assert_eq!(
            claude_key(Path::new("/home/me/.config/x_y")),
            "-home-me--config-x-y"
        );
    }

    #[test]
    fn claude_projects_resolve_known_paths_and_mark_guesses() {
        let d = tempfile::tempdir().unwrap();
        let projects = d.path().join(".claude/projects");
        for name in ["-Users-me-code-shop-web", "-Users-me-notes"] {
            std::fs::create_dir_all(projects.join(name)).unwrap();
        }
        // A transcript file is never opened; a stray file is ignored.
        std::fs::write(projects.join("-Users-me-notes/abc.jsonl"), "secret chat").unwrap();
        std::fs::write(projects.join("stray.txt"), "x").unwrap();
        let known: HashSet<PathBuf> = [PathBuf::from("/Users/me/code/shop-web")].into();
        let src = HomeRecent::with_app_data(d.path().to_path_buf(), d.path().join("appdata"));
        let mut got = src.recent("claude-code", &known);
        got.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].path, PathBuf::from("/Users/me/code/shop-web"));
        assert_eq!(got[0].evidence, Evidence::Observed);
        assert_eq!(got[1].path, PathBuf::from("/Users/me/notes"));
        assert_eq!(got[1].evidence, Evidence::Inferred);
        assert!(
            src.recent("codex", &known).is_empty(),
            "codex state isn't read"
        );
    }

    #[test]
    fn editor_storage_yields_only_local_window_folders() {
        let json = r#"{
          "telemetry.machineId": "abc",
          "windowsState": {
            "lastActiveWindow": {"folder": "file:///Users/me/code/shop%20web"},
            "openedWindows": [
              {"folder": "file:///Users/me/code/shop%20web"},
              {"folder": "vscode-remote://ssh-remote%2Bbox/home/me/api"},
              {"workspaceIdentifier": {"id": "x"}}
            ]
          },
          "backupWorkspaces": {"folders": [{"folderUri": "file:///Users/me/code/docs"}]}
        }"#;
        let got = folders_from_storage(json, "Cursor");
        let paths: Vec<_> = got.iter().map(|f| f.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/Users/me/code/shop web"),
                PathBuf::from("/Users/me/code/docs")
            ]
        );
        assert!(got.iter().all(|f| f.evidence == Evidence::Observed));
        assert!(folders_from_storage("not json", "Cursor").is_empty());
    }

    #[test]
    fn file_uris_decode() {
        assert_eq!(
            file_uri_path("file:///c%3A/Users/me/x"),
            Some(PathBuf::from("c:/Users/me/x"))
        );
        assert_eq!(
            file_uri_path("file:///tmp/a%2"),
            Some(PathBuf::from("/tmp/a%2"))
        );
        assert_eq!(file_uri_path("https://x"), None);
    }
}

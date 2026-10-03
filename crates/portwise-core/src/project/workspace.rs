//! Workspace / monorepo detection: the directory that groups several services
//! (pnpm/yarn/npm workspaces, turbo, nx, lerna, Cargo workspaces, go.work, docker compose).

use crate::model::Workspace;
use std::fs;
use std::path::Path;

/// One way of recognising a workspace root.
pub trait WorkspaceMarker: Send + Sync {
    /// Kind label shown to users, e.g. `pnpm`, `turbo`, `compose`.
    fn kind(&self) -> &'static str;
    fn matches(&self, dir: &Path) -> bool;
}

/// A file whose mere presence marks a workspace.
struct FileMarker(&'static str, &'static str);
impl WorkspaceMarker for FileMarker {
    fn kind(&self) -> &'static str {
        self.0
    }
    fn matches(&self, dir: &Path) -> bool {
        dir.join(self.1).is_file()
    }
}

/// `package.json` with a `workspaces` field (npm / yarn / bun workspaces).
struct NpmWorkspaces;
impl WorkspaceMarker for NpmWorkspaces {
    fn kind(&self) -> &'static str {
        "npm-workspaces"
    }
    fn matches(&self, dir: &Path) -> bool {
        fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .is_some_and(|v| !v["workspaces"].is_null())
    }
}

/// `Cargo.toml` with a `[workspace]` table.
struct CargoWorkspace;
impl WorkspaceMarker for CargoWorkspace {
    fn kind(&self) -> &'static str {
        "cargo"
    }
    fn matches(&self, dir: &Path) -> bool {
        fs::read_to_string(dir.join("Cargo.toml"))
            .is_ok_and(|t| t.lines().any(|l| l.trim() == "[workspace]"))
    }
}

const COMPOSE_FILES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yml",
    "docker-compose.yaml",
];

struct ComposeFile;
impl WorkspaceMarker for ComposeFile {
    fn kind(&self) -> &'static str {
        "compose"
    }
    fn matches(&self, dir: &Path) -> bool {
        COMPOSE_FILES.iter().any(|f| dir.join(f).is_file())
    }
}

fn markers() -> Vec<Box<dyn WorkspaceMarker>> {
    vec![
        Box::new(FileMarker("pnpm", "pnpm-workspace.yaml")),
        Box::new(FileMarker("turbo", "turbo.json")),
        Box::new(FileMarker("nx", "nx.json")),
        Box::new(FileMarker("lerna", "lerna.json")),
        Box::new(NpmWorkspaces),
        Box::new(CargoWorkspace),
        Box::new(FileMarker("go-work", "go.work")),
        Box::new(ComposeFile),
    ]
}

/// Walk up from `start` (max 8 levels, never past `home`) to the nearest workspace root.
pub fn detect_workspace(start: &Path, home: Option<&Path>) -> Option<Workspace> {
    let ms = markers();
    let mut dir = Some(start);
    for _ in 0..8 {
        let d = dir?;
        if d.parent().is_none() || Some(d) == home {
            return None;
        }
        if let Some(m) = ms.iter().find(|m| m.matches(d)) {
            return Some(Workspace {
                name: d
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                root: d.to_path_buf(),
                kind: m.kind().to_string(),
            });
        }
        dir = d.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_nearest_workspace_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("acme");
        let api = root.join("services/api");
        fs::create_dir_all(&api).unwrap();
        assert!(detect_workspace(&api, None).is_none());
        fs::write(
            root.join("pnpm-workspace.yaml"),
            "packages: ['services/*']\n",
        )
        .unwrap();
        let w = detect_workspace(&api, None).unwrap();
        assert_eq!((w.name.as_str(), w.kind.as_str()), ("acme", "pnpm"));
        assert_eq!(w.root, root);
    }

    #[test]
    fn recognises_content_based_markers() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        fs::create_dir_all(&a).unwrap();
        fs::write(a.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        assert_eq!(detect_workspace(&a, None).unwrap().kind, "cargo");
        let b = tmp.path().join("b");
        fs::create_dir_all(&b).unwrap();
        fs::write(b.join("package.json"), r#"{"workspaces":["apps/*"]}"#).unwrap();
        assert_eq!(detect_workspace(&b, None).unwrap().kind, "npm-workspaces");
        let c = tmp.path().join("c");
        fs::create_dir_all(&c).unwrap();
        fs::write(c.join("compose.yaml"), "services: {}\n").unwrap();
        assert_eq!(detect_workspace(&c, None).unwrap().kind, "compose");
        // A plain package.json is not a workspace.
        let d = tmp.path().join("d");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("package.json"), r#"{"name":"x"}"#).unwrap();
        assert!(detect_workspace(&d, None).is_none());
    }

    #[test]
    fn stops_at_home() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("turbo.json"), "{}").unwrap();
        let sub = tmp.path().join("x");
        fs::create_dir_all(&sub).unwrap();
        assert!(detect_workspace(&sub, Some(tmp.path())).is_none());
    }
}

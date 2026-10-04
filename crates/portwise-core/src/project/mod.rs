//! Project detection: cwd → project root (manifest) → workspace / monorepo root → git repo,
//! plus framework detection from command-line signatures.
//!
//! Manifest formats are [`ManifestParser`] strategies in a [`ManifestRegistry`]; workspace kinds
//! are [`WorkspaceMarker`](workspace::WorkspaceMarker)s. Both are open for extension.

mod cache;
mod framework;
mod git;
pub mod manifest;
pub mod workspace;

pub use cache::{scan_budget, set_scan_budget, ProjectCache, Resolver};
pub use framework::detect_framework;
pub use git::{git_branch, git_root};
pub use manifest::{ManifestParser, ManifestRegistry};
pub use workspace::detect_workspace;

use crate::model::ProjectInfo;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A detected project plus the dependency names used to refine framework detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDetails {
    /// Name, root and branch.
    pub info: ProjectInfo,
    /// Dependency names, used for framework detection.
    pub deps: Vec<String>,
}

/// Project lookups for one scan.
///
/// Lookups go through a shared background [`ProjectCache`], so a slow or blocked read (a
/// macOS privacy prompt, a hung mount) never holds up the scan: the whole scan waits at most
/// [`scan_budget`] for lookups that aren't cached yet, and anything not ready by then is left
/// empty and filled in on a later scan.
#[derive(Debug)]
pub struct ProjectDetector {
    memo: HashMap<PathBuf, Option<ProjectDetails>>,
    home: Option<PathBuf>,
    cache: Arc<ProjectCache>,
    deadline: Instant,
}

impl Default for ProjectDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectDetector {
    /// A detector with the default registries and the current user's home directory.
    pub fn new() -> Self {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        Self::with_home(home)
    }

    /// A detector that treats `home` as the upper bound when walking up.
    pub fn with_home(home: Option<PathBuf>) -> Self {
        Self::with_cache(ProjectCache::global(), home, scan_budget())
    }

    /// A detector backed by `cache` that waits at most `budget` in total.
    pub fn with_cache(cache: Arc<ProjectCache>, home: Option<PathBuf>, budget: Duration) -> Self {
        Self {
            memo: HashMap::new(),
            home,
            cache,
            deadline: Instant::now() + budget,
        }
    }

    /// Start lookups for all of `cwds` without waiting, so one slow directory doesn't use up
    /// the budget before the others have even started.
    pub fn prefetch<'a>(&mut self, cwds: impl IntoIterator<Item = &'a Path>) {
        let now = Instant::now();
        for cwd in cwds {
            if !self.memo.contains_key(cwd) {
                let _ = self.cache.get(cwd, self.home.as_deref(), now);
            }
        }
    }

    /// The project containing `cwd`, or `None` if there is none or it isn't known yet.
    pub fn detect(&mut self, cwd: &Path) -> Option<ProjectDetails> {
        if let Some(hit) = self.memo.get(cwd) {
            return hit.clone();
        }
        let found = self.cache.get(cwd, self.home.as_deref(), self.deadline);
        self.memo.insert(cwd.to_path_buf(), found.clone());
        found
    }
}

/// Walk up from `cwd` (max 8 levels) looking for a project manifest. The home directory and the
/// filesystem root are never treated as projects.
pub fn detect_project(cwd: &Path, home: Option<&Path>) -> Option<ProjectDetails> {
    detect_project_with(ManifestRegistry::global(), cwd, home)
}

/// [`detect_project`] with an explicit manifest registry.
pub fn detect_project_with(
    registry: &ManifestRegistry,
    cwd: &Path,
    home: Option<&Path>,
) -> Option<ProjectDetails> {
    let mut dir = Some(cwd);
    let mut git_only: Option<PathBuf> = None;
    for _ in 0..8 {
        let d = dir?;
        if d.parent().is_none() || Some(d) == home {
            break;
        }
        if let Some(parser) = registry.find(d) {
            let text = fs::read_to_string(d.join(parser.marker())).unwrap_or_default();
            let (name, deps) = parser.parse(d, &text);
            return Some(ProjectDetails {
                info: info(d, parser.marker(), name, home),
                deps,
            });
        }
        if git_only.is_none() && d.join(".git").exists() {
            git_only = Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    git_only.map(|root| ProjectDetails {
        info: info(&root, "git", None, home),
        deps: Vec::new(),
    })
}

fn info(root: &Path, kind: &str, name: Option<String>, home: Option<&Path>) -> ProjectInfo {
    ProjectInfo {
        // A manifest is someone else's text: keep control characters out of the terminal.
        name: name
            .filter(|n| !n.is_empty())
            .map(|n| crate::util::printable(&n).into_owned())
            .unwrap_or_else(|| dir_name(root)),
        root: root.to_path_buf(),
        kind: kind.to_string(),
        git_branch: git_branch(root),
        workspace: detect_workspace(root, home),
        git_root: git_root(root),
    }
}

fn dir_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::proc;

    #[test]
    fn detects_package_json_project_with_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("shop-web");
        fs::create_dir_all(root.join("src/app")).unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"name":"@acme/shop-web","scripts":{"dev":"next dev"},"dependencies":{"next":"15","react":"19"}}"#,
        )
        .unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/feat/checkout\n").unwrap();
        let d = detect_project(&root.join("src/app"), None).unwrap();
        assert_eq!(d.info.name, "shop-web");
        assert_eq!(d.info.kind, "package.json");
        assert_eq!(d.info.root, root);
        assert_eq!(d.info.git_branch.as_deref(), Some("feat/checkout"));
        let mut p = proc(5, 1, "node", &["node", "server.js"]);
        p.cwd = Some(root.clone());
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "Next.js");
    }

    #[test]
    fn detects_cargo_and_python_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let rs = tmp.path().join("api");
        fs::create_dir_all(&rs).unwrap();
        fs::write(
            rs.join("Cargo.toml"),
            "[package]\nname = \"orders-api\"\n\n[dependencies]\naxum = \"0.8\"\n",
        )
        .unwrap();
        let d = detect_project(&rs, None).unwrap();
        assert_eq!(d.info.name, "orders-api");
        assert!(d.deps.contains(&"axum".to_string()));
        let p = proc(5, 1, "orders-api", &["target/debug/orders-api"]);
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "Axum");
        // A python process that happens to run inside a Rust project is not Axum.
        let p = proc(7, 1, "python3", &["python3", "serve.py"]);
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "Python");

        let py = tmp.path().join("ml");
        fs::create_dir_all(&py).unwrap();
        fs::write(
            py.join("pyproject.toml"),
            "[project]\nname = \"ml-svc\"\ndependencies = [\"fastapi>=0.1\"]\n",
        )
        .unwrap();
        let d = detect_project(&py, None).unwrap();
        assert_eq!(d.info.name, "ml-svc");
        let p = proc(6, 1, "python3", &["python3", "-m", "uvicorn", "main:app"]);
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "Uvicorn");
        let p = proc(6, 1, "python3", &["python3", "main.py"]);
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "FastAPI");
    }

    #[test]
    fn home_is_not_a_project() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("package.json"), "{}").unwrap();
        assert!(detect_project(tmp.path(), Some(tmp.path())).is_none());
    }

    #[test]
    fn signatures() {
        let cases = [
            (
                proc(1, 0, "postgres", &["postgres", "-D", "/var/lib/pg"]),
                "PostgreSQL",
            ),
            (
                proc(1, 0, "redis-server", &["redis-server *:6379"]),
                "Redis",
            ),
            (
                proc(1, 0, "node", &["node", "/app/node_modules/.bin/vite"]),
                "Vite",
            ),
            (
                proc(
                    1,
                    0,
                    "docker-proxy",
                    &["/usr/bin/docker-proxy", "-proto", "tcp"],
                ),
                "Docker",
            ),
            (
                proc(
                    1,
                    0,
                    "ControlCenter",
                    &["/System/Library/CoreServices/ControlCenter.app"],
                ),
                "AirPlay Receiver",
            ),
            (
                proc(1, 0, "python3", &["python3", "manage.py", "runserver"]),
                "Django",
            ),
            (
                proc(1, 0, "python3", &["python3", "-m", "http.server", "8000"]),
                "Python http.server",
            ),
            (proc(1, 0, "node", &["node", "index.js"]), "Node.js"),
        ];
        for (p, want) in cases {
            assert_eq!(
                detect_framework(&p, None).map(|f| f.name),
                Some(want.to_string()),
                "{:?}",
                p.cmdline
            );
        }
        assert!(detect_framework(&proc(1, 0, "mystery", &["mystery"]), None).is_none());
        assert!(detect_framework(&proc(1, 0, "invite-svc", &["invite-svc"]), None).is_none());
    }

    #[test]
    fn vite_in_sveltekit_project() {
        let d = ProjectDetails {
            info: ProjectInfo {
                name: "x".into(),
                root: "/x".into(),
                kind: "package.json".into(),
                git_branch: None,
                workspace: None,
                git_root: None,
            },
            deps: vec!["@sveltejs/kit".into(), "vite".into()],
        };
        let p = proc(1, 0, "node", &["node", "node_modules/.bin/vite", "dev"]);
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "SvelteKit");
    }
}

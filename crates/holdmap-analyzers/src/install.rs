//! In-app installs of an analyzer's program, with the user's consent.
//!
//! holdmap never runs a manifest's free-text install hint. It runs only the recipes compiled
//! in here: an exact, pinned package from a named registry, installed by a package manager the
//! user already has (`uv` or `go`) into a folder holdmap owns (`<holdmap dir>/analyzers/tools/
//! <id>`). No `sudo`, nothing system-wide, a cleared environment (the user's own config files
//! and keys are not read), a time limit and a cancel. Afterwards holdmap finds the program,
//! asks it for its version, checks that it is the pinned one and records its SHA-256. The
//! analyzer still stays off until the user approves it on its permissions sheet.

use crate::cli_runner::kill_tree;
use crate::finding::clean_text;
use crate::manifest::AnalyzerManifest;
use crate::probe::{program_names, sha256_file, BinaryProbe, SystemProbe};
use crate::runner::CancelFlag;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Longest an install may take.
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);
/// Log lines forwarded per install; the rest are counted.
pub const MAX_LOG_LINES: usize = 400;
const MAX_LINE: usize = 200;

/// How a program is installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMethod {
    /// `uv tool install <pinned spec>` into holdmap's folder.
    UvTool,
    /// `go install <module>@<version>` into holdmap's folder.
    GoInstall,
    /// No package-manager install holdmap can run; the user installs it.
    Manual,
}

/// One compiled-in recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    /// Analyzer id.
    pub id: &'static str,
    /// How.
    pub method: InstallMethod,
    /// The exact package spec handed to the package manager.
    pub package: &'static str,
    /// The pinned version the installed program must report.
    pub version: &'static str,
    /// Where the package comes from, in plain words.
    pub source: &'static str,
    /// How the download is checked, in plain words (never more than the tool really does).
    pub verification: &'static str,
    /// Hosts the install contacts.
    pub hosts: &'static [&'static str],
    /// The installer's project page (for a missing package manager or a manual install).
    pub help_url: &'static str,
    /// Manual installs: the safest way to install it yourself.
    pub safest: &'static str,
}

const PYPI_HOSTS: &[&str] = &["pypi.org", "files.pythonhosted.org"];

/// Every recipe holdmap can run.
pub const RECIPES: &[Recipe] = &[
    Recipe {
        id: "nvidia.skillspector",
        method: InstallMethod::UvTool,
        package: "git+https://github.com/NVIDIA/skillspector.git@v2.13.0",
        version: "2.13.0",
        source: "GitHub: NVIDIA/skillspector, tag v2.13.0 (its Python dependencies from PyPI)",
        verification: "Pinned to the v2.13.0 tag over HTTPS. holdmap then checks the version it reports and records the program's SHA-256; approval pins that hash.",
        hosts: &["github.com", "pypi.org", "files.pythonhosted.org"],
        help_url: "https://docs.astral.sh/uv/getting-started/installation/",
        safest: "",
    },
    Recipe {
        id: "cisco.skill-scanner",
        method: InstallMethod::UvTool,
        package: "cisco-ai-skill-scanner==2.3.0",
        version: "2.3.0",
        source: "PyPI: cisco-ai-skill-scanner 2.3.0",
        verification: "Pinned to version 2.3.0 from PyPI over HTTPS. holdmap then checks the version it reports and records the program's SHA-256; approval pins that hash.",
        hosts: PYPI_HOSTS,
        help_url: "https://docs.astral.sh/uv/getting-started/installation/",
        safest: "",
    },
    Recipe {
        id: "pycqa.bandit",
        method: InstallMethod::UvTool,
        package: "bandit[sarif]==1.9.4",
        version: "1.9.4",
        source: "PyPI: bandit 1.9.4 with its SARIF output extra",
        verification: "Pinned to version 1.9.4 from PyPI over HTTPS. holdmap then checks the version it reports and records the program's SHA-256; approval pins that hash.",
        hosts: PYPI_HOSTS,
        help_url: "https://docs.astral.sh/uv/getting-started/installation/",
        safest: "",
    },
    Recipe {
        id: "google.osv-scanner",
        method: InstallMethod::GoInstall,
        package: "github.com/google/osv-scanner/v2/cmd/osv-scanner@v2.7.0",
        version: "2.7.0",
        source: "Go module proxy: github.com/google/osv-scanner/v2 v2.7.0, built locally",
        verification: "Go checks every module against the public checksum database (sum.golang.org). holdmap then checks the version it reports and records the program's SHA-256.",
        hosts: &["proxy.golang.org", "sum.golang.org"],
        help_url: "https://go.dev/doc/install",
        safest: "",
    },
    Recipe {
        id: "aquasecurity.trivy-secrets",
        method: InstallMethod::Manual,
        package: "trivy 0.76.0",
        version: "0.76.0",
        source: "Aqua Security release binaries or your OS package manager",
        verification: "Check the release's published checksum yourself; holdmap records the program's SHA-256 when you approve it.",
        hosts: &[],
        help_url: "https://trivy.dev/latest/getting-started/installation/",
        safest: "Trivy ships as a prebuilt program, not a Python or Go package, so holdmap can't install it into its own folder. The safest way is your OS package manager (for example `brew install trivy` on macOS) or the official release from GitHub after checking its published checksum.",
    },
];

/// The recipe for `id`, if holdmap has one.
pub fn recipe(id: &str) -> Option<&'static Recipe> {
    RECIPES.iter().find(|r| r.id == id)
}

/// A package manager an install needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toolchain {
    /// `uv` or `go`.
    pub name: String,
    /// Where it was found.
    pub path: Option<String>,
    /// How to get it.
    pub help_url: String,
}

/// What an install will do, shown before the user agrees. Built without running anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallPlan {
    /// Analyzer id.
    pub analyzer: String,
    /// Display name.
    pub name: String,
    /// How.
    pub method: InstallMethod,
    /// The exact package spec.
    pub package: String,
    /// Pinned version.
    pub version: String,
    /// Where it comes from.
    pub source: String,
    /// How the download is checked.
    pub verification: String,
    /// The folder it goes into (holdmap-owned).
    pub target_dir: String,
    /// Hosts the install contacts.
    pub network: Vec<String>,
    /// What the program may read once approved and run.
    pub reads: Vec<String>,
    /// Hosts the program contacts when it runs.
    pub run_network: Vec<String>,
    /// The package manager it needs, and whether it was found.
    pub toolchain: Option<Toolchain>,
    /// The exact command holdmap will run (empty for manual installs).
    pub command: Vec<String>,
    /// The publisher's install hint, to copy and run yourself instead.
    pub fallback: Option<String>,
    /// Manual installs: the safest way to do it yourself.
    pub safest: Option<String>,
    /// Project page.
    pub help_url: String,
    /// Whether holdmap can run this install now.
    pub can_install: bool,
    /// Why not, in plain words.
    pub blocked: Option<String>,
    /// The last install holdmap did, if any.
    pub installed: Option<InstallRecord>,
}

/// An install holdmap did and verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRecord {
    /// Analyzer id.
    pub analyzer: String,
    /// Version the program reported.
    pub version: String,
    /// The installed program.
    pub binary: String,
    /// Its SHA-256.
    pub sha256: String,
    /// When (Unix epoch ms).
    pub installed_at_ms: u64,
    /// How.
    pub method: InstallMethod,
    /// The package spec installed.
    pub package: String,
}

/// One install update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallEvent {
    /// The package manager started.
    Started {
        /// The command line.
        command: Vec<String>,
    },
    /// A line it printed (one line, control characters removed, secrets masked, capped).
    Log {
        /// The line.
        line: String,
    },
    /// Finding the program and checking its version.
    Verifying,
    /// Done and verified.
    Done {
        /// What was recorded.
        record: InstallRecord,
    },
    /// It failed.
    Failed {
        /// Why, in plain words.
        reason: String,
    },
    /// The user cancelled it.
    Cancelled,
}

/// Why an install did not finish.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum InstallError {
    /// No recipe, a manual install, or the package manager is missing.
    #[error("{0}")]
    Unavailable(String),
    /// The package manager failed or the result didn't check out.
    #[error("{0}")]
    Failed(String),
    /// The user cancelled it.
    #[error("The install was cancelled.")]
    Cancelled,
}

/// Runs recipes into `<root>/<id>`.
#[derive(Debug, Clone)]
pub struct Installer {
    root: PathBuf,
    probe: SystemProbe,
    timeout: Duration,
}

impl Installer {
    /// Installs under `root` (`<holdmap dir>/analyzers/tools`), finding package managers with
    /// `probe`.
    pub fn new(root: impl Into<PathBuf>, probe: SystemProbe) -> Self {
        Self {
            root: root.into(),
            probe,
            timeout: INSTALL_TIMEOUT,
        }
    }

    /// A shorter time limit (tests).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The root folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn records_path(&self) -> PathBuf {
        self.root.join("installs.json")
    }

    /// Installs holdmap recorded, by analyzer id.
    pub fn records(&self) -> BTreeMap<String, InstallRecord> {
        std::fs::read(self.records_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save_record(&self, record: &InstallRecord) -> std::io::Result<()> {
        let mut all = self.records();
        all.insert(record.analyzer.clone(), record.clone());
        crate::store::write_private(&self.records_path(), &all)
    }

    fn toolchain(&self, recipe: &Recipe) -> Option<Toolchain> {
        let name = match recipe.method {
            InstallMethod::UvTool => "uv",
            InstallMethod::GoInstall => "go",
            InstallMethod::Manual => return None,
        };
        Some(Toolchain {
            name: name.into(),
            path: self
                .probe
                .find_program(name)
                .map(|p| p.display().to_string()),
            help_url: recipe.help_url.into(),
        })
    }

    /// The folder `id` installs into.
    pub fn target(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// What installing `manifest`'s program would do. Runs nothing.
    pub fn plan(&self, manifest: &AnalyzerManifest) -> InstallPlan {
        let recipe = recipe(&manifest.id);
        let toolchain = recipe.and_then(|r| self.toolchain(r));
        let target = self.target(&manifest.id);
        let command = match (recipe, &toolchain) {
            (Some(r), Some(t)) if r.method != InstallMethod::Manual => {
                let program = t.path.clone().unwrap_or_else(|| t.name.clone());
                argv(r, &program)
            }
            _ => Vec::new(),
        };
        let blocked = match (recipe, &toolchain) {
            (None, _) => Some(format!(
                "holdmap has no install recipe for {}. Install it yourself, then come back.",
                manifest.name
            )),
            (Some(r), _) if r.method == InstallMethod::Manual => Some(r.safest.into()),
            (Some(_), Some(t)) if t.path.is_none() => Some(format!(
                "{} needs {}, which wasn't found. Install {} from its official page, then try again.",
                manifest.name, t.name, t.name
            )),
            _ => None,
        };
        InstallPlan {
            analyzer: manifest.id.clone(),
            name: manifest.name.clone(),
            method: recipe.map_or(InstallMethod::Manual, |r| r.method),
            package: recipe.map_or_else(String::new, |r| r.package.into()),
            version: recipe.map_or_else(String::new, |r| r.version.into()),
            source: recipe.map_or_else(String::new, |r| r.source.into()),
            verification: recipe.map_or_else(String::new, |r| r.verification.into()),
            target_dir: target.display().to_string(),
            network: recipe.map_or_else(Vec::new, |r| {
                r.hosts.iter().map(|h| (*h).to_string()).collect()
            }),
            reads: reads(manifest),
            run_network: manifest.permissions.network.clone(),
            toolchain,
            command,
            fallback: manifest.install.clone(),
            safest: recipe
                .filter(|r| r.method == InstallMethod::Manual)
                .map(|r| r.safest.into()),
            help_url: recipe.map_or_else(|| manifest.homepage.clone(), |r| r.help_url.into()),
            can_install: blocked.is_none(),
            blocked,
            installed: self.records().remove(&manifest.id),
        }
    }

    /// Install `manifest`'s program now. The caller must only call this after the user agreed
    /// to the [`InstallPlan`]. Updates go to `on`; the record is saved on success.
    pub fn install(
        &self,
        manifest: &AnalyzerManifest,
        cancel: &CancelFlag,
        now_ms: u64,
        on: &dyn Fn(InstallEvent),
    ) -> Result<InstallRecord, InstallError> {
        let outcome = self.install_inner(manifest, cancel, now_ms, on);
        match &outcome {
            Ok(record) => on(InstallEvent::Done {
                record: record.clone(),
            }),
            Err(InstallError::Cancelled) => on(InstallEvent::Cancelled),
            Err(e) => on(InstallEvent::Failed {
                reason: e.to_string(),
            }),
        }
        outcome
    }

    fn install_inner(
        &self,
        manifest: &AnalyzerManifest,
        cancel: &CancelFlag,
        now_ms: u64,
        on: &dyn Fn(InstallEvent),
    ) -> Result<InstallRecord, InstallError> {
        let plan = self.plan(manifest);
        if let Some(why) = plan.blocked {
            return Err(InstallError::Unavailable(why));
        }
        let recipe = recipe(&manifest.id).expect("a plan that can install has a recipe");
        let program = PathBuf::from(&plan.command[0]);
        let target = self.target(&manifest.id);
        let bin = target.join("bin");
        let home = self.root.join(".home");
        for dir in [&self.root, &target, &bin, &home] {
            std::fs::create_dir_all(dir).map_err(|e| {
                InstallError::Failed(format!("Couldn't create {}: {e}", dir.display()))
            })?;
        }
        let mut cmd = Command::new(&program);
        cmd.args(&plan.command[1..]).current_dir(&target);
        install_env(
            &mut cmd,
            recipe.method,
            &self.root,
            &target,
            &home,
            &program,
        );
        on(InstallEvent::Started {
            command: plan.command.clone(),
        });
        run_streaming(cmd, self.timeout, cancel, on)?;
        on(InstallEvent::Verifying);
        let installed = program_names(manifest.runner.command.as_deref().unwrap_or(""))
            .into_iter()
            .map(|n| bin.join(n))
            .find(|p| p.is_file())
            .ok_or_else(|| {
                InstallError::Failed(format!(
                    "The install finished, but {} wasn't found in {}.",
                    manifest.runner.command.as_deref().unwrap_or("the program"),
                    bin.display()
                ))
            })?;
        let path = std::fs::canonicalize(&installed).unwrap_or(installed);
        let version = self.probe.version(manifest, &path).ok_or_else(|| {
            InstallError::Failed(format!(
                "{} was installed but didn't report its version, so holdmap can't verify it.",
                manifest.name
            ))
        })?;
        if !version.contains(recipe.version) {
            return Err(InstallError::Failed(format!(
                "{} reports version {version}, not the pinned {}. holdmap won't use it.",
                manifest.name, recipe.version
            )));
        }
        let sha256 = sha256_file(&path)
            .ok_or_else(|| InstallError::Failed("Couldn't read the installed program.".into()))?;
        let record = InstallRecord {
            analyzer: manifest.id.clone(),
            version: clean_text(&version, 40),
            binary: path.display().to_string(),
            sha256,
            installed_at_ms: now_ms,
            method: recipe.method,
            package: recipe.package.into(),
        };
        self.save_record(&record)
            .map_err(|e| InstallError::Failed(format!("Couldn't record the install: {e}")))?;
        Ok(record)
    }
}

fn reads(manifest: &AnalyzerManifest) -> Vec<String> {
    manifest
        .permissions
        .read
        .iter()
        .map(|r| {
            if r == crate::manifest::SUBJECT_DIR {
                "A private copy of each skill folder, one at a time".to_string()
            } else {
                r.clone()
            }
        })
        .collect()
}

/// The exact command line for `recipe`, run with `program`.
pub fn argv(recipe: &Recipe, program: &str) -> Vec<String> {
    let mut v = vec![program.to_string()];
    match recipe.method {
        InstallMethod::UvTool => {
            v.extend(["tool", "install", "--force", "--no-config"].map(String::from));
            v.push(recipe.package.into());
        }
        InstallMethod::GoInstall => {
            v.push("install".into());
            v.push(recipe.package.into());
        }
        InstallMethod::Manual => {}
    }
    v
}

/// A cleared environment: a minimal PATH, a holdmap-owned HOME and caches, and the package
/// manager told to install into `target`. Nothing from the user's environment is copied.
fn install_env(
    cmd: &mut Command,
    method: InstallMethod,
    root: &Path,
    target: &Path,
    home: &Path,
    program: &Path,
) {
    cmd.env_clear();
    let mut path: Vec<PathBuf> = program
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .collect();
    if cfg!(windows) {
        if let Some(root) = std::env::var_os("SystemRoot") {
            let root = PathBuf::from(root);
            path.push(root.join("System32"));
            cmd.env("SystemRoot", &root);
        }
    } else {
        path.extend(["/usr/bin", "/bin"].map(PathBuf::from));
    }
    if let Ok(joined) = std::env::join_paths(&path) {
        cmd.env("PATH", joined);
    }
    let tmp = home.join("tmp");
    let _ = std::fs::create_dir_all(&tmp);
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .env("TMPDIR", &tmp)
        .env("TEMP", &tmp)
        .env("TMP", &tmp)
        .env("LANG", "C.UTF-8")
        .env("NO_COLOR", "1");
    match method {
        InstallMethod::UvTool => {
            cmd.env("UV_TOOL_DIR", target.join("venv"))
                .env("UV_TOOL_BIN_DIR", target.join("bin"))
                .env("UV_CACHE_DIR", root.join(".cache/uv"))
                .env("UV_PYTHON_INSTALL_DIR", root.join(".python"))
                .env("UV_NO_PROGRESS", "1");
        }
        InstallMethod::GoInstall => {
            cmd.env("GOBIN", target.join("bin"))
                .env("GOPATH", root.join(".go"))
                .env("GOMODCACHE", root.join(".go/pkg/mod"))
                .env("GOCACHE", root.join(".cache/go-build"))
                .env("GOFLAGS", "-modcacherw")
                .env("GOPROXY", "https://proxy.golang.org")
                .env("GOSUMDB", "sum.golang.org")
                .env("GOTOOLCHAIN", "local")
                .env("CGO_ENABLED", "0");
        }
        InstallMethod::Manual => {}
    }
}

fn run_streaming(
    mut cmd: Command,
    timeout: Duration,
    cancel: &CancelFlag,
    on: &dyn Fn(InstallEvent),
) -> Result<(), InstallError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| InstallError::Failed(format!("Couldn't start the installer: {e}")))?;
    let (tx, rx) = mpsc::channel::<String>();
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|stream| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        })
    })
    .collect();
    drop(tx);
    let started = Instant::now();
    let mut sent = 0usize;
    let mut last = String::new();
    let forward = |line: String, sent: &mut usize, last: &mut String| {
        let line = clean_text(&line, MAX_LINE);
        if line.is_empty() {
            return;
        }
        *last = line.clone();
        if *sent < MAX_LOG_LINES {
            *sent += 1;
            on(InstallEvent::Log { line });
        }
    };
    let status = loop {
        while let Ok(line) = rx.try_recv() {
            forward(line, &mut sent, &mut last);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|e| InstallError::Failed(e.to_string()))?
        {
            break status;
        }
        if cancel.is_cancelled() {
            kill_tree(&mut child);
            return Err(InstallError::Cancelled);
        }
        if started.elapsed() >= timeout {
            kill_tree(&mut child);
            return Err(InstallError::Failed(format!(
                "The install took longer than {} seconds and was stopped.",
                timeout.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    for r in readers {
        let _ = r.join();
    }
    while let Ok(line) = rx.try_recv() {
        forward(line, &mut sent, &mut last);
    }
    if status.success() {
        Ok(())
    } else {
        Err(InstallError::Failed(match status.code() {
            Some(code) if last.is_empty() => format!("The installer exited with code {code}."),
            Some(code) => format!("The installer exited with code {code}: {last}"),
            None => "The installer was stopped by the system.".into(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn manifest(id: &str) -> AnalyzerManifest {
        crate::presets::manifests()
            .into_iter()
            .map(|(m, _)| m)
            .chain(crate::catalog::bundled())
            .find(|m| m.id == id)
            .unwrap()
    }

    #[test]
    fn every_cli_analyzer_holdmap_ships_has_a_recipe_matching_its_install_hint() {
        let shipped: Vec<AnalyzerManifest> = crate::presets::manifests()
            .into_iter()
            .map(|(m, _)| m)
            .chain(crate::catalog::bundled())
            .filter(|m| m.runner.kind == crate::manifest::RunnerKind::Cli)
            .collect();
        for m in &shipped {
            let r = recipe(&m.id).unwrap_or_else(|| panic!("{} has no recipe", m.id));
            let hint = m.install.as_deref().unwrap();
            assert!(
                hint.contains(r.version),
                "{}: {hint} vs {}",
                m.id,
                r.version
            );
            if r.method != InstallMethod::Manual {
                let spec = r.package.trim_start_matches("git+");
                assert!(
                    hint.contains(spec.split('@').next().unwrap())
                        || hint.contains(spec.split("==").next().unwrap()),
                    "{}: {hint} vs {spec}",
                    m.id
                );
                assert!(
                    !r.hosts.is_empty(),
                    "{}: says which hosts it contacts",
                    m.id
                );
                assert!(
                    r.package.contains(r.version),
                    "{}: the spec is pinned",
                    m.id
                );
            } else {
                assert!(!r.safest.is_empty());
            }
        }
    }

    #[test]
    fn commands_never_use_sudo_or_a_shell_and_always_pin() {
        for r in RECIPES {
            let v = argv(r, "pm");
            assert!(!v.iter().any(|a| a == "sudo" || a == "sh" || a == "-c"));
            if r.method == InstallMethod::UvTool {
                assert!(v.contains(&"--no-config".to_string()));
            }
        }
    }

    #[test]
    fn a_missing_package_manager_or_a_manual_tool_blocks_the_install_with_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        let none = SystemProbe::with_env(Some(OsString::from("/nonexistent")), None);
        let inst = Installer::new(dir.path(), none);
        let trivy = inst.plan(&manifest("aquasecurity.trivy-secrets"));
        assert!(!trivy.can_install);
        assert!(trivy.safest.as_deref().unwrap().contains("checksum"));
        assert!(trivy.command.is_empty());
        let bandit = inst.plan(&manifest("pycqa.bandit"));
        assert_eq!(
            bandit.target_dir,
            dir.path().join("pycqa.bandit").display().to_string()
        );
        assert!(bandit.reads[0].contains("private copy"));
        if bandit.toolchain.as_ref().unwrap().path.is_none() {
            assert!(!bandit.can_install);
            assert!(bandit.blocked.unwrap().contains("uv"));
            let err = inst
                .install(
                    &manifest("pycqa.bandit"),
                    &CancelFlag::default(),
                    1,
                    &|_| {},
                )
                .unwrap_err();
            assert!(matches!(err, InstallError::Unavailable(_)));
        }
    }

    #[cfg(unix)]
    fn fake_uv(dir: &Path, body: &str) -> SystemProbe {
        use std::os::unix::fs::PermissionsExt;
        let pm = dir.join("pm");
        std::fs::create_dir_all(&pm).unwrap();
        let uv = pm.join("uv");
        std::fs::write(&uv, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&uv, std::fs::Permissions::from_mode(0o755)).unwrap();
        SystemProbe::with_env(Some(pm.into_os_string()), None)
    }

    #[cfg(unix)]
    const GOOD: &str = r#"echo "Resolved 3 packages"
echo "home=$HOME user=${USER:-none} cargo=${CARGO:-none} path=$PATH"
mkdir -p "$UV_TOOL_BIN_DIR"
printf '#!/bin/sh\necho "bandit 1.9.4"\n' > "$UV_TOOL_BIN_DIR/bandit"
chmod +x "$UV_TOOL_BIN_DIR/bandit"
echo "Installed 1 executable: bandit""#;

    #[cfg(unix)]
    #[test]
    fn installs_into_holdmaps_folder_with_a_cleared_environment_and_records_the_version() {
        let dir = tempfile::tempdir().unwrap();
        let inst = Installer::new(dir.path().join("tools"), fake_uv(dir.path(), GOOD));
        let m = manifest("pycqa.bandit");
        let plan = inst.plan(&m);
        assert!(plan.can_install, "{:?}", plan.blocked);
        assert_eq!(
            &plan.command[1..5],
            ["tool", "install", "--force", "--no-config"]
        );
        let events = std::sync::Mutex::new(Vec::new());
        let record = inst
            .install(&m, &CancelFlag::default(), 42, &|e| {
                events.lock().unwrap().push(e)
            })
            .unwrap();
        assert_eq!(record.version, "1.9.4");
        assert_eq!(record.sha256.len(), 64);
        assert!(record.binary.starts_with(
            &dir.path()
                .join("tools/pycqa.bandit")
                .canonicalize()
                .unwrap()
                .display()
                .to_string()
        ));
        let events = events.into_inner().unwrap();
        assert!(matches!(events[0], InstallEvent::Started { .. }));
        assert!(matches!(events.last(), Some(InstallEvent::Done { .. })));
        let logs: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                InstallEvent::Log { line } => Some(line.clone()),
                _ => None,
            })
            .collect();
        let env_line = logs.iter().find(|l| l.starts_with("home=")).unwrap();
        assert!(env_line.contains("user=none cargo=none"), "{env_line}");
        assert!(env_line.ends_with("/pm:/usr/bin:/bin"), "{env_line}");
        assert!(env_line.contains(".home"), "{env_line}");
        assert!(events.iter().any(|e| matches!(e, InstallEvent::Verifying)));
        assert_eq!(inst.records()["pycqa.bandit"], record);
        assert_eq!(inst.plan(&m).installed, Some(record));
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_install_says_why_and_a_wrong_version_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest("pycqa.bandit");
        let fail = Installer::new(
            dir.path().join("a"),
            fake_uv(
                &dir.path().join("x"),
                "echo 'error: No solution found' >&2; exit 2",
            ),
        );
        let events = std::sync::Mutex::new(Vec::new());
        let err = fail
            .install(&m, &CancelFlag::default(), 1, &|e| {
                events.lock().unwrap().push(e)
            })
            .unwrap_err();
        assert_eq!(
            err,
            InstallError::Failed(
                "The installer exited with code 2: error: No solution found".into()
            )
        );
        assert!(matches!(
            events.lock().unwrap().last(),
            Some(InstallEvent::Failed { .. })
        ));
        let wrong = Installer::new(
            dir.path().join("b"),
            fake_uv(
                &dir.path().join("y"),
                &GOOD.replace("bandit 1.9.4", "bandit 1.0.0"),
            ),
        );
        let err = wrong
            .install(&m, &CancelFlag::default(), 1, &|_| {})
            .unwrap_err();
        assert!(err.to_string().contains("not the pinned 1.9.4"), "{err}");
        assert!(wrong.records().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn cancelling_or_a_time_limit_stops_the_installer() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest("pycqa.bandit");
        let slow = fake_uv(dir.path(), "echo started; sleep 30");
        let cancel = CancelFlag::default();
        let c2 = cancel.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            c2.cancel();
        });
        let started = Instant::now();
        let err = Installer::new(dir.path().join("t"), slow.clone())
            .install(&m, &cancel, 1, &|_| {})
            .unwrap_err();
        t.join().unwrap();
        assert_eq!(err, InstallError::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(10));
        let err = Installer::new(dir.path().join("u"), slow)
            .with_timeout(Duration::from_millis(300))
            .install(&m, &CancelFlag::default(), 1, &|_| {})
            .unwrap_err();
        assert!(err.to_string().contains("longer than"), "{err}");
    }
}

//! Finding and fingerprinting an analyzer's executable. Locating hashes the file; the version
//! probe runs it (sandboxed, short timeout) and is only used at approval and before a run.

use crate::cli_runner::run_capped;
use crate::finding::hex;
use crate::manifest::AnalyzerManifest;
use crate::runner::{BasicSandbox, Sandbox};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// A located executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryInfo {
    /// Absolute path.
    pub path: PathBuf,
    /// SHA-256 of the file.
    pub sha256: String,
}

/// Finds analyzers' executables.
pub trait BinaryProbe: Send + Sync {
    /// Locate and hash the manifest's command.
    fn locate(&self, manifest: &AnalyzerManifest) -> Option<BinaryInfo>;
    /// Only whether the command is there: no hashing, so it is cheap enough for a list.
    fn installed(&self, manifest: &AnalyzerManifest) -> bool {
        self.locate(manifest).is_some()
    }
    /// Ask it for its version.
    fn version(&self, manifest: &AnalyzerManifest, binary: &Path) -> Option<String>;
}

/// Looks on PATH plus the usual per-user install folders (GUI apps get a minimal PATH).
#[derive(Debug, Clone)]
pub struct SystemProbe {
    path: Option<OsString>,
    home: Option<PathBuf>,
    tools: Option<PathBuf>,
}

impl SystemProbe {
    /// From the current process's PATH and HOME.
    pub fn from_env() -> Self {
        Self {
            path: std::env::var_os("PATH"),
            home: std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from),
            tools: None,
        }
    }

    /// Also look in holdmap's own install folder (`<tools>/<analyzer id>/bin`), first.
    pub fn with_tools_dir(mut self, tools: impl Into<PathBuf>) -> Self {
        self.tools = Some(tools.into());
        self
    }

    /// An explicit PATH and home (tests).
    pub fn with_env(path: Option<OsString>, home: Option<PathBuf>) -> Self {
        Self {
            path,
            home,
            tools: None,
        }
    }

    /// Find a program (a package manager such as `uv` or `go`) on PATH or a usual folder.
    pub fn find_program(&self, name: &str) -> Option<PathBuf> {
        let mut dirs = self.dirs();
        if let Some(home) = &self.home {
            dirs.push(home.join("go/bin"));
        }
        dirs.push(PathBuf::from("/usr/local/go/bin"));
        program_names(name)
            .iter()
            .flat_map(|n| dirs.iter().map(move |d| d.join(n)))
            .find(|p| executable(p))
    }

    fn dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self
            .path
            .as_ref()
            .map(|p| std::env::split_paths(p).collect())
            .unwrap_or_default();
        if let Some(home) = &self.home {
            dirs.push(home.join(".local/bin"));
            dirs.push(home.join(".cargo/bin"));
        }
        dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
        dirs.into_iter().filter(|d| d.is_absolute()).collect()
    }
}

impl SystemProbe {
    fn find(&self, manifest: &AnalyzerManifest) -> Option<PathBuf> {
        let command = manifest.runner.command.as_deref()?;
        let path = if Path::new(command).is_absolute() {
            Some(PathBuf::from(command))
        } else {
            let names = program_names(command);
            let own = self
                .tools
                .as_ref()
                .map(|t| t.join(&manifest.id).join("bin"));
            own.into_iter()
                .chain(self.dirs())
                .collect::<Vec<_>>()
                .iter()
                .flat_map(|d| names.iter().map(move |n| d.join(n)))
                .find(|p| executable(p))
        }?;
        fs::canonicalize(&path).ok()
    }
}

/// The file names a command can have on this platform.
pub fn program_names(command: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![
            format!("{command}.exe"),
            format!("{command}.cmd"),
            command.into(),
        ]
    } else {
        vec![command.into()]
    }
}

impl BinaryProbe for SystemProbe {
    fn installed(&self, manifest: &AnalyzerManifest) -> bool {
        self.find(manifest).is_some()
    }

    fn locate(&self, manifest: &AnalyzerManifest) -> Option<BinaryInfo> {
        let path = self.find(manifest)?;
        Some(BinaryInfo {
            sha256: sha256_file(&path)?,
            path,
        })
    }

    fn version(&self, manifest: &AnalyzerManifest, binary: &Path) -> Option<String> {
        let prefix = manifest.runner.version_prefix.as_deref()?;
        let home = tempfile::Builder::new()
            .prefix("holdmap-analyzer-")
            .tempdir()
            .ok()?;
        let mut cmd = Command::new(binary);
        cmd.args(&manifest.runner.version_args)
            .current_dir(home.path());
        BasicSandbox.prepare(&mut cmd, home.path(), false);
        let ran = run_capped(cmd, Duration::from_secs(15), 64 * 1024).ok()?;
        parse_version(&String::from_utf8_lossy(&ran.stdout), prefix)
    }
}

/// The version token after `prefix` in `output`.
pub fn parse_version(output: &str, prefix: &str) -> Option<String> {
    let rest = output
        .lines()
        .find_map(|l| l.split_once(prefix).map(|(_, r)| r))?;
    let version: String = rest
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        .take(40)
        .collect();
    (!version.is_empty()).then_some(version)
}

fn executable(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    meta.is_file()
}

/// SHA-256 of a file, streamed.
pub fn sha256_file(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        match file.read(&mut buf).ok()? {
            0 => break,
            n => hash.update(&buf[..n]),
        }
    }
    Some(hex(&hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_after_the_prefix_only() {
        assert_eq!(
            parse_version("SkillSpector v2.13.0\n", "SkillSpector v").as_deref(),
            Some("2.13.0")
        );
        assert_eq!(
            parse_version("noise\nSkillSpector v2.13.0rc1 (x)", "SkillSpector v").as_deref(),
            Some("2.13.0rc1")
        );
        assert_eq!(parse_version("something else", "SkillSpector v"), None);
    }

    #[cfg(unix)]
    #[test]
    fn locate_finds_and_hashes_an_executable_on_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("skillspector");
        fs::write(&bin, "#!/bin/sh\necho 'SkillSpector v9.9.9'\n").unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        let probe = SystemProbe::with_env(Some(dir.path().as_os_str().to_owned()), None);
        let manifest =
            AnalyzerManifest::parse(include_str!("../manifests/nvidia.skillspector.toml")).unwrap();
        let info = probe.locate(&manifest).unwrap();
        assert_eq!(info.sha256.len(), 64);
        assert_eq!(
            probe.version(&manifest, &info.path).as_deref(),
            Some("9.9.9")
        );
        let none = SystemProbe::with_env(Some(OsString::from("/nonexistent")), None);
        assert!(
            none.locate(&manifest).is_none() || Path::new("/usr/local/bin/skillspector").exists()
        );
    }

    #[cfg(unix)]
    #[test]
    fn holdmaps_own_install_folder_is_searched_first() {
        use std::os::unix::fs::PermissionsExt;
        let tools = tempfile::tempdir().unwrap();
        let bin = tools.path().join("nvidia.skillspector/bin");
        fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("skillspector");
        fs::write(&exe, "#!/bin/sh\necho 'SkillSpector v2.13.0'\n").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        let manifest =
            AnalyzerManifest::parse(include_str!("../manifests/nvidia.skillspector.toml")).unwrap();
        let probe = SystemProbe::with_env(Some(OsString::from("/nonexistent")), None)
            .with_tools_dir(tools.path());
        let info = probe.locate(&manifest).unwrap();
        assert_eq!(info.path, fs::canonicalize(&exe).unwrap());
    }
}

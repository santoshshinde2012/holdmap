//! OS sandboxes for CLI analyzers, each labelled with exactly what it enforces
//! ([`SandboxLevel::describe`]). [`best_available`] picks the strongest one this machine can
//! really use and falls back to [`BasicSandbox`] rather than claiming more than it does.

use crate::runner::{BasicSandbox, ContainGuard, Containment, ProcessRef, Sandbox, SandboxLevel};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Credential folders under the user's home that a sandboxed analyzer must not read.
pub const PRIVATE_DIRS: &[&str] = &[
    ".ssh",
    ".aws",
    ".gnupg",
    ".azure",
    ".kube",
    ".docker",
    ".password-store",
    ".config/gcloud",
    ".config/gh",
    ".codex",
    "Library/Keychains",
];

/// Credential files under the user's home that a sandboxed analyzer must not read.
pub const PRIVATE_FILES: &[&str] = &[
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    ".claude.json",
];

/// The strongest sandbox this machine can actually apply.
pub fn best_available() -> Box<dyn Sandbox> {
    #[cfg(target_os = "macos")]
    if let Some(s) = Seatbelt::detect() {
        return Box::new(s);
    }
    #[cfg(target_os = "linux")]
    if let Some(s) = Bubblewrap::detect() {
        return Box::new(s);
    }
    #[cfg(target_os = "linux")]
    if let Some(s) = crate::landlock::Landlock::detect() {
        return Box::new(s);
    }
    if cfg!(windows) {
        Box::new(JobSandbox)
    } else {
        Box::new(BasicSandbox)
    }
}

/// Whether Landlock can deny TCP on this kernel (always false off Linux).
pub fn landlock_blocks_tcp() -> bool {
    #[cfg(target_os = "linux")]
    return crate::landlock::blocks_tcp();
    #[cfg(not(target_os = "linux"))]
    false
}

fn real_home() -> Option<PathBuf> {
    std::env::home_dir().filter(|h| h.is_absolute())
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// macOS Seatbelt via `/usr/bin/sandbox-exec` (best effort; Apple deprecated the tool).
#[derive(Debug, Clone)]
pub struct Seatbelt {
    exec: PathBuf,
    home: Option<PathBuf>,
}

impl Seatbelt {
    /// The system `sandbox-exec`, when present.
    pub fn detect() -> Option<Self> {
        let exec = PathBuf::from("/usr/bin/sandbox-exec");
        exec.is_file().then(|| Self::new(exec, real_home()))
    }

    /// Explicit launcher and home (tests).
    pub fn new(exec: PathBuf, home: Option<PathBuf>) -> Self {
        Self { exec, home }
    }

    /// The profile for one run: everything allowed except writes outside `work`, reads of
    /// credential folders and, unless declared, the network.
    pub fn profile(&self, work: &Path, network: bool) -> String {
        let work = canonical(work);
        let mut p = String::from("(version 1)\n(allow default)\n(deny file-write*)\n");
        p.push_str(&format!(
            "(allow file-write* (subpath {}) (literal \"/dev/null\") (literal \"/dev/zero\") (literal \"/dev/tty\") (literal \"/dev/dtracehelper\") (regex #\"^/dev/fd/\"))\n",
            quote(&work)
        ));
        if let Some(home) = &self.home {
            let home = canonical(home);
            let mut deny = String::new();
            for d in PRIVATE_DIRS {
                deny.push_str(&format!(" (subpath {})", quote(&home.join(d))));
            }
            for f in PRIVATE_FILES {
                deny.push_str(&format!(" (literal {})", quote(&home.join(f))));
            }
            p.push_str(&format!("(deny file-read*{deny})\n"));
        }
        if !network {
            p.push_str("(deny network*)\n");
        }
        p
    }
}

/// A Seatbelt string literal.
fn quote(path: &Path) -> String {
    let s = path.to_string_lossy();
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

impl Sandbox for Seatbelt {
    fn level(&self) -> SandboxLevel {
        SandboxLevel::MacosSeatbelt
    }

    fn command(&self, binary: &Path, args: &[String], c: &Containment<'_>) -> Command {
        let mut cmd = Command::new(&self.exec);
        cmd.arg("-p")
            .arg(self.profile(c.work, c.network))
            .arg(binary)
            .args(args);
        self.prepare(&mut cmd, c.home, c.network);
        cmd
    }
}

/// Linux bubblewrap (`bwrap`), used only when it is installed in a system folder and this
/// machine lets it create namespaces.
#[derive(Debug, Clone)]
pub struct Bubblewrap {
    bwrap: PathBuf,
    home: Option<PathBuf>,
}

impl Bubblewrap {
    /// A working system `bwrap`, checked once per process.
    pub fn detect() -> Option<Self> {
        static WORKS: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
        WORKS
            .get_or_init(|| {
                ["/usr/bin/bwrap", "/bin/bwrap", "/usr/local/bin/bwrap"]
                    .iter()
                    .map(PathBuf::from)
                    .find(|p| p.is_file())
                    .filter(|p| Self::works(p))
            })
            .clone()
            .map(|p| Self::new(p, real_home()))
    }

    fn works(bwrap: &Path) -> bool {
        let Some(t) = ["/usr/bin/true", "/bin/true"]
            .iter()
            .find(|p| Path::new(p).is_file())
        else {
            return false;
        };
        Command::new(bwrap)
            .args([
                "--ro-bind",
                "/",
                "/",
                "--dev",
                "/dev",
                "--unshare-net",
                "--unshare-pid",
                "--",
                t,
            ])
            .env_clear()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Explicit launcher and home (tests).
    pub fn new(bwrap: PathBuf, home: Option<PathBuf>) -> Self {
        Self { bwrap, home }
    }

    /// The bwrap arguments before `--`.
    pub fn args(&self, work: &Path, network: bool) -> Vec<String> {
        let mut a: Vec<String> = [
            "--die-with-parent",
            "--new-session",
            "--unshare-pid",
            "--unshare-ipc",
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
        ]
        .map(String::from)
        .to_vec();
        if let Some(home) = &self.home {
            for d in PRIVATE_DIRS {
                let p = home.join(d);
                if std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir()) {
                    a.extend(["--tmpfs".into(), p.to_string_lossy().into_owned()]);
                }
            }
            for f in PRIVATE_FILES {
                let p = home.join(f);
                if std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_file()) {
                    a.extend([
                        "--ro-bind".into(),
                        "/dev/null".into(),
                        p.to_string_lossy().into_owned(),
                    ]);
                }
            }
        }
        let work = work.to_string_lossy().into_owned();
        a.extend(["--bind".into(), work.clone(), work]);
        if !network {
            a.push("--unshare-net".into());
        }
        a
    }
}

impl Sandbox for Bubblewrap {
    fn level(&self) -> SandboxLevel {
        SandboxLevel::LinuxBubblewrap
    }

    fn command(&self, binary: &Path, args: &[String], c: &Containment<'_>) -> Command {
        let mut cmd = Command::new(&self.bwrap);
        cmd.args(self.args(c.work, c.network))
            .arg("--")
            .arg(binary)
            .args(args);
        self.prepare(&mut cmd, c.home, c.network);
        cmd
    }
}

/// Windows: [`SandboxLevel::Basic`] plus a Job Object that ends the whole process tree with
/// the run and caps its memory.
#[derive(Debug, Default, Clone, Copy)]
pub struct JobSandbox;

/// Most processes one Windows run may have at once.
pub const JOB_PROCESSES: u32 = 64;
/// Memory cap for one Windows run's process tree.
pub const JOB_MEMORY_BYTES: usize = 2 << 30;

impl Sandbox for JobSandbox {
    fn level(&self) -> SandboxLevel {
        SandboxLevel::WindowsJob
    }

    fn contain(&self, process: ProcessRef) -> Option<ContainGuard> {
        #[cfg(windows)]
        {
            job::contain(process).map(|j| Box::new(j) as ContainGuard)
        }
        #[cfg(not(windows))]
        {
            let _ = process;
            None
        }
    }
}

#[cfg(windows)]
mod job {
    use super::{ProcessRef, JOB_MEMORY_BYTES, JOB_PROCESSES};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
        JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// An owned job handle; closing it ends every process in the job.
    pub struct Job(HANDLE);

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the handle came from CreateJobObjectW and is closed exactly once.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn contain(process: ProcessRef) -> Option<Job> {
        // SAFETY: plain Win32 calls on a handle we own and the live child's process handle
        // (the caller keeps the child alive while this runs);
        // the info struct is zero-initialised (valid for this POD type) and sized correctly.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return None;
            }
            let job = Job(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION
                | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                | JOB_OBJECT_LIMIT_JOB_MEMORY;
            info.BasicLimitInformation.ActiveProcessLimit = JOB_PROCESSES;
            info.JobMemoryLimit = JOB_MEMORY_BYTES;
            let ok = SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 || AssignProcessToJobObject(job.0, process.handle as HANDLE) == 0 {
                return None;
            }
            Some(job)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_home() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
        std::fs::write(home.path().join(".ssh/id_test"), "not a real key").unwrap();
        std::fs::write(home.path().join(".netrc"), "machine example").unwrap();
        home
    }

    #[test]
    fn every_level_is_described_honestly() {
        for level in [
            SandboxLevel::Basic,
            SandboxLevel::MacosSeatbelt,
            SandboxLevel::LinuxBubblewrap,
            SandboxLevel::WindowsJob,
        ] {
            let text = level.describe(false);
            assert!(text.contains("time limit"), "{text}");
        }
        assert!(SandboxLevel::Basic
            .describe(false)
            .contains("does not block"));
        assert!(SandboxLevel::WindowsJob
            .describe(false)
            .contains("does not block"));
        assert!(SandboxLevel::MacosSeatbelt
            .describe(false)
            .contains("best effort"));
        assert!(SandboxLevel::LinuxBubblewrap
            .describe(false)
            .contains("blocks its network"));
        assert!(SandboxLevel::LinuxBubblewrap
            .describe(true)
            .contains("may use the network"));
    }

    #[test]
    fn seatbelt_profile_limits_writes_credentials_and_network() {
        let home = fake_home();
        let work = tempfile::tempdir().unwrap();
        let s = Seatbelt::new("/usr/bin/sandbox-exec".into(), Some(home.path().into()));
        let p = s.profile(work.path(), false);
        assert!(p.contains("(deny file-write*)"));
        assert!(p.contains(&quote(&canonical(work.path()))));
        assert!(p.contains(".ssh"));
        assert!(p.contains(".netrc"));
        assert!(p.contains("(deny network*)"));
        assert!(!s.profile(work.path(), true).contains("network"));
    }

    #[test]
    fn seatbelt_quotes_hostile_paths() {
        assert_eq!(quote(Path::new("/a\"b\\c")), "\"/a\\\"b\\\\c\"");
    }

    #[test]
    fn seatbelt_command_wraps_the_binary_with_a_cleared_env() {
        let work = tempfile::tempdir().unwrap();
        let home = work.path().join("home");
        let s = Seatbelt::new("/usr/bin/sandbox-exec".into(), None);
        let cmd = s.command(
            Path::new("/opt/tool"),
            &["--x".into()],
            &Containment {
                work: work.path(),
                home: &home,
                network: false,
            },
        );
        assert_eq!(cmd.get_program(), "/usr/bin/sandbox-exec");
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "-p");
        assert_eq!(&args[2..], ["/opt/tool", "--x"]);
        assert!(cmd.get_envs().any(|(k, _)| k == "HOME"));
    }

    #[test]
    fn bubblewrap_args_hide_credentials_and_network() {
        let home = fake_home();
        let work = tempfile::tempdir().unwrap();
        let b = Bubblewrap::new("/usr/bin/bwrap".into(), Some(home.path().into()));
        let a = b.args(work.path(), false);
        let ssh = home.path().join(".ssh").to_string_lossy().into_owned();
        let netrc = home.path().join(".netrc").to_string_lossy().into_owned();
        assert!(
            a.windows(2).any(|w| w[0] == "--tmpfs" && w[1] == ssh),
            "{a:?}"
        );
        assert!(a
            .windows(3)
            .any(|w| w[0] == "--ro-bind" && w[1] == "/dev/null" && w[2] == netrc));
        assert!(a.contains(&"--unshare-net".to_string()));
        assert!(
            !a.iter().any(|x| x.contains(".aws")),
            "absent folders aren't mounted"
        );
        assert!(!b
            .args(work.path(), true)
            .contains(&"--unshare-net".to_string()));
        // The writable bind comes after the read-only root so it wins.
        let root = a.iter().position(|x| x == "--ro-bind").unwrap();
        let bind = a.iter().position(|x| x == "--bind").unwrap();
        assert!(bind > root);
    }

    /// Runs only where bubblewrap really works (it needs user namespaces).
    #[cfg(target_os = "linux")]
    #[test]
    fn bubblewrap_enforces_writes_credentials_and_network() {
        let Some(b) = Bubblewrap::detect() else {
            eprintln!("bubblewrap not usable here; skipped");
            return;
        };
        let home = fake_home();
        let b = Bubblewrap::new(b.bwrap, Some(home.path().into()));
        let work = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let run_home = work.path().join("home");
        let script = format!(
            "echo ok > {w}/inside && echo yes; echo x > {o}/outside 2>/dev/null && echo leaked; cat {h}/.ssh/id_test 2>/dev/null; cat {h}/.netrc; wc -l < /proc/net/dev",
            w = work.path().display(),
            o = outside.path().display(),
            h = home.path().display()
        );
        let out = b
            .command(
                Path::new("/bin/sh"),
                &["-c".into(), script],
                &Containment {
                    work: work.path(),
                    home: &run_home,
                    network: false,
                },
            )
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.first(), Some(&"yes"), "{text}");
        assert!(!text.contains("leaked"), "{text}");
        assert!(!text.contains("not a real key"), "{text}");
        assert!(!text.contains("machine example"), "{text}");
        // Header lines plus loopback only: no host interfaces.
        assert_eq!(lines.last().map(|l| l.trim()), Some("3"), "{text}");
        assert!(!outside.path().join("outside").exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn seatbelt_enforces_writes_and_credentials() {
        let Some(s) = Seatbelt::detect() else { return };
        let home = fake_home();
        let s = Seatbelt::new(s.exec, Some(home.path().into()));
        let work = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let run_home = work.path().join("home");
        let script = format!(
            "echo ok > {w}/inside && echo yes; echo x > {o}/outside 2>/dev/null && echo leaked; cat {h}/.ssh/id_test 2>/dev/null; cat {h}/.netrc 2>/dev/null; true",
            w = work.path().display(),
            o = outside.path().display(),
            h = home.path().display()
        );
        let out = s
            .command(
                Path::new("/bin/sh"),
                &["-c".into(), script],
                &Containment {
                    work: work.path(),
                    home: &run_home,
                    network: false,
                },
            )
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.starts_with("yes"), "{text}");
        assert!(!text.contains("leaked"), "{text}");
        assert!(!text.contains("not a real key"), "{text}");
        assert!(!text.contains("machine example"), "{text}");
    }

    #[cfg(windows)]
    #[test]
    fn job_object_contains_the_child() {
        let mut child = Command::new("cmd")
            .args(["/C", "ping -n 2 127.0.0.1 >NUL"])
            .spawn()
            .unwrap();
        let guard = JobSandbox.contain(ProcessRef::of(&child));
        assert!(guard.is_some());
        drop(guard);
        // Closing the job ends the child.
        let status = child.wait().unwrap();
        assert!(!status.success() || status.code().is_some());
    }

    #[test]
    fn best_available_never_claims_more_than_it_has() {
        let level = best_available().level();
        #[cfg(windows)]
        assert_eq!(level, SandboxLevel::WindowsJob);
        #[cfg(target_os = "linux")]
        assert!(matches!(
            level,
            SandboxLevel::Basic | SandboxLevel::LinuxBubblewrap | SandboxLevel::LinuxLandlock
        ));
        #[cfg(target_os = "macos")]
        assert!(matches!(
            level,
            SandboxLevel::Basic | SandboxLevel::MacosSeatbelt
        ));
    }
}

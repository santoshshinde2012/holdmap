//! Linux Landlock, used when bubblewrap isn't available. It needs no setuid helper or user
//! namespaces. The rule set is built in the parent; the child only calls
//! `prctl(PR_SET_NO_NEW_PRIVS)` and `landlock_restrict_self` between fork and exec, which are
//! plain system calls (no allocation), so this is safe in a multithreaded process.
//!
//! The policy:
//! - Read and execute everything except the credential paths in [`PRIVATE_DIRS`] and
//!   [`PRIVATE_FILES`]. Landlock only allows paths, so holdmap grants every sibling along the
//!   way instead.
//! - Write only inside the run's private folder, plus a few harmless devices.
//! - Without declared network, TCP connect and bind are denied (Landlock ABI 4, Linux 6.7+).
//!   Other traffic such as UDP is not blocked.
//! - Since ABI 6, it can't signal other processes or reach their abstract UNIX sockets.
//!   ptrace and `/proc/<pid>/environ` of processes outside the sandbox are always denied.

use crate::runner::{Containment, Sandbox, SandboxLevel};
use crate::sandbox::{PRIVATE_DIRS, PRIVATE_FILES};
use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};

const EXECUTE: u64 = 1 << 0;
const WRITE_FILE: u64 = 1 << 1;
const READ_FILE: u64 = 1 << 2;
const READ_DIR: u64 = 1 << 3;
const REFER: u64 = 1 << 13;
const TRUNCATE: u64 = 1 << 14;
const IOCTL_DEV: u64 = 1 << 15;
/// Rights that apply to a regular file (rules on files must use only these).
const FILE_RIGHTS: u64 = EXECUTE | WRITE_FILE | READ_FILE | TRUNCATE | IOCTL_DEV;
const NET_BIND_TCP: u64 = 1 << 0;
const NET_CONNECT_TCP: u64 = 1 << 1;
const SCOPE_ABSTRACT_UNIX_SOCKET: u64 = 1 << 0;
const SCOPE_SIGNAL: u64 = 1 << 1;
const RULE_PATH_BENEATH: libc::c_int = 1;
const CREATE_RULESET_VERSION: libc::c_uint = 1 << 0;
/// Devices a run may write to.
const WRITABLE_DEVICES: &[&str] = &["/dev/null", "/dev/zero", "/dev/full", "/dev/tty"];

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
    scoped: u64,
}

#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

/// The kernel's Landlock ABI version (0 when unavailable), queried once.
pub fn abi() -> u32 {
    static ABI: OnceLock<u32> = OnceLock::new();
    *ABI.get_or_init(|| {
        // SAFETY: the documented version query: null attributes, size 0, the version flag.
        let v = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<RulesetAttr>(),
                0usize,
                CREATE_RULESET_VERSION,
            )
        };
        u32::try_from(v).unwrap_or(0)
    })
}

/// Whether this kernel lets Landlock deny TCP (ABI 4+).
pub fn blocks_tcp() -> bool {
    abi() >= 4
}

fn handled_fs(abi: u32) -> u64 {
    let mut fs = (1 << 13) - 1;
    if abi >= 2 {
        fs |= REFER;
    }
    if abi >= 3 {
        fs |= TRUNCATE;
    }
    if abi >= 5 {
        fs |= IOCTL_DEV;
    }
    fs
}

/// A Landlock rule set ready to be applied to a child.
struct Ruleset {
    fd: OwnedFd,
}

impl Ruleset {
    fn new(abi: u32, network: bool) -> io::Result<Self> {
        let attr = RulesetAttr {
            handled_access_fs: handled_fs(abi),
            handled_access_net: if abi >= 4 && !network {
                NET_BIND_TCP | NET_CONNECT_TCP
            } else {
                0
            },
            scoped: if abi >= 6 {
                SCOPE_ABSTRACT_UNIX_SOCKET | SCOPE_SIGNAL
            } else {
                0
            },
        };
        // Older kernels reject fields they don't know, so pass only the ones they have.
        let size = match abi {
            0..=3 => 8,
            4 | 5 => 16,
            _ => std::mem::size_of::<RulesetAttr>(),
        };
        // SAFETY: `attr` is a valid, initialised struct of at least `size` bytes.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &attr as *const RulesetAttr,
                size,
                0u32,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the kernel returned a new descriptor that nothing else owns.
        Ok(Self {
            fd: unsafe { OwnedFd::from_raw_fd(fd as RawFd) },
        })
    }

    /// Allow `access` beneath `path`. Paths that vanish, symlinks and unopenable entries are
    /// skipped: nothing is granted for them, which is the safe side.
    fn allow(&self, path: &Path, access: u64) {
        let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
            return;
        };
        // SAFETY: `c` is a valid C string; O_PATH|O_NOFOLLOW never follows a symlink.
        let raw = unsafe {
            libc::open(
                c.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if raw < 0 {
            return;
        }
        // SAFETY: `raw` was just opened and is owned here.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let attr = PathBeneathAttr {
            allowed_access: access,
            parent_fd: fd.as_raw_fd(),
        };
        // SAFETY: both descriptors are valid and `attr` is the packed kernel layout.
        unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                self.fd.as_raw_fd(),
                RULE_PATH_BENEATH,
                &attr as *const PathBeneathAttr,
                0u32,
            );
        }
    }
}

/// Grant `rights` on everything under `dir` except the `hidden` paths, descending only into
/// directories that contain a hidden path.
fn grant_except(rules: &Ruleset, dir: &Path, hidden: &[PathBuf], rights: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if hidden.iter().any(|h| h == &path) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() && hidden.iter().any(|h| h.starts_with(&path)) {
            grant_except(rules, &path, hidden, rights);
        } else if meta.is_dir() {
            rules.allow(&path, rights);
        } else {
            rules.allow(&path, rights & FILE_RIGHTS);
        }
    }
}

/// Landlock on Linux: see the module documentation for exactly what it enforces.
#[derive(Debug, Clone)]
pub struct Landlock {
    abi: u32,
    home: Option<PathBuf>,
}

impl Landlock {
    /// Landlock if this kernel has it and a sandboxed child really starts, checked once.
    pub fn detect() -> Option<Self> {
        static WORKS: OnceLock<bool> = OnceLock::new();
        let abi = abi();
        let works = *WORKS.get_or_init(|| {
            let Some(t) = ["/usr/bin/true", "/bin/true"]
                .iter()
                .find(|p| Path::new(p).is_file())
            else {
                return false;
            };
            let work = std::env::temp_dir();
            let probe = Self { abi, home: None };
            abi >= 1
                && probe
                    .command(
                        Path::new(t),
                        &[],
                        &Containment {
                            work: &work,
                            home: &work,
                            network: false,
                        },
                    )
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success())
        });
        works.then(|| Self::new(abi, std::env::home_dir().filter(|h| h.is_absolute())))
    }

    /// Explicit ABI and home (tests).
    pub fn new(abi: u32, home: Option<PathBuf>) -> Self {
        Self { abi, home }
    }

    fn hidden(&self) -> Vec<PathBuf> {
        let Some(home) = &self.home else {
            return Vec::new();
        };
        let home = std::fs::canonicalize(home).unwrap_or_else(|_| home.clone());
        PRIVATE_DIRS
            .iter()
            .chain(PRIVATE_FILES)
            .map(|p| home.join(p))
            .collect()
    }

    fn ruleset(&self, work: &Path, network: bool) -> io::Result<Ruleset> {
        let rules = Ruleset::new(self.abi, network)?;
        let all = handled_fs(self.abi);
        let read = EXECUTE | READ_FILE | READ_DIR;
        grant_except(&rules, Path::new("/"), &self.hidden(), read);
        rules.allow(work, all);
        for d in WRITABLE_DEVICES {
            rules.allow(Path::new(d), (READ_FILE | WRITE_FILE | IOCTL_DEV) & all);
        }
        Ok(rules)
    }
}

impl Sandbox for Landlock {
    fn level(&self) -> SandboxLevel {
        SandboxLevel::LinuxLandlock
    }

    fn command(&self, binary: &Path, args: &[String], c: &Containment<'_>) -> Command {
        let mut cmd = Command::new(binary);
        cmd.args(args);
        self.prepare(&mut cmd, c.home, c.network);
        let rules = self.ruleset(c.work, c.network).map(Arc::new);
        // SAFETY: the closure makes only async-signal-safe system calls on a descriptor the
        // parent already created; it allocates nothing.
        unsafe {
            cmd.pre_exec(move || {
                let rules = rules.as_ref().map_err(|e| io::Error::from(e.kind()))?;
                if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::syscall(libc::SYS_landlock_restrict_self, rules.fd.as_raw_fd(), 0u32) != 0
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handled_rights_follow_the_abi() {
        assert_eq!(handled_fs(1), (1 << 13) - 1);
        assert_eq!(handled_fs(3) & (REFER | TRUNCATE), REFER | TRUNCATE);
        assert_eq!(handled_fs(4) & IOCTL_DEV, 0);
        assert_ne!(handled_fs(5) & IOCTL_DEV, 0);
    }

    /// Runs only where Landlock works (Linux 5.13+ without a seccomp block).
    #[test]
    fn landlock_enforces_writes_credentials_and_tcp() {
        let Some(l) = Landlock::detect() else {
            eprintln!("landlock not usable here; skipped");
            return;
        };
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
        std::fs::write(home.path().join(".ssh/id_test"), "not a real key").unwrap();
        std::fs::write(home.path().join(".netrc"), "machine example").unwrap();
        std::fs::write(home.path().join("notes.txt"), "readable").unwrap();
        let l = Landlock::new(l.abi, Some(home.path().into()));
        let work = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let run_home = work.path().join("home");
        // bash's /dev/tcp gives a TCP connect without extra tools; port 9 is "discard".
        let script = format!(
            "echo ok > {w}/inside && echo yes; echo x > {o}/outside 2>/dev/null && echo leaked; cat {h}/.ssh/id_test 2>/dev/null; cat {h}/.netrc 2>/dev/null; cat {h}/notes.txt; echo; (exec 3<>/dev/tcp/127.0.0.1/9) 2>&1 | grep -q -i 'permission denied' && echo tcp-denied; true",
            w = work.path().display(),
            o = outside.path().display(),
            h = home.path().display()
        );
        let bash = ["/usr/bin/bash", "/bin/bash"]
            .into_iter()
            .find(|p| Path::new(p).is_file())
            .unwrap_or("/bin/sh");
        let out = l
            .command(
                Path::new(bash),
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
        assert!(work.path().join("inside").exists());
        assert!(!text.contains("leaked"), "{text}");
        assert!(!outside.path().join("outside").exists());
        assert!(!text.contains("not a real key"), "{text}");
        assert!(!text.contains("machine example"), "{text}");
        assert!(
            text.contains("readable"),
            "other files stay readable: {text}"
        );
        if blocks_tcp() && bash != "/bin/sh" {
            assert!(text.contains("tcp-denied"), "{text}");
        }
    }
}

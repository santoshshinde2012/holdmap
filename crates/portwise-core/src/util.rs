//! Small helpers: running external commands with a timeout, time helpers.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Output of a finished command.
#[derive(Debug, Clone)]
pub struct CmdOutput {
    /// True when the command exited with status 0.
    pub success: bool,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
}

/// Run a command, killing it if it exceeds `timeout`. Returns `None` if it couldn't start or
/// timed out. Never inherits stdin, so it can't block on prompts.
pub fn run_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<CmdOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                let mut err = String::new();
                if let Some(mut s) = child.stdout.take() {
                    let _ = s.read_to_string(&mut out);
                }
                if let Some(mut s) = child.stderr.take() {
                    let _ = s.read_to_string(&mut err);
                }
                return Some(CmdOutput {
                    success: status.success(),
                    stdout: out,
                    stderr: err,
                });
            }
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Current Unix time in milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Current Unix time in seconds.
pub fn now_secs() -> u64 {
    now_ms() / 1000
}

/// "3h 12m", "45s", "2d 4h".
pub fn human_duration(secs: u64) -> String {
    let (d, h, m, s) = (
        secs / 86_400,
        (secs % 86_400) / 3600,
        (secs % 3600) / 60,
        secs % 60,
    );
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

/// "12.3 MB".
pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// The URL a listener is most likely served on (`http://localhost:PORT`, https for 443/8443).
pub fn local_url(port: u16) -> String {
    let scheme = if matches!(port, 443 | 8443) {
        "https"
    } else {
        "http"
    };
    format!("{scheme}://localhost:{port}")
}

/// Open `url` in the default browser without blocking (open / xdg-open / start).
pub fn open_url(url: &str) -> std::io::Result<()> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "only http(s) URLs can be opened",
        ));
    }
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// `HH:MM:SS` in local time (UTC with a `Z` suffix where local time isn't available).
pub fn local_hms(epoch_secs: u64) -> String {
    #[cfg(unix)]
    {
        let t = epoch_secs as libc::time_t;
        // SAFETY: localtime_r writes into the provided, properly sized `tm`.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if !unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec);
        }
    }
    let s = epoch_secs % 86_400;
    format!("{:02}:{:02}:{:02}Z", s / 3600, (s / 60) % 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_and_bytes() {
        assert_eq!(human_duration(45), "45s");
        assert_eq!(human_duration(125), "2m 5s");
        assert_eq!(human_duration(3 * 3600 + 720), "3h 12m");
        assert_eq!(human_duration(2 * 86_400 + 4 * 3600), "2d 4h");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(50 * 1024 * 1024), "50.0 MB");
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_slow_commands() {
        let t = Instant::now();
        assert!(run_with_timeout("sleep", &["5"], Duration::from_millis(100)).is_none());
        assert!(t.elapsed() < Duration::from_secs(2));
        let ok = run_with_timeout("echo", &["hi"], Duration::from_secs(2)).unwrap();
        assert!(ok.success);
        assert_eq!(ok.stdout.trim(), "hi");
    }

    #[test]
    fn urls_and_clock() {
        assert_eq!(local_url(3000), "http://localhost:3000");
        assert_eq!(local_url(8443), "https://localhost:8443");
        assert!(open_url("file:///etc/passwd").is_err());
        let t = local_hms(0);
        assert!(t.len() >= 8 && t.as_bytes()[2] == b':', "{t}");
    }
}

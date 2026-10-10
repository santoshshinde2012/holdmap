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
/// timed out or produced more than 4 MiB on either stream. The timeout covers inherited
/// pipes too, even if the direct child exits first. Never inherits stdin.
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
    let deadline = Instant::now() + timeout;
    let stdout = child.stdout.take()?;
    let stderr = child.stderr.take()?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        for fd in [stdout.as_raw_fd(), stderr.as_raw_fd()] {
            // SAFETY: these are the live pipe descriptors owned by stdout/stderr.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    #[cfg(windows)]
    let handles = {
        use std::os::windows::io::AsRawHandle;
        (
            stdout.as_raw_handle() as usize,
            stderr.as_raw_handle() as usize,
        )
    };
    #[cfg(not(windows))]
    let handles = (0, 0);
    // Drain both pipes while waiting: a child that writes more than the pipe buffer (64 KB on
    // Linux) would otherwise block forever and only end at the timeout.
    let read = |mut pipe: Box<dyn Read + Send>, pipe_handle: usize| {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut chunk = [0; 8192];
            #[cfg(not(windows))]
            let _ = pipe_handle;
            loop {
                if Instant::now() >= deadline {
                    let _ = sender.send(None);
                    return;
                }
                #[cfg(windows)]
                {
                    let mut available = 0;
                    // SAFETY: pipe owns this handle for the closure lifetime; only query the
                    // byte count, avoiding a blocking read while a descendant holds it open.
                    if unsafe {
                        windows_sys::Win32::System::Pipes::PeekNamedPipe(
                            pipe_handle as windows_sys::Win32::Foundation::HANDLE,
                            std::ptr::null_mut(),
                            0,
                            std::ptr::null_mut(),
                            &mut available,
                            std::ptr::null_mut(),
                        )
                    } == 0
                    {
                        use windows_sys::Win32::Foundation::{
                            GetLastError, ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED,
                        };
                        // SAFETY: read this thread's error immediately after the failed call.
                        let error = unsafe { GetLastError() };
                        let output = matches!(error, ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED)
                            .then(|| String::from_utf8_lossy(&buf).into_owned());
                        let _ = sender.send(output);
                        return;
                    }
                    if available == 0 {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                }
                match pipe.read(&mut chunk) {
                    Ok(0) => {
                        let _ = sender.send(Some(String::from_utf8_lossy(&buf).into_owned()));
                        return;
                    }
                    Ok(n) if buf.len() + n <= 4 * 1024 * 1024 => buf.extend_from_slice(&chunk[..n]),
                    Ok(_) => {
                        let _ = sender.send(None);
                        return;
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => {
                        let _ = sender.send(None);
                        return;
                    }
                }
            }
        });
        receiver
    };
    let out = read(Box::new(stdout), handles.0);
    let err = read(Box::new(stderr), handles.1);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let status = status?;
    Some(CmdOutput {
        success: status.success(),
        stdout: out
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok()??,
        stderr: err
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok()??,
    })
}

/// Whether `HOLDMAP_TRACE` asks for diagnostics on `topic` (`scan`, …): a comma-separated
/// list of topics, or `1` / `all` for everything.
pub fn tracing(topic: &str) -> bool {
    let var = std::env::var("HOLDMAP_TRACE")
        .or_else(|_| std::env::var("PORTWISE_TRACE"))
        .ok();
    trace_wants(var.as_deref(), topic)
}

fn trace_wants(var: Option<&str>, topic: &str) -> bool {
    var.is_some_and(|v| {
        v.split(',')
            .map(str::trim)
            .any(|t| t == topic || t == "1" || t == "all")
    })
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

/// `1 process`, `2 processes`: a count with the right noun form.
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
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

/// Text from outside holdmap (process names, command lines, HTTP titles and headers,
/// container names) made safe to print: control characters such as ESC can't reach the
/// terminal, so a process or a local web page can't rewrite the screen or the title bar.
/// Whitespace controls become spaces; anything else becomes `�`.
pub fn printable(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.chars().any(char::is_control) {
        return std::borrow::Cow::Borrowed(s);
    }
    std::borrow::Cow::Owned(
        s.chars()
            .map(|c| match c {
                '\t' | '\n' | '\r' => ' ',
                c if c.is_control() => '\u{FFFD}',
                c => c,
            })
            .collect(),
    )
}

/// Validate an exact systemd unit argument, preserving canonical `\xNN` name escapes while
/// refusing option prefixes and glob patterns that could select unrelated units.
pub(crate) fn exact_systemd_unit(unit: &str, suffix: &str) -> bool {
    if unit.len() > 255
        || unit.starts_with('-')
        || !unit.ends_with(suffix)
        || unit.len() <= suffix.len()
    {
        return false;
    }
    let bytes = unit.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            if bytes.get(index + 1) != Some(&b'x')
                || !bytes.get(index + 2).is_some_and(u8::is_ascii_hexdigit)
                || !bytes.get(index + 3).is_some_and(u8::is_ascii_hexdigit)
            {
                return false;
            }
            index += 4;
        } else if bytes[index].is_ascii_alphanumeric() || b"_-.:@".contains(&bytes[index]) {
            index += 1;
        } else {
            return false;
        }
    }
    true
}

/// Create (or truncate) a regular single-link file without following a final link. Unix
/// checks ownership and enforces 0600; Windows inherits the parent directory's ACL.
pub fn create_private(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        o.mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        let f = o.open(path)?;
        check_owned_file(&f)?;
        // `mode` only applies to new files; tighten one left over from an older version.
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.set_len(0)?;
        Ok(f)
    }
    #[cfg(not(unix))]
    {
        if std::fs::symlink_metadata(path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink())
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private files must be regular files, not links",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            o.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let f = o.open(path)?;
        #[cfg(windows)]
        check_windows_private_file(&f)?;
        if !f.metadata()?.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private files must be regular files",
            ));
        }
        f.set_len(0)?;
        Ok(f)
    }
}

#[cfg(windows)]
pub(crate) fn check_windows_private_file(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT,
    };
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: File owns a live handle, and info points to writable storage of the required size.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the successful call initialized info.
    let info = unsafe { info.assume_init() };
    if info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private files must be regular files without reparse points or extra links",
        ));
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn check_owned_file(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    // SAFETY: geteuid has no preconditions.
    let me = unsafe { libc::geteuid() };
    if !metadata.is_file() || metadata.uid() != me || metadata.nlink() != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private files must be owned regular files with no extra links",
        ));
    }
    Ok(())
}

/// Open an existing owner-controlled state file without following its final symlink.
/// Unix files writable by other accounts or linked to another name are refused.
#[cfg(not(unix))]
pub(crate) fn open_private_read(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    {
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "state files must be regular files, not links",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(
                windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
            );
        }
        let file = options.open(path)?;
        #[cfg(windows)]
        check_windows_private_file(&file)?;
        Ok(file)
    }
}

/// Create a directory (and parents) that only its owner can enter (0700 on Unix for the
/// directories this creates; an existing owned directory is tightened too). Links and
/// directories owned by another Unix account are refused.
pub fn create_private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir)?;
    let metadata = std::fs::symlink_metadata(dir)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private state directories must not be links",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private state directories must not be reparse points",
            ));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
            .open(dir)?;
        // SAFETY: geteuid has no preconditions.
        if file.metadata()?.uid() != unsafe { libc::geteuid() } {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private state directory belongs to another account",
            ));
        }
        file.set_permissions(std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Is `url` a plain http(s) URL that is safe to hand to the OS opener? No whitespace, quotes
/// or shell/`cmd.exe` metacharacters, so it can't be read as an option or a second command.
pub fn is_safe_url(url: &str) -> bool {
    (url.starts_with("http://") || url.starts_with("https://"))
        && url.len() <= 2048
        && url
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~:/?#[]@!$&'()*+,;=%".contains(&b))
        && !url.contains(['\'', '!', '$', '(', ')', ';'])
}

/// Open `url` in the default browser without blocking (open / xdg-open / the URL handler).
/// Only [`is_safe_url`] URLs are accepted, and the URL is always passed as one argument.
pub fn open_url(url: &str) -> std::io::Result<()> {
    if !is_safe_url(url) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "only plain http(s) URLs can be opened",
        ));
    }
    let cmd = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    } else if cfg!(windows) {
        // Not `cmd /C start`: cmd.exe would parse `&`, `|` and `^` in the URL.
        let mut c = std::process::Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    spawn_quiet(cmd)
}

/// Show a directory in the file manager (Finder / Explorer / the desktop's handler).
pub fn reveal(path: &std::path::Path) -> std::io::Result<()> {
    if !path.is_absolute() || !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} doesn't exist", path.display()),
        ));
    }
    let cmd = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(path);
        c
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("explorer");
        c.arg(path);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
        c
    };
    spawn_quiet(cmd)
}

/// Editors holdmap knows how to open a folder in, in order of preference: (command, macOS app).
const EDITORS: [(&str, &str); 5] = [
    ("cursor", "Cursor"),
    ("code", "Visual Studio Code"),
    ("zed", "Zed"),
    ("subl", "Sublime Text"),
    ("idea", "IntelliJ IDEA"),
];

/// Open a project folder in the user's editor: `$HOLDMAP_EDITOR`, then the first known
/// editor installed (Cursor, VS Code, Zed, Sublime, IntelliJ). Returns the editor's name.
/// The folder is passed as a single argument; nothing goes through a shell.
pub fn open_in_editor(dir: &std::path::Path) -> std::io::Result<String> {
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} isn't a folder", dir.display()),
        ));
    }
    let custom = std::env::var("HOLDMAP_EDITOR")
        .or_else(|_| std::env::var("PORTWISE_EDITOR"))
        .ok()
        .filter(|s| !s.trim().is_empty());
    if let Some(bin) = custom.as_deref().and_then(|e| which(e.trim())) {
        let mut c = std::process::Command::new(&bin);
        c.arg(dir);
        spawn_quiet(c)?;
        return Ok(bin
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default());
    }
    for (bin, app) in EDITORS {
        if let Some(path) = which(bin) {
            let mut c = std::process::Command::new(path);
            c.arg(dir);
            spawn_quiet(c)?;
            return Ok(app.to_string());
        }
        // GUI apps on macOS start without the shell's PATH: look for the app bundle.
        if cfg!(target_os = "macos") {
            let installed = [
                std::path::PathBuf::from("/Applications"),
                home_dir().join("Applications"),
            ]
            .iter()
            .any(|d| d.join(format!("{app}.app")).exists());
            if installed {
                let mut c = std::process::Command::new("open");
                c.args(["-a", app]).arg(dir);
                spawn_quiet(c)?;
                return Ok(app.to_string());
            }
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no editor found (set HOLDMAP_EDITOR, or install VS Code, Cursor or Zed)",
    ))
}

fn home_dir() -> std::path::PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}

/// Find an executable on PATH (plus the usual Homebrew / local bins a GUI app doesn't see).
/// Only bare names or absolute paths; never a relative path from the current directory.
pub fn which(name: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(name);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    if name.contains(['/', '\\']) || name.is_empty() {
        return None;
    }
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    dirs.extend(
        ["/opt/homebrew/bin", "/usr/local/bin"]
            .iter()
            .map(std::path::PathBuf::from),
    );
    dirs.push(home_dir().join(".local/bin"));
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ""]
    } else {
        &[""]
    };
    dirs.iter().filter(|d| d.is_absolute()).find_map(|d| {
        exts.iter()
            .map(|e| d.join(format!("{name}{e}")))
            .find(|c| c.is_file())
    })
}

fn spawn_quiet(mut cmd: std::process::Command) -> std::io::Result<()> {
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// `YYYY-MM-DD HH:MM` in local time (UTC with a `Z` suffix where local time isn't available).
pub fn local_datetime(epoch_secs: u64) -> String {
    #[cfg(unix)]
    {
        #[allow(deprecated)]
        let t = epoch_secs as libc::time_t;
        // SAFETY: localtime_r writes into the provided, properly sized `tm`.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if !unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday,
                tm.tm_hour,
                tm.tm_min
            );
        }
    }
    // Civil date from days since the epoch (Howard Hinnant's algorithm).
    let days = (epoch_secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let s = epoch_secs % 86_400;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}Z", s / 3600, (s / 60) % 60)
}

/// `HH:MM:SS` in local time (UTC with a `Z` suffix where local time isn't available).
pub fn local_hms(epoch_secs: u64) -> String {
    #[cfg(unix)]
    {
        // musl deprecates the `time_t` alias (it is moving to 64 bits); it is still exactly the
        // type `localtime_r` takes on every libc.
        #[allow(deprecated)]
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

/// A [`std::process::Command`] that runs `line` through the platform shell (`sh -c` on Unix,
/// `cmd /C` on Windows), so stack files can use pipes, `&&` and quoting.
pub fn shell_command(line: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", line]);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", line]);
        c
    }
}

/// Spawn `cmd` detached from the caller: stdin closed, stdout and stderr written to `log`
/// (created or truncated), and on Unix in its own process group so it survives the launching
/// terminal's Ctrl-C and holdmap exiting.
pub fn spawn_detached(
    cmd: &mut std::process::Command,
    log: &std::path::Path,
) -> std::io::Result<std::process::Child> {
    use std::process::Stdio;
    if let Some(dir) = log.parent() {
        create_private_dir(dir)?;
    }
    // Logs can hold whatever the program prints (tokens, connection strings): owner-only.
    let out = create_private(log)?;
    let err = out.try_clone()?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    let child = cmd.spawn()?;
    DETACHED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(child.id());
    Ok(child)
}

/// PIDs this process started on the user's behalf with [`spawn_detached`] (a restarted dev
/// server). They're the user's services, not part of holdmap, so self-protection skips them.
static DETACHED: std::sync::Mutex<std::collections::BTreeSet<u32>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Was `pid` started detached by this process (see [`spawn_detached`])?
pub fn started_detached(pid: u32) -> bool {
    DETACHED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&pid)
}

#[cfg(test)]
mod tests {
    #[test]
    fn trace_topics() {
        assert!(!trace_wants(None, "scan"));
        assert!(!trace_wants(Some("0"), "scan"));
        assert!(trace_wants(Some("scan"), "scan"));
        assert!(trace_wants(Some("http, scan"), "scan"));
        assert!(trace_wants(Some("1"), "scan"));
        assert!(!trace_wants(Some("http"), "scan"));
    }

    #[cfg(unix)]
    #[test]
    fn run_with_timeout_reads_output_larger_than_a_pipe_buffer() {
        let t = std::time::Instant::now();
        let out = super::run_with_timeout(
            "sh",
            &["-c", "head -c 300000 /dev/zero | tr '\\0' x"],
            std::time::Duration::from_secs(10),
        )
        .expect("finishes well before the timeout");
        assert_eq!(out.stdout.len(), 300_000);
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn run_with_timeout_kills_a_hung_command() {
        let t = std::time::Instant::now();
        // `sh` forks `sleep`, which keeps the pipes open after `sh` is killed.
        let out = super::run_with_timeout(
            "sh",
            &["-c", "sleep 5; echo done"],
            std::time::Duration::from_millis(200),
        );
        assert!(out.is_none());
        assert!(t.elapsed() < std::time::Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn security_timeout_includes_pipes_held_by_an_exited_commands_child() {
        let started = std::time::Instant::now();
        let output = super::run_with_timeout(
            "sh",
            &["-c", "sleep 2 & exit 0"],
            std::time::Duration::from_millis(150),
        );
        assert!(output.is_none(), "incomplete inherited pipes must time out");
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[test]
    fn security_command_capture_rejects_unbounded_output() {
        let output = super::run_with_timeout(
            "sh",
            &["-c", "head -c 5000000 /dev/zero"],
            std::time::Duration::from_secs(3),
        );
        assert!(output.is_none());
    }

    #[test]
    fn counts_use_the_right_noun() {
        assert_eq!(super::count(1, "process", "processes"), "1 process");
        assert_eq!(super::count(0, "process", "processes"), "0 processes");
        assert_eq!(super::count(3, "process", "processes"), "3 processes");
    }

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

#[cfg(test)]
mod safety_tests {
    use super::*;

    #[test]
    fn only_plain_http_urls_open() {
        assert!(is_safe_url("http://localhost:3000"));
        assert!(is_safe_url("http://localhost:3000/a/b?x=1&y=2#top"));
        assert!(is_safe_url("https://127.0.0.1:8443/"));
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "-a Calculator",
            "http://x\" & calc",
            "http://x/ & calc",
            "http://x/|calc",
            "http://x/^calc",
            "http://x/\ncalc",
            "http://x/$(id)",
            "http://x/`id`",
            "http://x/\"",
            "http://x/<a>",
        ] {
            assert!(!is_safe_url(bad), "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn private_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f");
        std::fs::write(&p, "old").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        create_private(&p).unwrap();
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let sub = d.path().join("a/b");
        create_private_dir(&sub).unwrap();
        assert_eq!(
            std::fs::metadata(&sub).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[cfg(unix)]
    #[test]
    fn security_private_files_reject_symlinks_and_hardlinks_before_truncation() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let victim = d.path().join("victim");
        std::fs::write(&victim, "fixture-private-content").unwrap();
        for hardlink in [false, true] {
            let link = d.path().join(if hardlink { "hard" } else { "symbolic" });
            if hardlink {
                std::fs::hard_link(&victim, &link).unwrap();
            } else {
                symlink(&victim, &link).unwrap();
            }
            assert!(create_private(&link).is_err());
            assert_eq!(
                std::fs::read_to_string(&victim).unwrap(),
                "fixture-private-content"
            );
        }
    }

    #[test]
    fn printable_strips_terminal_escapes() {
        assert_eq!(printable("vite"), "vite");
        assert_eq!(
            printable("a\x1b]0;pwned\x07b"),
            "a\u{FFFD}]0;pwned\u{FFFD}b"
        );
        assert_eq!(printable("x\ny\tz"), "x y z");
        assert_eq!(printable("\u{9b}31m"), "\u{FFFD}31m");
    }

    #[test]
    fn which_ignores_relative_paths() {
        assert!(which("./evil").is_none());
        assert!(which("../evil").is_none());
        assert!(which("").is_none());
    }
}
#[test]
fn security_private_file_rejects_hardlink_before_truncating() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    std::fs::write(&target, "fixture must survive").unwrap();
    std::fs::hard_link(&target, &link).unwrap();
    assert!(create_private(&link).is_err());
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "fixture must survive"
    );
}

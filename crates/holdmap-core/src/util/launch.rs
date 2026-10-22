//! Handing URLs, folders and editors to the OS.

#[allow(unused_imports)]
use super::*;
#[allow(unused_imports)]
use std::io::Read;
#[allow(unused_imports)]
use std::process::{Command, Stdio};
#[allow(unused_imports)]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

pub(crate) fn validate_directory(path: &std::path::Path) -> std::io::Result<()> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} isn't an existing folder", path.display()),
        ));
    }
    Ok(())
}

pub(crate) fn file_manager_command(
    path: &std::path::Path,
) -> std::io::Result<std::process::Command> {
    validate_directory(path)?;
    let cmd = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.args(["-a", "Finder"]).arg(path);
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
    Ok(cmd)
}

/// Open an existing directory in Finder / Explorer / the desktop's file manager.
/// The directory is passed as one argument; files and relative paths are rejected.
pub fn reveal(path: &std::path::Path) -> std::io::Result<()> {
    spawn_quiet(file_manager_command(path)?)
}

/// Editors holdmap knows how to open a folder in, in order of preference: (command, macOS app).
pub(crate) const EDITORS: [(&str, &str); 5] = [
    ("cursor", "Cursor"),
    ("code", "Visual Studio Code"),
    ("zed", "Zed"),
    ("subl", "Sublime Text"),
    ("idea", "IntelliJ IDEA"),
];

/// Open a project folder in the user's editor: `$HOLDMAP_EDITOR`, then the first known
/// editor installed (Cursor, VS Code, Zed, Sublime, IntelliJ). Returns the editor's name.
/// An explicit editor that cannot be found returns an error instead of selecting another app.
/// The folder is passed as a single argument; no shell command is assembled here.
pub fn open_in_editor(dir: &std::path::Path) -> std::io::Result<String> {
    let custom = std::env::var("HOLDMAP_EDITOR")
        .or_else(|_| std::env::var("PORTWISE_EDITOR"))
        .ok()
        .filter(|s| !s.trim().is_empty());
    let (cmd, name) = editor_command(dir, custom.as_deref())?;
    spawn_quiet(cmd)?;
    Ok(name)
}

pub(crate) fn editor_command(
    dir: &std::path::Path,
    custom: Option<&str>,
) -> std::io::Result<(std::process::Command, String)> {
    validate_directory(dir)?;
    if let Some(editor) = custom.map(str::trim).filter(|s| !s.is_empty()) {
        let bin = which(editor).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Configured editor `{editor}` was not found. Set HOLDMAP_EDITOR to an installed editor executable."),
            )
        })?;
        let mut c = std::process::Command::new(&bin);
        c.arg(dir);
        let name = bin
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Ok((c, name));
    }
    for (bin, app) in EDITORS {
        if let Some(path) = which(bin) {
            let mut c = std::process::Command::new(path);
            c.arg(dir);
            return Ok((c, app.to_string()));
        }
        // GUI apps on macOS start without the shell's PATH: look for the app bundle.
        if cfg!(target_os = "macos") {
            let installed = [
                std::path::PathBuf::from("/Applications"),
                home_dir().join("Applications"),
            ]
            .iter()
            .any(|d| d.join(format!("{app}.app")).is_dir());
            if installed {
                let mut c = std::process::Command::new("open");
                c.args(["-a", app]).arg(dir);
                return Ok((c, app.to_string()));
            }
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no editor found (set HOLDMAP_EDITOR, or install VS Code, Cursor or Zed)",
    ))
}

pub(crate) fn home_dir() -> std::path::PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}

pub(crate) fn spawn_quiet(mut cmd: std::process::Command) -> std::io::Result<()> {
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    let stdio_inheritance = WindowsStdioInheritance::exclude()?;
    let mut child = cmd.spawn()?;
    #[cfg(windows)]
    drop(stdio_inheritance);
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

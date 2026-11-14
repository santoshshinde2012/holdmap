//! One private log file for desktop failures (`<config dir>/logs/holdmap.log`).
//!
//! Every command error and panic is written here after secrets are masked
//! ([`holdmap_core::redact::line`]) and the home folder is shortened to `~`. The same tidy
//! text is what the window shows, so a toast or a shared log never carries a token or the
//! full home path. The file is owner-only and is rotated once it passes [`MAX_BYTES`].

use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

/// Size at which the log moves to `holdmap.1.log` and starts again.
pub(crate) const MAX_BYTES: u64 = 512 * 1024;

static LOG: OnceLock<PathBuf> = OnceLock::new();
static WRITE: Mutex<()> = Mutex::new(());

/// Log to `<dir>/logs/holdmap.log` from now on, and log panics there too.
pub(crate) fn init(dir: &Path) {
    if LOG.set(dir.join("logs").join("holdmap.log")).is_err() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let place = info
            .location()
            .map(|l| format!(" at {}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".into());
        record("panic", &format!("{message}{place}"));
        previous(info);
    }));
}

/// Mask secrets and shorten the home folder to `~`.
pub(crate) fn tidy(message: &str) -> String {
    // Word by word, not as a shell line: prose such as "couldn't" must not open a quote.
    let masked = message
        .lines()
        .map(|line| {
            let words: Vec<String> = line.split_whitespace().map(String::from).collect();
            holdmap_core::redact::args(&words).join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    match std::env::home_dir().map(|h| h.display().to_string()) {
        Some(home) if home.len() > 1 => masked.replace(&home, "~"),
        _ => masked,
    }
}

/// Write `message` (tidied) to the log and return the tidy text for the window.
pub(crate) fn user_error(context: &str, message: impl AsRef<str>) -> String {
    let text = tidy(message.as_ref());
    write_line(context, &text);
    text
}

/// Write one tidied line to the log. Logging never fails a command: errors are dropped.
pub(crate) fn record(context: &str, message: &str) {
    write_line(context, &tidy(message));
}

fn write_line(context: &str, text: &str) {
    let Some(path) = LOG.get() else { return };
    let _ = append(path, context, text);
}

fn append(path: &Path, context: &str, text: &str) -> std::io::Result<()> {
    let _guard = WRITE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(dir) = path.parent() {
        holdmap_core::util::create_private_dir(dir)?;
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(path, path.with_file_name("holdmap.1.log"));
    }
    let mut file = holdmap_core::util::open_private(path)?;
    file.seek(SeekFrom::End(0))?;
    let at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    // One line per event: a newline in the message can't forge another entry.
    let line = format!("{at_ms} {context}: {}\n", text.replace(['\n', '\r'], " "));
    file.write_all(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidy_masks_secrets_and_the_home_folder() {
        let home = std::env::home_dir().unwrap().display().to_string();
        let text = tidy(&format!(
            "couldn't start node server.js --api-key=hunter2 in {home}/code/app"
        ));
        assert!(!text.contains("hunter2"), "{text}");
        assert!(!text.contains(&home), "{text}");
        assert!(text.contains("~/code/app"), "{text}");
        let pair = tidy("psql --password hunter2 failed\nGITHUB_TOKEN=ghp_x node");
        assert!(
            !pair.contains("hunter2") && !pair.contains("ghp_x"),
            "{pair}"
        );
        assert_eq!(pair.lines().count(), 2);
    }

    #[test]
    fn lines_are_appended_privately_one_per_event_and_rotated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logs").join("holdmap.log");
        append(&path, "scan", "first\nforged 0 panic: no").unwrap();
        append(&path, "stop", "second").unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(body.lines().count(), 2, "{body}");
        assert!(body.contains("scan: first forged 0 panic: no"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::write(&path, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        append(&path, "scan", "after rotation").unwrap();
        assert!(path.with_file_name("holdmap.1.log").exists());
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .starts_with(|c: char| c.is_ascii_digit()));
    }
}

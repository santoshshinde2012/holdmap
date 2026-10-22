//! Small helpers, split by responsibility and re-exported here so callers keep using `util::…`.

#[cfg(test)]
use std::time::{Duration, Instant};

mod launch;
mod private_fs;
mod process;
mod text;
mod trace;

pub use launch::*;
pub use private_fs::*;
pub use process::*;
pub use text::*;
pub use trace::*;

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

#[cfg(test)]
mod folder_tests {
    use super::{editor_command, file_manager_command, open_in_editor, reveal};
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn folder_actions_reject_files_missing_paths_and_relative_paths_before_launch() {
        let fixture = tempfile::tempdir().unwrap();
        let file = fixture.path().join("source.rs");
        std::fs::write(&file, "fixture").unwrap();
        let missing = fixture.path().join("removed-project");
        for path in [
            file.as_path(),
            missing.as_path(),
            Path::new("relative/project"),
        ] {
            assert!(reveal(path).is_err());
            assert!(open_in_editor(path).is_err());
        }
    }

    #[test]
    fn file_manager_opens_directory_contents_and_keeps_the_path_one_argument() {
        let fixture = tempfile::tempdir().unwrap();
        let path = fixture.path().join("project space & literal-name");
        std::fs::create_dir(&path).unwrap();
        // Inspect the launch plan only; tests never launch Finder, Explorer or an editor.
        let cmd = file_manager_command(&path).unwrap();
        let args = cmd.get_args().collect::<Vec<_>>();
        assert_eq!(args.last(), Some(&path.as_os_str()));
        if cfg!(target_os = "macos") {
            assert_eq!(cmd.get_program(), OsStr::new("open"));
            assert_eq!(
                args,
                [OsStr::new("-a"), OsStr::new("Finder"), path.as_os_str()]
            );
        } else {
            assert_eq!(args, [path.as_os_str()]);
            assert_eq!(
                cmd.get_program(),
                OsStr::new(if cfg!(windows) {
                    "explorer"
                } else {
                    "xdg-open"
                })
            );
        }
    }

    #[test]
    fn configured_editor_takes_priority_and_receives_one_folder_argument() {
        let fixture = tempfile::tempdir().unwrap();
        let editor = fixture.path().join("preferred editor");
        std::fs::write(&editor, "not executed").unwrap();
        let path = fixture.path().join("code project & literal-name");
        std::fs::create_dir(&path).unwrap();
        let (cmd, name) = editor_command(&path, editor.to_str()).unwrap();
        assert_eq!(cmd.get_program(), editor.as_os_str());
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), [path.as_os_str()]);
        assert_eq!(name, "preferred editor");
    }

    #[test]
    fn unavailable_configured_editor_does_not_fall_back_to_a_different_app() {
        let fixture = tempfile::tempdir().unwrap();
        let missing = fixture.path().join("missing-editor");
        let error = editor_command(fixture.path(), missing.to_str()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(error.to_string().contains("Configured editor"));
        assert!(error.to_string().contains("HOLDMAP_EDITOR"));
    }

    #[test]
    fn editor_preference_is_an_executable_not_a_shell_command() {
        let fixture = tempfile::tempdir().unwrap();
        for preference in [
            "./editor",
            "../editor",
            "code --wait",
            "cursor & other-command",
        ] {
            assert!(editor_command(fixture.path(), Some(preference)).is_err());
        }
    }
}

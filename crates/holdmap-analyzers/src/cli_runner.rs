//! Runs a local command-line analyzer once per skill: copy the folder into a private staging
//! area, start the pinned binary with a cleared environment, stop it at the time limit and
//! read at most the declared amount of output.

use crate::input::{AnalyzerInput, InputSubject};
use crate::manifest::{AnalyzerManifest, EMPTY_FILE, OUTPUT, SUBJECT_DIR};
use crate::runner::{
    AnalyzerRunner, CancelFlag, Containment, RawOutput, RunContext, RunError, Sandbox, CANCELLED,
};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Limits for the staged copy of one subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagingLimits {
    /// Most files copied.
    pub max_files: usize,
    /// Most bytes copied.
    pub max_bytes: u64,
}

impl Default for StagingLimits {
    fn default() -> Self {
        Self {
            max_files: 5_000,
            max_bytes: 64 * 1024 * 1024,
        }
    }
}

/// The CLI runner.
pub struct CliRunner {
    sandbox: Box<dyn Sandbox>,
    limits: StagingLimits,
}

impl CliRunner {
    /// A runner using `sandbox`.
    pub fn new(sandbox: Box<dyn Sandbox>) -> Self {
        Self {
            sandbox,
            limits: StagingLimits::default(),
        }
    }

    /// Override staging limits (tests).
    pub fn with_limits(mut self, limits: StagingLimits) -> Self {
        self.limits = limits;
        self
    }

    fn run_one(
        &self,
        manifest: &AnalyzerManifest,
        binary: &Path,
        cx: &RunContext,
        subject: &InputSubject,
    ) -> RawOutput {
        let mut out = RawOutput {
            subjects: vec![subject.subject.clone()],
            exit_code: None,
            bytes: Vec::new(),
            notes: Vec::new(),
            failed: true,
            tool: None,
            nothing_to_check: false,
        };
        let Some(dir) = subject.dir.as_deref() else {
            out.notes.push("No folder to analyse.".into());
            return out;
        };
        if cx.cancel.is_cancelled() {
            out.notes.push(CANCELLED.into());
            return out;
        }
        let work = match tempfile::Builder::new()
            .prefix("holdmap-analyzer-")
            .tempdir()
        {
            Ok(w) => w,
            Err(_) => {
                out.notes
                    .push("Could not create a private staging folder.".into());
                return out;
            }
        };
        let staged = work.path().join(STAGED_SUBJECT);
        let home = work.path().join("home");
        let report = work.path().join("report.out");
        match stage(dir, &staged, self.limits) {
            Ok(st) => {
                if st.truncated {
                    out.notes.push(
                        "The folder is larger than the staging limit; only part of it was analysed."
                            .into(),
                    );
                }
                if st.settings_left_out > 0 {
                    out.notes.push(settings_note(st.settings_left_out));
                }
            }
            Err(_) => {
                out.notes.push("The folder could not be read.".into());
                return out;
            }
        }
        let _ = fs::create_dir_all(&home);
        let empty = work.path().join("empty");
        if manifest.runner.args.iter().any(|a| a.contains(EMPTY_FILE))
            && fs::write(&empty, b"").is_err()
        {
            out.notes
                .push("Could not create a private staging folder.".into());
            return out;
        }
        let args: Vec<String> = manifest
            .runner
            .args
            .iter()
            .map(|a| {
                a.replace(SUBJECT_DIR, &staged.to_string_lossy())
                    .replace(OUTPUT, &report.to_string_lossy())
                    .replace(EMPTY_FILE, &empty.to_string_lossy())
            })
            .collect();
        let mut cmd = self.sandbox.command(
            binary,
            &args,
            &Containment {
                work: work.path(),
                home: &home,
                network: manifest.uses_network(),
            },
        );
        cmd.current_dir(work.path());
        finish_env(&mut cmd, manifest, binary, &cx.secrets);
        let cap = (manifest.runner.max_output_mb * 1024 * 1024) as usize;
        let timeout = Duration::from_secs(manifest.runner.timeout_secs);
        let ran = run_until(cmd, timeout, cap, self.sandbox.as_ref(), &cx.cancel);
        let Ok(ran) = ran else {
            out.notes.push("The analyzer could not be started.".into());
            return out;
        };
        out.exit_code = ran.code;
        if ran.uncontained {
            out.notes.push(
                "The operating system sandbox could not be applied; it ran with basic containment."
                    .into(),
            );
        }
        if ran.cancelled {
            out.notes.push(CANCELLED.into());
            return out;
        }
        if ran.timed_out {
            out.notes.push(format!(
                "Stopped after the {}-second time limit.",
                manifest.runner.timeout_secs
            ));
            return out;
        }
        let report_bytes = read_capped(&report, cap);
        let no_report = report_bytes
            .as_ref()
            .is_none_or(|(b, t)| b.is_empty() && !t);
        if no_report
            && ran
                .code
                .is_some_and(|c| manifest.runner.exit_no_findings.contains(&c))
        {
            out.nothing_to_check = true;
            out.failed = false;
            return out;
        }
        let bytes = match report_bytes {
            Some((bytes, truncated)) => {
                if truncated {
                    out.notes.push(
                        "The report was larger than the output limit and was not read.".into(),
                    );
                    return out;
                }
                bytes
            }
            None => ran.stdout,
        };
        if ran.stdout_truncated && bytes.is_empty() {
            out.notes
                .push("The output was larger than the output limit and was not read.".into());
            return out;
        }
        let ok = ran
            .code
            .is_some_and(|c| manifest.runner.exit_ok.contains(&c));
        if !ok {
            out.notes.push(match ran.code {
                Some(code) => format!("The analyzer exited with code {code}."),
                None => "The analyzer was stopped by a signal.".into(),
            });
            return out;
        }
        out.bytes = bytes;
        out.failed = out.bytes.is_empty();
        if out.failed {
            out.notes.push("The analyzer produced no report.".into());
        }
        out
    }
}

impl AnalyzerRunner for CliRunner {
    fn run(
        &self,
        manifest: &AnalyzerManifest,
        input: &AnalyzerInput,
        cx: &RunContext,
    ) -> Result<Vec<RawOutput>, RunError> {
        let binary = cx
            .binary
            .as_deref()
            .ok_or_else(|| RunError::Unavailable(format!("{} is not installed.", manifest.name)))?;
        if input.subjects.is_empty() {
            return Err(RunError::NoSubjects("no skills were found".into()));
        }
        Ok(input
            .subjects
            .iter()
            .map(|s| self.run_one(manifest, binary, cx, s))
            .collect())
    }
}

/// After the sandbox cleared the environment: the keys the approved manifest declares (only
/// for this analyzer's own process) and the program's own folder first on PATH.
pub(crate) fn finish_env(
    cmd: &mut Command,
    manifest: &AnalyzerManifest,
    binary: &Path,
    secrets: &[(String, crate::secrets::Secret)],
) {
    for (name, value) in secrets {
        if manifest.permissions.secrets.contains(name) {
            cmd.env(name, value.expose());
        }
    }
    if let Some(parent) = binary.parent() {
        let path = std::env::join_paths(
            std::iter::once(parent.to_path_buf()).chain(std::env::split_paths(
                &cmd.get_envs()
                    .find(|(k, _)| *k == "PATH")
                    .and_then(|(_, v)| v)
                    .unwrap_or_default(),
            )),
        );
        if let Ok(path) = path {
            cmd.env("PATH", path);
        }
    }
}

/// The staged copy's folder inside a CLI run's private folder, which is also the working
/// directory. A tool that prints paths relative to its working directory (Cisco Skill Scanner)
/// gives `holdmap-subject/…`; the name is unusual so that prefix can be removed safely.
pub const STAGED_SUBJECT: &str = "holdmap-subject";

/// What staging copied.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Staged {
    /// Files copied.
    pub files: usize,
    /// Bytes copied.
    pub bytes: u64,
    /// Whether a limit stopped the copy.
    pub truncated: bool,
    /// Scanner settings files left out (see [`TOOL_SETTINGS`]).
    pub settings_left_out: usize,
}

/// Settings and ignore files that scanners read from the folder they scan. A skill could ship
/// one to switch checks off or hide its own findings, so staging leaves them out for every
/// analyzer, not just the presets. Matched by file name at any depth, ignoring ASCII case
/// (macOS and Windows file systems do).
pub const TOOL_SETTINGS: &[&str] = &[
    ".bandit",                     // Bandit
    "osv-scanner.toml",            // OSV-Scanner
    ".trivyignore",                // Trivy
    ".trivyignore.yaml",           // Trivy
    "trivy.yaml",                  // Trivy
    "trivy-secret.yaml",           // Trivy secret rules
    ".gitleaks.toml",              // Gitleaks
    ".gitleaksignore",             // Gitleaks
    ".semgrepignore",              // Semgrep
    ".secrets.baseline",           // detect-secrets
    ".snyk",                       // Snyk
    ".skillspector-baseline.yaml", // SkillSpector (applied only on request, left out anyway)
];

/// How [`settings_note`] starts.
const SETTINGS_NOTE_START: &str = "Left out ";

/// The note shown when staging left settings files out.
pub fn settings_note(count: usize) -> String {
    format!(
        "{SETTINGS_NOTE_START}{count} scanner settings file{} the folder ships (such as .bandit or \
         osv-scanner.toml), so it can't change how it is checked.",
        if count == 1 { "" } else { "s" }
    )
}

/// Whether a run note means part of the subject went unchecked. Leaving a skill's own scanner
/// settings out is how holdmap makes the check complete, so that note alone doesn't.
pub fn note_limits_coverage(note: &str) -> bool {
    !(note.starts_with(SETTINGS_NOTE_START) && note.contains(" scanner settings file"))
}

fn is_tool_settings(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .is_some_and(|n| TOOL_SETTINGS.iter().any(|t| t.eq_ignore_ascii_case(n)))
}

/// Copy regular files under `from` into `to`. Symlinks, devices and `.git` are skipped, so a
/// link inside a skill can't pull in files outside it, and so are [`TOOL_SETTINGS`] files.
pub fn stage(from: &Path, to: &Path, limits: StagingLimits) -> io::Result<Staged> {
    if !fs::symlink_metadata(from)?.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a folder"));
    }
    let mut staged = Staged::default();
    let mut stack: Vec<(PathBuf, PathBuf)> = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((src, dst)) = stack.pop() {
        fs::create_dir_all(&dst)?;
        let mut entries: Vec<_> = fs::read_dir(&src)?.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let meta = entry.metadata()?; // DirEntry::metadata does not follow symlinks.
            let name = entry.file_name();
            if meta.is_dir() {
                if name != ".git" {
                    stack.push((entry.path(), dst.join(&name)));
                }
            } else if meta.is_file() {
                if is_tool_settings(&name) {
                    staged.settings_left_out += 1;
                    continue;
                }
                if staged.files + 1 > limits.max_files
                    || staged.bytes + meta.len() > limits.max_bytes
                {
                    staged.truncated = true;
                    continue;
                }
                fs::copy(entry.path(), dst.join(&name))?;
                staged.files += 1;
                staged.bytes += meta.len();
            }
        }
    }
    Ok(staged)
}

/// Facts about a finished process.
#[derive(Debug, Default)]
pub struct Ran {
    /// Exit code.
    pub code: Option<i32>,
    /// Captured stdout (capped).
    pub stdout: Vec<u8>,
    /// Whether stdout hit the cap.
    pub stdout_truncated: bool,
    /// Whether the time limit stopped it.
    pub timed_out: bool,
    /// Whether the user stopped it.
    pub cancelled: bool,
    /// Whether the OS sandbox this level promises could not be applied.
    pub uncontained: bool,
}

/// Run `cmd`, capturing at most `cap` bytes of stdout, killing its whole process group at
/// `timeout`. stderr is discarded: it is logging, and may echo inputs.
pub fn run_capped(cmd: Command, timeout: Duration, cap: usize) -> io::Result<Ran> {
    run_contained(cmd, timeout, cap, &crate::runner::BasicSandbox)
}

/// [`run_capped`], handing the started child to `sandbox` (a Windows job) for the whole run.
pub fn run_contained(
    cmd: Command,
    timeout: Duration,
    cap: usize,
    sandbox: &dyn Sandbox,
) -> io::Result<Ran> {
    run_until(cmd, timeout, cap, sandbox, &CancelFlag::default())
}

/// [`run_contained`], also killing the process group when `cancel` is set.
pub fn run_until(
    mut cmd: Command,
    timeout: Duration,
    cap: usize,
    sandbox: &dyn Sandbox,
    cancel: &CancelFlag,
) -> io::Result<Ran> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    let guard = sandbox.contain(crate::runner::ProcessRef::of(&child));
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 16 * 1024];
        let mut truncated = false;
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) if buf.len() + n <= cap => buf.extend_from_slice(&chunk[..n]),
                Ok(_) => truncated = true, // keep draining so the child never blocks
            }
        }
        (buf, truncated)
    });
    let started = Instant::now();
    let mut ran = Ran {
        uncontained: guard.is_none() && sandbox.level() == crate::runner::SandboxLevel::WindowsJob,
        ..Ran::default()
    };
    loop {
        if let Some(status) = child.try_wait()? {
            ran.code = status.code();
            break;
        }
        if started.elapsed() >= timeout {
            kill_tree(&mut child);
            ran.timed_out = true;
            break;
        }
        if cancel.is_cancelled() {
            kill_tree(&mut child);
            ran.cancelled = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    // Ends anything the run left behind in its job (Windows).
    drop(guard);
    let (stdout, truncated) = reader.join().unwrap_or_default();
    ran.stdout = stdout;
    ran.stdout_truncated = truncated;
    Ok(ran)
}

fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    if let Ok(pid) = i32::try_from(child.id()) {
        // SAFETY: signals the process group this function created with process_group(0).
        unsafe { libc::kill(-pid, libc::SIGKILL) };
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn read_capped(path: &Path, cap: usize) -> Option<(Vec<u8>, bool)> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    if meta.len() as usize > cap {
        return Some((Vec::new(), true));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(cap as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    Some((bytes, false))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::finding::{Subject, SubjectKind};
    use crate::manifest::AnalyzerManifest;
    use crate::runner::BasicSandbox;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("fake-analyzer");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn manifest(timeout: u64) -> AnalyzerManifest {
        let text = include_str!("../manifests/nvidia.skillspector.toml")
            .replace("timeout_secs = 120", &format!("timeout_secs = {timeout}"))
            .replace("max_output_mb = 8", "max_output_mb = 1");
        AnalyzerManifest::parse(&text).unwrap()
    }

    fn input(dir: &Path) -> AnalyzerInput {
        AnalyzerInput {
            subjects: vec![InputSubject {
                subject: Subject {
                    id: "skill:a".into(),
                    kind: SubjectKind::Skill,
                    name: "a".into(),
                    agents: vec![],
                },
                dir: Some(dir.to_path_buf()),
                package: None,
            }],
            skipped: vec![],
        }
    }

    fn skill(root: &Path) -> PathBuf {
        let skill = root.join("skill");
        fs::create_dir_all(skill.join("scripts")).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: a\n---\n").unwrap();
        fs::write(skill.join("scripts/run.sh"), "echo hi").unwrap();
        skill
    }

    #[test]
    fn runs_on_a_staged_copy_with_a_cleared_environment() {
        let root = tempfile::tempdir().unwrap();
        let skill = skill(root.path());
        // Writes its view of the world as the report: argv dir, env names, and whether HOME is private.
        let bin = script(
            root.path(),
            r#"dir="$2"; out="$7"
               names=$(env | cut -d= -f1 | sort | tr '\n' ',')
               printf '{"dir":"%s","names":"%s","home":"%s","files":"%s"}' "$dir" "$names" "$HOME" "$(ls "$dir" | tr '\n' ',')" > "$out""#,
        );
        std::env::set_var("HOLDMAP_TEST_SECRET", "sk-should-not-pass");
        let out = CliRunner::new(Box::new(BasicSandbox))
            .run(
                &manifest(10),
                &input(&skill),
                &RunContext {
                    binary: Some(bin),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(out.len(), 1);
        assert!(!out[0].failed, "{:?}", out[0].notes);
        let v: serde_json::Value = serde_json::from_slice(&out[0].bytes).unwrap();
        let dir = v["dir"].as_str().unwrap();
        assert_ne!(
            Path::new(dir),
            skill.as_path(),
            "ran on the original folder"
        );
        assert!(dir.contains("holdmap-analyzer-"));
        assert_eq!(v["files"], "SKILL.md,scripts,");
        let names = v["names"].as_str().unwrap();
        assert!(!names.contains("HOLDMAP_TEST_SECRET"), "{names}");
        assert!(
            v["home"].as_str().unwrap().contains("holdmap-analyzer-"),
            "HOME is private"
        );
        assert!(!Path::new(dir).exists(), "staging is removed after the run");
    }

    #[test]
    fn only_declared_keys_reach_the_analyzer() {
        let root = tempfile::tempdir().unwrap();
        let skill = skill(root.path());
        let bin = script(
            root.path(),
            r#"out="$7"; printf '{"declared":"%s","other":"%s"}' "$NVIDIA_API_KEY" "$OTHER_KEY" > "$out""#,
        );
        let mut m = manifest(10);
        m.permissions.secrets = vec!["NVIDIA_API_KEY".into()];
        let cx = RunContext {
            binary: Some(bin),
            secrets: vec![
                (
                    "NVIDIA_API_KEY".into(),
                    crate::secrets::Secret::new("k-123"),
                ),
                ("OTHER_KEY".into(), crate::secrets::Secret::new("nope")),
            ],
            ..RunContext::default()
        };
        let out = CliRunner::new(Box::new(BasicSandbox))
            .run(&m, &input(&skill), &cx)
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out[0].bytes).unwrap();
        assert_eq!(v["declared"], "k-123");
        assert_eq!(v["other"], "", "an undeclared name is never set");
    }

    /// End to end through bubblewrap, where it works: the report is written inside the private
    /// folder, the original skill folder can't be touched and the time limit still applies.
    #[cfg(target_os = "linux")]
    #[test]
    fn bubblewrap_runs_contain_writes_and_still_time_out() {
        let Some(bwrap) = crate::sandbox::Bubblewrap::detect() else {
            eprintln!("bubblewrap not usable here; skipped");
            return;
        };
        let root = tempfile::tempdir().unwrap();
        let skill = skill(root.path());
        let target = skill.join("SKILL.md");
        let bin = script(
            root.path(),
            &format!(
                r#"out="$7"; if echo x >> "{}" 2>/dev/null; then w=leaked; else w=blocked; fi; printf '{{"write":"%s"}}' "$w" > "$out""#,
                target.display()
            ),
        );
        let cx = RunContext {
            binary: Some(bin.clone()),
            ..Default::default()
        };
        let out = CliRunner::new(Box::new(bwrap.clone()))
            .run(&manifest(10), &input(&skill), &cx)
            .unwrap();
        assert!(!out[0].failed, "{:?}", out[0].notes);
        let v: serde_json::Value = serde_json::from_slice(&out[0].bytes).unwrap();
        assert_eq!(v["write"], "blocked");
        assert_eq!(fs::read_to_string(&target).unwrap(), "---\nname: a\n---\n");

        let slow = script(root.path(), "sleep 30");
        let started = Instant::now();
        let out = CliRunner::new(Box::new(bwrap))
            .run(
                &manifest(1),
                &input(&skill),
                &RunContext {
                    binary: Some(slow),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(out[0].failed);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn the_time_limit_kills_the_run() {
        let root = tempfile::tempdir().unwrap();
        let skill = skill(root.path());
        let bin = script(root.path(), "sleep 30");
        let started = Instant::now();
        let out = CliRunner::new(Box::new(BasicSandbox))
            .run(
                &manifest(1),
                &input(&skill),
                &RunContext {
                    binary: Some(bin),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(out[0].failed);
        assert!(out[0].notes[0].contains("time limit"), "{:?}", out[0].notes);
    }

    #[test]
    fn an_exit_no_findings_code_without_a_report_is_a_clean_run() {
        let root = tempfile::tempdir().unwrap();
        let skill = skill(root.path());
        let text = include_str!("../presets/google.osv-scanner.toml");
        let m = AnalyzerManifest::parse(text).unwrap();
        let run = |body: &str| {
            CliRunner::new(Box::new(BasicSandbox))
                .run(
                    &m,
                    &input(&skill),
                    &RunContext {
                        binary: Some(script(root.path(), body)),
                        ..Default::default()
                    },
                )
                .unwrap()
                .remove(0)
        };
        let none = run("echo 'No package sources found'; exit 128");
        assert!(none.nothing_to_check && !none.failed && none.bytes.is_empty());
        // A report written alongside that code is not trusted as "clean".
        let wrote = run(r#"echo '{}' > "${10}"; exit 128"#);
        assert!(!wrote.nothing_to_check && wrote.failed);
        let other = run("exit 2");
        assert!(!other.nothing_to_check && other.failed);
    }

    #[test]
    fn oversized_output_and_bad_exit_codes_are_failures_without_producer_text() {
        let root = tempfile::tempdir().unwrap();
        let skill = skill(root.path());
        let big = script(root.path(), r#"head -c 2000000 /dev/zero > "$7""#);
        let out = CliRunner::new(Box::new(BasicSandbox))
            .run(
                &manifest(10),
                &input(&skill),
                &RunContext {
                    binary: Some(big),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(out[0].failed && out[0].bytes.is_empty());
        let bad = script(root.path(), "echo 'secret-ish stderr' >&2; exit 2");
        let out = CliRunner::new(Box::new(BasicSandbox))
            .run(
                &manifest(10),
                &input(&skill),
                &RunContext {
                    binary: Some(bad),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(out[0].failed);
        assert_eq!(
            out[0].notes,
            vec!["The analyzer exited with code 2.".to_string()]
        );
    }

    /// Every analyzer, not just the presets, gets a copy without the settings files a skill could
    /// ship to hide findings, and `{empty.file}` is a real empty file (a portable /dev/null).
    #[test]
    fn runs_leave_out_tool_settings_and_get_an_empty_file() {
        let root = tempfile::tempdir().unwrap();
        let skill = skill(root.path());
        fs::write(skill.join(".bandit"), "[bandit]\nskips = B602\n").unwrap();
        fs::write(skill.join("scripts/OSV-Scanner.toml"), "[[IgnoredVulns]]\n").unwrap();
        fs::write(skill.join("requirements.txt"), "requests==2.19.0\n").unwrap();
        let m =
            AnalyzerManifest::parse(include_str!("../presets/google.osv-scanner.toml")).unwrap();
        // $6 is {empty.file}, $10 is {output}, $11 is {subject.dir}.
        let bin = script(
            root.path(),
            r#"[ -f "$6" ] && [ ! -s "$6" ] || exit 9
[ -e "${11}/.bandit" ] && exit 8
[ -e "${11}/scripts/OSV-Scanner.toml" ] && exit 7
[ -f "${11}/requirements.txt" ] || exit 6
echo '{"version":"2.1.0","runs":[]}' > "${10}""#,
        );
        let out = CliRunner::new(Box::new(BasicSandbox))
            .run(
                &m,
                &input(&skill),
                &RunContext {
                    binary: Some(bin),
                    ..Default::default()
                },
            )
            .unwrap()
            .remove(0);
        assert!(!out.failed, "{:?} {:?}", out.exit_code, out.notes);
        assert!(
            out.notes
                .iter()
                .any(|n| n.starts_with("Left out 2 scanner settings files")),
            "{:?}",
            out.notes
        );
        assert!(
            skill.join(".bandit").exists(),
            "the original folder is untouched"
        );
    }

    #[test]
    fn staging_skips_symlinks_and_respects_limits() {
        let root = tempfile::tempdir().unwrap();
        let skill = skill(root.path());
        fs::write(root.path().join("outside.txt"), "private").unwrap();
        std::os::unix::fs::symlink(root.path().join("outside.txt"), skill.join("link.txt"))
            .unwrap();
        std::os::unix::fs::symlink(root.path(), skill.join("up")).unwrap();
        let to = root.path().join("staged");
        let s = stage(&skill, &to, StagingLimits::default()).unwrap();
        assert_eq!(s.files, 2);
        assert!(!to.join("link.txt").exists() && !to.join("up").exists());
        let to2 = root.path().join("staged2");
        let s = stage(
            &skill,
            &to2,
            StagingLimits {
                max_files: 1,
                max_bytes: 1 << 20,
            },
        )
        .unwrap();
        assert!(s.truncated && s.files == 1);
    }

    #[test]
    fn a_missing_binary_is_unavailable() {
        let err = CliRunner::new(Box::new(BasicSandbox))
            .run(
                &manifest(10),
                &AnalyzerInput::default(),
                &RunContext::default(),
            )
            .unwrap_err();
        assert!(matches!(err, RunError::Unavailable(_)));
    }

    #[cfg(unix)]
    #[test]
    fn cancelling_kills_the_process_group() {
        let cancel = CancelFlag::default();
        let flag = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            flag.cancel();
        });
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30"]);
        let started = Instant::now();
        let ran = run_until(
            cmd,
            Duration::from_secs(30),
            1024,
            &crate::runner::BasicSandbox,
            &cancel,
        )
        .unwrap();
        assert!(ran.cancelled && !ran.timed_out);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}

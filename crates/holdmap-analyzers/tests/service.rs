//! End-to-end behaviour of the analyzer service with fake programs and a fake OSV.dev:
//! consent gating, pins, normalisation, reviews, and that no secret in producer output or the
//! environment reaches a result, a store file or a network request.

use holdmap_analyzers::catalog;
use holdmap_analyzers::finding::{FindingState, RunStatus, Severity};
use holdmap_analyzers::input::AnalyzerInput;
use holdmap_analyzers::manifest::AnalyzerManifest;
use holdmap_analyzers::osv::{OsvRunner, OsvTransport};
use holdmap_analyzers::parse::{OutputParser, SarifParser};
use holdmap_analyzers::probe::{BinaryInfo, BinaryProbe};
use holdmap_analyzers::runner::{AnalyzerRunner, RawOutput, RunContext, RunError};
use holdmap_analyzers::service::ServiceError;
use holdmap_analyzers::{AnalyzerService, AnalyzerState, FileStore, Stores};
use holdmap_core::agents::AgentsReport;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SKILLSPECTOR: &str = "nvidia.skillspector";
const OSV: &str = "holdmap.osv";
const REPORT: &str = include_str!("fixtures/skillspector_report.json");

/// Fragments that appear only in unmapped producer fields, the environment or config values.
const SECRETS: &[&str] = &[
    "ghp_LEAK",
    "AKIAIOSFODNN7LEAKED1",
    "AWS_SECRET_ACCESS_KEY",
    "sk-live-LEAK",
    "BEGIN OPENSSH",
    "xoxb-LEAK",
    "evil.example",
    "/Users/someone",
    "Ignore all previous",
    "GITHUB_TOKEN",
    "super-secret-env-value",
];

/// Lets a test keep a handle on a fake it hands to the service.
struct Shared<T>(Arc<T>);

impl<T> std::ops::Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

#[derive(Default)]
struct FakeProbe {
    info: Mutex<Option<BinaryInfo>>,
    version: Mutex<Option<String>>,
}

impl BinaryProbe for Shared<FakeProbe> {
    fn locate(&self, _: &AnalyzerManifest) -> Option<BinaryInfo> {
        self.info.lock().unwrap().clone()
    }
    fn version(&self, _: &AnalyzerManifest, _: &Path) -> Option<String> {
        self.version.lock().unwrap().clone()
    }
}

/// Answers every subject with the SkillSpector fixture (a fresh `finding_id` each run).
#[derive(Default)]
struct FakeCli {
    runs: Mutex<usize>,
    /// Answer like OSV-Scanner on a skill without packages: exit 128, no report.
    nothing: Mutex<bool>,
}

impl AnalyzerRunner for Shared<FakeCli> {
    fn run(
        &self,
        _: &AnalyzerManifest,
        input: &AnalyzerInput,
        cx: &RunContext,
    ) -> Result<Vec<RawOutput>, RunError> {
        assert!(cx.binary.is_some(), "runs only the pinned binary");
        let mut runs = self.runs.lock().unwrap();
        *runs += 1;
        let nothing = *self.nothing.lock().unwrap();
        let bytes = REPORT
            .replace("8f6b8c1e", &format!("run{runs:05}"))
            .into_bytes();
        Ok(input
            .subjects
            .iter()
            .map(|s| RawOutput {
                tool: None,
                nothing_to_check: nothing,
                subjects: vec![s.subject.clone()],
                exit_code: Some(if nothing { 128 } else { 0 }),
                bytes: if nothing { vec![] } else { bytes.clone() },
                notes: vec![],
                failed: false,
            })
            .collect())
    }
}

#[derive(Default)]
struct FakeOsv {
    sent: Mutex<Vec<String>>,
}

impl OsvTransport for Shared<FakeOsv> {
    fn post(&self, url: &str, body: &[u8]) -> std::io::Result<Vec<u8>> {
        assert_eq!(url, "https://api.osv.dev/v1/querybatch");
        self.sent
            .lock()
            .unwrap()
            .push(String::from_utf8(body.to_vec()).unwrap());
        Ok(json!({"results": [
            {},
            {"vulns": [{"id": "GHSA-aaaa-bbbb-cccc", "modified": "2026-01-01T00:00:00Z"}]}
        ]})
        .to_string()
        .into_bytes())
    }
    fn get(&self, url: &str) -> std::io::Result<Vec<u8>> {
        self.sent.lock().unwrap().push(url.to_string());
        assert_eq!(url, "https://api.osv.dev/v1/vulns/GHSA-aaaa-bbbb-cccc");
        Ok(json!({
            "id": "GHSA-aaaa-bbbb-cccc",
            "summary": "Path traversal in server-filesystem",
            "details": "Long text with ghp_LEAKLEAKLEAK that must not be kept",
            "database_specific": {"severity": "HIGH"},
            "affected": [{"package": {"name": "@modelcontextprotocol/server-filesystem", "ecosystem": "npm"},
                          "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "2025.9.1"}]}]}]
        })
        .to_string()
        .into_bytes())
    }
}

struct Harness {
    _dir: tempfile::TempDir,
    store_dir: PathBuf,
    skill_dir: PathBuf,
    probe: Arc<FakeProbe>,
    cli: Arc<FakeCli>,
    osv: Arc<FakeOsv>,
    service: AnalyzerService,
}

fn harness() -> Harness {
    std::env::set_var("HOLDMAP_TEST_TOKEN", "super-secret-env-value");
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir.path().join("skills/deploy-helper");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: deploy-helper\n---\n",
    )
    .unwrap();
    let store_dir = dir.path().join("analyzers");
    let probe = Arc::new(FakeProbe::default());
    *probe.info.lock().unwrap() = Some(BinaryInfo {
        path: "/opt/bin/skillspector".into(),
        sha256: "a".repeat(64),
    });
    *probe.version.lock().unwrap() = Some("2.13.0".into());
    let cli = Arc::new(FakeCli::default());
    let osv = Arc::new(FakeOsv::default());
    let service = AnalyzerService::with_parts(
        catalog::bundled(),
        Stores::of(FileStore::new(&store_dir)),
        Box::new(Shared(probe.clone())),
        Box::new(Shared(cli.clone())),
        Box::new(OsvRunner::new(Some(Box::new(Shared(osv.clone()))))),
    )
    .with_clock(|| 1_000_000);
    Harness {
        _dir: dir,
        store_dir,
        skill_dir,
        probe,
        cli,
        osv,
        service,
    }
}

fn report(skill_dir: &Path) -> AgentsReport {
    let mcp = |name: &str, runner: &str, package: &str, pinned: bool| {
        json!({
            "id": format!("mcp:{name}"), "name": name, "scope": "user", "source_path": "/home/dev/.claude.json",
            "project_path": null, "transport": "stdio", "enabled": true, "evidence": "observed",
            "launch": {"runner": runner, "package": package, "pinned": pinned},
            "env_names": ["GITHUB_TOKEN"], "header_names": []
        })
    };
    serde_json::from_value(json!({
        "platform": "macos", "taken_at_ms": 1, "limits": [],
        "agents": [{
            "id": "agent:42", "product": "claude-code", "name": "Claude Code", "vendor": "Anthropic", "kind": "cli",
            "pid": 42, "process_name": "claude", "command": "claude --api-key super-secret-env-value", "started_at": 1,
            "parent": null, "memory_bytes": 1, "cpu_percent": 0.0, "processes": [], "more_processes": 0,
            "folders": [], "ports": [], "links": [], "more_links": 0,
            "access": {"user": "dev", "uid": 501, "root": false, "mine": true, "facts": []},
            "capabilities": {
                "inspected": true,
                "skills": [{"id": "skill:user:deploy-helper", "name": "deploy-helper", "scope": "user",
                            "source_path": skill_dir.join("SKILL.md"), "project_path": null, "evidence": "observed"}],
                "mcp_servers": [
                    mcp("filesystem", "npx", "@modelcontextprotocol/server-filesystem@2025.8.21", true),
                    mcp("git", "uvx", "mcp-server-git==0.6.2", true),
                    mcp("latest-thing", "npx", "some-mcp", false)
                ]
            }
        }]
    }))
    .unwrap()
}

fn assert_no_secrets(label: &str, text: &str) {
    // Report only the corpus index: never echo the fake secret or the text that held it.
    for (index, secret) in SECRETS.iter().enumerate() {
        assert!(
            !text.contains(secret),
            "{label} leaked corpus entry #{index}"
        );
    }
}

fn store_corpus(dir: &Path) -> String {
    let mut out = String::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                stack.push(e.path());
            } else {
                out.push_str(&std::fs::read_to_string(e.path()).unwrap());
            }
        }
    }
    out
}

#[test]
fn nothing_runs_until_approved_and_list_never_runs_anything() {
    let h = harness();
    let r = report(&h.skill_dir);
    let states: Vec<_> = h
        .service
        .list()
        .into_iter()
        .map(|a| (a.id, a.state))
        .collect();
    assert_eq!(
        states,
        vec![
            (SKILLSPECTOR.into(), AnalyzerState::NotApproved),
            (OSV.into(), AnalyzerState::NotApproved)
        ]
    );
    assert!(matches!(
        h.service.run(SKILLSPECTOR, &r),
        Err(ServiceError::Run(RunError::NotApproved(_)))
    ));
    assert!(matches!(
        h.service.run(OSV, &r),
        Err(ServiceError::Run(RunError::NotApproved(_)))
    ));
    assert!(h.service.run_changed(&r).is_empty());
    assert_eq!(*h.cli.runs.lock().unwrap(), 0);
    assert!(
        h.osv.sent.lock().unwrap().is_empty(),
        "no network before consent"
    );
    *h.probe.info.lock().unwrap() = None;
    assert_eq!(
        h.service.get(SKILLSPECTOR).unwrap().state,
        AnalyzerState::NotInstalled
    );
}

#[test]
fn skillspector_results_are_normalised_stable_and_leak_nothing() {
    let h = harness();
    let r = report(&h.skill_dir);
    let view = h.service.approve(SKILLSPECTOR).unwrap();
    assert_eq!(view.state, AnalyzerState::Approved);
    assert_eq!(view.version.as_deref(), Some("2.13.0"));
    let first = h.service.run(SKILLSPECTOR, &r).unwrap();
    assert_eq!(
        first.status,
        RunStatus::Partial,
        "SkillSpector said partial"
    );
    assert_eq!(first.coverage, Some(0.875));
    assert_eq!(first.findings.len(), 3);
    let top = &first.findings[0];
    assert_eq!(
        (top.severity, top.rule_id.as_str()),
        (Severity::Critical, "E2")
    );
    assert_eq!(top.title, "Credential file read and sent over the network");
    assert_eq!(top.category.as_deref(), Some("data exfiltration"));
    assert_eq!(top.location.file.as_deref(), Some("scripts/deploy.sh"));
    assert_eq!(top.location.line, Some(14));
    assert_eq!(top.subject, "skill:user:deploy-helper");
    let unknown = first.findings.iter().find(|f| f.rule_id == "Q9").unwrap();
    assert_eq!(
        unknown.severity,
        Severity::Medium,
        "unknown labels are medium, not dropped"
    );
    assert_eq!(unknown.location.file, None, "absolute paths are dropped");
    let details: Vec<_> = first
        .details
        .iter()
        .map(|d| (d.label.as_str(), d.value.as_str()))
        .collect();
    assert_eq!(
        details,
        vec![
            ("Risk score", "72 / 100"),
            ("Recommendation", "DO NOT INSTALL")
        ]
    );
    assert_eq!(first.subjects[0].agents, vec!["claude-code".to_string()]);

    let second = h.service.run(SKILLSPECTOR, &r).unwrap();
    let fps = |r: &holdmap_analyzers::AnalyzerResult| {
        r.findings
            .iter()
            .map(|f| f.fingerprint.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        fps(&first),
        fps(&second),
        "per-run finding ids don't change identity"
    );

    assert_no_secrets("result", &serde_json::to_string(&second).unwrap());
    assert_no_secrets("store", &store_corpus(&h.store_dir));
    assert_no_secrets(
        "insights",
        &serde_json::to_string(&h.service.insights()).unwrap(),
    );
}

#[test]
fn any_pin_change_requires_approval_again() {
    let h = harness();
    let r = report(&h.skill_dir);
    h.service.approve(SKILLSPECTOR).unwrap();
    *h.probe.info.lock().unwrap() = Some(BinaryInfo {
        path: "/opt/bin/skillspector".into(),
        sha256: "b".repeat(64),
    });
    let view = h.service.get(SKILLSPECTOR).unwrap();
    assert_eq!(view.state, AnalyzerState::Changed);
    assert_eq!(view.changes, vec!["its program file changed".to_string()]);
    assert!(matches!(
        h.service.run(SKILLSPECTOR, &r),
        Err(ServiceError::Run(RunError::NotApproved(_)))
    ));
    assert!(h.service.run_changed(&r).is_empty());

    *h.probe.info.lock().unwrap() = Some(BinaryInfo {
        path: "/elsewhere/skillspector".into(),
        sha256: "a".repeat(64),
    });
    assert!(h.service.get(SKILLSPECTOR).unwrap().changes[0].contains("different program"));

    h.service.approve(SKILLSPECTOR).unwrap();
    *h.probe.version.lock().unwrap() = Some("3.0.0".into());
    let err = h.service.run(SKILLSPECTOR, &r).unwrap_err().to_string();
    assert!(err.contains("different version"), "{err}");
    assert_eq!(*h.cli.runs.lock().unwrap(), 0);
}

#[test]
fn a_manifest_change_after_an_upgrade_requires_approval_again() {
    let h = harness();
    h.service.approve(OSV).unwrap();
    let grants = h.store_dir.join("grants.json");
    let text = std::fs::read_to_string(&grants).unwrap();
    let sha = catalog::bundled()
        .into_iter()
        .find(|m| m.id == OSV)
        .unwrap()
        .sha256;
    std::fs::write(&grants, text.replace(&sha, &"0".repeat(64))).unwrap();
    let view = h.service.get(OSV).unwrap();
    assert_eq!(view.state, AnalyzerState::Changed);
    assert!(view.changes[0].contains("manifest"));
}

#[test]
fn osv_with_no_pinned_packages_is_a_clean_empty_run_not_a_failure() {
    let h = harness();
    let mut v = serde_json::to_value(report(&h.skill_dir)).unwrap();
    let servers = v["agents"][0]["capabilities"]["mcp_servers"]
        .as_array_mut()
        .unwrap();
    servers.retain(|s| s["name"] == "latest-thing");
    let mut docker = servers[0].clone();
    docker["name"] = json!("container");
    docker["launch"] = json!({"runner": "docker", "package": "ghcr.io/x/y", "pinned": false});
    servers.push(docker);
    let r: AgentsReport = serde_json::from_value(v).unwrap();
    h.service.approve(OSV).unwrap();
    let result = h
        .service
        .run(OSV, &r)
        .expect("an empty input is not an error");
    assert_eq!(result.status, RunStatus::Complete);
    assert!(result.subjects.is_empty() && result.findings.is_empty());
    assert!(
        h.osv.sent.lock().unwrap().is_empty(),
        "nothing is sent to OSV.dev"
    );
    assert!(
        result.notes[0].starts_with("No pinned packages to check: 2 MCP servers use unpinned"),
        "{:?}",
        result.notes
    );
    assert!(result
        .notes
        .iter()
        .any(|n| n.contains("latest-thing (some-mcp) is not pinned")));
    for n in &result.notes {
        assert!(!n.contains("/home/") && !n.contains(".claude.json"), "{n}");
    }
    let history = h.service.history(OSV).unwrap();
    assert_eq!(history[0].status, RunStatus::Complete);
}

#[test]
fn osv_sends_exactly_the_previewed_payload_and_only_pinned_coordinates() {
    let h = harness();
    let r = report(&h.skill_dir);
    let preview = h.service.preview(OSV, &r).unwrap();
    assert_eq!(preview.payload.len(), 1);
    let body = &preview.payload[0];
    assert_eq!(
        body,
        r#"{"queries":[{"package":{"ecosystem":"PyPI","name":"mcp-server-git"},"version":"0.6.2"},{"package":{"ecosystem":"npm","name":"@modelcontextprotocol/server-filesystem"},"version":"2025.8.21"}]}"#
    );
    assert_eq!(preview.skipped.len(), 1);
    assert!(preview.skipped[0].contains("not pinned"));
    assert!(preview.env.is_empty() && preview.command.is_none());

    h.service.approve(OSV).unwrap();
    let result = h.service.run(OSV, &r).unwrap();
    let sent = h.osv.sent.lock().unwrap().clone();
    assert_eq!(sent[0], *body, "the request is byte-for-byte the preview");
    for request in &sent {
        assert_no_secrets("osv request", request);
        assert!(
            !request.contains("/home/") && !request.contains("claude"),
            "{request}"
        );
    }
    assert_eq!(result.findings.len(), 1);
    let f = &result.findings[0];
    assert_eq!(f.severity, Severity::High);
    assert_eq!(
        f.subject,
        "package:npm/@modelcontextprotocol/server-filesystem@2025.8.21"
    );
    assert_eq!(
        f.fix.as_deref(),
        Some("Pin @modelcontextprotocol/server-filesystem to 2025.9.1 or later.")
    );
    assert_eq!(
        f.refs,
        vec!["https://osv.dev/vulnerability/GHSA-aaaa-bbbb-cccc".to_string()]
    );
    assert_no_secrets("osv result", &serde_json::to_string(&result).unwrap());
    assert!(result.notes.iter().any(|n| n.contains("not pinned")));
}

#[test]
fn reviews_hide_findings_and_snoozes_expire() {
    let h = harness();
    let r = report(&h.skill_dir);
    h.service.approve(SKILLSPECTOR).unwrap();
    let result = h.service.run(SKILLSPECTOR, &r).unwrap();
    assert_eq!(h.service.insights().open.total(), 3);
    h.service
        .review(
            &result.findings[0].fingerprint,
            FindingState::Reviewed,
            None,
        )
        .unwrap();
    h.service
        .review(
            &result.findings[1].fingerprint,
            FindingState::Snoozed,
            Some(10),
        )
        .unwrap();
    let insights = h.service.insights();
    assert_eq!(insights.open.total(), 1);
    assert_eq!(
        insights.results[0].findings[0].state,
        FindingState::Reviewed
    );
    // The clock is fixed, so build a later one to see the snooze lapse (snoozes last an hour at least).
    let later = AnalyzerService::with_parts(
        catalog::bundled(),
        Stores::of(FileStore::new(&h.store_dir)),
        Box::new(Shared(h.probe.clone())),
        Box::new(Shared(h.cli.clone())),
        Box::new(OsvRunner::new(None)),
    )
    .with_clock(|| 1_000_000 + holdmap_analyzers::service::MIN_SNOOZE_MS + 1);
    assert_eq!(later.insights().open.total(), 2);
    h.service
        .review(&result.findings[0].fingerprint, FindingState::Open, None)
        .unwrap();
    assert_eq!(h.service.insights().open.total(), 2);
}

#[test]
fn a_second_run_reports_what_changed_and_history_keeps_the_counts() {
    let h = harness();
    let r = report(&h.skill_dir);
    h.service.approve(SKILLSPECTOR).unwrap();
    let first = h.service.run(SKILLSPECTOR, &r).unwrap();
    assert_eq!(
        first.changes, None,
        "a first run has nothing to compare with"
    );
    let second = h.service.run(SKILLSPECTOR, &r).unwrap();
    let changes = second.changes.expect("compared with the first run");
    assert_eq!((changes.new_total, changes.fixed_total), (0, 0));
    assert_eq!(changes.since_ms, first.run_at_ms);
    let history = h.service.history(SKILLSPECTOR).unwrap();
    assert_eq!((history[0].new, history[0].fixed), (Some(0), Some(0)));
    assert_eq!((history[1].new, history[1].fixed), (None, None));
}

#[test]
fn a_review_keeps_its_reason_capped_and_reopening_drops_it() {
    let h = harness();
    let r = report(&h.skill_dir);
    h.service.approve(SKILLSPECTOR).unwrap();
    let fp = h.service.run(SKILLSPECTOR, &r).unwrap().findings[0]
        .fingerprint
        .clone();
    let long = "Expected: the skill documents this webhook. ".repeat(10);
    h.service
        .review_with_reason(&fp, FindingState::Reviewed, None, Some(&long))
        .unwrap();
    let note = h.service.insights().results[0]
        .findings
        .iter()
        .find(|f| f.fingerprint == fp)
        .and_then(|f| f.note.clone())
        .expect("reason kept");
    assert!(note.starts_with("Expected: the skill documents this webhook."));
    assert!(note.chars().count() <= holdmap_analyzers::finding::MAX_NOTE);
    h.service.review(&fp, FindingState::Open, None).unwrap();
    assert!(h.service.insights().results[0]
        .findings
        .iter()
        .all(|f| f.note.is_none()));
}

#[test]
fn snoozes_are_clamped_to_ninety_days_and_report_when_they_reopen() {
    use holdmap_analyzers::service::{MAX_SNOOZE_MS, MIN_SNOOZE_MS};
    let h = harness();
    let r = report(&h.skill_dir);
    h.service.approve(SKILLSPECTOR).unwrap();
    let fp = h.service.run(SKILLSPECTOR, &r).unwrap().findings[0]
        .fingerprint
        .clone();
    let until = |ms: u64| {
        h.service
            .review_with_reason(&fp, FindingState::Snoozed, Some(ms), Some("later"))
            .unwrap();
        let f = h.service.insights().results[0]
            .findings
            .iter()
            .find(|f| f.fingerprint == fp)
            .cloned()
            .unwrap();
        assert_eq!(f.state, FindingState::Snoozed);
        f.until_ms.expect("snooze end exported") - 1_000_000
    };
    assert_eq!(until(u64::MAX), MAX_SNOOZE_MS);
    assert_eq!(until(1), MIN_SNOOZE_MS);
    assert_eq!(until(86_400_000), 86_400_000);
    h.service.review(&fp, FindingState::Reviewed, None).unwrap();
    assert!(h.service.insights().results[0]
        .findings
        .iter()
        .all(|f| f.until_ms.is_none()));
}

#[test]
fn revoking_turns_it_off_and_forgets_results() {
    let h = harness();
    let r = report(&h.skill_dir);
    h.service.approve(SKILLSPECTOR).unwrap();
    h.service.run(SKILLSPECTOR, &r).unwrap();
    assert!(h.service.revoke(SKILLSPECTOR).unwrap());
    assert!(h.service.insights().results.is_empty());
    assert_eq!(
        h.service.get(SKILLSPECTOR).unwrap().state,
        AnalyzerState::NotApproved
    );
    assert!(h.service.get(SKILLSPECTOR).unwrap().last_run.is_none());
}

#[test]
fn run_changed_runs_only_when_the_subjects_change() {
    let h = harness();
    let r = report(&h.skill_dir);
    h.service.approve(SKILLSPECTOR).unwrap();
    h.service.set_background(SKILLSPECTOR, true).unwrap();
    assert_eq!(h.service.run_changed(&r), vec![SKILLSPECTOR.to_string()]);
    assert!(h.service.run_changed(&r).is_empty());
    std::fs::write(h.skill_dir.join("extra.md"), "new file").unwrap();
    assert_eq!(h.service.run_changed(&r), vec![SKILLSPECTOR.to_string()]);
    assert_eq!(*h.cli.runs.lock().unwrap(), 2);
}

#[test]
fn network_analyzers_run_on_their_own_only_when_allowed() {
    let h = harness();
    let r = report(&h.skill_dir);
    assert!(
        h.service.set_background(SKILLSPECTOR, true).is_err(),
        "needs approval first"
    );
    let view = h.service.approve(SKILLSPECTOR).unwrap();
    assert!(!view.background, "network analyzers start manual-only");
    assert!(h
        .service
        .preview(SKILLSPECTOR, &r)
        .unwrap()
        .schedule
        .starts_with("Only when you click Run now"));
    assert!(h.service.run_changed(&r).is_empty());
    assert_eq!(*h.cli.runs.lock().unwrap(), 0);
    // Run now still works.
    h.service.run(SKILLSPECTOR, &r).unwrap();
    std::fs::write(h.skill_dir.join("extra.md"), "new file").unwrap();
    assert!(
        h.service
            .set_background(SKILLSPECTOR, true)
            .unwrap()
            .background
    );
    assert_eq!(h.service.run_changed(&r), vec![SKILLSPECTOR.to_string()]);
    assert!(h
        .service
        .preview(SKILLSPECTOR, &r)
        .unwrap()
        .schedule
        .contains("you allowed"));
    // A new approval resets it.
    h.service.approve(SKILLSPECTOR).unwrap();
    assert!(!h.service.get(SKILLSPECTOR).unwrap().background);
    assert!(
        !h.service
            .set_background(SKILLSPECTOR, false)
            .unwrap()
            .background
    );
}

#[test]
fn the_preview_shows_the_real_folders_and_a_cleared_environment() {
    let h = harness();
    let r = report(&h.skill_dir);
    let p = h.service.preview(SKILLSPECTOR, &r).unwrap();
    assert_eq!(p.subjects.len(), 1);
    assert_eq!(p.subjects[0].input, h.skill_dir.display().to_string());
    let command = p.command.unwrap().join(" ");
    assert!(
        command.starts_with("skillspector scan <copy of the skill folder> --format json --no-llm"),
        "{command}"
    );
    assert!(p.env.contains(&"PATH".to_string()) && !p.env.iter().any(|e| e.contains("TOKEN")));
    assert!(
        p.sandbox.contains("does not block"),
        "the sheet is honest about the sandbox"
    );
    assert_eq!(
        p.analyzer.permissions.network,
        vec!["api.osv.dev".to_string()]
    );
}

fn sarif(fixture: &str) -> holdmap_analyzers::parse::Parsed {
    let manifest = catalog::bundled().remove(0);
    let raw = RawOutput {
        tool: None,
        nothing_to_check: false,
        subjects: vec![],
        exit_code: Some(0),
        bytes: fixture.as_bytes().to_vec(),
        notes: vec![],
        failed: false,
    };
    SarifParser.parse(&raw, &manifest).unwrap()
}

#[test]
fn sarif_from_semgrep_maps_rules_skips_suppressed_and_never_reads_messages() {
    let parsed = sarif(include_str!("fixtures/semgrep.sarif"));
    assert_eq!(parsed.findings.len(), 2, "the suppressed result is skipped");
    let first = &parsed.findings[0];
    assert_eq!(
        first.severity,
        Some(Severity::High),
        "security-severity 7.5"
    );
    assert_eq!(
        first.title.as_deref(),
        Some("subprocess call with shell=True")
    );
    assert_eq!(
        first.producer_fingerprint.as_deref(),
        Some("matchBasedId/v1:abc123")
    );
    assert_eq!(
        parsed.findings[1].severity,
        Some(Severity::High),
        "rule default level error"
    );
    assert_eq!(parsed.status, Some(RunStatus::Complete));
    assert_no_secrets("sarif", &format!("{parsed:?}"));
}

#[test]
fn sarif_from_trivy_reads_string_scores_and_execution_failures() {
    let parsed = sarif(include_str!("fixtures/trivy.sarif"));
    assert_eq!(parsed.findings[0].severity, Some(Severity::High));
    assert_eq!(
        parsed.findings[0].category.as_deref(),
        Some("vulnerability")
    );
    assert_eq!(parsed.status, Some(RunStatus::Partial));
}

/// Real preset output (trimmed): readable titles and fixes, never the result message.
#[test]
fn sarif_from_presets_gets_readable_titles_and_fixes() {
    let bandit = sarif(include_str!("fixtures/bandit.sarif"));
    let titles: Vec<_> = bandit
        .findings
        .iter()
        .map(|f| (f.rule_id.as_str(), f.title.as_deref().unwrap_or("")))
        .collect();
    assert!(
        titles.contains(&("B105", "Hardcoded password string")),
        "{titles:?}"
    );
    assert!(
        titles.contains(&("B404", "Import subprocess")),
        "{titles:?}"
    );
    assert!(
        titles.contains(&("B602", "Subprocess popen with shell equals true")),
        "{titles:?}"
    );

    let trivy = sarif(include_str!("fixtures/trivy_secret.sarif"));
    assert_eq!(
        trivy.findings[0].title.as_deref(),
        Some("GitHub Personal Access Token")
    );
    assert_eq!(trivy.findings[0].fix, None, "help only restates the match");

    let osv = sarif(include_str!("fixtures/osv_scanner.sarif"));
    let fix = osv.findings[0].fix.as_deref().unwrap();
    assert!(
        fix.starts_with(
            "Your dependency is vulnerable to CVE-2019-10906 (Also published as: PYSEC-2019-217, GHSA-462w-v97r-4m45)."
        ),
        "{fix}"
    );
    assert!(!fix.contains(['*', '<', '`', '#']), "{fix}");
    for parsed in [&bandit, &trivy, &osv] {
        assert!(!format!("{parsed:?}").contains("SECRET-IN-MESSAGE"));
    }
}

#[test]
fn configured_agents_that_are_not_running_are_covered_too() {
    let h = harness();
    let mut r = report(&h.skill_dir);
    let other = h.skill_dir.parent().unwrap().join("review-helper");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("SKILL.md"), "---\nname: review-helper\n---\n").unwrap();
    let mut caps = r.agents[0].capabilities.clone();
    caps.mcp_servers.clear();
    caps.skills[0].id = "skill:user:review-helper".into();
    caps.skills[0].name = "review-helper".into();
    caps.skills[0].source_path = other.join("SKILL.md");
    // The same folder as the running agent's skill is listed once, owned by both.
    let mut shared = r.agents[0].capabilities.skills[0].clone();
    shared.id = "skill:user:deploy-helper".into();
    caps.skills.push(shared);
    r.configured.push(holdmap_core::agents::ConfiguredAgent {
        product: "codex".into(),
        name: "Codex".into(),
        capabilities: caps,
    });
    let p = h.service.preview(SKILLSPECTOR, &r).unwrap();
    assert_eq!(p.subjects.len(), 2);
    let deploy = p
        .subjects
        .iter()
        .find(|s| s.subject.name == "deploy-helper")
        .unwrap();
    assert_eq!(deploy.subject.agents, ["claude-code", "codex"]);
    let review = p
        .subjects
        .iter()
        .find(|s| s.subject.name == "review-helper")
        .unwrap();
    assert_eq!(review.subject.agents, ["codex"]);
}

#[test]
fn api_keys_go_to_the_keychain_and_only_into_that_analyzer_s_run() {
    use holdmap_analyzers::secrets::{MemorySecrets, Secret, SecretStore};
    use std::sync::Arc as StdArc;
    #[derive(Default)]
    struct SharedSecrets(StdArc<MemorySecrets>);
    impl SecretStore for SharedSecrets {
        fn get(
            &self,
            a: &str,
            n: &str,
        ) -> Result<Option<Secret>, holdmap_analyzers::secrets::SecretError> {
            self.0.get(a, n)
        }
        fn set(
            &self,
            a: &str,
            n: &str,
            v: &Secret,
        ) -> Result<(), holdmap_analyzers::secrets::SecretError> {
            self.0.set(a, n, v)
        }
        fn delete(
            &self,
            a: &str,
            n: &str,
        ) -> Result<bool, holdmap_analyzers::secrets::SecretError> {
            self.0.delete(a, n)
        }
        fn label(&self) -> &'static str {
            "test keychain"
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let text = include_str!("../manifests/nvidia.skillspector.toml").replace(
        "spawns_mcp_servers = false",
        "spawns_mcp_servers = false\nsecrets = [\"NVIDIA_API_KEY\"]",
    );
    let mut catalog = catalog::bundled();
    catalog[0] = holdmap_analyzers::AnalyzerManifest::parse(&text).unwrap();
    let keychain = StdArc::new(MemorySecrets::default());
    let store_dir = dir.path().join("analyzers");
    let probe = Arc::new(FakeProbe::default());
    *probe.info.lock().unwrap() = Some(BinaryInfo {
        path: "/opt/bin/skillspector".into(),
        sha256: "a".repeat(64),
    });
    let service = AnalyzerService::with_parts(
        catalog,
        Stores::of(FileStore::new(&store_dir)),
        Box::new(Shared(probe)),
        Box::new(Shared(Arc::new(FakeCli::default()))),
        Box::new(OsvRunner::new(None)),
    )
    .with_secrets(Box::new(SharedSecrets(keychain.clone())));
    let skill_dir = dir.path().join("skills/deploy-helper");
    std::fs::create_dir_all(&skill_dir).unwrap();
    let r = report(&skill_dir);

    let view = service.get(SKILLSPECTOR).unwrap();
    assert_eq!(view.secrets.len(), 1);
    assert!(!view.secrets[0].stored);
    let p = service.preview(SKILLSPECTOR, &r).unwrap();
    assert!(p
        .env
        .iter()
        .any(|e| e == "NVIDIA_API_KEY (not stored; runs without it)"));

    // Undeclared names and bad values are refused; nothing reaches the keychain.
    assert!(service
        .set_secret(SKILLSPECTOR, "OTHER_KEY", &Secret::new("x"))
        .is_err());
    assert!(service
        .set_secret(SKILLSPECTOR, "NVIDIA_API_KEY", &Secret::new("a\nb"))
        .is_err());
    assert!(service
        .set_secret(OSV, "NVIDIA_API_KEY", &Secret::new("x"))
        .is_err());
    assert_eq!(keychain.get(SKILLSPECTOR, "NVIDIA_API_KEY").unwrap(), None);

    service
        .set_secret(
            SKILLSPECTOR,
            "NVIDIA_API_KEY",
            &Secret::new("nvapi-test-value"),
        )
        .unwrap();
    assert!(service.get(SKILLSPECTOR).unwrap().secrets[0].stored);
    let p = service.preview(SKILLSPECTOR, &r).unwrap();
    assert!(p
        .env
        .iter()
        .any(|e| e.starts_with("NVIDIA_API_KEY (from your test keychain")));
    // The value never appears in anything holdmap writes or returns.
    let listed = serde_json::to_string(&(service.list(), &p)).unwrap();
    assert!(!listed.contains("nvapi-test-value"));
    assert!(!store_corpus(&store_dir).contains("nvapi-test-value"));

    assert!(service
        .delete_secret(SKILLSPECTOR, "NVIDIA_API_KEY")
        .unwrap());
    assert!(!service.get(SKILLSPECTOR).unwrap().secrets[0].stored);
    assert_eq!(keychain.get(SKILLSPECTOR, "NVIDIA_API_KEY").unwrap(), None);
}

#[test]
fn a_run_gets_its_stored_keys_and_notes_missing_ones() {
    use holdmap_analyzers::secrets::{MemorySecrets, Secret};
    #[derive(Default)]
    struct Recording(Mutex<Vec<Vec<String>>>);
    impl AnalyzerRunner for Shared<Recording> {
        fn run(
            &self,
            _: &AnalyzerManifest,
            input: &AnalyzerInput,
            cx: &RunContext,
        ) -> Result<Vec<RawOutput>, RunError> {
            self.0 .0.lock().unwrap().push(
                cx.secrets
                    .iter()
                    .map(|(n, v)| format!("{n}={}", v.expose()))
                    .collect(),
            );
            Ok(input
                .subjects
                .iter()
                .map(|s| RawOutput {
                    tool: None,
                    nothing_to_check: false,
                    subjects: vec![s.subject.clone()],
                    exit_code: Some(0),
                    bytes: REPORT.as_bytes().to_vec(),
                    notes: vec![],
                    failed: false,
                })
                .collect())
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let text = include_str!("../manifests/nvidia.skillspector.toml").replace(
        "spawns_mcp_servers = false",
        "spawns_mcp_servers = false\nsecrets = [\"NVIDIA_API_KEY\", \"SECOND_KEY\"]",
    );
    let mut catalog = catalog::bundled();
    catalog[0] = holdmap_analyzers::AnalyzerManifest::parse(&text).unwrap();
    let probe = Arc::new(FakeProbe::default());
    *probe.info.lock().unwrap() = Some(BinaryInfo {
        path: "/opt/bin/skillspector".into(),
        sha256: "a".repeat(64),
    });
    let cli = Arc::new(Recording::default());
    let service = AnalyzerService::with_parts(
        catalog,
        Stores::of(FileStore::new(dir.path().join("analyzers"))),
        Box::new(Shared(probe)),
        Box::new(Shared(cli.clone())),
        Box::new(OsvRunner::new(None)),
    )
    .with_secrets(Box::new(MemorySecrets::default()));
    let skill_dir = dir.path().join("skills/deploy-helper");
    std::fs::create_dir_all(&skill_dir).unwrap();
    service
        .set_secret(SKILLSPECTOR, "NVIDIA_API_KEY", &Secret::new("v1"))
        .unwrap();
    service.approve(SKILLSPECTOR).unwrap();
    let result = service.run(SKILLSPECTOR, &report(&skill_dir)).unwrap();
    assert_eq!(cli.0.lock().unwrap()[0], ["NVIDIA_API_KEY=v1"]);
    assert!(result
        .notes
        .iter()
        .any(|n| n == "SECOND_KEY is not stored, so it ran without it."));
}

#[test]
fn a_preset_is_added_as_the_users_own_analyzer_and_stays_off_until_approved() {
    let h = harness();
    let r = report(&h.skill_dir);
    let presets = h.service.presets();
    assert!(presets.iter().all(|p| !p.added));
    assert!(presets
        .iter()
        .any(|p| p.id == "google.osv-scanner" && p.network == ["api.osv.dev"]));
    let added = h.service.add_preset("google.osv-scanner").unwrap();
    assert_eq!(added.state, AnalyzerState::NotApproved);
    assert_eq!(
        added.source,
        holdmap_analyzers::service::ManifestSource::User
    );
    assert!(h
        .service
        .presets()
        .iter()
        .any(|p| p.id == "google.osv-scanner" && p.added));
    assert!(matches!(
        h.service.run("google.osv-scanner", &r),
        Err(ServiceError::Run(RunError::NotApproved(_)))
    ));
    assert_eq!(
        *h.cli.runs.lock().unwrap(),
        0,
        "nothing ran before approval"
    );
    assert!(h.service.add_preset("nope").is_err());

    h.service.approve("google.osv-scanner").unwrap();
    *h.cli.nothing.lock().unwrap() = true;
    let result = h.service.run("google.osv-scanner", &r).unwrap();
    assert!(result.findings.is_empty());
    assert_ne!(result.status, holdmap_analyzers::finding::RunStatus::Failed);
    assert!(
        result.notes.iter().any(|n| n.contains("nothing to check")),
        "{:?}",
        result.notes
    );

    assert!(h.service.remove_manifest("google.osv-scanner").unwrap());
    assert!(h.service.presets().iter().all(|p| !p.added));
}

#[test]
fn a_run_reports_its_steps_and_keeps_a_history_and_the_skill_folder() {
    use holdmap_analyzers::runner::{ProgressSink, RunStep};
    let h = harness();
    let r = report(&h.skill_dir);
    h.service.approve(SKILLSPECTOR).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = {
        let events = events.clone();
        ProgressSink::new(move |e| events.lock().unwrap().push(e))
    };
    let result = h.service.run_with_progress(SKILLSPECTOR, &r, sink).unwrap();
    let steps: Vec<RunStep> = events.lock().unwrap().iter().map(|e| e.step).collect();
    assert_eq!(steps.first(), Some(&RunStep::Preparing));
    assert_eq!(steps.last(), Some(&RunStep::Collecting));
    // The fake runner doesn't report per subject; the real CLI runner's per-skill steps are
    // covered in cli_runner's tests.
    assert!(events.lock().unwrap().iter().all(|e| e.subject.is_none()));
    let subject = &result.subjects[0];
    assert_eq!(subject.dir.as_deref(), Some(h.skill_dir.to_str().unwrap()));
    assert!(subject.scope.is_some());
    assert_eq!(
        h.service.subject_dir(SKILLSPECTOR, &subject.id).unwrap(),
        h.skill_dir
    );
    assert!(h.service.subject_dir(SKILLSPECTOR, "nope").is_err());
    // A folder swapped for a symlink after the scan is never revealed.
    #[cfg(unix)]
    {
        let moved = h.skill_dir.with_extension("moved");
        std::fs::rename(&h.skill_dir, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &h.skill_dir).unwrap();
        assert!(h.service.subject_dir(SKILLSPECTOR, &subject.id).is_err());
        std::fs::remove_file(&h.skill_dir).unwrap();
        std::fs::rename(&moved, &h.skill_dir).unwrap();
    }
    h.service.run(SKILLSPECTOR, &r).unwrap();
    let history = h.service.history(SKILLSPECTOR).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].subjects, result.subjects.len());
    assert_eq!(
        history[0].counts.values().sum::<usize>(),
        result.findings.len()
    );
    assert!(history[0].run_at_ms >= history[1].run_at_ms);
}

#[test]
fn installs_need_a_holdmap_folder_and_presets_can_be_planned_before_they_are_added() {
    let h = harness();
    assert!(matches!(
        h.service.install_plan("pycqa.bandit"),
        Err(ServiceError::Install(_))
    ));
    let tools = tempfile::tempdir().unwrap();
    let service = h.service.with_tools_dir(
        tools.path(),
        holdmap_analyzers::probe::SystemProbe::with_env(Some("/nonexistent".into()), None),
    );
    let plan = service.install_plan("pycqa.bandit").unwrap();
    assert_eq!(plan.version, "1.9.4");
    assert!(plan.target_dir.starts_with(tools.path().to_str().unwrap()));
    assert_eq!(plan.can_install, plan.blocked.is_none());
    assert!(service.install_plan("unknown.tool").is_err());
}

/// A store written by holdmap 0.11.0 (approval, latest result, one review; no install records,
/// run history, folders or scopes) loads as is, and the next run adds history without losing it.
#[test]
fn a_v0110_store_loads_and_keeps_working() {
    let h = harness();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v0.11.0-store");
    for rel in [
        "grants.json",
        "reviews.json",
        "results/nvidia.skillspector.json",
    ] {
        let to = h.store_dir.join(rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(fixture.join(rel), &to).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(to.parent().unwrap(), std::fs::Permissions::from_mode(0o700))
                .unwrap();
            std::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&h.store_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert_eq!(
        h.service.get(SKILLSPECTOR).unwrap().state,
        AnalyzerState::Approved
    );
    let old = h.service.insights();
    assert_eq!(old.results.len(), 1);
    let result = &old.results[0];
    assert_eq!(result.findings.len(), 3);
    assert!(result
        .subjects
        .iter()
        .all(|s| s.dir.is_none() && s.scope.is_none()));
    assert_eq!(
        result
            .findings
            .iter()
            .filter(|f| f.state == FindingState::Reviewed)
            .count(),
        1,
        "the 0.11.0 review still applies"
    );
    assert!(h.service.history(SKILLSPECTOR).unwrap().is_empty());

    let r = report(&h.skill_dir);
    let again = h.service.run(SKILLSPECTOR, &r).unwrap();
    assert_eq!(again.findings.len(), 3);
    assert_eq!(h.service.history(SKILLSPECTOR).unwrap().len(), 1);
    let now = h.service.insights();
    assert_eq!(
        now.results[0]
            .findings
            .iter()
            .filter(|f| f.state == FindingState::Reviewed)
            .count(),
        1
    );
    assert!(now.results[0]
        .subjects
        .iter()
        .any(|s| s.scope.as_deref() == Some("user")));
}

#[test]
fn a_skill_analyzer_with_no_skills_is_a_clean_empty_run_not_a_failure() {
    let h = harness();
    let mut v = serde_json::to_value(report(&h.skill_dir)).unwrap();
    for list in ["agents", "configured"] {
        if let Some(items) = v.get_mut(list).and_then(|l| l.as_array_mut()) {
            for a in items {
                if let Some(caps) = a.get_mut("capabilities").and_then(|c| c.as_object_mut()) {
                    caps.insert("skills".into(), json!([]));
                }
            }
        }
    }
    let r: AgentsReport = serde_json::from_value(v).unwrap();
    h.service.approve(SKILLSPECTOR).unwrap();
    let result = h
        .service
        .run(SKILLSPECTOR, &r)
        .expect("no skills is not an error");
    assert_eq!(result.status, RunStatus::Complete);
    assert!(result.subjects.is_empty() && result.findings.is_empty());
    assert!(
        result.notes[0].starts_with("No skills to check: "),
        "{:?}",
        result.notes
    );
    assert_eq!(
        h.service.history(SKILLSPECTOR).unwrap()[0].status,
        RunStatus::Complete
    );
}

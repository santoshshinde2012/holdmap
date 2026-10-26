//! Stores written by holdmap 0.11.0 and by the #107 build (before skill trust, risk and
//! scheduled scans) load unchanged: new files are absent and new fields take their defaults.
use holdmap_analyzers::consent::{ResultStore, ReviewStore};
use holdmap_analyzers::finding::FindingState;
use holdmap_analyzers::risk;
use holdmap_analyzers::schedule::ScheduleStore;
use holdmap_analyzers::trust::TrustStore;
use holdmap_analyzers::FileStore;
use serde_json::json;
use std::fs;

fn write(dir: &std::path::Path, name: &str, value: serde_json::Value) {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn old_result() -> serde_json::Value {
    json!({
        "analyzer": "nvidia.skillspector", "analyzer_version": "0.4.0", "run_at_ms": 1_760_000_000_000u64,
        "status": "complete", "coverage": 1.0,
        "subjects": [{ "id": "claude:project:release-check", "kind": "skill", "name": "release-check", "agents": ["claude"], "dir": "/tmp/p/.claude/skills/release-check" }],
        "findings": [{ "fingerprint": "fp-1", "analyzer": "nvidia.skillspector", "rule_id": "P1", "category": null, "severity": "high",
                       "title": "Instruction Override", "subject": "claude:project:release-check", "location": { "file": "SKILL.md", "line": 12 }, "fix": null }]
    })
}

#[test]
fn a_v0_11_0_store_loads_without_trust_or_schedule_files() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "results/nvidia.skillspector.json", old_result());
    // 0.11.0 reviews: a state and, for snoozes, an end; no reason.
    write(
        tmp.path(),
        "reviews.json",
        json!({ "fp-1": { "state": "reviewed", "until_ms": null } }),
    );
    let store = FileStore::new(tmp.path());

    let result = store
        .latest("nvidia.skillspector")
        .expect("old result parses");
    assert_eq!(result.findings[0].until_ms, None);
    assert_eq!(store.reviews()["fp-1"].state, FindingState::Reviewed);
    assert_eq!(store.reviews()["fp-1"].reason, None);
    assert!(!store.schedule().enabled, "scheduled scans stay off");
    assert!(
        store.trust_records().is_empty(),
        "no skill is trusted until you say so"
    );
    let risks = risk::risks(std::slice::from_ref(&result), None);
    assert_eq!(risks.len(), 2, "one agent and one folder");
}

#[test]
fn a_pr_107_store_keeps_reasons_and_snooze_ends() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "results/nvidia.skillspector.json", old_result());
    write(
        tmp.path(),
        "reviews.json",
        json!({ "fp-1": { "state": "snoozed", "until_ms": 1_760_600_000_000u64, "reason": "Fix next sprint" } }),
    );
    let store = FileStore::new(tmp.path());
    let review = &store.reviews()["fp-1"];
    assert_eq!(review.state, FindingState::Snoozed);
    assert_eq!(review.until_ms, Some(1_760_600_000_000));
    assert_eq!(review.reason.as_deref(), Some("Fix next sprint"));
    assert!(!store.schedule().enabled);
    assert!(store.trust_records().is_empty());
}

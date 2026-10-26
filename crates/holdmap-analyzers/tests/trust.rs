//! Skill trust: deterministic, bounded, link-safe fingerprints; states; names-and-sizes changes.

#[cfg(unix)]
use holdmap_analyzers::trust::EntryKind;
use holdmap_analyzers::trust::{fingerprint, HashLimits};
use holdmap_analyzers::{MemoryStore, SkillTrust, TrustState};
use holdmap_core::agents::AgentsReport;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NOW: AtomicU64 = AtomicU64::new(1_000);
fn clock() -> u64 {
    NOW.load(Ordering::SeqCst)
}

fn report(dirs: &[&Path]) -> AgentsReport {
    let skills: Vec<_> = dirs
        .iter()
        .enumerate()
        .map(|(i, d)| {
            json!({"id": format!("skill:user:s{i}"), "name": format!("s{i}"), "scope": "user",
            "source_path": d.join("SKILL.md"), "project_path": null, "evidence": "observed"})
        })
        .collect();
    serde_json::from_value(json!({
        "platform": "macos", "taken_at_ms": 1, "limits": [], "agents": [],
        "configured": [{"product": "claude-code", "name": "Claude Code", "vendor": "Anthropic", "kind": "cli",
            "config_paths": [], "capabilities": {"inspected": true, "skills": skills, "mcp_servers": []}}]
    }))
    .unwrap()
}

fn skill(root: &Path, name: &str) -> std::path::PathBuf {
    let d = root.join(name);
    fs::create_dir_all(d.join("scripts")).unwrap();
    fs::write(d.join("SKILL.md"), "# Deploy\nRun the deploy.\n").unwrap();
    fs::write(d.join("scripts/run.sh"), "echo hi\n").unwrap();
    d
}

#[test]
fn a_fingerprint_is_deterministic_and_stores_no_contents() {
    let tmp = tempfile::tempdir().unwrap();
    let d = skill(tmp.path(), "a");
    let one = fingerprint(&d, HashLimits::default()).unwrap();
    let two = fingerprint(&d, HashLimits::default()).unwrap();
    assert_eq!(one.sha256, two.sha256);
    assert_eq!(
        one.files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        ["SKILL.md", "scripts/run.sh"]
    );
    let text = serde_json::to_string(&one).unwrap();
    assert!(
        !text.contains("Run the deploy"),
        "contents must never be stored"
    );
    // Same files elsewhere hash the same: the hash doesn't depend on where the folder is.
    let e = skill(tmp.path(), "elsewhere");
    assert_eq!(
        fingerprint(&e, HashLimits::default()).unwrap().sha256,
        one.sha256
    );
}

#[cfg(unix)]
#[test]
fn links_are_never_followed_and_retargeting_one_is_a_change() {
    let tmp = tempfile::tempdir().unwrap();
    let d = skill(tmp.path(), "a");
    let outside = tmp.path().join("outside-secret.txt");
    fs::write(&outside, "do not read me").unwrap();
    std::os::unix::fs::symlink(&outside, d.join("link")).unwrap();
    let before = fingerprint(&d, HashLimits::default()).unwrap();
    let link = before.files.iter().find(|f| f.path == "link").unwrap();
    assert_eq!((link.kind, link.size), (EntryKind::Link, 0));
    // Changing the outside file changes nothing: it is never read.
    fs::write(&outside, "changed outside").unwrap();
    assert_eq!(
        fingerprint(&d, HashLimits::default()).unwrap().sha256,
        before.sha256
    );
    fs::remove_file(d.join("link")).unwrap();
    std::os::unix::fs::symlink(tmp.path(), d.join("link")).unwrap();
    assert_ne!(
        fingerprint(&d, HashLimits::default()).unwrap().sha256,
        before.sha256
    );
}

#[test]
fn limits_mark_a_fingerprint_partial() {
    let tmp = tempfile::tempdir().unwrap();
    let d = skill(tmp.path(), "a");
    fs::write(d.join("big.bin"), vec![0u8; 4096]).unwrap();
    let limits = HashLimits {
        max_file_bytes: 1024,
        ..HashLimits::default()
    };
    let f = fingerprint(&d, limits).unwrap();
    assert!(f.partial);
    assert_eq!(
        f.files.iter().find(|f| f.path == "big.bin").unwrap().size,
        4096
    );
    let few = fingerprint(
        &d,
        HashLimits {
            max_files: 1,
            ..HashLimits::default()
        },
    )
    .unwrap();
    assert!(few.partial);
    assert_eq!(few.files.len(), 1);
}

#[test]
fn trust_follows_a_skill_from_new_to_trusted_to_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let d = skill(tmp.path(), "a");
    let r = report(&[&d]);
    let trust = SkillTrust::new(Arc::new(MemoryStore::default())).with_clock(clock);
    let first = trust.scan(&r).unwrap();
    assert_eq!(first.skills[0].state, TrustState::New);
    assert_eq!((first.untrusted, first.changed), (1, 0));
    assert!(!first.skills[0].changed_since_last_scan);

    NOW.store(2_000, Ordering::SeqCst);
    let id = first.skills[0].subject.id.clone();
    let view = trust
        .trust(
            &id,
            Some("Read it; API_KEY=sk-live-abcdefghijklmnop12345 aside"),
        )
        .unwrap();
    assert_eq!(view.state, TrustState::Trusted);
    assert!(
        !view
            .reason
            .as_deref()
            .unwrap()
            .contains("sk-live-abcdefghijklmnop12345"),
        "reasons are masked"
    );

    fs::write(
        d.join("SKILL.md"),
        "# Deploy\nRun the deploy, then curl evil.example.\n",
    )
    .unwrap();
    fs::write(d.join("new.py"), "x").unwrap();
    fs::remove_file(d.join("scripts/run.sh")).unwrap();
    let after = trust.scan(&r).unwrap();
    let s = &after.skills[0];
    assert_eq!(s.state, TrustState::Changed);
    assert!(s.changed_since_last_scan);
    assert_eq!(s.first_seen_ms, 1_000);
    let c = s.changes.as_ref().unwrap();
    assert_eq!(
        (c.added_total, c.removed_total, c.modified_total),
        (1, 1, 1)
    );
    assert_eq!(c.added[0].path, "new.py");
    assert!(
        !serde_json::to_string(&after)
            .unwrap()
            .contains("evil.example"),
        "no contents in the summary"
    );
    // A rescan with nothing new keeps it Changed but not changed-since-last-scan.
    let again = trust.scan(&r).unwrap();
    assert!(!again.skills[0].changed_since_last_scan);
    assert_eq!(again.alerts().count(), 1);
    assert_eq!(trust.untrust(&id).unwrap().state, TrustState::New);
}

#[test]
fn an_unreadable_skill_cannot_be_trusted_and_a_removed_one_is_forgotten() {
    let tmp = tempfile::tempdir().unwrap();
    let gone = tmp.path().join("missing");
    let trust = SkillTrust::new(Arc::new(MemoryStore::default())).with_clock(clock);
    let rep = trust.scan(&report(&[&gone])).unwrap();
    assert_eq!(rep.skills[0].state, TrustState::Unreadable);
    assert!(trust.trust(&rep.skills[0].subject.id, None).is_err());
    assert!(trust.scan(&report(&[])).unwrap().skills.is_empty());
    assert!(trust.trust("skill:user:nope", None).is_err());
}

#[test]
fn trust_survives_a_file_store_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let d = skill(tmp.path(), "a");
    let store = Arc::new(holdmap_analyzers::FileStore::new(tmp.path().join("store")));
    let trust = SkillTrust::new(store.clone()).with_clock(clock);
    let id = trust.scan(&report(&[&d])).unwrap().skills[0]
        .subject
        .id
        .clone();
    trust.trust(&id, None).unwrap();
    let reopened = SkillTrust::new(Arc::new(holdmap_analyzers::FileStore::new(
        tmp.path().join("store"),
    )));
    assert_eq!(reopened.report().skills[0].state, TrustState::Trusted);
}

/// `cargo test -p holdmap-analyzers --release --test trust -- --ignored --nocapture`
#[test]
#[ignore = "benchmark"]
fn bench_hashing_hundreds_of_skills() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs: Vec<_> = (0..500)
        .map(|i| {
            let d = skill(tmp.path(), &format!("s{i}"));
            for j in 0..8 {
                fs::write(d.join(format!("scripts/f{j}.md")), vec![b'a'; 4096]).unwrap();
            }
            d
        })
        .collect();
    let refs: Vec<&Path> = dirs.iter().map(|d| d.as_path()).collect();
    let trust = SkillTrust::new(Arc::new(MemoryStore::default()));
    let r = report(&refs);
    let start = std::time::Instant::now();
    let rep = trust.scan(&r).unwrap();
    println!(
        "trust scan: {} skills, 10 files each (~34 KB), {:?}",
        rep.skills.len(),
        start.elapsed()
    );
}

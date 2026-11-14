//! Memory aggregation, trend rules and location census.

use super::*;
use crate::agents::{
    Agent, AgentCapabilities, AgentKind, AgentProcess, AgentTool, Evidence, ProcessRole, ToolKind,
};
use crate::process::tests::{proc, table};
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct FixedProbe {
    reading: ProcessReading,
    machine: ObservedU64,
}

impl MemoryProbe for FixedProbe {
    fn reading(&self, _: u32) -> ProcessReading {
        self.reading.clone()
    }

    fn machine_bytes(&self) -> ObservedU64 {
        self.machine.clone()
    }
}

fn reading_with(resident: ObservedU64) -> ProcessReading {
    let mut reading = ProcessReading::all_unknown("not exposed by this fixture");
    reading.resident = resident;
    reading
}

fn bare_agent(pid: u32, name: &str, processes: Vec<AgentProcess>, tools: Vec<AgentTool>) -> Agent {
    Agent {
        id: format!("agent:{pid}"),
        product: "cursor".into(),
        name: name.into(),
        vendor: "Anysphere".into(),
        kind: AgentKind::Ide,
        pid,
        process_name: name.into(),
        command: name.into(),
        started_at: 50,
        parent: None,
        memory_bytes: processes.iter().map(|p| p.memory_bytes).sum(),
        cpu_percent: 0.0,
        cpu_unknown: None,
        processes: processes.clone(),
        more_processes: 0,
        process_ids: processes.iter().map(|p| p.pid).collect(),
        tools,
        more_tools: 0,
        capabilities: AgentCapabilities::default(),
        folders: Vec::new(),
        more_folders: 0,
        ports: Vec::new(),
        links: Vec::new(),
        more_links: 0,
        access: crate::agents::AgentAccess {
            user: Some("dev".into()),
            uid: Some(1000),
            root: false,
            mine: true,
            facts: Vec::new(),
        },
    }
}

fn process(pid: u32, name: &str, role: ProcessRole, memory: u64) -> AgentProcess {
    AgentProcess {
        pid,
        ppid: Some(1),
        name: name.into(),
        command: name.into(),
        role,
        cwd: None,
        memory_bytes: memory,
        cpu_percent: 0.0,
        cpu_unknown: None,
    }
}

#[test]
fn partial_counters_stay_unknown_and_the_headline_matches_the_process_table() {
    let agent_proc = process(10, "Cursor", ProcessRole::Agent, 100);
    let server = process(11, "rust-analyzer", ProcessRole::Helper, 300 * 1024 * 1024);
    let mut rows = vec![
        proc(10, 1, "Cursor", &["Cursor"]),
        proc(11, 10, "rust-analyzer", &["rust-analyzer"]),
    ];
    rows[0].memory_bytes = 100;
    rows[1].memory_bytes = 300 * 1024 * 1024;
    let table = table(rows, 1);
    let probe = FixedProbe {
        reading: reading_with(ObservedU64::unknown("smaps_rollup was unreadable")),
        machine: ObservedU64::observed(1_000_000_000),
    };
    let agent = bare_agent(10, "Cursor", vec![agent_proc, server], Vec::new());
    let report = agent_memory(&agent, &table, &probe, 9);
    assert_eq!(report.headline_bytes, 100 + 300 * 1024 * 1024);
    assert_eq!(report.resident.get(), None);
    assert!(report.resident.reason().contains("smaps"));
    assert_eq!(
        report.share_ppm.get(),
        Some(report.headline_bytes * 1_000_000 / 1_000_000_000)
    );
    let language = report
        .processes
        .iter()
        .find(|row| row.pid == 11)
        .expect("language server");
    assert_eq!(language.role, MemoryRole::LanguageServer);
    assert_eq!(language.evidence, Evidence::Inferred);
    assert!(language.dominant);
    assert_eq!(report.peak_resident.get(), None);
    assert!(report.peak_resident.reason().contains("not simultaneous"));
}

#[test]
fn an_mcp_child_is_grouped_separately_and_a_zero_machine_is_not_a_share() {
    let main = process(4, "claude", ProcessRole::Agent, 10);
    let mcp = process(5, "node", ProcessRole::Child, 20);
    let tool = AgentTool {
        pid: 5,
        ppid: Some(4),
        name: "filesystem".into(),
        kind: ToolKind::McpServer,
        command: "npx server".into(),
        cwd: None,
        evidence: Evidence::Inferred,
        ports: Vec::new(),
        memory_bytes: 20,
        cpu_percent: 0.0,
        cpu_unknown: None,
    };
    let mut rows = vec![
        proc(4, 1, "claude", &["claude"]),
        proc(5, 4, "node", &["node"]),
    ];
    rows[0].memory_bytes = 10;
    rows[1].memory_bytes = 20;
    let report = agent_memory(
        &bare_agent(4, "Claude Code", vec![main, mcp], vec![tool]),
        &table(rows, 1),
        &FixedProbe {
            reading: reading_with(ObservedU64::observed(1)),
            machine: ObservedU64::observed(0),
        },
        1,
    );
    assert_eq!(
        report
            .processes
            .iter()
            .find(|row| row.pid == 5)
            .unwrap()
            .role,
        MemoryRole::McpServer
    );
    assert!(report
        .groups
        .iter()
        .any(|group| group.role == MemoryRole::McpServer));
    assert_eq!(report.resident.get(), Some(2));
    assert_eq!(report.share_ppm.get(), None);
    assert!(report.share_ppm.reason().contains("zero"));
}

#[test]
fn trend_needs_two_samples_and_uses_the_published_band() {
    let one = MemoryTrend::from_samples(vec![MemorySample {
        at_ms: 1,
        bytes: 10,
    }]);
    assert_eq!(one.direction, TrendDirection::Unknown);
    let flat = MemoryTrend::from_samples(vec![
        MemorySample {
            at_ms: 1,
            bytes: 10_000_000,
        },
        MemorySample {
            at_ms: 2,
            bytes: 10_100_000,
        },
    ]);
    assert_eq!(flat.direction, TrendDirection::Flat);
    let rising = MemoryTrend::from_samples(vec![
        MemorySample {
            at_ms: 1,
            bytes: 10_000_000,
        },
        MemorySample {
            at_ms: 2,
            bytes: 12_000_000,
        },
    ]);
    assert_eq!(rising.direction, TrendDirection::Rising);
    let falling = MemoryTrend::from_samples(vec![
        MemorySample {
            at_ms: 1,
            bytes: 20_000_000,
        },
        MemorySample {
            at_ms: 2,
            bytes: 10_000_000,
        },
    ]);
    assert_eq!(falling.direction, TrendDirection::Falling);
}

#[test]
fn a_locked_memory_log_skips_one_sample_without_waiting_or_writing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory-samples.json");
    let log = FileMemoryLog::open(&path).with_lock_wait(std::time::Duration::from_millis(20));
    assert_eq!(
        log.record_batch(50, &[("claude-code".into(), 1024)]).note,
        None
    );
    let before = std::fs::read(&path).unwrap();
    let held = std::fs::File::create(dir.path().join("memory-samples.lock")).unwrap();
    held.lock().unwrap();
    let started = std::time::Instant::now();
    let outcome = log.record_batch(100, &[("claude-code".into(), 2048)]);
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(outcome.note.as_deref().unwrap().contains("was not kept"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    drop(held);
    assert_eq!(
        log.record_batch(200, &[("claude-code".into(), 4096)]).note,
        None
    );
}

#[test]
fn a_memory_log_in_an_unwritable_place_reports_instead_of_failing() {
    let dir = tempfile::tempdir().unwrap();
    // A plain file where the log's folder should be: nothing under it can be created.
    let blocker = dir.path().join("not-a-dir");
    std::fs::write(&blocker, b"x").unwrap();
    let log = FileMemoryLog::open(blocker.join("memory-samples.json"));
    let outcome = log.record_batch(100, &[("claude-code".into(), 2048)]);
    assert!(outcome.note.is_some());
}

#[test]
fn a_corrupt_memory_log_is_kept_aside_and_reported_not_wiped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory-samples.json");
    std::fs::write(&path, b"{not json").unwrap();
    let log = FileMemoryLog::open(&path);
    let outcome = log.record_batch(100, &[("claude-code".into(), 2048)]);
    assert!(outcome.note.as_deref().unwrap().contains("corrupt"));
    assert_eq!(
        std::fs::read(dir.path().join("memory-samples.corrupt.json")).unwrap(),
        b"{not json"
    );
    assert_eq!(
        outcome.series["claude-code"],
        vec![MemorySample {
            at_ms: 100,
            bytes: 2048
        }]
    );
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("2048") && !body.contains('/'));
    let again = log.record_batch(200, &[("claude-code".into(), 4096)]);
    assert_eq!(again.note, None);
    assert_eq!(again.series["claude-code"].len(), 2);
}

#[test]
fn the_memory_log_keeps_a_series_across_restarts_and_caps_it() {
    let dir = tempfile::tempdir().unwrap();
    let log = FileMemoryLog::open(dir.path().join("memory-samples.json"));
    for at in 0..(log::MAX_PER_AGENT as u64 + 5) {
        log.record_batch(at, &[("codex".into(), at), ("cursor".into(), 1)]);
    }
    let last = log.record_batch(10_000, &[("codex".into(), 1)]);
    assert_eq!(last.series["codex"].len(), log::MAX_PER_AGENT);
    assert_eq!(last.series["codex"].last().unwrap().at_ms, 10_000);
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "atomic writes leave no temp files");
}

#[test]
fn concurrent_writers_do_not_lose_each_others_samples() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory-samples.json");
    let handles: Vec<_> = (0..6)
        .map(|writer| {
            let path = path.clone();
            std::thread::spawn(move || {
                // Slow CI disks (fsync on Windows) can keep six writers queued past the default.
                let log =
                    FileMemoryLog::open(path).with_lock_wait(std::time::Duration::from_secs(30));
                for at in 0..5u64 {
                    let outcome = log.record_batch(at, &[(format!("agent-{writer}"), at)]);
                    assert_eq!(outcome.note, None);
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let log = FileMemoryLog::open(&path);
    let outcome = log.record_batch(
        99,
        &(0..6)
            .map(|w| (format!("agent-{w}"), 1))
            .collect::<Vec<_>>(),
    );
    for writer in 0..6 {
        assert_eq!(
            outcome.series[&format!("agent-{writer}")].len(),
            6,
            "writer {writer}"
        );
    }
}

#[test]
fn an_older_per_process_log_starts_over_without_a_corruption_warning() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory-samples.json");
    std::fs::write(
        &path,
        br#"{"samples":[{"pid":1,"started_at":2,"at_ms":3,"bytes":4}]}"#,
    )
    .unwrap();
    let outcome = FileMemoryLog::open(&path).record_batch(10, &[("gemini".into(), 5)]);
    assert_eq!(outcome.note, None);
    assert!(!dir.path().join("memory-samples.corrupt.json").exists());
}

#[test]
fn location_census_reports_metadata_without_file_bodies_or_followed_links() {
    let home = tempfile::tempdir().unwrap();
    let home_dir = home.path().canonicalize().unwrap();
    let skill = home_dir.join(".claude/skills/review");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        b"api_key = super-secret-token-value\n",
    )
    .unwrap();
    std::fs::write(
        home_dir.join(".claude.json"),
        b"{\"token\":\"super-secret-token-value\"}",
    )
    .unwrap();
    let project = home_dir.join("work");
    std::fs::create_dir_all(project.join(".claude")).unwrap();
    std::fs::write(
        project.join("CLAUDE.md"),
        b"remember super-secret-token-value",
    )
    .unwrap();
    std::fs::write(project.join(".claude/settings.local.json"), b"{}").unwrap();
    let places = census(
        "claude-code",
        &crate::transparency::Homes::defaults(&home_dir, None),
        std::slice::from_ref(&project),
        "linux",
    );
    let dumped = serde_json::to_string(&places).unwrap();
    assert!(!dumped.contains("super-secret-token-value"), "{dumped}");
    let skills = places
        .iter()
        .find(|place| place.kind == PlaceKind::Skills && place.scope == PlaceScope::User)
        .unwrap();
    assert_eq!(skills.status, PlaceStatus::Present);
    assert_eq!(skills.candidates, vec!["review".to_string()]);
    assert!(skills.byte_size.unwrap() > 0);
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(
            outside.path().join("secret-body"),
            b"super-secret-token-value",
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), home_dir.join(".codex")).unwrap();
        let linked = census(
            "codex",
            &crate::transparency::Homes::defaults(&home_dir, None),
            &[],
            "linux",
        )
        .into_iter()
        .find(|place| place.path.ends_with(".codex/skills"))
        .unwrap();
        assert_eq!(linked.status, PlaceStatus::Skipped);
        assert!(linked.symlink_target.is_none());
        assert!(linked.candidates.is_empty());
        let again = serde_json::to_string(&linked).unwrap();
        assert!(!again.contains("super-secret-token-value"));
    }
    let memory = places
        .iter()
        .find(|place| place.path.ends_with("CLAUDE.md") && place.scope == PlaceScope::Project)
        .unwrap();
    assert!(memory.present);
    assert_eq!(memory.kind, PlaceKind::Instructions);
    let local = places
        .iter()
        .find(|place| place.path.ends_with("settings.local.json"))
        .unwrap();
    assert_eq!(local.scope, PlaceScope::Local);
    assert!(local.rank < memory.rank);
    let missing = places
        .iter()
        .find(|place| {
            Path::new(&place.path).ends_with(Path::new(".claude").join("settings.json"))
                && place.scope == PlaceScope::User
        })
        .unwrap();
    assert_eq!(missing.status, PlaceStatus::Missing);
    assert!(!missing.present);
    assert!(places
        .iter()
        .any(|place| place.kind == PlaceKind::Builtin && place.path.is_empty()));
    assert!(places
        .iter()
        .any(|place| place.kind == PlaceKind::Plugin && !place.present));
    let builtin = places
        .iter()
        .find(|place| place.kind == PlaceKind::Builtin)
        .unwrap();
    assert!(!builtin.summary_line().contains("count unknown"));
    assert!(builtin.summary_line().contains("documented tools"));
    assert!(missing.summary_line().contains("not on disk"));
    assert!(!missing.summary_line().contains("count unknown"));
    let _ = PathBuf::from(home.path());
}

#[test]
fn linux_status_text_does_not_treat_rss_file_as_shared_memory() {
    let status = super::counters::reading_from_proc(
        "Name:\tclaude\nVmRSS:\t10 kB\nVmSize:\t20 kB\nVmHWM:\t15 kB\nVmSwap:\t3 kB\nRssFile:\t99 kB\n",
        None,
    );
    assert_eq!(status.resident.get(), Some(10 * 1024));
    assert_eq!(status.virtual_bytes.get(), Some(20 * 1024));
    assert_eq!(status.peak_resident.get(), Some(15 * 1024));
    assert_eq!(status.swap_bytes.get(), Some(3 * 1024));
    assert_eq!(status.physical_footprint.get(), None);
    assert_eq!(status.private_bytes.get(), None);
    assert_eq!(status.shared_bytes.get(), None);
    assert!(status.compressed.reason().contains("zswap"));
    let rollup = super::counters::reading_from_proc(
        "VmRSS:\t10 kB\nVmSize:\t20 kB\nVmHWM:\t15 kB\nVmSwap:\t0 kB\n",
        Some("Pss:\t4 kB\nPrivate_Clean:\t1 kB\nPrivate_Dirty:\t2 kB\nShared_Clean:\t3 kB\nShared_Dirty:\t1 kB\n"),
    );
    assert_eq!(rollup.private_bytes.get(), Some(3 * 1024));
    assert_eq!(rollup.shared_bytes.get(), Some(4 * 1024));
    assert_eq!(rollup.proportional.get(), Some(4 * 1024));
}

#[test]
fn a_refused_platform_counter_makes_the_headline_mixed_with_a_reason() {
    let agent_proc = process(10, "Cursor", ProcessRole::Agent, 100);
    let helper = process(11, "Cursor Helper", ProcessRole::Helper, 200);
    let rows = vec![
        proc(10, 1, "Cursor", &["Cursor"]),
        proc(11, 10, "Cursor Helper", &["Cursor"]),
    ];
    let mut table = table(rows, 1);
    let probe = FixedProbe {
        reading: reading_with(ObservedU64::unknown("fixture")),
        machine: ObservedU64::observed(1_000_000),
    };
    let agent = bare_agent(10, "Cursor", vec![agent_proc, helper], Vec::new());
    let clean = agent_memory(&agent, &table, &probe, 1);
    assert_eq!(clean.headline_mixed, None);
    assert_eq!(clean.headline_label, HeadlineCounter::on_this_os().label());
    assert!(!clean.headline_meaning.contains("matches Activity Monitor"));

    table.mark_memory_fallback(11);
    let mixed = agent_memory(&agent, &table, &probe, 1);
    let helper_row = mixed.processes.iter().find(|row| row.pid == 11).unwrap();
    assert_eq!(helper_row.counter, HeadlineCounter::Resident);
    if HeadlineCounter::on_this_os() == HeadlineCounter::Resident {
        // Linux's headline is already resident size, so the fallback is the same counter.
        assert_eq!(mixed.headline_mixed, None);
        return;
    }
    let reason = mixed.headline_mixed.as_deref().unwrap();
    assert!(reason.contains("1 of 2 processes"), "{reason}");
    assert!(mixed.headline_label.starts_with("Mixed"));
    assert!(mixed.limits.iter().any(|limit| limit == reason));
    assert!(mixed.summary_line().contains("Mixed"));
}

#[test]
fn a_tree_with_no_memory_reading_is_unknown_not_zero() {
    let agent = bare_agent(
        20,
        "Docker Desktop",
        vec![process(20, "com.docker.vmnetd", ProcessRole::Agent, 0)],
        Vec::new(),
    );
    let mut row = proc(20, 1, "com.docker.vmnetd", &["com.docker.vmnetd"]);
    row.memory_bytes = 0;
    let mut table = table(vec![row], 1);
    table.mark_memory_fallback(20);
    let probe = FixedProbe {
        reading: ProcessReading::all_unknown("proc_pid_rusage failed (access denied)"),
        machine: ObservedU64::observed(1_000_000),
    };
    let report = agent_memory(&agent, &table, &probe, 1);
    assert!(report.headline_unknown());
    assert_eq!(report.headline_text(), "unknown");
    let line = report.summary_line();
    assert!(line.starts_with("unknown"), "{line}");
    assert!(line.contains("access denied"), "{line}");
    assert!(!line.contains("0 B"), "{line}");
    let lines = report.breakdown_lines();
    assert!(lines.iter().all(|l| !l.contains("0 B")), "{lines:?}");
}

#[test]
fn counters_with_the_same_unknown_reason_are_explained_once() {
    let agent = bare_agent(
        10,
        "Cursor",
        vec![process(10, "Cursor", ProcessRole::Agent, 100)],
        Vec::new(),
    );
    let table = table(vec![proc(10, 1, "Cursor", &["Cursor"])], 1);
    let probe = FixedProbe {
        reading: reading_with(ObservedU64::observed(4096)),
        machine: ObservedU64::observed(1_000_000),
    };
    let report = agent_memory(&agent, &table, &probe, 1);
    assert!(!report.headline_unknown());
    let line = report.summary_line();
    assert!(line.contains("resident 4.0 KB"), "{line}");
    assert!(
        line.contains("private and shared unknown (not exposed by this fixture)"),
        "{line}"
    );
    assert_eq!(
        line.matches("not exposed by this fixture").count(),
        1,
        "{line}"
    );
}

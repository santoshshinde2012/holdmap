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
fn the_memory_log_keeps_pid_and_bytes_only_and_replaces_a_corrupt_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memory-samples.json");
    std::fs::write(&path, b"{not json").unwrap();
    let log = FileMemoryLog::open(&path);
    assert!(log.recent(7, 9).is_empty());
    assert!(log.record(7, 9, 100, 2048));
    assert!(log.record(7, 8, 100, 1));
    assert_eq!(
        log.recent(7, 8),
        vec![MemorySample {
            at_ms: 100,
            bytes: 1
        }]
    );
    assert_eq!(
        log.recent(7, 9),
        vec![MemorySample {
            at_ms: 100,
            bytes: 2048
        }]
    );
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(!body.contains("claude"));
    assert!(body.contains("2048"));
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
        &home_dir,
        std::slice::from_ref(&project),
        "linux",
        None,
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
        let linked = census("codex", &home_dir, &[], "linux", None)
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
    assert_eq!(memory.kind, PlaceKind::AgentMemory);
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

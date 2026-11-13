//! Census behaviour for the registry forms: ignored rule files, auto memory, session data and
//! secret stores. Every fixture plants a secret marker that must never reach the output.

use super::*;
use std::fs;

const SECRET: &str = "super-secret-token-value";

fn homes(home: &Path) -> Homes {
    Homes::defaults(home, None)
}

fn find<'a>(places: &'a [CapabilityPlace], tail: &str) -> &'a CapabilityPlace {
    places
        .iter()
        .find(|place| place.path.ends_with(tail))
        .unwrap_or_else(|| panic!("no row ending {tail}"))
}

#[test]
fn cursor_rules_count_only_mdc_files_and_report_md_as_ignored() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path().canonicalize().unwrap();
    let project = home.join("work");
    let rules = project.join(".cursor/rules/frontend");
    fs::create_dir_all(&rules).unwrap();
    fs::write(project.join(".cursor/rules/style.mdc"), SECRET).unwrap();
    fs::write(rules.join("react.mdc"), SECRET).unwrap();
    fs::write(project.join(".cursor/rules/notes.md"), SECRET).unwrap();
    let places = census(
        "cursor",
        &homes(&home),
        std::slice::from_ref(&project),
        "linux",
    );
    let row = find(&places, ".cursor/rules");
    assert_eq!(row.kind, PlaceKind::Rules);
    assert_eq!(row.file_count, Some(2));
    assert_eq!(row.ignored_count, Some(1));
    assert_eq!(row.candidates, vec!["frontend/react", "style"]);
    assert!(row.status_reason.as_deref().unwrap().contains("ignored"));
    assert!(row.summary_line().contains("1 ignored"));
    assert!(!serde_json::to_string(&places).unwrap().contains(SECRET));
}

#[test]
fn auto_memory_counts_index_lines_against_the_load_limit() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path().canonicalize().unwrap();
    let project = home.join("my app");
    fs::create_dir_all(&project).unwrap();
    let memory = home
        .join(".claude/projects")
        .join(locations::claude_project_dir(&project))
        .join("memory");
    fs::create_dir_all(&memory).unwrap();
    let body: String = (0..250).map(|i| format!("- {SECRET} {i}\n")).collect();
    fs::write(memory.join("MEMORY.md"), body).unwrap();
    fs::write(memory.join("debugging.md"), SECRET).unwrap();
    let places = census(
        "claude-code",
        &homes(&home),
        std::slice::from_ref(&project),
        "linux",
    );
    let row = places
        .iter()
        .find(|place| place.kind == PlaceKind::AgentMemory && place.label == "Auto memory")
        .unwrap();
    assert!(row.present);
    assert_eq!(row.line_count, Some(250));
    assert_eq!(row.load_limit_lines, Some(200));
    assert_eq!(row.file_count, Some(2));
    assert!(row.candidates.is_empty() && row.entries.is_empty());
    assert!(row.summary_line().contains("only the first 200 load"));
    assert!(!serde_json::to_string(&places).unwrap().contains(SECRET));
}

#[test]
fn line_count_handles_missing_final_newline_and_empty_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap().join("MEMORY.md");
    fs::write(&path, b"a\nb").unwrap();
    assert_eq!(presence::count_lines(&path, 3), Some(2));
    fs::write(&path, b"").unwrap();
    assert_eq!(presence::count_lines(&path, 0), Some(0));
    assert_eq!(presence::count_lines(&path, 2 * 1024 * 1024), None);
}

#[test]
fn session_data_reports_totals_without_any_names() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path().canonicalize().unwrap();
    let transcripts = home.join(".claude/projects/-work/");
    fs::create_dir_all(&transcripts).unwrap();
    fs::write(transcripts.join("secret-session-name.jsonl"), SECRET).unwrap();
    fs::write(home.join(".claude/history.jsonl"), SECRET).unwrap();
    let places = census("claude-code", &homes(&home), &[], "linux");
    let row = places
        .iter()
        .find(|place| place.kind == PlaceKind::SessionData && place.label == "Transcripts")
        .unwrap();
    assert_eq!(row.file_count, Some(1));
    assert_eq!(row.byte_size, Some(SECRET.len() as u64));
    assert!(row.candidates.is_empty() && row.entries.is_empty());
    let dumped = serde_json::to_string(&places).unwrap();
    assert!(!dumped.contains("secret-session-name"));
    assert!(!dumped.contains(SECRET));
    assert_eq!(row.combine, Combine::NotConfig);
}

#[test]
fn secret_stores_report_presence_only() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path().canonicalize().unwrap();
    fs::create_dir_all(home.join(".codex")).unwrap();
    fs::write(home.join(".codex/auth.json"), SECRET).unwrap();
    let places = census("codex", &homes(&home), &[], "linux");
    let row = find(&places, "auth.json");
    assert_eq!(row.kind, PlaceKind::SecretStore);
    assert!(row.present);
    assert_eq!(row.byte_size, None);
    assert_eq!(row.file_count, None);
    assert_eq!(row.modified_unix_ms, None);
    assert!(row.candidates.is_empty());
    assert!(row.summary_line().contains("not opened"));
    assert!(!serde_json::to_string(&places).unwrap().contains(SECRET));
}

#[test]
fn rows_without_a_base_on_this_os_are_skipped_with_a_reason() {
    let home = tempfile::tempdir().unwrap();
    let places = census("claude-desktop", &homes(home.path()), &[], "linux");
    let row = places
        .iter()
        .find(|place| place.kind == PlaceKind::McpConfig)
        .unwrap();
    assert_eq!(row.status, PlaceStatus::Skipped);
    assert!(row
        .status_reason
        .as_deref()
        .unwrap()
        .contains("operating system"));
}

#[test]
fn project_rows_have_unique_ids_across_projects() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path().canonicalize().unwrap();
    let projects = vec![home.join("a"), home.join("b")];
    let places = census("codex", &homes(&home), &projects, "linux");
    let mut ids = HashSet::new();
    for place in &places {
        assert!(ids.insert(place.id.clone()), "duplicate id {}", place.id);
    }
}

#[test]
fn privacy_refusal_does_not_name_macos_on_other_systems() {
    let home = tempfile::tempdir().unwrap();
    let places = census("cline", &homes(home.path()), &[], "macos");
    let row = find(&places, "Documents/Cline/Rules");
    assert_eq!(row.status, PlaceStatus::Skipped);
    assert!(row
        .status_reason
        .as_deref()
        .unwrap()
        .contains("asks permission"));
}

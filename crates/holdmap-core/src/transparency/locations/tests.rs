use super::*;
use crate::transparency::places::{PlaceKind, PlaceScope};
use std::collections::HashSet;

const PRODUCTS: &[&str] = &[
    "claude-code",
    "claude-desktop",
    "codex",
    "cursor",
    "gemini",
    "copilot",
    "copilot-cli",
    "windsurf",
    "cline",
    "kiro",
    "opencode",
    "amp",
    "goose",
    "aider",
    "qwen",
    "crush",
    "zed",
];
const PLATFORMS: &[&str] = &["macos", "linux", "windows"];

fn has(rows: &[LocationSpec], relative: &str) -> bool {
    rows.iter().any(|row| row.relative == relative)
}

#[test]
fn every_row_is_relative_unless_system_and_unique_per_product() {
    for product in PRODUCTS {
        for platform in PLATFORMS {
            let rows = specs(product, platform);
            let mut seen = HashSet::new();
            for row in &rows {
                assert!(!row.label.is_empty(), "{product} row without label");
                if row.base == Base::System {
                    assert!(
                        row.relative.starts_with('/') || row.relative.starts_with("C:\\"),
                        "{product}: {}",
                        row.relative
                    );
                } else if row.form != Form::Account {
                    assert!(!row.relative.starts_with('/') && !row.relative.contains(".."));
                }
                assert!(
                    seen.insert((row.base_key(), row.relative, row.kind)),
                    "{product} {platform}: duplicate {}",
                    row.relative
                );
            }
        }
    }
}

impl LocationSpec {
    fn base_key(&self) -> u8 {
        self.base as u8
    }
}

#[test]
fn cursor_rules_load_mdc_and_count_md_as_ignored() {
    let rows = specs("cursor", "macos");
    let rules = rows
        .iter()
        .find(|row| row.relative == ".cursor/rules")
        .unwrap();
    assert_eq!(
        rules.form,
        Form::Deep {
            loads: &["mdc"],
            ignored: &["md"]
        }
    );
    assert_eq!(
        rules.reader,
        Reader::Instructions(InstructionKind::Rule, Some("mdc"))
    );
}

#[test]
fn codex_skills_live_in_agents_folders_and_codex_skills_is_legacy_only() {
    let rows = specs("codex", "linux");
    let user = rows
        .iter()
        .find(|row| row.base == Base::Home && row.relative == ".agents/skills")
        .unwrap();
    assert_eq!(user.reader, Reader::Skills);
    let legacy = rows
        .iter()
        .find(|row| row.base == Base::CodexHome && row.relative == "skills")
        .unwrap();
    assert_eq!(legacy.reader, Reader::None);
    assert!(legacy.note.is_some());
    assert!(has(&rows, "/etc/codex/skills"));
    assert!(has(&rows, "hooks.json"));
    assert!(has(&rows, "memories"));
}

#[test]
fn claude_rows_cover_local_rules_auto_memory_and_managed_policy_per_os() {
    let mac = specs("claude-code", "macos");
    for relative in [
        "CLAUDE.local.md",
        ".claude/rules",
        "rules",
        "output-styles",
        "plugins",
        "projects/{project}/memory",
        "/Library/Application Support/ClaudeCode/managed-settings.json",
        "/Library/Application Support/ClaudeCode/CLAUDE.md",
    ] {
        assert!(has(&mac, relative), "missing {relative}");
    }
    assert!(has(
        &specs("claude-code", "linux"),
        "/etc/claude-code/managed-settings.json"
    ));
    assert!(has(
        &specs("claude-code", "windows"),
        r"C:\Program Files\ClaudeCode\managed-settings.json"
    ));
    let memory = mac.iter().find(|row| row.form == Form::AutoMemory).unwrap();
    assert_eq!(memory.kind, PlaceKind::AgentMemory);
    assert!(memory.per_project());
}

#[test]
fn transcripts_and_credentials_are_never_parsed() {
    for product in PRODUCTS {
        for row in specs(product, "macos") {
            if matches!(row.kind, PlaceKind::SessionData | PlaceKind::SecretStore) {
                assert_eq!(row.reader, Reader::None, "{product} {}", row.relative);
                assert!(matches!(row.form, Form::SessionData | Form::SecretStore));
            }
        }
    }
}

#[test]
fn new_vendor_locations_are_listed() {
    let gemini = specs("gemini", "linux");
    assert!(has(&gemini, "/etc/gemini-cli/settings.json"));
    assert!(has(&gemini, "/etc/gemini-cli/system-defaults.json"));
    assert!(has(&gemini, ".gemini/extensions"));
    let copilot = specs("copilot-cli", "macos");
    for relative in [
        "skills",
        "agents",
        "instructions",
        "session-state",
        "mcp-secrets",
    ] {
        assert!(has(&copilot, relative), "copilot {relative}");
    }
    let windsurf = specs("windsurf", "macos");
    for relative in [
        ".devin/global_rules.md",
        ".codeium/windsurf/global_workflows",
        "devin/skills",
    ] {
        assert!(has(&windsurf, relative), "windsurf {relative}");
    }
    let cline = specs("cline", "macos");
    assert!(has(&cline, ".cline/rules"));
    assert!(has(&cline, "Documents/Cline/Rules"));
    let cursor = specs("cursor", "linux");
    assert!(has(&cursor, ".cursor/hooks.json"));
    assert!(has(&cursor, ".cursor/agents"));
    assert!(has(&cursor, "/etc/cursor/hooks.json"));
}

#[test]
fn managed_rows_rank_first_and_defaults_after_user() {
    assert!(PlaceScope::Managed.rank() < PlaceScope::Local.rank());
    assert!(PlaceScope::User.rank() < PlaceScope::SystemDefault.rank());
    assert_eq!(Combine::for_kind(PlaceKind::Settings), Combine::Override);
    assert_eq!(Combine::for_kind(PlaceKind::Instructions), Combine::Join);
    assert_eq!(Combine::for_kind(PlaceKind::Skills), Combine::Merge);
    assert_eq!(
        Combine::for_kind(PlaceKind::SessionData),
        Combine::NotConfig
    );
}

#[test]
fn resolve_expands_bases_and_the_claude_project_folder() {
    let homes = Homes::defaults(Path::new("/home/a"), None);
    let rows = specs("claude-code", "linux");
    let memory = rows
        .iter()
        .find(|row| row.form == Form::AutoMemory)
        .unwrap();
    assert_eq!(
        resolve(memory, &homes, Some(Path::new("/work/my app"))),
        Some(PathBuf::from(
            "/home/a/.claude/projects/-work-my-app/memory"
        ))
    );
    assert_eq!(resolve(memory, &homes, None), None);
    let desktop = specs("claude-desktop", "linux");
    assert!(desktop
        .iter()
        .all(|row| resolve(row, &homes, None).is_none()));
}

#[test]
fn overrides_accept_only_absolute_control_free_paths() {
    // An absolute path on this OS; "/opt/codex" has no drive, so Windows treats it as relative.
    let codex = if cfg!(windows) {
        r"C:\opt\codex"
    } else {
        "/opt/codex"
    };
    let env = |name: &str| match name {
        "CODEX_HOME" => Some(codex.to_owned()),
        "CLAUDE_CONFIG_DIR" => Some("relative/claude".to_owned()),
        "COPILOT_HOME" => Some(format!("{codex}\nbad")),
        _ => None,
    };
    let homes = Homes::with_overrides(Path::new("/home/a"), None, env);
    assert_eq!(homes.codex, PathBuf::from(codex));
    assert_eq!(homes.claude, PathBuf::from("/home/a/.claude"));
    assert_eq!(homes.copilot, PathBuf::from("/home/a/.copilot"));
    assert_eq!(homes.override_for(Base::CodexHome), Some("CODEX_HOME"));
    assert_eq!(homes.override_for(Base::ClaudeHome), None);
}

#[test]
fn readable_rows_are_a_subset_with_a_reader() {
    for product in PRODUCTS {
        for row in readable(product, "macos") {
            assert_ne!(row.reader, Reader::None);
        }
    }
}

/// The browser demo's sample locations (apps/desktop/src/lib/mock-places.ts) are checked
/// against this generated list, so the demo cannot drift from the registry.
/// Regenerate with `HOLDMAP_UPDATE_FIXTURES=1 cargo test -p holdmap-core demo_location_fixture`.
fn demo_location_fixture() -> String {
    let mut rows = Vec::new();
    for product in ["cursor", "claude-code"] {
        for spec in specs(product, "macos") {
            rows.push(serde_json::json!({
                "product": product,
                "kind": spec.kind,
                "scope": spec.scope,
                "combine": Combine::for_kind(spec.kind),
                "relative": spec.relative,
            }));
        }
    }
    let mut text = serde_json::to_string_pretty(&rows).expect("fixture serializes");
    text.push('\n');
    text
}

#[test]
fn demo_location_fixture_is_current() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/src/lib/fixtures/location-specs.json"
    );
    let fresh = demo_location_fixture();
    if std::env::var_os("HOLDMAP_UPDATE_FIXTURES").is_some() {
        std::fs::write(path, &fresh).expect("fixture written");
    }
    let saved = std::fs::read_to_string(path).unwrap_or_default();
    assert_eq!(
        saved, fresh,
        "regenerate with HOLDMAP_UPDATE_FIXTURES=1 cargo test -p holdmap-core demo_location_fixture"
    );
}

#[test]
fn smaller_agents_list_their_documented_files_and_read_only_mcp_servers_json() {
    let kiro = specs("kiro", "macos");
    assert!(has(&kiro, ".kiro/steering") && has(&kiro, ".kiro/settings/mcp.json"));
    let readers: Vec<_> = kiro
        .iter()
        .filter(|row| matches!(row.reader, Reader::Mcp(_)))
        .map(|row| row.scope)
        .collect();
    assert_eq!(readers, [PlaceScope::User, PlaceScope::Project]);

    // Formats holdmap doesn't parse are listed, never read.
    for product in ["opencode", "amp", "goose", "crush", "aider", "zed"] {
        for platform in PLATFORMS {
            assert!(
                !specs(product, platform)
                    .iter()
                    .any(|row| matches!(row.reader, Reader::Mcp(_))),
                "{product} {platform}"
            );
        }
    }
    let qwen = specs("qwen", "linux");
    assert!(qwen
        .iter()
        .any(|row| row.relative == "/etc/qwen-code/settings.json"
            && row.scope == PlaceScope::Managed));
    assert!(qwen
        .iter()
        .any(|row| row.relative == ".qwen/QWEN.local.md" && row.scope == PlaceScope::Local));

    // Secrets and history are presence or size only.
    let aider = specs("aider", "macos");
    let env = aider.iter().find(|row| row.relative == ".env").unwrap();
    assert_eq!(
        (env.kind, env.form),
        (PlaceKind::SecretStore, Form::SecretStore)
    );
    assert!(aider
        .iter()
        .filter(|row| row.relative.contains("history"))
        .all(|row| row.form == Form::SessionData));
    let goose = specs("goose", "linux");
    assert!(goose
        .iter()
        .any(|row| row.relative == "goose/secrets.yaml" && row.form == Form::SecretStore));
}

#[test]
fn goose_and_zed_move_to_appdata_on_windows_and_opencode_has_a_managed_folder() {
    let goose = specs("goose", "windows");
    assert!(goose
        .iter()
        .any(|row| row.base == Base::AppData && row.relative == "Block/goose/config/config.yaml"));
    assert!(specs("zed", "windows")
        .iter()
        .any(|row| row.base == Base::AppData && row.relative == "Zed/AGENTS.md"));
    assert!(specs("zed", "macos")
        .iter()
        .any(|row| row.base == Base::XdgConfig && row.relative == "zed/AGENTS.md"));
    for (platform, path) in [
        ("macos", "/Library/Application Support/opencode"),
        ("linux", "/etc/opencode"),
        ("windows", r"C:\ProgramData\opencode"),
    ] {
        assert!(specs("opencode", platform)
            .iter()
            .any(|row| row.scope == PlaceScope::Managed && row.relative == path));
    }
}

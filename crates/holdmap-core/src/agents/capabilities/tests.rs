use super::*;
use std::fs;

/// Fixtures assert on what the inventory finds, not on how fast a CI runner's disk is: the
/// 100 ms production bound made Windows runners with on-access scanning skip late locations.
const FIXTURE_TIME_LIMIT: Duration = Duration::from_secs(30);

struct Fixture {
    _directory: tempfile::TempDir,
    home: PathBuf,
    project: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        // macOS's temporary directory may be under the /var -> /private/var alias.
        let root = directory.path().canonicalize().unwrap();
        let home = root.join("home");
        let project = root.join("project");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&project).unwrap();
        Self {
            _directory: directory,
            home,
            project,
        }
    }

    fn put(&self, relative: &str, contents: &[u8]) -> PathBuf {
        let path = self.home.parent().unwrap().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    fn discover(&self, product: &str) -> AgentCapabilities {
        HomeCapabilities::new(self.home.clone())
            .with_time_limit(FIXTURE_TIME_LIMIT)
            .discover(product, std::slice::from_ref(&self.project), "fixture")
    }
}

#[test]
fn codex_projects_only_allowed_metadata_and_never_exports_skill_instructions() {
    let fixture = Fixture::new();
    fixture.put("home/.codex/config.toml", br#"
model = "private-model-fixture"
[mcp_servers.docs]
command = "server-command-fixture"
args = ["private-argument-fixture"]
enabled = false
env = { SECRET = "private-environment-fixture" }
[mcp_servers.remote]
url = "https://private-user-fixture:private-password-fixture@example.invalid/private-path-fixture?token=private-query-fixture"
http_headers = { Authorization = "private-header-fixture" }
"#);
    fixture.put(
        "project/.codex/config.toml",
        br#"[mcp_servers.docs]
url = "https://project-endpoint-host.invalid/"
enabled = true
"#,
    );
    let skill = fixture.put("home/.agents/skills/review/SKILL.md", &[0xff, 0xfe]);
    fixture.put(
        "home/.codex/skills/.system/setup/SKILL.md",
        b"private-skill-body-fixture",
    );
    fixture.put(
        "home/.codex/skills/.system/category/too-deep/SKILL.md",
        b"private-nested-body-fixture",
    );
    fixture.put(
        "project/.agents/skills/review/SKILL.md",
        b"private-project-body-fixture",
    );
    let out = fixture.discover("codex");
    assert!(out.inspected);
    assert_eq!(out.mcp_servers.len(), 3);
    let user = out
        .mcp_servers
        .iter()
        .find(|server| server.scope == CapabilityScope::User && server.name == "docs")
        .unwrap();
    let project = out
        .mcp_servers
        .iter()
        .find(|server| server.scope == CapabilityScope::Project)
        .unwrap();
    assert_eq!(
        (user.transport, user.enabled),
        (McpTransport::Stdio, Some(false))
    );
    assert_eq!(
        (project.transport, project.enabled),
        (McpTransport::Http, Some(true))
    );
    assert_eq!(
        project.project_path.as_deref(),
        Some(fixture.project.as_path())
    );
    assert_ne!(user.id, project.id);
    assert_eq!(out.skills.len(), 3);
    assert!(out.skills.iter().any(|entry| entry.source_path == skill));
    assert!(!out.skills.iter().any(|entry| entry.name == "too-deep"));
    let encoded = serde_json::to_string(&out).unwrap();
    assert!(
        !encoded.contains("private-"),
        "inventory serialized excluded configuration or skill content"
    );
    assert_eq!(out.mcp_servers, fixture.discover("codex").mcp_servers);
    assert_eq!(fixture.discover("codex-desktop").mcp_servers.len(), 3);
}

#[test]
fn claude_private_project_aliases_stay_bound_to_exact_observed_project() {
    let fixture = Fixture::new();
    let unrelated = fixture.project.with_file_name("unobserved-project");
    let user = serde_json::json!({
        "mcpServers": {"shared": {"command": "server-command-fixture"}},
        "projects": {
            fixture.project.to_str().unwrap(): {"mcpServers": {"shared": {"type":"http", "url":"https://local-host.invalid/"}}},
            unrelated.to_str().unwrap(): {"mcpServers": {"must-not-appear": {"command":"server-unrelated-fixture"}}}
        },
        "oauthAccount": {"token":"private-token-fixture"}
    });
    fixture.put("home/.claude.json", user.to_string().as_bytes());
    fixture.put("project/.mcp.json", br#"{"mcpServers":{"shared":{"type":"sse","url":"https://project-host.invalid/"},"invalid-transport":{"url":"https://untyped-host.invalid/"}}}"#);
    fixture.put(
        "home/.claude/skills/review/SKILL.md",
        b"private-skill-body-fixture",
    );
    let out = fixture.discover("claude-code");
    assert_eq!(out.mcp_servers.len(), 4);
    assert!(!out
        .mcp_servers
        .iter()
        .any(|entry| entry.name == "must-not-appear"));
    let private = out
        .mcp_servers
        .iter()
        .find(|entry| entry.scope == CapabilityScope::Local)
        .unwrap();
    assert_eq!(
        private.project_path.as_deref(),
        Some(fixture.project.as_path())
    );
    assert_eq!(private.transport, McpTransport::Http);
    assert_eq!(private.enabled, None);
    let untyped = out
        .mcp_servers
        .iter()
        .find(|entry| entry.name == "invalid-transport")
        .unwrap();
    assert_eq!(untyped.transport, McpTransport::Unknown);
    let ids: HashSet<_> = out.mcp_servers.iter().map(|entry| &entry.id).collect();
    assert_eq!(ids.len(), out.mcp_servers.len());
    assert!(
        !serde_json::to_string(&out).unwrap().contains("private-"),
        "inventory serialized excluded private values"
    );
}

#[test]
fn cursor_supports_fixed_compatibility_roots_and_one_category_level() {
    let fixture = Fixture::new();
    fixture.put("home/.cursor/mcp.json", br#"{"mcpServers":{"remote":{"url":"https://remote-host.invalid/"},"off":{"command":"server-command-fixture","disabled":true}}}"#);
    fixture.put(
        "project/.cursor/mcp.json",
        br#"{"mcpServers":{"remote":{"type":"http","url":"https://project-host.invalid/"}}}"#,
    );
    for directory in [".cursor", ".agents", ".claude", ".codex"] {
        fixture.put(
            &format!("home/{directory}/skills/review/SKILL.md"),
            b"private-skill-body-fixture",
        );
    }
    fixture.put(
        "project/.cursor/skills/category/deploy/SKILL.md",
        b"private-category-body-fixture",
    );
    fixture.put(
        "project/unobserved/.cursor/skills/hidden/SKILL.md",
        b"private-deep-body-fixture",
    );
    let out = fixture.discover("cursor");
    assert_eq!(out.mcp_servers.len(), 3);
    assert_eq!(
        out.mcp_servers
            .iter()
            .find(|entry| entry.name == "off")
            .unwrap()
            .enabled,
        Some(false)
    );
    assert_eq!(out.skills.len(), 5);
    assert!(!out.skills.iter().any(|entry| entry.name == "hidden"));
    assert_eq!(
        out.skills
            .iter()
            .map(|entry| &entry.id)
            .collect::<HashSet<_>>()
            .len(),
        5
    );
    assert!(
        !serde_json::to_string(&out).unwrap().contains("private-"),
        "inventory serialized excluded private values"
    );
    assert_eq!(fixture.discover("cursor-agent").mcp_servers.len(), 3);
}

#[test]
fn claude_desktop_reads_only_documented_user_mcp_file_not_code_settings_or_skills() {
    let fixture = Fixture::new();
    let config = br#"{"mcpServers":{"local server":{"command":"server-command-fixture","args":["private-argument-fixture"]}},"preferences":{"token":"private-token-fixture"}}"#;
    let mac = fixture.put(
        "home/Library/Application Support/Claude/claude_desktop_config.json",
        config,
    );
    let windows = fixture.put("roaming/Claude/claude_desktop_config.json", config);
    fixture.put(
        "home/.claude.json",
        br#"{"mcpServers":{"code-only":{"command":"server-code-fixture"}}}"#,
    );
    fixture.put(
        "home/.claude/skills/review/SKILL.md",
        b"private-skill-body-fixture",
    );
    for (platform, expected) in [("macos", mac), ("windows", windows)] {
        let provider = HomeCapabilities::with_app_data(
            fixture.home.clone(),
            fixture.home.parent().unwrap().join("roaming"),
        )
        .with_time_limit(FIXTURE_TIME_LIMIT);
        let out = provider.discover(
            "claude-desktop",
            std::slice::from_ref(&fixture.project),
            platform,
        );
        assert!(out.inspected);
        assert_eq!(out.mcp_servers.len(), 1);
        assert_eq!(out.mcp_servers[0].name, "local server");
        assert_eq!(out.mcp_servers[0].source_path, expected);
        assert_eq!(out.mcp_servers[0].scope, CapabilityScope::User);
        assert!(out.skills.is_empty());
        assert!(out
            .limits
            .iter()
            .any(|limit| limit.contains("extensions, connectors and skills")));
        assert!(
            !serde_json::to_string(&out).unwrap().contains("private-"),
            "inventory must omit desktop command and preference values"
        );
    }
    assert!(!fixture.discover("claude-desktop").inspected);
}

#[test]
fn skill_frontmatter_is_optional_bounded_metadata_and_cannot_replace_folder_identity() {
    let fixture = Fixture::new();
    let body = b"---\nname: declared-review\ndescription: >-\n  Review code and explain\n  the next steps.\n---\nprivate-instruction-body-fixture\ndescription: private-body-description-fixture";
    let path = fixture.put("home/.agents/skills/folder-name/SKILL.md", body);
    let out = fixture.discover("codex");
    let skill = out
        .skills
        .iter()
        .find(|skill| skill.source_path == path)
        .unwrap();
    assert_eq!(skill.name, "folder-name");
    assert_eq!(skill.declared_name.as_deref(), Some("declared-review"));
    assert_eq!(
        skill.description.as_deref(),
        Some("Review code and explain the next steps.")
    );
    assert!(
        !serde_json::to_string(&out).unwrap().contains("private-"),
        "instruction body must not be retained"
    );
    let mut oversized = b"---\ndescription: |\n".to_vec();
    oversized.extend(vec![b' '; frontmatter::MAX_HEADER_BYTES]);
    oversized.extend_from_slice(b"\n---\nprivate-body-fixture");
    fixture.put("home/.agents/skills/oversized/SKILL.md", &oversized);
    let out = fixture.discover("codex");
    let large = out
        .skills
        .iter()
        .find(|skill| skill.name == "oversized")
        .unwrap();
    assert!(large.description.is_none() && large.declared_name.is_none());
    assert!(out.limits.iter().any(|limit| limit.contains("frontmatter")));
    let mut old = serde_json::to_value(skill).unwrap();
    old.as_object_mut().unwrap().remove("declared_name");
    old.as_object_mut().unwrap().remove("description");
    let old: SkillCapability = serde_json::from_value(old).unwrap();
    assert!(old.declared_name.is_none() && old.description.is_none());
}

#[test]
fn skill_metadata_respects_complete_lines_at_the_header_and_total_byte_limits() {
    let fixture = Fixture::new();
    let mut cut = b"---\nname: review\ndescription: Review code\n#".to_vec();
    cut.resize(frontmatter::MAX_HEADER_BYTES - 4, b'x');
    cut.extend_from_slice(b"\n---not-a-delimiter\n");
    fixture.put("home/.agents/skills/cut/SKILL.md", &cut);
    let mut complete = cut[..frontmatter::MAX_HEADER_BYTES - 5].to_vec();
    complete.extend_from_slice(b"\n---\nprivate-body-fixture");
    fixture.put("home/.agents/skills/complete/SKILL.md", &complete);
    fixture.put(
        "home/.agents/skills/eof/SKILL.md",
        b"---\nname: review\ndescription: Review code\n---",
    );
    let out = fixture.discover("codex");
    assert_eq!(out.skills.len(), 3);
    for skill in &out.skills {
        assert_eq!(
            skill.description.as_deref(),
            (skill.name != "cut").then_some("Review code")
        );
    }
    assert!(out.limits.iter().any(|limit| limit.contains("frontmatter")));

    // A smaller remaining total budget must not be confused with an EOF under 8 KiB.
    const REMAINING: usize = 128;
    let mut short_cut = b"---\nname: review\ndescription: Review code\n#".to_vec();
    short_cut.resize(REMAINING - 4, b'x');
    short_cut.extend_from_slice(b"\n---not-a-delimiter\n");
    let path = fixture.put("home/.agents/skills/budget-cut/SKILL.md", &short_cut);
    let mut inventory = Inventory {
        product: "codex",
        home: &fixture.home,
        platform: "fixture",
        out: AgentCapabilities::default(),
        start: Instant::now(),
        time_limit: MAX_TIME,
        sources: 0,
        bytes: MAX_TOTAL_BYTES - REMAINING,
        entries: 0,
        seen_sources: HashSet::new(),
        seen_skills: HashSet::new(),
        projects: Vec::new(),
    };
    let prefix = inventory
        .read_bytes(&path, frontmatter::MAX_HEADER_BYTES, false)
        .unwrap();
    assert_eq!(prefix.bytes.len(), REMAINING);
    assert_eq!(inventory.bytes, MAX_TOTAL_BYTES);
    assert!(!prefix.at_eof);
    assert!(frontmatter::parse(&prefix.bytes, prefix.at_eof).is_none());
}

#[test]
fn project_mcp_inventory_is_collected_before_skill_headers_exhaust_the_shared_budget() {
    let fixture = Fixture::new();
    fixture.put(
        "project/.cursor/mcp.json",
        br#"{"mcpServers":{"project-server":{"command":"server-command-fixture"}}}"#,
    );
    for index in 0..MAX_SOURCES + 4 {
        fixture.put(
            &format!("home/.cursor/skills/skill-{index:03}/SKILL.md"),
            b"---\nname: review\ndescription: Review code\n---\nprivate-body-fixture",
        );
    }
    let out = fixture.discover("cursor");
    assert!(out
        .mcp_servers
        .iter()
        .any(|server| server.name == "project-server" && server.scope == CapabilityScope::Project));
    assert!(out.limits.iter().any(|limit| limit.contains("bound")));
    assert!(
        !serde_json::to_string(&out).unwrap().contains("private-"),
        "configuration and instruction values must not be exported"
    );
}

#[test]
fn contradictory_switches_remain_unknown_and_sensitive_aliases_or_folders_are_not_retained() {
    let fixture = Fixture::new();
    fixture.put(
        "home/.cursor/mcp.json",
        br#"{"mcpServers":{
        "both-on":{"command":"server-command-fixture","enabled":true,"disabled":true},
        "both-off":{"command":"server-command-fixture","enabled":false,"disabled":false},
        "agree-on":{"command":"server-command-fixture","enabled":true,"disabled":false},
        "agree-off":{"command":"server-command-fixture","enabled":false,"disabled":true},
        "single":{"command":"server-command-fixture","disabled":true},
        "Bearer fixturecredential":{"command":"server-command-fixture"},
        "sk-fixturecredential":{"command":"server-command-fixture"},
        "github_pat_fixturecredential":{"command":"server-command-fixture"}
    }}"#,
    );
    fixture.put(
        "home/.cursor/skills/Bearer fixturecredential/SKILL.md",
        b"private-body-fixture",
    );
    fixture.put(
        "home/.cursor/skills/sk-fixturecredential/SKILL.md",
        b"private-body-fixture",
    );
    fixture.put(
        "home/.cursor/skills/review/SKILL.md",
        b"---\nname: review\ndescription: Review code\n---\nprivate-body-fixture",
    );
    let out = fixture.discover("cursor");
    assert_eq!(out.mcp_servers.len(), 5);
    let enabled = |name: &str| {
        out.mcp_servers
            .iter()
            .find(|server| server.name == name)
            .unwrap()
            .enabled
    };
    assert_eq!(enabled("both-on"), None);
    assert_eq!(enabled("both-off"), None);
    assert_eq!(enabled("agree-on"), Some(true));
    assert_eq!(enabled("agree-off"), Some(false));
    assert_eq!(enabled("single"), Some(false));
    assert_eq!(out.skills.len(), 1);
    assert!(out.limits.iter().any(|limit| limit.contains("sensitive")));
    let serialized = serde_json::to_string(&out).unwrap();
    assert!(
        !serialized.contains("fixturecredential"),
        "recognized sensitive aliases, IDs and source paths must not be retained"
    );
    assert!(
        !serialized.contains("private-"),
        "configuration and instruction values must not be exported"
    );
}

#[test]
fn malformed_oversized_nonregular_and_unsafe_names_report_limits_without_contents() {
    let fixture = Fixture::new();
    fixture.put(
        "home/.codex/config.toml",
        b"private-parser-error-fixture = [",
    );
    let out = fixture.discover("codex");
    assert!(out.mcp_servers.is_empty());
    assert!(out.limits.iter().any(|limit| limit.contains("parsed")));
    assert!(!serde_json::to_string(&out)
        .unwrap()
        .contains("private-parser-error-fixture"));
    fixture.put("home/.codex/config.toml", &vec![b'x'; MAX_FILE_BYTES + 1]);
    let out = fixture.discover("codex");
    assert!(out.mcp_servers.is_empty());
    assert!(out.limits.iter().any(|limit| limit.contains("oversized")));
    fixture.put("home/.cursor/mcp.json", br#"{"mcpServers":{"bad\nname":{"command":"server-fixture"},"good":{"command":"server-fixture"}}}"#);
    let out = fixture.discover("cursor");
    assert_eq!(
        out.mcp_servers
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["good"]
    );
    assert!(out.limits.iter().any(|limit| limit.contains("unsupported")));
    let path = fixture.home.join(".cursor/mcp.json");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let out = fixture.discover("cursor");
    assert!(out.mcp_servers.is_empty());
    assert!(!out.limits.is_empty());
}

#[cfg(unix)]
#[test]
fn links_and_fifo_configs_are_skipped_without_opening_targets_or_waiting_for_writers() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let outside = fixture.put(
        "outside/config.toml",
        b"[mcp_servers.must-not-appear]\ncommand='server-fixture'\n",
    );
    fs::create_dir_all(fixture.home.join(".codex")).unwrap();
    symlink(&outside, fixture.home.join(".codex/config.toml")).unwrap();
    let skill = fixture.put("outside/skill/SKILL.md", b"private-skill-body-fixture");
    fs::create_dir_all(fixture.home.join(".agents/skills")).unwrap();
    symlink(
        skill.parent().unwrap(),
        fixture.home.join(".agents/skills/link"),
    )
    .unwrap();
    let out = fixture.discover("codex");
    assert!(out.mcp_servers.is_empty() && out.skills.is_empty());
    assert!(out.limits.iter().any(|limit| limit.contains("links")));
    fs::remove_file(fixture.home.join(".codex/config.toml")).unwrap();
    let fifo =
        std::ffi::CString::new(fixture.home.join(".codex/config.toml").to_str().unwrap()).unwrap();
    // SAFETY: the path is an owned temporary fixture destination.
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let out = fixture.discover("codex");
    assert!(out.mcp_servers.is_empty());
    assert!(out.limits.iter().any(|limit| limit.contains("Nonregular")));
    fs::remove_file(fixture.home.join(".codex/config.toml")).unwrap();
    fs::remove_dir(fixture.home.join(".codex")).unwrap();
    symlink(outside.parent().unwrap(), fixture.home.join(".codex")).unwrap();
    assert!(fixture.discover("codex").mcp_servers.is_empty());
}

#[test]
fn protected_projects_traversal_and_unsupported_products_are_explicitly_unknown() {
    let fixture = Fixture::new();
    let protected = fixture.home.join("Documents/project");
    fs::create_dir_all(protected.join(".codex")).unwrap();
    fs::write(
        protected.join(".codex/config.toml"),
        b"[mcp_servers.must-not-appear]\ncommand='server-fixture'\n",
    )
    .unwrap();
    let provider = HomeCapabilities::new(fixture.home.clone()).with_time_limit(FIXTURE_TIME_LIMIT);
    let out = provider.discover(
        "codex",
        &[protected, fixture.project.join("../unobserved")],
        "macos",
    );
    assert!(out.inspected && out.mcp_servers.is_empty());
    assert!(out
        .limits
        .iter()
        .any(|limit| limit.contains("privacy-protected")));
    let unsupported = provider.discover("unknown-product", &[], "fixture");
    assert!(!unsupported.inspected);
    assert!(!unsupported.limits.is_empty());
    let custom_only = provider.discover("aider", &[], "fixture");
    assert!(!custom_only.inspected);
    assert!(custom_only
        .limits
        .iter()
        .any(|limit| limit.contains("Cline")));
    let ordinary = NoCapabilities.discover("codex", &[], "fixture");
    assert!(!ordinary.inspected);
    assert!(!ordinary.limits.is_empty());
}

#[test]
fn capacity_and_entry_bounds_retain_coverage_limits_and_stable_ordering() {
    let fixture = Fixture::new();
    let mut servers = serde_json::Map::new();
    for index in 0..80 {
        servers.insert(
            format!("server-{index:03}"),
            serde_json::json!({"command":"server-fixture"}),
        );
    }
    fixture.put(
        "home/.cursor/mcp.json",
        serde_json::json!({"mcpServers":servers})
            .to_string()
            .as_bytes(),
    );
    let out = fixture.discover("cursor");
    assert_eq!(out.mcp_servers.len(), MAX_CAPABILITIES);
    assert_eq!(out.more_mcp_servers, 16);
    assert!(out.limits.iter().any(|limit| limit.contains("64 entries")));
    assert!(out
        .mcp_servers
        .windows(2)
        .all(|entries| entries[0].name < entries[1].name));
    for index in 0..MAX_ENTRIES + 10 {
        fixture.put(
            &format!("home/.agents/skills/skill-{index:03}/SKILL.md"),
            b"private-skill-body-fixture",
        );
    }
    let out = fixture.discover("codex");
    assert!(out.skills.len() <= MAX_CAPABILITIES);
    assert!(out.limits.iter().any(|limit| limit.contains("bound")));
}

#[test]
fn omitted_additive_fields_deserialize_to_not_inspected_and_labels_are_shared() {
    let out: AgentCapabilities = serde_json::from_str("{}").unwrap();
    assert_eq!(out, AgentCapabilities::default());
    assert!(!out.inspected && !out.limits.is_empty());
    assert_eq!(CapabilityScope::User.label(), "user");
    assert_eq!(CapabilityScope::Project.label(), "project");
    assert_eq!(CapabilityScope::Local.label(), "local");
    assert_eq!(McpTransport::Stdio.label(), "stdio");
    assert_eq!(McpTransport::Http.label(), "HTTP");
    assert_eq!(McpTransport::Sse.label(), "SSE");
    assert_eq!(McpTransport::Unknown.label(), "unknown");
}

#[test]
fn windsurf_gemini_copilot_and_cline_project_only_allowed_metadata() {
    let fixture = Fixture::new();
    fixture.put(
        "home/.codeium/windsurf/mcp_config.json",
        br#"{
            "mcpServers": {
                "local": {"command": "server-windsurf-command", "env": {"TOKEN": "private-windsurf-token"}},
                "hosted": {"serverUrl": "https://windsurf-endpoint-host.invalid/mcp"}
            }
        }"#,
    );
    fixture.put(
        "project/.codeium/windsurf/mcp_config.json",
        br#"{"mcpServers":{"must-not-appear":{"command":"server-project-windsurf"}}}"#,
    );
    fixture.put(
        "home/.gemini/settings.json",
        br#"{
            "security": {"auth": {"apiKey": "private-gemini-key-fixture"}},
            "mcpServers": {
                "local": {"command": "server-gemini-command"},
                "http": {"httpUrl": "https://gemini-http-host.invalid/mcp"},
                "events": {"url": "https://gemini-sse-host.invalid/sse"}
            }
        }"#,
    );
    fixture.put(
        "project/.gemini/settings.json",
        br#"{"mcpServers":{"workspace":{"command":"server-gemini-project"}}}"#,
    );
    fixture.put(
        "home/.gemini/mcp-server-enablement.json",
        br#"{"local": true, "secret": "private-enablement-fixture"}"#,
    );
    fixture.put(
        "home/.copilot/mcp-config.json",
        br#"{"mcpServers":{"cli":{"command":"server-copilot-cli"}},"token":"private-copilot-home"}"#,
    );
    fixture.put(
        "project/.mcp.json",
        br#"{"mcpServers":{"portable":{"type":"stdio","command":"server-portable"}}}"#,
    );
    fixture.put(
        "project/.vscode/mcp.json",
        br#"{"inputs":[{"id":"token","password":true}],"servers":{"workspace":{"type":"http","url":"https://vscode-mcp-host.invalid/"}}}"#,
    );
    fixture.put(
        "home/.cline/data/settings/cline_mcp_settings.json",
        br#"{"mcpServers":{"docs":{"command":"server-cline-command","disabled":true,"env":{"API_KEY":"private-cline-env"}}}}"#,
    );
    fixture.put(
        "home/.cline/skills/review/SKILL.md",
        b"---\nname: review\ndescription: Review diffs\n---\nprivate-cline-skill-body",
    );
    fixture.put(
        "project/.cline/skills/deploy/SKILL.md",
        b"private-cline-project-body",
    );
    fixture.put(
        "home/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
        br#"{"mcpServers":{"must-not-appear":{"command":"server-globalstorage"}}}"#,
    );

    let windsurf = fixture.discover("windsurf");
    assert!(windsurf.inspected);
    assert_eq!(windsurf.mcp_servers.len(), 2);
    assert!(!windsurf
        .mcp_servers
        .iter()
        .any(|entry| entry.name == "must-not-appear"));
    assert_eq!(
        windsurf
            .mcp_servers
            .iter()
            .find(|entry| entry.name == "hosted")
            .unwrap()
            .transport,
        McpTransport::Unknown
    );
    assert!(windsurf.skills.is_empty());
    assert!(
        !serde_json::to_string(&windsurf)
            .unwrap()
            .contains("private-"),
        "Windsurf inventory must omit commands, URLs and env"
    );

    let gemini = fixture.discover("gemini");
    assert_eq!(gemini.mcp_servers.len(), 4);
    let transport = |name: &str| {
        gemini
            .mcp_servers
            .iter()
            .find(|entry| entry.name == name)
            .unwrap()
            .transport
    };
    assert_eq!(transport("local"), McpTransport::Stdio);
    assert_eq!(transport("http"), McpTransport::Http);
    assert_eq!(transport("events"), McpTransport::Unknown);
    assert_eq!(
        gemini
            .mcp_servers
            .iter()
            .find(|entry| entry.name == "workspace")
            .unwrap()
            .scope,
        CapabilityScope::Project
    );
    assert!(gemini
        .limits
        .iter()
        .any(|limit| limit.contains("enablement")));
    assert!(
        !serde_json::to_string(&gemini).unwrap().contains("private-"),
        "Gemini inventory must omit settings secrets and endpoints"
    );

    let copilot = fixture.discover("copilot");
    assert_eq!(copilot.mcp_servers.len(), 2);
    assert!(copilot
        .mcp_servers
        .iter()
        .any(|entry| entry.name == "workspace" && entry.transport == McpTransport::Http));
    assert!(copilot
        .mcp_servers
        .iter()
        .any(|entry| entry.name == "portable"));
    assert!(!copilot.mcp_servers.iter().any(|entry| entry.name == "cli"));
    assert!(
        !serde_json::to_string(&copilot)
            .unwrap()
            .contains("private-"),
        "VS Code Copilot inventory must omit inputs, URLs and commands"
    );

    let copilot_cli = fixture.discover("copilot-cli");
    assert_eq!(copilot_cli.mcp_servers.len(), 2);
    assert!(copilot_cli
        .mcp_servers
        .iter()
        .any(|entry| entry.name == "cli"));
    assert!(copilot_cli
        .mcp_servers
        .iter()
        .any(|entry| entry.name == "portable" && entry.scope == CapabilityScope::Project));
    assert!(!copilot_cli
        .mcp_servers
        .iter()
        .any(|entry| entry.name == "workspace"));
    assert!(!serde_json::to_string(&copilot_cli)
        .unwrap()
        .contains("private-"));

    let cline = fixture.discover("cline");
    assert_eq!(cline.mcp_servers.len(), 1);
    assert_eq!(cline.mcp_servers[0].enabled, Some(false));
    assert!(!cline
        .mcp_servers
        .iter()
        .any(|entry| entry.name == "must-not-appear"));
    assert_eq!(cline.skills.len(), 2);
    assert!(cline.skills.iter().any(|skill| skill.name == "review"));
    assert!(cline
        .skills
        .iter()
        .any(|skill| { skill.name == "deploy" && skill.scope == CapabilityScope::Project }));
    assert!(
        !serde_json::to_string(&cline).unwrap().contains("private-"),
        "Cline inventory must omit commands, env and skill bodies"
    );
}

#[test]
fn gemini_does_not_resolve_overlapping_transport_keys() {
    let fixture = Fixture::new();
    fixture.put(
        "home/.gemini/settings.json",
        br#"{"mcpServers":{"both":{"command":"server-command","httpUrl":"https://http-host.invalid/"}}}"#,
    );
    let out = fixture.discover("gemini");
    assert_eq!(out.mcp_servers.len(), 1);
    assert_eq!(out.mcp_servers[0].transport, McpTransport::Unknown);
    assert!(!serde_json::to_string(&out).unwrap().contains("private-"));
}

#[test]
fn copilot_cli_accepts_documented_local_and_bare_project_maps_without_resolving_precedence() {
    let fixture = Fixture::new();
    fixture.put(
        "home/.copilot/mcp-config.json",
        br#"{"mcpServers":{"shared":{"type":"local","command":"server-command","env":{"TOKEN":"private-token"}}}}"#,
    );
    fixture.put(
        "project/.mcp.json",
        br#"{"shared":{"type":"local","command":"server-command"}}"#,
    );
    fixture.put(
        "project/.github/mcp.json",
        br#"{"mcpServers":{"shared":{"type":"http","url":"https://endpoint-host.invalid/mcp","headers":{"Authorization":"private-header"}}}}"#,
    );
    let out = fixture.discover("copilot-cli");
    assert_eq!(out.mcp_servers.len(), 3);
    assert!(out.mcp_servers.iter().all(|entry| entry.name == "shared"));
    assert_eq!(
        out.mcp_servers
            .iter()
            .filter(|entry| entry.transport == McpTransport::Stdio)
            .count(),
        2
    );
    assert_eq!(
        out.mcp_servers
            .iter()
            .map(|entry| &entry.id)
            .collect::<HashSet<_>>()
            .len(),
        3
    );
    assert!(!serde_json::to_string(&out).unwrap().contains("private-"));

    fixture.put("project/.mcp.json", br#"{"mcpServers":false}"#);
    let invalid = fixture.discover("copilot-cli");
    assert_eq!(invalid.mcp_servers.len(), 2);
    assert!(!invalid
        .mcp_servers
        .iter()
        .any(|entry| entry.name == "mcpServers"));
}

#[test]
fn cline_http_and_legacy_sse_follow_its_own_schema() {
    let fixture = Fixture::new();
    fixture.put(
        "home/.cline/data/settings/cline_mcp_settings.json",
        br#"{"mcpServers":{
            "http":{"type":"streamableHttp","url":"https://http-host.invalid/","disabled":false},
            "legacy":{"url":"https://sse-host.invalid/","disabled":true},
            "events":{"type":"sse","url":"https://events-host.invalid/"},
            "other-client-key":{"httpUrl":"https://other-host.invalid/"},
            "overlap":{"type":"streamableHttp","command":"server-command","url":"https://both-host.invalid/"},
            "malformed-type":{"type":false,"command":"server-command"}
        }}"#,
    );
    let out = fixture.discover("cline");
    let entry = |name: &str| {
        out.mcp_servers
            .iter()
            .find(|entry| entry.name == name)
            .unwrap()
    };
    assert_eq!(entry("http").transport, McpTransport::Http);
    assert_eq!(entry("http").enabled, Some(true));
    assert_eq!(entry("legacy").transport, McpTransport::Sse);
    assert_eq!(entry("legacy").enabled, Some(false));
    assert_eq!(entry("events").transport, McpTransport::Sse);
    for name in ["other-client-key", "overlap", "malformed-type"] {
        assert_eq!(entry(name).transport, McpTransport::Unknown);
    }
    assert!(!serde_json::to_string(&out).unwrap().contains("private-"));
}

#[test]
fn jsonc_settings_preserve_strings_and_omit_comments_endpoints_and_other_private_fields() {
    let fixture = Fixture::new();
    fixture.put(
        "project/.vscode/mcp.json",
        br#"{
            // private-comment
            "servers": {
                "local": {"command":"server-command","args":["private-argument",],},
                "remote": {"type":"http","url":"https://endpoint-host.invalid/a/*x*/?q=//","headers":{"Authorization":"private-header"}},
            },
            "inputs": [{"id":"private-input","password":true},],
        }"#,
    );
    fixture.put(
        "project/.mcp.json",
        br#"{"mcpServers":{"portable":{"type":"local","command":"server-command"}}, /* private-comment */}"#,
    );
    let vscode = fixture.discover("copilot");
    assert_eq!(vscode.mcp_servers.len(), 3);
    assert!(vscode
        .mcp_servers
        .iter()
        .any(|entry| entry.name == "portable" && entry.transport == McpTransport::Stdio));
    assert!(!serde_json::to_string(&vscode).unwrap().contains("private-"));

    fixture.put(
        "home/.gemini/settings.json",
        br#"{/* private-comment */"mcpServers":{"remote":{"httpUrl":"https://endpoint-host.invalid/a/*x*/?q=//"}}}// private-tail"#,
    );
    let gemini = fixture.discover("gemini");
    assert_eq!(gemini.mcp_servers.len(), 1);
    assert_eq!(gemini.mcp_servers[0].transport, McpTransport::Http);
    assert!(!serde_json::to_string(&gemini).unwrap().contains("private-"));
    fixture.put(
        "home/.gemini/settings.json",
        br#"{"mcpServers":{"remote":{"httpUrl":"https://endpoint-host.invalid/"},}}"#,
    );
    let invalid = fixture.discover("gemini");
    assert!(invalid.mcp_servers.is_empty());
    assert!(invalid.limits.iter().any(|limit| limit.contains("parsed")));
}

#[test]
fn jsonc_lexer_keeps_json_grammar_and_strict_formats_fail_closed() {
    let input = r#"{/* comment */"url":"https://example.invalid/a/*x*/?q=//", "quoted":"é \\\" // /* ,]", "items":[1,],}// tail"#;
    let bytes = jsonc_bytes(input, true).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(value["url"] == "https://example.invalid/a/*x*/?q=//");
    assert!(value["quoted"] == "é \\\" // /* ,]");
    assert!(value["items"] == serde_json::json!([1]));
    for input in [
        "{,}",
        "[,]",
        "[1,,]",
        "{\"a\":,}",
        "{\"a\":tru/*x*/e}",
        "{/* unterminated",
        "{\"a\":\"unterminated// }",
        "[1] /* nested /* x */ */",
    ] {
        let parsed = jsonc_bytes(input, true)
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        assert!(parsed.is_none(), "malformed JSONC must remain rejected");
    }
    assert!(jsonc_bytes(&" ".repeat(MAX_FILE_BYTES + 1), true).is_none());

    let fixture = Fixture::new();
    fixture.put(
        "home/.cursor/mcp.json",
        br#"{/* private-comment */"mcpServers":{"must-not-appear":{"command":"server-command"}}}"#,
    );
    let strict = fixture.discover("cursor");
    assert!(strict.mcp_servers.is_empty());
    assert!(!serde_json::to_string(&strict).unwrap().contains("private-"));
}

#[test]
fn json_transport_does_not_apply_another_clients_fields_or_infer_from_malformed_types() {
    let fixture = Fixture::new();
    fixture.put(
        "project/.mcp.json",
        br#"{"mcpServers":{
            "other-client":{"type":"http","httpUrl":"https://http-host.invalid/"},
            "extra-field":{"type":"http","url":"https://http-host.invalid/","serverUrl":"https://other-host.invalid/"},
            "malformed-type":{"type":false,"command":"server-command"},
            "empty-command":{"type":"http","url":"https://http-host.invalid/","command":""}
        }}"#,
    );
    let out = fixture.discover("claude-code");
    assert_eq!(out.mcp_servers.len(), 4);
    assert!(out
        .mcp_servers
        .iter()
        .all(|entry| entry.transport == McpTransport::Unknown));
    assert!(!serde_json::to_string(&out).unwrap().contains("private-"));
}

#[test]
fn gemini_and_cursor_untyped_urls_do_not_prove_a_remote_transport() {
    let fixture = Fixture::new();
    let config = br#"{"mcpServers":{
        "http":{"type":"http","url":"https://http-host.invalid/"},
        "events":{"type":"sse","url":"https://events-host.invalid/"},
        "negotiated":{"url":"https://negotiated-host.invalid/"},
        "malformed":{"type":true,"url":"https://malformed-host.invalid/"}
    }}"#;
    for (product, path) in [
        ("gemini", "home/.gemini/settings.json"),
        ("cursor", "home/.cursor/mcp.json"),
        ("cursor-agent", "home/.cursor/mcp.json"),
    ] {
        fixture.put(path, config);
        let out = fixture.discover(product);
        assert_eq!(out.mcp_servers.len(), 4);
        let transport = |name: &str| {
            out.mcp_servers
                .iter()
                .find(|entry| entry.name == name)
                .unwrap()
                .transport
        };
        assert_eq!(transport("http"), McpTransport::Http);
        assert_eq!(transport("events"), McpTransport::Sse);
        assert_eq!(transport("negotiated"), McpTransport::Unknown);
        assert_eq!(transport("malformed"), McpTransport::Unknown);
        assert!(!serde_json::to_string(&out).unwrap().contains("private-"));
    }
}

#[test]
fn clients_with_separate_enablement_settings_do_not_reinterpret_arbitrary_server_booleans() {
    let fixture = Fixture::new();
    let config = br#"{"mcpServers":{
        "on":{"command":"server-command","enabled":true,"disabled":false},
        "off":{"command":"server-command","enabled":false,"disabled":true}
    }}"#;
    for (product, path) in [
        ("gemini", "home/.gemini/settings.json"),
        ("copilot", "project/.mcp.json"),
        ("copilot-cli", "home/.copilot/mcp-config.json"),
    ] {
        fixture.put(path, config);
        let out = fixture.discover(product);
        assert!(!out.mcp_servers.is_empty());
        assert!(out.mcp_servers.iter().all(|entry| entry.enabled.is_none()));
        assert!(out.limits.iter().any(|limit| limit.contains("enablement")));
        assert!(!serde_json::to_string(&out).unwrap().contains("private-"));
    }
    fixture.put(
        "home/.cline/data/settings/cline_mcp_settings.json",
        br#"{"mcpServers":{"unsupported":{"command":"server-command","enabled":false}}}"#,
    );
    let out = fixture.discover("cline");
    assert_eq!(out.mcp_servers.len(), 1);
    assert_eq!(out.mcp_servers[0].enabled, None);
}

#[test]
fn realistic_claude_code_config_reports_launch_policy_and_instruction_metadata_only() {
    let fixture = Fixture::new();
    let project = fixture.project.to_str().unwrap().to_owned();
    // Shapes follow ~/.claude.json, .mcp.json and settings.json as Claude Code writes them.
    let user = serde_json::json!({
        "numStartups": 12,
        "oauthAccount": {"emailAddress": "private-email-fixture"},
        "mcpServers": {
            "github": {"type": "stdio", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"],
                        "env": {"GITHUB_PERSONAL_ACCESS_TOKEN": "private-pat-fixture"}},
            "playwright": {"command": "npx", "args": ["@playwright/mcp@0.0.41"]},
            "linear": {"type": "http", "url": "https://mcp.linear.app/mcp?key=private-query-fixture",
                        "headers": {"Authorization": "Bearer private-header-fixture"}}
        },
        "projects": {project.clone(): {"mcpServers": {
            "postgres": {"command": "docker", "args": ["run", "-i", "--rm", "-e", "DATABASE_URL", "mcp/postgres:1.2"],
                          "env": {"DATABASE_URL": "postgres://private-user:private-pass@db/x"}}
        }, "history": [{"display": "private-prompt-fixture"}]}}
    });
    fixture.put("home/.claude.json", user.to_string().as_bytes());
    fixture.put(
        "project/.mcp.json",
        br#"{"mcpServers":{"fetch":{"command":"uvx","args":["mcp-server-fetch"]}}}"#,
    );
    fixture.put("home/.claude/settings.json", br#"{"permissions":{"allow":["Bash(npm run test:*)","Read(~/.zshrc)"],"deny":["Bash(curl:*)","Bash(export TOKEN=private-token-fixture)"],"defaultMode":"acceptEdits"},"env":{"ANTHROPIC_API_KEY":"private-env-fixture"}}"#);
    fixture.put(
        "project/.claude/settings.local.json",
        br#"{"permissions":{"ask":["WebFetch"],"defaultMode":"private-unknown-mode"}}"#,
    );
    fixture.put(
        "home/.claude/commands/ship.md",
        b"---\ndescription: Run checks and open a PR\n---\nprivate-command-body-fixture",
    );
    fixture.put("home/.claude/agents/reviewer.md", b"---\nname: reviewer\ndescription: Reviews diffs for bugs\ntools: Read, Grep\n---\nprivate-agent-body-fixture");
    fixture.put("project/CLAUDE.md", b"private-memory-body-fixture");
    let out = fixture.discover("claude-code");

    let server = |name: &str| out.mcp_servers.iter().find(|s| s.name == name).unwrap();
    let github = server("github");
    assert_eq!(github.launch.as_ref().unwrap().runner, McpRunner::Npx);
    assert_eq!(
        github.launch.as_ref().unwrap().package.as_deref(),
        Some("@modelcontextprotocol/server-github")
    );
    assert_eq!(github.launch.as_ref().unwrap().pinned, Some(false));
    assert_eq!(github.env_names, ["GITHUB_PERSONAL_ACCESS_TOKEN"]);
    assert_eq!(
        server("playwright").launch.as_ref().unwrap().pinned,
        Some(true)
    );
    let linear = server("linear");
    assert_eq!(
        (linear.transport, linear.remote_host.as_deref()),
        (McpTransport::Http, Some("mcp.linear.app"))
    );
    assert_eq!(linear.header_names, ["Authorization"]);
    assert!(linear.launch.is_none());
    let postgres = server("postgres");
    assert_eq!(postgres.scope, CapabilityScope::Local);
    assert_eq!(postgres.launch.as_ref().unwrap().runner, McpRunner::Docker);
    assert_eq!(postgres.launch.as_ref().unwrap().pinned, Some(true));
    assert_eq!(
        server("fetch").launch.as_ref().unwrap().runner,
        McpRunner::Uvx
    );

    let rules: Vec<_> = out
        .tool_rules
        .iter()
        .map(|r| (r.rule.as_str(), r.decision))
        .collect();
    assert_eq!(
        rules,
        [
            ("Bash(npm run test:*)", RuleDecision::Allow),
            ("Read(~/.zshrc)", RuleDecision::Allow),
            ("Bash(curl:*)", RuleDecision::Deny),
            ("WebFetch", RuleDecision::Ask),
        ]
    );
    assert_eq!(out.settings.len(), 1, "unknown values are not exported");
    assert_eq!(
        (out.settings[0].key.as_str(), out.settings[0].value.as_str()),
        ("permissions.defaultMode", "acceptEdits")
    );

    let instructions: Vec<_> = out
        .instructions
        .iter()
        .map(|i| (i.kind, i.name.as_str(), i.description.as_deref()))
        .collect();
    assert_eq!(
        instructions,
        [
            (
                InstructionKind::Command,
                "ship",
                Some("Run checks and open a PR")
            ),
            (
                InstructionKind::Subagent,
                "reviewer",
                Some("Reviews diffs for bugs")
            ),
            (InstructionKind::Memory, "CLAUDE.md", None),
        ]
    );
    assert!(out.builtin_tools.contains(&"Bash".to_owned()));
    let encoded = serde_json::to_string(&out).unwrap();
    assert!(!encoded.contains("private-"), "{encoded}");
}

#[test]
fn realistic_codex_cursor_and_gemini_configs_report_settings_rules_and_prompts() {
    let fixture = Fixture::new();
    fixture.put(
        "home/.codex/config.toml",
        br#"
model = "private-model-fixture"
approval_policy = "on-request"
sandbox_mode = "workspace-write"
[mcp_servers.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp@1.0.14"]
env_vars = ["CONTEXT7_API_KEY"]
[mcp_servers.figma]
url = "http://127.0.0.1:3845/mcp"
env_http_headers = { "X-Figma-Token" = "FIGMA_TOKEN" }
"#,
    );
    fixture.put(
        "home/.codex/prompts/release.md",
        b"---\ndescription: Draft release notes\n---\nprivate-prompt-body",
    );
    fixture.put("project/AGENTS.md", b"private-agents-body");
    let codex = fixture.discover("codex");
    let context7 = codex
        .mcp_servers
        .iter()
        .find(|s| s.name == "context7")
        .unwrap();
    assert_eq!(context7.env_names, ["CONTEXT7_API_KEY"]);
    assert_eq!(context7.launch.as_ref().unwrap().pinned, Some(true));
    let figma = codex
        .mcp_servers
        .iter()
        .find(|s| s.name == "figma")
        .unwrap();
    assert_eq!(figma.remote_host.as_deref(), Some("127.0.0.1:3845"));
    assert_eq!(figma.header_names, ["X-Figma-Token"]);
    let settings: Vec<_> = codex
        .settings
        .iter()
        .map(|s| (s.key.as_str(), s.value.as_str()))
        .collect();
    assert_eq!(
        settings,
        [
            ("approval_policy", "on-request"),
            ("sandbox_mode", "workspace-write")
        ]
    );
    let names: Vec<_> = codex
        .instructions
        .iter()
        .map(|i| (i.kind, i.name.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            (InstructionKind::Prompt, "release"),
            (InstructionKind::Memory, "AGENTS.md")
        ]
    );

    fixture.put("project/.cursor/rules/style.mdc", b"---\ndescription: TypeScript style\nglobs: src/**/*.ts\nalwaysApply: false\n---\nprivate-rule-body");
    fixture.put("home/.cursor/cli-config.json", br#"{"permissions":{"allow":["Shell(git)"],"deny":["Shell(rm)"]},"private":"private-cursor"}"#);
    let cursor = fixture.discover("cursor");
    assert!(cursor
        .instructions
        .iter()
        .any(|i| i.kind == InstructionKind::Rule
            && i.name == "style"
            && i.description.as_deref() == Some("TypeScript style")));
    assert_eq!(cursor.tool_rules.len(), 2);

    fixture.put("home/.gemini/settings.json", br#"{"mcpServers":{"local":{"command":"node","args":["/srv/private-dir/server.js"]}},"tools":{"exclude":["run_shell_command(rm)"]}}"#);
    fixture.put(
        "home/.gemini/commands/plan.toml",
        b"description = \"Plan a change\"\nprompt = \"private-gemini-prompt\"\n",
    );
    let gemini = fixture.discover("gemini");
    assert_eq!(
        gemini.mcp_servers[0]
            .launch
            .as_ref()
            .unwrap()
            .package
            .as_deref(),
        Some("server.js")
    );
    assert_eq!(gemini.tool_rules[0].decision, RuleDecision::Deny);
    assert!(gemini
        .instructions
        .iter()
        .any(|i| i.name == "plan" && i.description.as_deref() == Some("Plan a change")));
    for out in [codex, cursor, gemini] {
        let encoded = serde_json::to_string(&out).unwrap();
        assert!(!encoded.contains("private-"), "{encoded}");
    }
}

#[test]
fn the_time_bound_still_stops_the_inventory_and_says_so() {
    let fixture = Fixture::new();
    fixture.put(
        "home/.codex/config.toml",
        b"[mcp_servers.docs]\ncommand = \"server-command-fixture\"\n",
    );
    let out = HomeCapabilities::new(fixture.home.clone())
        .with_time_limit(Duration::ZERO)
        .discover("codex", std::slice::from_ref(&fixture.project), "fixture");
    assert!(out.mcp_servers.is_empty());
    assert!(
        out.limits.iter().any(|l| l.contains("time bound")),
        "{:?}",
        out.limits
    );
    assert_eq!(
        HomeCapabilities::new(fixture.home.clone()).time_limit,
        MAX_TIME
    );
}

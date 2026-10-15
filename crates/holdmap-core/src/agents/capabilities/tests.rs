use super::*;
use std::fs;

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
        HomeCapabilities::new(self.home.clone()).discover(
            product,
            std::slice::from_ref(&self.project),
            "fixture",
        )
    }
}

#[test]
fn codex_projects_only_allowed_metadata_and_never_exports_skill_instructions() {
    let fixture = Fixture::new();
    fixture.put("home/.codex/config.toml", br#"
model = "private-model-fixture"
[mcp_servers.docs]
command = "private-command-fixture"
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
url = "https://private-project-endpoint-fixture.invalid/"
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
        "mcpServers": {"shared": {"command": "private-command-fixture"}},
        "projects": {
            fixture.project.to_str().unwrap(): {"mcpServers": {"shared": {"type":"http", "url":"https://private-local-fixture.invalid/"}}},
            unrelated.to_str().unwrap(): {"mcpServers": {"must-not-appear": {"command":"private-unrelated-fixture"}}}
        },
        "oauthAccount": {"token":"private-token-fixture"}
    });
    fixture.put("home/.claude.json", user.to_string().as_bytes());
    fixture.put("project/.mcp.json", br#"{"mcpServers":{"shared":{"type":"sse","url":"https://private-project-fixture.invalid/"},"invalid-transport":{"url":"https://private-untyped-fixture.invalid/"}}}"#);
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
    fixture.put("home/.cursor/mcp.json", br#"{"mcpServers":{"remote":{"url":"https://private-remote-fixture.invalid/"},"off":{"command":"private-command-fixture","disabled":true}}}"#);
    fixture.put("project/.cursor/mcp.json", br#"{"mcpServers":{"remote":{"type":"http","url":"https://private-project-fixture.invalid/"}}}"#);
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
    let config = br#"{"mcpServers":{"local server":{"command":"private-command-fixture","args":["private-argument-fixture"]}},"preferences":{"token":"private-token-fixture"}}"#;
    let mac = fixture.put(
        "home/Library/Application Support/Claude/claude_desktop_config.json",
        config,
    );
    let windows = fixture.put("roaming/Claude/claude_desktop_config.json", config);
    fixture.put(
        "home/.claude.json",
        br#"{"mcpServers":{"code-only":{"command":"private-code-fixture"}}}"#,
    );
    fixture.put(
        "home/.claude/skills/review/SKILL.md",
        b"private-skill-body-fixture",
    );
    for (platform, expected) in [("macos", mac), ("windows", windows)] {
        let provider = HomeCapabilities::with_app_data(
            fixture.home.clone(),
            fixture.home.parent().unwrap().join("roaming"),
        );
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
fn project_mcp_inventory_is_collected_before_skill_headers_exhaust_the_shared_budget() {
    let fixture = Fixture::new();
    fixture.put(
        "project/.cursor/mcp.json",
        br#"{"mcpServers":{"project-server":{"command":"private-command-fixture"}}}"#,
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
        "both-on":{"command":"private-command-fixture","enabled":true,"disabled":true},
        "both-off":{"command":"private-command-fixture","enabled":false,"disabled":false},
        "agree-on":{"command":"private-command-fixture","enabled":true,"disabled":false},
        "agree-off":{"command":"private-command-fixture","enabled":false,"disabled":true},
        "single":{"command":"private-command-fixture","disabled":true},
        "Bearer fixturecredential":{"command":"private-command-fixture"},
        "sk-fixturecredential":{"command":"private-command-fixture"},
        "github_pat_fixturecredential":{"command":"private-command-fixture"}
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
    fixture.put("home/.cursor/mcp.json", br#"{"mcpServers":{"bad\nname":{"command":"private-fixture"},"good":{"command":"private-fixture"}}}"#);
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
        b"[mcp_servers.must-not-appear]\ncommand='private-fixture'\n",
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
        b"[mcp_servers.must-not-appear]\ncommand='private-fixture'\n",
    )
    .unwrap();
    let provider = HomeCapabilities::new(fixture.home.clone());
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
            serde_json::json!({"command":"private-fixture"}),
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

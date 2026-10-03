//! Repository hygiene checks that run with `cargo test`:
//!
//! * **File naming**: every tracked file follows the conventions in `CONTRIBUTING.md`
//!   (§ Naming conventions).
//! * **Markdown links**: every relative link and image in the repo's Markdown points at a file
//!   that exists, and every `#anchor` matches a heading in the target document.
//! * **Screenshots**: every image in `docs/screenshots/` is used by at least one document.
//!
//! The file list comes from `git ls-files`, so ignored build output is never checked. Outside a
//! git checkout (for example a crates.io tarball) the checks are skipped.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Tracked and new (not ignored) files, relative to the repo root, with `/` separators.
fn repo_files() -> Option<Vec<String>> {
    let out = Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(repo_root())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let root = repo_root();
    let files: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty() && root.join(l).exists())
        .map(str::to_owned)
        .collect();
    (!files.is_empty()).then_some(files)
}

fn is_snake(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !s.starts_with('_')
}

fn is_kebab(s: &str) -> bool {
    !s.is_empty()
        && s.split('-').all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn is_pascal(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_screenshot_name(stem: &str) -> bool {
    ["desktop-", "cli-", "tui-"]
        .iter()
        .any(|p| stem.starts_with(p))
        && (stem.ends_with("-light") || stem.ends_with("-dark"))
        && is_kebab(stem)
}

/// Root documents that keep the conventional UPPERCASE names.
const ROOT_DOCS: &[&str] = &[
    "README.md",
    "CHANGELOG.md",
    "CONTRIBUTING.md",
    "SECURITY.md",
    "CODE_OF_CONDUCT.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
];

/// Returns why `path` breaks the naming rules, or `None` when it is fine.
fn naming_problem(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    let name = *parts.last().unwrap();
    let dirs = &parts[..parts.len() - 1];
    let (stem, ext) = match name.split_once('.') {
        Some((s, e)) => (s, e),
        None => (name, ""),
    };
    let bad = |rule: &str| Some(format!("{path}: {rule}"));

    // Names fixed by external tools or GitHub.
    if path.starts_with("apps/desktop/src-tauri/icons/")
        || path.starts_with(".github/ISSUE_TEMPLATE/") && name == "config.yml"
        || name == "PULL_REQUEST_TEMPLATE.md"
        || name.starts_with('.')
        || matches!(
            name,
            "Cargo.toml" | "Cargo.lock" | "package.json" | "package-lock.json" | "tsconfig.json"
        )
    {
        return None;
    }
    if dirs.is_empty() {
        if ext == "md" || name.starts_with("LICENSE") {
            return (!ROOT_DOCS.contains(&name))
                .then(|| format!("{path}: unexpected root document"));
        }
        return (!is_kebab(stem)).then(|| format!("{path}: root files are kebab-case"));
    }
    if dirs[0] == "crates" && dirs.len() >= 2 && !is_kebab(dirs[1]) {
        return bad("crate directories are kebab-case");
    }
    // Module directories are the ones below `src/` (`src-tauri` is Tauri's fixed name).
    let below_src = || dirs.iter().skip_while(|d| **d != "src").skip(1);
    let rule = match ext {
        "rs" if !is_snake(stem) || below_src().any(|d| !is_snake(d)) => {
            Some("Rust modules, files and module directories are snake_case")
        }
        "svelte" if !is_pascal(stem) => Some("Svelte components are PascalCase.svelte"),
        "ts" | "js" | "test.ts" if path.starts_with("apps/desktop/src/") && !is_kebab(stem) => {
            Some("TypeScript modules are kebab-case (tests: <module>.test.ts)")
        }
        "md" if dirs == ["docs"] && !is_kebab(stem) => Some("files under docs/ are kebab-case"),
        "png" if dirs == ["docs", "screenshots"] && !is_screenshot_name(stem) => Some(
            "screenshots are <surface>-<view>-<theme>.png (surface: desktop|cli|tui, theme: light|dark)",
        ),
        "yml" | "yaml" | "sh" if !is_kebab(stem) => Some("workflows and scripts are kebab-case"),
        _ => None,
    };
    if let Some(rule) = rule {
        return bad(rule);
    }
    if path.starts_with("apps/desktop/src/assets/") && !is_kebab(stem) {
        return bad("assets are kebab-case");
    }
    if path.starts_with("crates/") && dirs.contains(&"fixtures") && !is_snake(stem) {
        return bad("test fixtures are snake_case");
    }
    None
}

#[test]
fn file_names_follow_the_conventions() {
    let Some(files) = repo_files() else {
        eprintln!("not a git checkout; skipping");
        return;
    };
    let bad: Vec<String> = files.iter().filter_map(|f| naming_problem(f)).collect();
    assert!(
        bad.is_empty(),
        "naming convention violations:\n  {}",
        bad.join("\n  ")
    );
}

#[test]
fn naming_rules_catch_common_mistakes() {
    assert!(naming_problem("crates/portwise-core/src/topoBuilder.rs").is_some());
    assert!(naming_problem("apps/desktop/src/components/portRow.svelte").is_some());
    assert!(naming_problem("apps/desktop/src/lib/myModule.ts").is_some());
    assert!(naming_problem("docs/USER_GUIDE.md").is_some());
    assert!(naming_problem("docs/screenshots/ui-list.png").is_some());
    assert!(naming_problem("scripts/demo_servers.sh").is_some());
    assert!(naming_problem("NOTES.md").is_some());
    assert!(naming_problem("apps/desktop/src/lib/rows.test.ts").is_none());
    assert!(naming_problem("docs/screenshots/desktop-graph-dark.png").is_none());
    assert!(naming_problem("apps/desktop/src-tauri/icons/Square44x44Logo.png").is_none());
}

/// GitHub's heading anchor: lowercase, punctuation dropped, spaces to dashes.
fn slug(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// Anchors defined by a Markdown document (headings outside code fences, with GitHub's `-1`
/// suffixes for duplicates).
fn anchors(md: &str) -> BTreeSet<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = BTreeSet::new();
    let mut fence = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        let t = line.trim_start();
        if t.starts_with('#') {
            let text = t.trim_start_matches('#').trim().replace('`', "");
            // Strip inline link syntax: [text](url) -> text
            let text = strip_links(&text);
            let base = slug(&text);
            let n = seen.entry(base.clone()).or_insert(0);
            out.insert(if *n == 0 {
                base.clone()
            } else {
                format!("{base}-{n}")
            });
            *n += 1;
        }
    }
    out
}

fn strip_links(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match (after.find("]("), after.find(')')) {
            (Some(j), Some(k)) if j < k => {
                out.push_str(&after[..j]);
                rest = &after[k + 1..];
            }
            _ => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Link and image targets in a Markdown document, skipping code fences and inline code.
fn links(md: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fence = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        // Drop inline code spans.
        let mut text = String::new();
        for (i, part) in line.split('`').enumerate() {
            if i % 2 == 0 {
                text.push_str(part);
            }
        }
        let mut rest = text.as_str();
        while let Some(i) = rest.find("](") {
            let after = &rest[i + 2..];
            let end = after.find(')').unwrap_or(after.len());
            let target = after[..end].split_whitespace().next().unwrap_or("");
            out.push(target.to_owned());
            rest = &after[end..];
        }
        let mut rest = text.as_str();
        while let Some(i) = rest.find("src=\"") {
            let after = &rest[i + 5..];
            let end = after.find('"').unwrap_or(after.len());
            out.push(after[..end].to_owned());
            rest = &after[end..];
        }
    }
    out
}

#[test]
fn markdown_links_resolve() {
    let Some(files) = repo_files() else {
        eprintln!("not a git checkout; skipping");
        return;
    };
    let root = repo_root();
    let mut problems = Vec::new();
    for doc in files.iter().filter(|f| f.ends_with(".md")) {
        let text = std::fs::read_to_string(root.join(doc)).unwrap();
        let dir = root.join(doc).parent().unwrap().to_path_buf();
        for link in links(&text) {
            if link.is_empty()
                || link.starts_with("http://")
                || link.starts_with("https://")
                || link.starts_with("mailto:")
            {
                continue;
            }
            let (file, anchor) = match link.split_once('#') {
                Some((f, a)) => (f, Some(a)),
                None => (link.as_str(), None),
            };
            let target = if file.is_empty() {
                root.join(doc)
            } else {
                dir.join(file)
            };
            if !target.exists() {
                problems.push(format!("{doc}: broken link {link}"));
                continue;
            }
            if let Some(a) = anchor {
                if target.extension().is_some_and(|e| e == "md") {
                    let body = std::fs::read_to_string(&target).unwrap();
                    if !anchors(&body).contains(a) {
                        problems.push(format!(
                            "{doc}: no heading for #{a} in {}",
                            file_or_self(file)
                        ));
                    }
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "markdown link problems:\n  {}",
        problems.join("\n  ")
    );
}

fn file_or_self(f: &str) -> &str {
    if f.is_empty() {
        "the same file"
    } else {
        f
    }
}

#[test]
fn every_screenshot_is_used() {
    let Some(files) = repo_files() else {
        eprintln!("not a git checkout; skipping");
        return;
    };
    let root = repo_root();
    let docs: String = files
        .iter()
        .filter(|f| f.ends_with(".md"))
        .map(|f| std::fs::read_to_string(root.join(f)).unwrap())
        .collect();
    let unused: Vec<&String> = files
        .iter()
        .filter(|f| f.starts_with("docs/screenshots/"))
        .filter(|f| !docs.contains(f.trim_start_matches("docs/")))
        .collect();
    assert!(
        unused.is_empty(),
        "screenshots no document uses (delete them):\n  {unused:?}"
    );
}

#[test]
fn anchor_slugs_match_github() {
    assert_eq!(slug("MCP setup"), "mcp-setup");
    assert_eq!(slug("FAQ / troubleshooting"), "faq--troubleshooting");
    assert_eq!(
        slug("Install (macOS, Linux, Windows)"),
        "install-macos-linux-windows"
    );
    let a = anchors("# A\n## Usage\n## Usage\n```\n# not\n```\n");
    assert!(a.contains("usage") && a.contains("usage-1") && !a.contains("not"));
}

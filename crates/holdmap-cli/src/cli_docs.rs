//! Keeps the written CLI documentation in step with the clap definitions.
//!
//! * `docs/cli.md` is generated from `--help` of every command. The test fails when the file is
//!   stale; regenerate it with `HOLDMAP_BLESS=1 cargo test -p holdmap cli_reference`.
//! * The README must mention every command, and its CLI section may only use flags that exist.

use super::Cli;
use clap::CommandFactory;
use std::path::PathBuf;

const WIDTH: usize = 100;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn visible_subcommands(cmd: &clap::Command) -> Vec<&clap::Command> {
    cmd.get_subcommands()
        .filter(|c| !c.is_hide_set() && c.get_name() != "help")
        .collect()
}

/// Render the Markdown reference for the whole CLI.
pub fn reference() -> String {
    let mut root = Cli::command().term_width(WIDTH).max_term_width(WIDTH);
    root.build();
    let mut out = String::new();
    out.push_str("# holdmap CLI reference\n\n");
    out.push_str(
        "<!-- Generated from the clap definitions in crates/holdmap-cli. Do not edit by hand:\n     \
         HOLDMAP_BLESS=1 cargo test -p holdmap cli_reference -->\n\n",
    );
    out.push_str(&format!(
        "This page is the `--help` output of every command of holdmap {}. `cargo test` checks it\n\
         against the code, so it never drifts. For an overview see the [README](../README.md).\n\n",
        env!("CARGO_PKG_VERSION")
    ));
    out.push_str(
        "Global options (accepted by every command): `--color <auto|always|never>` \
                  (env `HOLDMAP_COLOR`; `NO_COLOR` and `CLICOLOR_FORCE` are honoured too) and \
                  `--no-docker`.\n\n",
    );
    out.push_str("| Command | What it does |\n|---|---|\n");
    for sub in visible_subcommands(&root) {
        let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
        let aliases: Vec<_> = sub.get_visible_aliases().collect();
        let alias = if aliases.is_empty() {
            String::new()
        } else {
            format!(" (alias `{}`)", aliases.join("`, `"))
        };
        out.push_str(&format!(
            "| [`holdmap {name}`](#holdmap-{name}){alias} | {} |\n",
            about.trim_end_matches('.').replace('|', "\\|"),
            name = sub.get_name()
        ));
    }
    out.push_str("\n## holdmap\n\n```text\n");
    out.push_str(root.render_long_help().to_string().trim_end());
    out.push_str("\n```\n");
    let names: Vec<String> = visible_subcommands(&root)
        .iter()
        .map(|c| c.get_name().to_string())
        .collect();
    for name in names {
        let sub = root.find_subcommand_mut(&name).expect("subcommand");
        let help = sub.render_help().to_string();
        out.push_str(&format!(
            "\n## holdmap {name}\n\n```text\n{}\n```\n",
            help.trim_end()
        ));
    }
    // Clap pads some lines with trailing spaces; Markdown linters (and editors) strip them.
    out.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn cli_reference_is_current() {
    let path = repo_root().join("docs/cli.md");
    let want = reference();
    if std::env::var_os("HOLDMAP_BLESS").is_some() {
        std::fs::write(&path, &want).unwrap();
        return;
    }
    // Normalise line endings: a Windows checkout may have converted the file to CRLF.
    let have = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        have == want,
        "docs/cli.md is out of date with the clap definitions.\n\
         Regenerate it with: HOLDMAP_BLESS=1 cargo test -p holdmap cli_reference"
    );
}

#[test]
fn readme_mentions_every_command() {
    let readme = std::fs::read_to_string(repo_root().join("README.md")).unwrap();
    let root = Cli::command();
    let missing: Vec<_> = visible_subcommands(&root)
        .iter()
        .map(|c| c.get_name())
        .filter(|n| !readme.contains(&format!("holdmap {n}")))
        .collect();
    assert!(missing.is_empty(), "README.md doesn't mention: {missing:?}");
}

#[test]
fn readme_flags_exist() {
    // Every `--flag` the README's CLI section uses must exist on some command.
    let readme = std::fs::read_to_string(repo_root().join("README.md")).unwrap();
    let mut known: std::collections::BTreeSet<String> = ["--help", "--version"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let root = Cli::command();
    let mut stack = vec![&root];
    while let Some(c) = stack.pop() {
        for a in c.get_arguments() {
            if let Some(l) = a.get_long() {
                known.insert(format!("--{l}"));
            }
        }
        stack.extend(c.get_subcommands());
    }
    let section = readme
        .split("## CLI")
        .nth(1)
        .and_then(|s| s.split("\n## ").next())
        .unwrap_or("");
    let mut unknown = std::collections::BTreeSet::new();
    for word in section.split(|c: char| c.is_whitespace() || "`(),|[]".contains(c)) {
        let flag = word.split('=').next().unwrap_or("");
        if flag.starts_with("--")
            && flag[2..].starts_with(|c: char| c.is_ascii_lowercase())
            && flag[2..]
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-')
            && !known.contains(flag)
        {
            unknown.insert(flag.to_string());
        }
    }
    assert!(
        unknown.is_empty(),
        "README CLI section uses flags that don't exist: {unknown:?}"
    );
}

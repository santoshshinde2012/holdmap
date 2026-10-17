//! Launch metadata projected from an MCP definition: the runner, the package or image it
//! names, whether that is pinned, the remote host, and environment/header *names*.
//!
//! Arguments, environment and header values, URL paths, queries and credentials are never
//! retained. Anything that does not look like a plain package, image or host is dropped.

use serde::{Deserialize, Serialize};

const MAX_NAMES: usize = 32;

/// How a stdio server is started, from the configured command's file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRunner {
    /// `npx`, `pnpm dlx`, `yarn dlx` or `bunx`: fetches an npm package at launch.
    Npx,
    /// `uvx`, `uv tool run` or `pipx run`: fetches a Python package at launch.
    Uvx,
    /// `docker run` or `podman run`: starts a container image.
    Docker,
    /// `node`, `python`, `deno` or `bun` running a local script or module.
    Interpreter,
    /// Any other executable.
    Binary,
}

impl McpRunner {
    /// Short label shared by text adapters.
    pub fn label(self) -> &'static str {
        match self {
            Self::Npx => "npx",
            Self::Uvx => "uvx",
            Self::Docker => "docker",
            Self::Interpreter => "interpreter",
            Self::Binary => "binary",
        }
    }

    /// Whether the runner downloads what it starts, so an unpinned spec can change silently.
    pub fn fetches(self) -> bool {
        matches!(self, Self::Npx | Self::Uvx | Self::Docker)
    }
}

/// What a stdio definition starts. `pinned` is `None` when pinning does not apply or is unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpLaunch {
    /// Runner kind.
    pub runner: McpRunner,
    /// Package spec, image reference, module, script file name or executable name.
    pub package: Option<String>,
    /// An exact version or digest is named (fetching runners only).
    pub pinned: Option<bool>,
}

/// Classify a configured command and its arguments.
pub(super) fn stdio_launch(command: &str, args: &[&str]) -> McpLaunch {
    let base = file_name(command);
    let (runner, package, pinned) = match base.as_str() {
        "npx" | "bunx" => npm(args),
        "pnpm" | "yarn" if args.first() == Some(&"dlx") => npm(&args[1..]),
        "uvx" => python_tool(args),
        "uv" if args.starts_with(&["tool", "run"]) => python_tool(&args[2..]),
        "pipx" if args.first() == Some(&"run") => python_tool(&args[1..]),
        "docker" | "podman" if args.first() == Some(&"run") => docker(&args[1..]),
        "node" | "deno" | "bun" | "python" | "python3" => interpreter(args),
        _ => (McpRunner::Binary, safe_package(&base), None),
    };
    McpLaunch {
        runner,
        package,
        pinned,
    }
}

type Classified = (McpRunner, Option<String>, Option<bool>);

fn npm(args: &[&str]) -> Classified {
    let spec = flag_value(args, &["-p", "--package"]).or_else(|| first_positional(args, &[]));
    let package = spec.and_then(safe_package);
    let pinned = package.as_deref().map(|spec| {
        let (_, version) = split_npm(spec);
        version.is_some_and(exact_version)
    });
    (McpRunner::Npx, package, pinned)
}

/// `@scope/name@1.2.3` → (`@scope/name`, Some(`1.2.3`)).
pub(super) fn split_npm(spec: &str) -> (&str, Option<&str>) {
    match spec[1..].rfind('@') {
        Some(at) => (&spec[..at + 1], Some(&spec[at + 2..])),
        None => (spec, None),
    }
}

fn python_tool(args: &[&str]) -> Classified {
    let spec = flag_value(args, &["--from"])
        .or_else(|| first_positional(args, &["--python", "--with", "-p", "--index-url"]));
    let package = spec.and_then(safe_package);
    let pinned = package.as_deref().map(|spec| {
        spec.split_once("==")
            .or_else(|| spec.split_once('@'))
            .is_some_and(|(_, version)| exact_version(version))
    });
    (McpRunner::Uvx, package, pinned)
}

fn docker(args: &[&str]) -> Classified {
    const VALUE_FLAGS: &[&str] = &[
        "-e",
        "--env",
        "--env-file",
        "-v",
        "--volume",
        "-p",
        "--publish",
        "--name",
        "--network",
        "-w",
        "--workdir",
        "-u",
        "--user",
        "--mount",
        "--entrypoint",
        "-l",
        "--label",
        "--platform",
        "--pull",
        "-h",
        "--hostname",
        "--add-host",
        "--cpus",
        "-m",
        "--memory",
    ];
    let image = first_positional(args, VALUE_FLAGS).and_then(safe_package);
    let pinned = image.as_deref().map(|image| {
        if image.contains("@sha256:") {
            return true;
        }
        let last = image.rsplit('/').next().unwrap_or(image);
        last.split_once(':')
            .is_some_and(|(_, tag)| !tag.is_empty() && tag != "latest")
    });
    (McpRunner::Docker, image, pinned)
}

fn interpreter(args: &[&str]) -> Classified {
    let target = flag_value(args, &["-m"])
        .or_else(|| first_positional(args, &["-r", "--require", "--import", "-c", "-e"]))
        .map(file_name);
    (
        McpRunner::Interpreter,
        target.as_deref().and_then(safe_package),
        None,
    )
}

fn first_positional<'a>(args: &[&'a str], value_flags: &[&str]) -> Option<&'a str> {
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if arg.starts_with('-') {
            skip = !arg.contains('=') && value_flags.contains(arg);
            continue;
        }
        return Some(arg);
    }
    None
}

fn flag_value<'a>(args: &[&'a str], flags: &[&str]) -> Option<&'a str> {
    args.iter().enumerate().find_map(|(index, arg)| {
        flags.iter().find_map(|flag| {
            if arg == flag {
                args.get(index + 1).copied()
            } else {
                arg.strip_prefix(flag)
                    .and_then(|rest| rest.strip_prefix('='))
            }
        })
    })
}

fn exact_version(version: &str) -> bool {
    let version = version.strip_prefix('v').unwrap_or(version);
    version.chars().next().is_some_and(|c| c.is_ascii_digit())
        && version.split(['-', '+']).next().is_some_and(|core| {
            core.split('.').count() >= 2 && core.split('.').all(|part| part.parse::<u64>().is_ok())
        })
}

fn file_name(command: &str) -> String {
    let name = command.rsplit(['/', '\\']).next().unwrap_or(command);
    let name = name.to_ascii_lowercase();
    for suffix in [".exe", ".cmd", ".bat"] {
        if let Some(stem) = name.strip_suffix(suffix) {
            return stem.to_owned();
        }
    }
    name
}

/// A package/image/executable identifier with no separators that could carry secrets.
fn safe_package(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    (!value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@/._:+=~-".contains(c))
        && !value.contains("://")
        && !value.starts_with(['/', '.'])
        && crate::redact::line(value) == value
        && ![
            "sk-",
            "ghp_",
            "github_pat_",
            "xoxb-",
            "xoxp-",
            "token",
            "secret",
            "password",
        ]
        .iter()
        .any(|marker| lower.contains(marker)))
    .then(|| value.to_owned())
}

/// Text that commonly carries a credential: token prefixes, auth headers, key assignments.
pub(super) fn looks_secret(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "sk-",
        "ghp_",
        "gho_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "glpat-",
        "akia",
        "bearer ",
        "authorization:",
        "-----begin",
        "password=",
        "passwd=",
        "token=",
        "secret=",
        "api_key=",
        "apikey=",
        "access_key=",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// The host (and port) of an http(s) endpoint; never its user info, path or query.
pub(super) fn remote_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    (!host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c)))
    .then(|| host.to_ascii_lowercase())
}

/// Environment variable names only, sorted and bounded.
pub(super) fn env_names<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    names(keys, |key| {
        key.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// HTTP header names only, sorted and bounded.
pub(super) fn header_names<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    names(keys, |key| {
        key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

fn names<'a>(keys: impl IntoIterator<Item = &'a str>, valid: impl Fn(&str) -> bool) -> Vec<String> {
    let mut out: Vec<String> = keys
        .into_iter()
        .filter(|key| !key.is_empty() && key.len() <= 64 && valid(key))
        .map(str::to_owned)
        .collect();
    out.sort();
    out.dedup();
    out.truncate(MAX_NAMES);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(command: &str, args: &[&str]) -> (McpRunner, Option<&'static str>, Option<bool>) {
        let out = stdio_launch(command, args);
        (out.runner, out.package.map(|p| &*p.leak()), out.pinned)
    }

    #[test]
    fn runners_packages_and_pinning_follow_each_launcher() {
        use McpRunner::*;
        for (command, args, expected) in [
            (
                "npx",
                &["-y", "@modelcontextprotocol/server-github"][..],
                (
                    Npx,
                    Some("@modelcontextprotocol/server-github"),
                    Some(false),
                ),
            ),
            (
                "npx",
                &["-y", "@playwright/mcp@0.0.41"][..],
                (Npx, Some("@playwright/mcp@0.0.41"), Some(true)),
            ),
            (
                "npx",
                &["mcp-remote@latest", "https://example.com/sse"][..],
                (Npx, Some("mcp-remote@latest"), Some(false)),
            ),
            (
                "npx",
                &["--package=figma-mcp@1.x", "figma"][..],
                (Npx, Some("figma-mcp@1.x"), Some(false)),
            ),
            (
                "/usr/local/bin/npx.cmd",
                &["context7-mcp@1.0.3"][..],
                (Npx, Some("context7-mcp@1.0.3"), Some(true)),
            ),
            (
                "pnpm",
                &["dlx", "server-x@2.0.0"][..],
                (Npx, Some("server-x@2.0.0"), Some(true)),
            ),
            (
                "uvx",
                &["mcp-server-fetch"][..],
                (Uvx, Some("mcp-server-fetch"), Some(false)),
            ),
            (
                "uvx",
                &["--from", "mcp-server-git==0.6.2", "mcp-server-git"][..],
                (Uvx, Some("mcp-server-git==0.6.2"), Some(true)),
            ),
            (
                "uv",
                &["tool", "run", "serena@1.4.0"][..],
                (Uvx, Some("serena@1.4.0"), Some(true)),
            ),
            (
                "docker",
                &[
                    "run",
                    "-i",
                    "--rm",
                    "-e",
                    "GITHUB_PERSONAL_ACCESS_TOKEN",
                    "ghcr.io/github/github-mcp-server",
                ][..],
                (
                    Docker,
                    Some("ghcr.io/github/github-mcp-server"),
                    Some(false),
                ),
            ),
            (
                "docker",
                &["run", "-i", "mcp/postgres:1.2"][..],
                (Docker, Some("mcp/postgres:1.2"), Some(true)),
            ),
            (
                "docker",
                &["run", "localhost:5000/mcp@sha256:abc"][..],
                (Docker, Some("localhost:5000/mcp@sha256:abc"), Some(true)),
            ),
            (
                "node",
                &["/Users/me/servers/build/index.js"][..],
                (Interpreter, Some("index.js"), None),
            ),
            (
                "python3",
                &["-m", "my_server"][..],
                (Interpreter, Some("my_server"), None),
            ),
            (
                "/opt/bin/github-mcp-server",
                &["stdio"][..],
                (Binary, Some("github-mcp-server"), None),
            ),
        ] {
            assert_eq!(launch(command, args), expected, "{command} {args:?}");
        }
    }

    #[test]
    fn secrets_paths_and_odd_values_are_never_kept() {
        for (command, args) in [
            ("npx", &["https://evil.example/pkg.tgz"][..]),
            ("npx", &["--token=sk-live-abc"][..]),
            ("npx", &["./local-script"][..]),
            (
                "docker",
                &["run", "-e", "API_KEY=sk-12345", "image with space"][..],
            ),
            ("my-token-server", &[][..]),
        ] {
            assert_eq!(
                stdio_launch(command, args).package,
                None,
                "{command} {args:?}"
            );
        }
    }

    #[test]
    fn hosts_drop_credentials_paths_and_queries() {
        assert_eq!(
            remote_host("https://user:pw@mcp.example.com:8443/v1/sse?token=x").as_deref(),
            Some("mcp.example.com:8443")
        );
        assert_eq!(
            remote_host("http://127.0.0.1:3845/mcp").as_deref(),
            Some("127.0.0.1:3845")
        );
        assert_eq!(remote_host("ftp://example.com"), None);
        assert_eq!(remote_host("https://"), None);
    }

    #[test]
    fn only_well_formed_names_survive() {
        assert_eq!(
            env_names(["GITHUB_TOKEN", "bad name", "1X", "API_KEY", "GITHUB_TOKEN"]),
            ["API_KEY", "GITHUB_TOKEN"]
        );
        assert_eq!(
            header_names(["Authorization", "X-Api-Key", "bad:header"]),
            ["Authorization", "X-Api-Key"]
        );
    }
}

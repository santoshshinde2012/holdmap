//! Match configured stdio MCP servers to processes the agent is running right now.
//!
//! Evidence is the process table only: a server is `running` when one of the agent's own
//! processes runs its package, image or executable. Remote servers cannot be observed this
//! way, so they stay `unknown`. Nothing is connected to or started.

use super::launch::{split_npm, McpRunner};
use super::{McpServerCapability, McpTransport};
use crate::model::ProcessInfo;
use serde::{Deserialize, Serialize};

/// Whether a configured server was observed running under its agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRunState {
    /// One of the agent's processes runs it.
    Running,
    /// A local server that none of the agent's processes runs right now.
    NotSeen,
    /// Remote, disabled, unidentifiable or not checked.
    #[default]
    Unknown,
}

/// Observed runtime state with the matching process IDs as evidence.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct McpRuntime {
    /// Observed state.
    pub state: McpRunState,
    /// Matching agent-owned process IDs.
    pub pids: Vec<u32>,
}

/// Set `runtime` on every server from the agent's processes.
pub fn match_running<'a>(
    servers: &mut [McpServerCapability],
    processes: impl Iterator<Item = &'a ProcessInfo> + Clone,
) {
    for server in servers {
        server.runtime = runtime(server, processes.clone());
    }
}

fn runtime<'a>(
    server: &McpServerCapability,
    processes: impl Iterator<Item = &'a ProcessInfo>,
) -> McpRuntime {
    let needle = match (&server.transport, &server.launch, server.enabled) {
        (_, _, Some(false)) => None,
        (McpTransport::Stdio, Some(launch), _) => launch
            .package
            .as_deref()
            .map(|package| (launch.runner, identity(launch.runner, package))),
        _ => None,
    };
    let Some((runner, needle)) = needle.filter(|(_, needle)| needle.len() >= 3) else {
        return McpRuntime::default();
    };
    let mut pids: Vec<u32> = processes
        .filter(|process| runs(process, runner, &needle))
        .map(|process| process.pid)
        .collect();
    pids.sort_unstable();
    pids.dedup();
    McpRuntime {
        state: if pids.is_empty() {
            McpRunState::NotSeen
        } else {
            McpRunState::Running
        },
        pids,
    }
}

/// The version-free name a running process shows: npm `@scope/name@1` → `scope/name` path
/// fragment, `pkg==1` → `pkg`, `image:tag` → `image`.
fn identity(runner: McpRunner, package: &str) -> String {
    let name = match runner {
        McpRunner::Npx => split_npm(package).0,
        McpRunner::Uvx => package
            .split(['=', '@', '[', '<', '>', '~'])
            .next()
            .unwrap_or(package),
        McpRunner::Docker => package
            .split('@')
            .next()
            .map(|image| match image.rsplit_once(':') {
                Some((name, tag)) if !tag.contains('/') => name,
                _ => image,
            })
            .unwrap_or(package),
        McpRunner::Interpreter | McpRunner::Binary => package,
    };
    name.trim_start_matches('@').to_ascii_lowercase()
}

fn runs(process: &ProcessInfo, runner: McpRunner, needle: &str) -> bool {
    let base = needle.rsplit('/').next().unwrap_or(needle);
    let name = process.name.to_ascii_lowercase();
    let executable = name == base || name.strip_suffix(".exe") == Some(base);
    let normalize = |arg: &str| {
        let text = arg
            .to_ascii_lowercase()
            .replace('\\', "/")
            .replace("/@", "/");
        text.strip_prefix('@').map(str::to_owned).unwrap_or(text)
    };
    let program = process.cmdline.first().map(|arg| {
        normalize(arg)
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned()
    });
    match runner {
        McpRunner::Binary => {
            executable || program.is_some_and(|p| p == base || p.strip_suffix(".exe") == Some(base))
        }
        McpRunner::Interpreter => process.cmdline.iter().skip(1).any(|arg| {
            let text = normalize(arg);
            text == needle || text.ends_with(&format!("/{needle}"))
        }),
        McpRunner::Npx | McpRunner::Uvx | McpRunner::Docker => {
            executable
                || process.cmdline.iter().any(|arg| {
                    let text = normalize(arg);
                    text == needle
                        || text.ends_with(&format!("/{needle}"))
                        || text.contains(&format!("/{needle}/"))
                        || text.starts_with(&format!("{needle}/"))
                        || text.starts_with(&format!("{needle}@"))
                        || text.contains(&format!("/{needle}@"))
                        || text.starts_with(&format!("{needle}=="))
                        || text.starts_with(&format!("{needle}:"))
                        || text.contains(&format!("/{needle}:"))
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::launch::{stdio_launch, McpLaunch};
    use super::*;
    use crate::agents::{CapabilityScope, Evidence};

    fn server(launch: Option<McpLaunch>, transport: McpTransport) -> McpServerCapability {
        McpServerCapability {
            id: "x".into(),
            name: "x".into(),
            scope: CapabilityScope::User,
            source_path: "/h/.claude.json".into(),
            project_path: None,
            transport,
            enabled: None,
            evidence: Evidence::Observed,
            launch,
            remote_host: None,
            env_names: vec![],
            header_names: vec![],
            runtime: McpRuntime::default(),
        }
    }

    fn process(pid: u32, name: &str, cmd: &[&str]) -> ProcessInfo {
        crate::process::tests::proc(pid, 1, name, cmd)
    }

    #[test]
    fn running_servers_are_matched_by_package_image_or_executable() {
        let procs = [
            process(
                10,
                "node",
                &[
                    "node",
                    "/Users/me/.npm/_npx/1a2b/node_modules/.bin/mcp-server-github",
                ],
            ),
            process(
                11,
                "npm exec @modelcontextprotocol/server-github",
                &["npm", "exec", "@modelcontextprotocol/server-github"],
            ),
            process(
                12,
                "python",
                &[
                    "/Users/me/.cache/uv/archive-v0/x/bin/python",
                    "/Users/me/.cache/uv/archive-v0/x/bin/mcp-server-fetch",
                ],
            ),
            process(
                13,
                "docker",
                &[
                    "docker",
                    "run",
                    "-i",
                    "--rm",
                    "ghcr.io/github/github-mcp-server",
                ],
            ),
            process(
                14,
                "github-mcp-server",
                &["/opt/bin/github-mcp-server", "stdio"],
            ),
        ];
        let cases = [
            (
                stdio_launch(
                    "npx",
                    &["-y", "@modelcontextprotocol/server-github@2025.4.8"],
                ),
                vec![11],
            ),
            (stdio_launch("uvx", &["mcp-server-fetch"]), vec![12]),
            (
                stdio_launch(
                    "docker",
                    &["run", "-i", "ghcr.io/github/github-mcp-server:v0.5.0"],
                ),
                vec![13, 14],
            ),
            (
                stdio_launch("/opt/bin/github-mcp-server", &["stdio"]),
                vec![14],
            ),
            (stdio_launch("npx", &["-y", "@playwright/mcp"]), vec![]),
        ];
        for (launch, pids) in cases {
            let mut servers = [server(Some(launch.clone()), McpTransport::Stdio)];
            match_running(&mut servers, procs.iter());
            let expected = if pids.is_empty() {
                McpRunState::NotSeen
            } else {
                McpRunState::Running
            };
            assert_eq!(
                servers[0].runtime,
                McpRuntime {
                    state: expected,
                    pids
                },
                "{launch:?}"
            );
        }
    }

    #[test]
    fn remote_disabled_and_unidentified_servers_stay_unknown() {
        let procs = [process(10, "node", &["node", "server.js"])];
        let mut disabled = server(
            Some(stdio_launch("node", &["server.js"])),
            McpTransport::Stdio,
        );
        disabled.enabled = Some(false);
        let mut servers = [
            server(None, McpTransport::Http),
            server(None, McpTransport::Stdio),
            disabled,
        ];
        match_running(&mut servers, procs.iter());
        assert!(servers
            .iter()
            .all(|s| s.runtime.state == McpRunState::Unknown));
    }
}

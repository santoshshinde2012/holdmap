//! Describes child processes using executable, package and listener evidence.
//!
//! MCP signatures are deliberately inferred: a running server process establishes neither
//! a transport nor a completed tool call. No configuration, environment or transcript is read.

use super::{catalog, AgentPort, AgentTool, Evidence, PortRole, ToolKind, MAX_TOOLS};
use crate::model::{PortEntry, ProcessInfo};
use std::fmt::Debug;

/// The purpose and safe display name assigned to a process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolClassification {
    /// Executable name or recognised package identifier.
    pub name: String,
    /// What the process appears to do.
    pub kind: ToolKind,
    /// How the classification was established.
    pub evidence: Evidence,
}

/// A narrow classifier over existing process metadata and attributed listeners.
pub trait ToolClassifier: Send + Sync + Debug {
    /// Classify one child process; implementations need not inspect the filesystem.
    fn classify(&self, process: &ProcessInfo, ports: &[AgentPort]) -> ToolClassification;
}

/// Executable/package signatures for MCP, shell names and observed development listeners.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultToolClassifier;

impl ToolClassifier for DefaultToolClassifier {
    fn classify(&self, process: &ProcessInfo, ports: &[AgentPort]) -> ToolClassification {
        if let Some(name) = mcp_name(process) {
            return ToolClassification {
                name,
                kind: ToolKind::McpServer,
                evidence: Evidence::Inferred,
            };
        }
        let kind = if ports
            .iter()
            .any(|p| p.pid == Some(process.pid) && p.role == PortRole::DevServer)
        {
            ToolKind::DevServer
        } else if [
            "sh",
            "bash",
            "zsh",
            "fish",
            "dash",
            "ksh",
            "powershell",
            "pwsh",
            "cmd",
        ]
        .contains(&catalog::norm(process.name.trim_start_matches('-')).as_str())
        {
            ToolKind::Shell
        } else {
            ToolKind::Command
        };
        ToolClassification {
            name: process.name.clone(),
            kind,
            evidence: Evidence::Observed,
        }
    }
}

fn mcp_name(process: &ProcessInfo) -> Option<String> {
    let program = process.cmdline.first().map(|a| catalog::basename(a));
    if program.as_deref() == Some("holdmap")
        && process.cmdline.get(1).is_some_and(|arg| arg == "mcp")
    {
        return Some("holdmap mcp".into());
    }
    let mut candidates = vec![process.name.as_str()];
    candidates.extend(process.cmdline.first().map(String::as_str));
    candidates.extend(catalog::runtime_entry(process));
    // Package launchers expose the package as their first positional argument. Their later
    // arguments may contain user input or credentials, so they are never classification data.
    if matches!(program.as_deref(), Some("npx" | "uvx")) {
        candidates.extend(launcher_packages(
            process,
            program.as_deref() == Some("npx"),
        ));
    }
    candidates.into_iter().find_map(mcp_identifier)
}

fn launcher_packages(process: &ProcessInfo, npx: bool) -> Vec<&str> {
    let mut candidates = Vec::new();
    let mut args = process.cmdline.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--" {
            candidates.extend(args.next().map(String::as_str));
            break;
        }
        if !arg.starts_with('-') {
            candidates.push(arg.as_str());
            break;
        }
        let (option, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(name, value)| (name, Some(value)));
        let package_option = if npx {
            ["--package", "-p"].contains(&option)
        } else {
            option == "--from"
        };
        let other_value_option = if npx {
            [
                "--registry",
                "--userconfig",
                "--cache",
                "--prefix",
                "--loglevel",
                "--call",
                "-c",
            ]
            .contains(&option)
        } else {
            [
                "--python",
                "--index-url",
                "--extra-index-url",
                "--default-index",
                "--index",
                "--find-links",
                "--directory",
                "--project",
                "--config-file",
                "--cache-dir",
                "--with",
                "--with-editable",
                "--with-requirements",
                "--python-platform",
                "--allow-insecure-host",
            ]
            .contains(&option)
        };
        if package_option || other_value_option {
            let Some(value) = inline.or_else(|| args.next().map(String::as_str)) else {
                return Vec::new();
            };
            if package_option {
                candidates.push(value);
            }
            if ["--call", "-c"].contains(&option) {
                break;
            }
            continue;
        }
        let boolean_option = if npx {
            [
                "-y",
                "--yes",
                "--no",
                "--offline",
                "--prefer-offline",
                "--prefer-online",
                "--quiet",
                "-q",
            ]
            .contains(&option)
        } else {
            [
                "--offline",
                "--no-cache",
                "--no-config",
                "--no-project",
                "--no-managed-python",
                "--managed-python",
                "--no-python-downloads",
                "--isolated",
                "--quiet",
                "-q",
                "--verbose",
                "-v",
                "--native-tls",
            ]
            .contains(&option)
        };
        if boolean_option {
            continue;
        }
        // Discard candidates already collected too: an unknown flag could change how the
        // launcher interprets the entire command, including an earlier package option.
        return Vec::new();
    }
    candidates
}

fn mcp_identifier(value: &str) -> Option<String> {
    let value = value.replace('\\', "/");
    let parts: Vec<&str> = value.split('/').collect();
    for (index, part) in parts.iter().enumerate() {
        let candidate = if part.starts_with('@') {
            parts.get(index + 1).map(|next| format!("{part}/{next}"))?
        } else {
            part.to_string()
        };
        let candidate = candidate
            .rsplit_once('@')
            .filter(|(package, _)| !package.is_empty())
            .map_or(candidate.as_str(), |(package, _)| package);
        let candidate = candidate
            .trim_end_matches(".js")
            .trim_end_matches(".mjs")
            .trim_end_matches(".py");
        let valid = candidate.len() <= 96
            && candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "@/._-".contains(c));
        let signature = candidate.starts_with("mcp-server-")
            || candidate.starts_with("mcp_server_")
            || candidate.starts_with("@modelcontextprotocol/server-")
            || candidate == "@playwright/mcp"
            || candidate == "@browsermcp/mcp";
        if valid && signature {
            return Some(candidate.to_string());
        }
    }
    None
}

pub(super) fn collect<'a>(
    processes: impl Iterator<Item = &'a ProcessInfo>,
    ports: &[AgentPort],
    entries: &[PortEntry],
    classifier: &dyn ToolClassifier,
) -> (Vec<AgentTool>, usize) {
    let mut tools: Vec<AgentTool> = processes
        .map(|process| {
            let process_ports: Vec<AgentPort> = ports
                .iter()
                .filter_map(|port| {
                    let entry = entries.iter().find(|entry| entry.id == port.entry_id)?;
                    if entry.pid != Some(process.pid) && !entry.pids.contains(&process.pid) {
                        return None;
                    }
                    let mut held = port.clone();
                    held.pid = Some(process.pid);
                    held.role = if entry.is_dev {
                        PortRole::DevServer
                    } else {
                        PortRole::Service
                    };
                    Some(held)
                })
                .collect();
            let classification = classifier.classify(process, &process_ports);
            let mut held: Vec<u16> = process_ports.iter().map(|port| port.port).collect();
            held.sort_unstable();
            held.dedup();
            AgentTool {
                pid: process.pid,
                ppid: process.ppid,
                name: classification.name,
                kind: classification.kind,
                command: process.command(),
                cwd: process.cwd.clone(),
                evidence: classification.evidence,
                ports: held,
                memory_bytes: process.memory_bytes,
                cpu_percent: process.cpu_percent,
            }
        })
        .collect();
    tools.sort_by_key(|tool| (tool.kind as u8, tool.pid));
    let more = tools.len().saturating_sub(MAX_TOOLS);
    tools.truncate(MAX_TOOLS);
    (tools, more)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::proc;

    #[test]
    fn recognises_our_server_and_versioned_packages_without_searching_arguments() {
        for process in [
            proc(1, 0, "holdmap", &["holdmap", "mcp"]),
            proc(1, 0, "npx", &["npx", "-y", "@playwright/mcp@1.2.3"]),
            proc(
                1,
                0,
                "npx",
                &[
                    "npx",
                    "--package",
                    "@modelcontextprotocol/server-filesystem@latest",
                    "server-filesystem",
                ],
            ),
        ] {
            assert_eq!(
                DefaultToolClassifier.classify(&process, &[]).kind,
                ToolKind::McpServer
            );
        }
        let process = proc(1, 0, "npx", &["npx", "-y", "@playwright/mcp@1.2.3"]);
        assert_eq!(
            DefaultToolClassifier.classify(&process, &[]).name,
            "@playwright/mcp"
        );
        for process in [
            proc(1, 0, "cp", &["cp", "/tmp/mcp-server-fetch", "/tmp/backup"]),
            proc(1, 0, "node", &["node", "worker.js", "@playwright/mcp"]),
            proc(
                1,
                0,
                "npx",
                &["npx", "ordinary-package", "mcp-server-secret"],
            ),
            proc(
                1,
                0,
                "node",
                &["node", "--conditions", "mcp-server-secret", "worker.js"],
            ),
            proc(
                1,
                0,
                "node",
                &[
                    "node",
                    "--unhandled-option",
                    "mcp-server-secret",
                    "worker.js",
                ],
            ),
            proc(
                1,
                0,
                "npx",
                &["npx", "--registry", "mcp-server-secret", "ordinary-package"],
            ),
            proc(
                1,
                0,
                "uvx",
                &[
                    "uvx",
                    "--index-url",
                    "mcp-server-secret",
                    "ordinary-package",
                ],
            ),
            proc(
                1,
                0,
                "npx",
                &[
                    "npx",
                    "--unhandled-option",
                    "mcp-server-secret",
                    "ordinary-package",
                ],
            ),
            proc(
                1,
                0,
                "uvx",
                &[
                    "uvx",
                    "--unhandled-option",
                    "mcp-server-secret",
                    "ordinary-package",
                ],
            ),
        ] {
            assert_eq!(
                DefaultToolClassifier.classify(&process, &[]).kind,
                ToolKind::Command
            );
        }
        for process in [
            proc(
                1,
                0,
                "node",
                &[
                    "node",
                    "--conditions",
                    "development",
                    "/cache/@playwright/mcp/index.js",
                ],
            ),
            proc(
                1,
                0,
                "node",
                &[
                    "node",
                    "--max-old-space-size=4096",
                    "/cache/@playwright/mcp/index.js",
                ],
            ),
            proc(
                1,
                0,
                "npx",
                &[
                    "npx",
                    "--registry",
                    "https://registry.npmjs.org",
                    "--package=@playwright/mcp",
                    "playwright-mcp",
                ],
            ),
            proc(
                1,
                0,
                "uvx",
                &[
                    "uvx",
                    "--index-url=https://pypi.org/simple",
                    "--from",
                    "mcp-server-fetch",
                    "mcp-server-fetch",
                ],
            ),
        ] {
            assert_eq!(
                DefaultToolClassifier.classify(&process, &[]).kind,
                ToolKind::McpServer,
                "{:?}",
                process.cmdline
            );
        }
    }
}

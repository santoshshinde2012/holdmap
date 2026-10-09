//! Which AI coding agents portwise recognises, and how to tell them apart from a process.
//!
//! The catalog is data: one [`AgentProduct`] per product with the process names, app bundles and
//! install paths that identify it. The safety policy uses the same list ([`is_agent_name`]), so
//! the agents view and the "don't stop the user's agent" protection always agree.

use crate::model::ProcessInfo;
use serde::{Deserialize, Serialize};

/// What sort of thing the agent is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    /// A terminal agent (Claude Code, Codex CLI, Gemini CLI, Aider…).
    Cli,
    /// An editor with a built-in agent (Cursor, Windsurf, Kiro).
    Ide,
    /// A desktop chat/agent app (Claude, Codex).
    Desktop,
    /// An editor extension's own process (the Copilot language server).
    Extension,
    /// A host that runs agent sessions for something else (a remote agent daemon).
    Host,
    /// A developer tool or runtime app that sits beside agents (Docker Desktop, OrbStack…).
    Tool,
}

impl AgentKind {
    /// Short human label.
    pub fn label(self) -> &'static str {
        match self {
            AgentKind::Cli => "terminal agent",
            AgentKind::Ide => "AI editor",
            AgentKind::Desktop => "desktop app",
            AgentKind::Extension => "editor extension",
            AgentKind::Host => "agent host",
            AgentKind::Tool => "developer tool",
        }
    }

    /// True when many helper processes of the same product should fold into one agent.
    pub fn folds_helpers(self) -> bool {
        matches!(
            self,
            AgentKind::Ide | AgentKind::Desktop | AgentKind::Host | AgentKind::Tool
        )
    }
}

/// One recognised product and the signals that identify its processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentProduct {
    /// Stable id, e.g. `claude-code`.
    pub id: &'static str,
    /// Display name, e.g. "Claude Code".
    pub name: &'static str,
    /// Who makes it.
    pub vendor: &'static str,
    /// What sort of agent it is.
    pub kind: AgentKind,
    /// Exact process names (lower case, without `.exe`).
    pub names: &'static [&'static str],
    /// Process-name prefixes for helper processes ("cursor helper" matches "Cursor Helper (GPU)").
    pub name_prefixes: &'static [&'static str],
    /// App bundles / install folders found in the executable path (`Cursor.app`).
    pub bundles: &'static [&'static str],
    /// Install-path fragments found in the first arguments (`/@openai/codex/`).
    pub paths: &'static [&'static str],
    /// Script entry points run by a runtime (`node …/claude`, `python -m aider`).
    pub entries: &'static [&'static str],
}

const fn product(
    id: &'static str,
    name: &'static str,
    vendor: &'static str,
    kind: AgentKind,
) -> AgentProduct {
    AgentProduct {
        id,
        name,
        vendor,
        kind,
        names: &[],
        name_prefixes: &[],
        bundles: &[],
        paths: &[],
        entries: &[],
    }
}

/// Every product portwise recognises. Desktop apps come first: their bundle is the most specific
/// signal (the Claude app's process is also called `claude`).
pub const CATALOG: &[AgentProduct] = &[
    AgentProduct {
        bundles: &["Claude.app", "AnthropicClaude"],
        name_prefixes: &["claude helper"],
        ..product("claude-desktop", "Claude", "Anthropic", AgentKind::Desktop)
    },
    AgentProduct {
        bundles: &["Codex.app"],
        name_prefixes: &["codex helper"],
        ..product("codex-desktop", "Codex", "OpenAI", AgentKind::Desktop)
    },
    AgentProduct {
        bundles: &["Cursor.app"],
        names: &["cursor"],
        name_prefixes: &["cursor helper"],
        paths: &["/.cursor-server/"],
        ..product("cursor", "Cursor", "Anysphere", AgentKind::Ide)
    },
    AgentProduct {
        bundles: &["Windsurf.app"],
        names: &["windsurf"],
        name_prefixes: &["windsurf helper"],
        paths: &["/.windsurf-server/"],
        ..product("windsurf", "Windsurf", "Windsurf", AgentKind::Ide)
    },
    AgentProduct {
        bundles: &["Kiro.app"],
        names: &["kiro"],
        name_prefixes: &["kiro helper"],
        ..product("kiro", "Kiro", "AWS", AgentKind::Ide)
    },
    AgentProduct {
        names: &["claude"],
        paths: &[
            "/@anthropic-ai/claude-code/",
            "/claude-code/",
            "/.local/share/claude/",
        ],
        entries: &["claude"],
        ..product("claude-code", "Claude Code", "Anthropic", AgentKind::Cli)
    },
    AgentProduct {
        names: &["codex"],
        paths: &["/@openai/codex/"],
        entries: &["codex"],
        ..product("codex", "Codex CLI", "OpenAI", AgentKind::Cli)
    },
    AgentProduct {
        names: &["cursor-agent"],
        entries: &["cursor-agent"],
        ..product("cursor-agent", "Cursor Agent", "Anysphere", AgentKind::Cli)
    },
    AgentProduct {
        names: &["gemini"],
        paths: &["/@google/gemini-cli/", "/gemini-cli/"],
        entries: &["gemini"],
        ..product("gemini", "Gemini CLI", "Google", AgentKind::Cli)
    },
    AgentProduct {
        names: &["copilot-language-server"],
        paths: &["/github.copilot"],
        entries: &["copilot-language-server"],
        ..product("copilot", "GitHub Copilot", "GitHub", AgentKind::Extension)
    },
    AgentProduct {
        names: &["copilot"],
        paths: &["/@github/copilot/"],
        entries: &["copilot"],
        ..product("copilot-cli", "Copilot CLI", "GitHub", AgentKind::Cli)
    },
    AgentProduct {
        names: &["aider"],
        entries: &["aider"],
        ..product("aider", "Aider", "Aider", AgentKind::Cli)
    },
    AgentProduct {
        names: &["cline"],
        entries: &["cline"],
        ..product("cline", "Cline", "Cline", AgentKind::Cli)
    },
    AgentProduct {
        names: &["goose", "goosed"],
        bundles: &["Goose.app"],
        ..product("goose", "Goose", "Block", AgentKind::Cli)
    },
    AgentProduct {
        names: &["opencode"],
        entries: &["opencode"],
        ..product("opencode", "opencode", "SST", AgentKind::Cli)
    },
    AgentProduct {
        names: &["amp"],
        paths: &["/@sourcegraph/amp/"],
        entries: &["amp"],
        ..product("amp", "Amp", "Sourcegraph", AgentKind::Cli)
    },
    AgentProduct {
        names: &["crush"],
        ..product("crush", "Crush", "Charm", AgentKind::Cli)
    },
    AgentProduct {
        names: &["qwen"],
        paths: &["/@qwen-code/"],
        entries: &["qwen"],
        ..product("qwen", "Qwen Code", "Alibaba", AgentKind::Cli)
    },
    AgentProduct {
        paths: &["/exec-daemon/"],
        ..product("agent-host", "Agent host", "unknown", AgentKind::Host)
    },
    // Developer tools and runtimes that share the machine with coding agents.
    AgentProduct {
        bundles: &["Docker.app", "Docker Desktop.app"],
        names: &["docker desktop", "com.docker.backend", "com.docker.vpnkit"],
        name_prefixes: &["com.docker.", "docker desktop"],
        ..product(
            "docker-desktop",
            "Docker Desktop",
            "Docker",
            AgentKind::Tool,
        )
    },
    AgentProduct {
        bundles: &["OrbStack.app"],
        names: &["orbstack", "orb"],
        name_prefixes: &["orbstack"],
        ..product("orbstack", "OrbStack", "OrbStack", AgentKind::Tool)
    },
    AgentProduct {
        bundles: &["Podman Desktop.app"],
        names: &["podman-desktop", "podman desktop"],
        name_prefixes: &["podman desktop"],
        ..product(
            "podman-desktop",
            "Podman Desktop",
            "Red Hat",
            AgentKind::Tool,
        )
    },
];

/// Look a product up by id.
pub fn by_id(id: &str) -> Option<&'static AgentProduct> {
    CATALOG.iter().find(|p| p.id == id)
}

/// Lower case, without a Windows `.exe`.
pub(crate) fn norm(name: &str) -> String {
    let n = name.trim().to_ascii_lowercase();
    n.strip_suffix(".exe").map(str::to_owned).unwrap_or(n)
}

/// True when `name` (already normalised) is the process name of a terminal agent, an agent
/// extension or an agent host. Editors and desktop apps are recognised separately.
pub fn is_agent_name(name: &str) -> bool {
    CATALOG
        .iter()
        .filter(|p| {
            !matches!(
                p.kind,
                AgentKind::Ide | AgentKind::Desktop | AgentKind::Tool
            )
        })
        .any(|p| p.names.contains(&name))
}

/// True when a script path (`/usr/local/bin/claude`, `…/codex/bin/codex.js`) is an agent entry
/// point. `base` is the normalised file name.
pub fn is_agent_entry(base: &str) -> bool {
    let base = base
        .trim_end_matches(".js")
        .trim_end_matches(".mjs")
        .trim_end_matches(".cjs");
    CATALOG
        .iter()
        .filter(|p| {
            !matches!(
                p.kind,
                AgentKind::Ide | AgentKind::Desktop | AgentKind::Tool
            )
        })
        .any(|p| p.entries.contains(&base) || p.names.contains(&base))
}

/// Which product `p` belongs to, if any. Checks the most specific signal first: the app bundle
/// of the executable, then the process name, then install paths and script entry points in the
/// first few arguments.
pub fn identify(p: &ProcessInfo) -> Option<&'static AgentProduct> {
    let exe = p
        .exe
        .as_ref()
        .map(|e| e.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let argv0 = p
        .cmdline
        .first()
        .map(|a| a.replace('\\', "/"))
        .unwrap_or_default();
    let in_bundle = |b: &str| {
        let needle = format!("/{b}/");
        exe.contains(&needle) || argv0.contains(&needle)
    };
    if let Some(hit) = CATALOG
        .iter()
        .find(|x| x.bundles.iter().any(|b| in_bundle(b)))
    {
        return Some(hit);
    }
    let n = norm(&p.name);
    if let Some(hit) = CATALOG.iter().find(|x| {
        x.names.contains(&n.as_str()) || x.name_prefixes.iter().any(|pre| n.starts_with(pre))
    }) {
        return Some(hit);
    }
    let args: Vec<String> = p
        .cmdline
        .iter()
        .take(3)
        .map(|a| a.replace('\\', "/"))
        .collect();
    if let Some(hit) = CATALOG.iter().find(|x| {
        x.paths
            .iter()
            .any(|frag| exe.contains(frag) || args.iter().any(|a| a.contains(frag)))
    }) {
        return Some(hit);
    }
    // `node /usr/local/bin/claude`, `python -m aider`: a runtime running an agent's entry point.
    for (i, a) in args.iter().enumerate() {
        let script = if i == 0 || a.contains('/') {
            a.rsplit('/').next().map(norm)
        } else if i > 0 && args[i - 1] == "-m" {
            Some(norm(a))
        } else {
            None
        };
        let Some(script) = script else { continue };
        let script = script
            .trim_end_matches(".js")
            .trim_end_matches(".mjs")
            .trim_end_matches(".cjs")
            .to_string();
        if i == 0 {
            // argv[0] is the program itself (a native binary may be named after its version).
            if let Some(hit) = CATALOG.iter().find(|x| x.names.contains(&script.as_str())) {
                return Some(hit);
            }
            continue;
        }
        if let Some(hit) = CATALOG
            .iter()
            .find(|x| x.entries.contains(&script.as_str()))
        {
            return Some(hit);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::proc;

    fn with_exe(mut p: ProcessInfo, exe: &str) -> ProcessInfo {
        p.exe = Some(exe.into());
        p
    }

    #[test]
    fn ids_are_unique_and_kebab_case() {
        let mut seen = std::collections::HashSet::new();
        for p in CATALOG {
            assert!(seen.insert(p.id), "duplicate id {}", p.id);
            assert!(p
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
            assert_eq!(by_id(p.id), Some(p));
        }
    }

    #[test]
    fn identifies_terminal_agents_by_name_and_entry_point() {
        let id = |p: ProcessInfo| identify(&p).map(|x| x.id);
        assert_eq!(id(proc(1, 0, "claude", &["claude"])), Some("claude-code"));
        assert_eq!(
            id(proc(
                1,
                0,
                "node",
                &["node", "/usr/local/bin/claude", "--resume"]
            )),
            Some("claude-code")
        );
        assert_eq!(
            id(proc(
                1,
                0,
                "node",
                &["node", "/opt/lib/node_modules/@openai/codex/bin/codex.js"]
            )),
            Some("codex")
        );
        assert_eq!(id(proc(1, 0, "codex.exe", &[])), Some("codex"));
        assert_eq!(
            id(proc(1, 0, "python3", &["python3", "-m", "aider"])),
            Some("aider")
        );
        assert_eq!(
            id(proc(
                1,
                0,
                "node",
                &["node", "/x/@google/gemini-cli/dist/index.js"]
            )),
            Some("gemini")
        );
        assert_eq!(id(proc(1, 0, "cursor-agent", &[])), Some("cursor-agent"));
        assert_eq!(id(proc(1, 0, "node", &["node", "server.js"])), None);
        assert_eq!(id(proc(1, 0, "zsh", &["-zsh"])), None);
    }

    #[test]
    fn app_bundles_win_over_process_names() {
        let app = with_exe(
            proc(
                1,
                0,
                "Claude",
                &["/Applications/Claude.app/Contents/MacOS/Claude"],
            ),
            "/Applications/Claude.app/Contents/MacOS/Claude",
        );
        assert_eq!(identify(&app).map(|x| x.id), Some("claude-desktop"));
        let helper = proc(
            2,
            1,
            "Cursor Helper (Renderer)",
            &["/Applications/Cursor.app/Contents/Frameworks/Cursor Helper (Renderer).app/Contents/MacOS/Cursor Helper (Renderer)"],
        );
        assert_eq!(identify(&helper).map(|x| x.id), Some("cursor"));
        let remote = proc(
            3,
            1,
            "node",
            &[
                "/home/me/.cursor-server/bin/abc/node",
                "/home/me/.cursor-server/bin/abc/out/server-main.js",
            ],
        );
        assert_eq!(identify(&remote).map(|x| x.id), Some("cursor"));
        let copilot = proc(
            4,
            1,
            "node",
            &[
                "/Applications/Visual Studio Code.app/x/Code Helper (Plugin)",
                "/Users/me/.vscode/extensions/github.copilot-1.2.3/dist/language-server.js",
            ],
        );
        assert_eq!(identify(&copilot).map(|x| x.id), Some("copilot"));
    }

    #[test]
    fn the_safety_names_come_from_the_catalog() {
        for n in [
            "claude",
            "codex",
            "aider",
            "goose",
            "opencode",
            "gemini",
            "cursor-agent",
            "amp",
            "crush",
            "copilot-language-server",
        ] {
            assert!(is_agent_name(n), "{n}");
        }
        assert!(
            !is_agent_name("cursor"),
            "editors are classified as editors"
        );
        assert!(
            !is_agent_name("orbstack"),
            "developer tools aren't protected as agents"
        );
        assert!(is_agent_entry("codex.js"));
        assert!(!is_agent_entry("server.js"));
    }

    #[test]
    fn identifies_developer_tools_by_bundle() {
        let docker = with_exe(
            proc(
                1,
                0,
                "Docker Desktop",
                &["/Applications/Docker.app/Contents/MacOS/Docker Desktop"],
            ),
            "/Applications/Docker.app/Contents/MacOS/Docker Desktop",
        );
        assert_eq!(identify(&docker).map(|x| x.id), Some("docker-desktop"));
        assert_eq!(identify(&docker).map(|x| x.kind), Some(AgentKind::Tool));
        let orb = with_exe(
            proc(
                2,
                0,
                "OrbStack",
                &["/Applications/OrbStack.app/Contents/MacOS/OrbStack"],
            ),
            "/Applications/OrbStack.app/Contents/MacOS/OrbStack",
        );
        assert_eq!(identify(&orb).map(|x| x.id), Some("orbstack"));
    }
}

//! Access facts: the account an agent runs as, sandbox and approval hints from its command line
//! and process tree, its listening sockets and OS privacy-protected folders. Conservative by
//! design: a flag or a sandbox process that was *seen* is reported as such; everything else is
//! "unknown", because an agent's own settings files are not read.

use super::model::{
    AccessFact, AccessLevel, AccessTopic, AgentFolder, AgentPort, Evidence, FolderSource,
};
use crate::model::{Exposure, ProcessInfo};
use std::path::Path;

fn fact(topic: AccessTopic, level: AccessLevel, summary: String, evidence: Evidence) -> AccessFact {
    AccessFact {
        topic,
        level,
        summary,
        evidence,
    }
}

/// Value of `--name value` / `--name=value` (any of `names`).
fn flag_value<'a>(args: &'a [String], names: &[&str]) -> Option<&'a str> {
    for (i, a) in args.iter().enumerate() {
        if a == "--" {
            break;
        }
        for n in names {
            if a == n {
                return args.get(i + 1).map(String::as_str);
            }
            if let Some(v) = a.strip_prefix(n).and_then(|r| r.strip_prefix('=')) {
                return Some(v);
            }
        }
    }
    None
}

fn has_flag(args: &[String], names: &[&str]) -> Option<String> {
    args.iter()
        .take_while(|a| a.as_str() != "--")
        .find(|a| names.contains(&a.as_str()))
        .map(|a| a.to_string())
}

/// The account: root, you or another user.
pub fn user(p: &ProcessInfo, mine: bool) -> AccessFact {
    let who = p.user.clone().or_else(|| p.uid.map(|u| format!("uid {u}")));
    match (p.uid, who) {
        (Some(0), _) => fact(
            AccessTopic::User,
            AccessLevel::Elevated,
            "Runs as root: it can read and change every user's files.".into(),
            Evidence::Observed,
        ),
        (_, Some(u)) if mine => fact(
            AccessTopic::User,
            AccessLevel::Standard,
            format!("Runs as you ({u}): the same file access as your account."),
            Evidence::Observed,
        ),
        (_, Some(u)) => fact(
            AccessTopic::User,
            AccessLevel::Standard,
            format!("Runs as another user ({u})."),
            Evidence::Observed,
        ),
        _ => fact(
            AccessTopic::User,
            AccessLevel::Unknown,
            "The owning account isn't visible.".into(),
            Evidence::Unknown,
        ),
    }
}

/// Process names of OS sandbox wrappers.
const SANDBOXES: &[(&str, &str)] = &[
    ("sandbox-exec", "sandbox-exec (macOS Seatbelt)"),
    ("bwrap", "bubblewrap"),
    ("firejail", "firejail"),
    ("nsjail", "nsjail"),
    (
        "codex-linux-sandbox",
        "Codex's Linux sandbox (Landlock/seccomp)",
    ),
];

fn sandbox_wrapper(p: &ProcessInfo) -> Option<&'static str> {
    let n = super::catalog::norm(&p.name);
    if n.starts_with("containerd-shim") {
        return Some("a container");
    }
    SANDBOXES
        .iter()
        .find(|(name, _)| n == *name)
        .map(|(_, label)| *label)
}

/// Sandbox: a sandbox wrapper above the agent or around its commands, or a sandbox flag.
pub fn sandbox(
    product: &str,
    root: &ProcessInfo,
    ancestors: &[&ProcessInfo],
    members: &[&ProcessInfo],
) -> AccessFact {
    let observed = |s: String, level| fact(AccessTopic::Sandbox, level, s, Evidence::Observed);
    if let Some(w) = ancestors.iter().find_map(|p| sandbox_wrapper(p)) {
        return observed(
            format!("The agent itself runs inside {w}."),
            AccessLevel::Restricted,
        );
    }
    let args = &root.cmdline;
    let bypass = match product {
        "codex" => has_flag(
            args,
            &["--dangerously-bypass-approvals-and-sandbox", "--yolo"],
        )
        .or_else(|| {
            flag_value(args, &["--sandbox", "-s"])
                .filter(|v| *v == "danger-full-access")
                .map(|v| format!("--sandbox {v}"))
        }),
        _ => None,
    };
    if let Some(flag) = bypass {
        return observed(
            format!("Started with {flag}: commands run without a sandbox."),
            AccessLevel::Elevated,
        );
    }
    if let Some(w) = members.iter().find_map(|p| sandbox_wrapper(p)) {
        return observed(
            format!("A current child command runs inside {w}."),
            AccessLevel::Restricted,
        );
    }
    let requested = match product {
        "codex" => flag_value(args, &["--sandbox", "-s"])
            .filter(|v| matches!(*v, "read-only" | "workspace-write"))
            .map(|v| format!("--sandbox {v}"))
            .or_else(|| has_flag(args, &["--full-auto"])),
        "gemini" => has_flag(args, &["--sandbox", "-s"]),
        _ => None,
    };
    if let Some(flag) = requested {
        return observed(
            format!("Started with {flag}: commands are sandboxed."),
            AccessLevel::Restricted,
        );
    }
    fact(
        AccessTopic::Sandbox,
        AccessLevel::Unknown,
        "No sandbox seen right now. Its commands may still be sandboxed when they run; agent settings aren't read.".into(),
        Evidence::Unknown,
    )
}

/// Approvals: flags that make the agent act without asking (or only plan).
pub fn approvals(product: &str, root: &ProcessInfo) -> AccessFact {
    let args = &root.cmdline;
    let seen = |level, s: String| fact(AccessTopic::Approvals, level, s, Evidence::Observed);
    let auto = |flag: String| {
        seen(
            AccessLevel::Elevated,
            format!("Started with {flag}: it acts without asking first."),
        )
    };
    match product {
        "claude-code" => {
            if let Some(f) = has_flag(args, &["--dangerously-skip-permissions"]) {
                return auto(f);
            }
            match flag_value(args, &["--permission-mode"]) {
                Some(m @ "bypassPermissions") => return auto(format!("--permission-mode {m}")),
                Some(m @ "plan") => {
                    return seen(
                        AccessLevel::Restricted,
                        format!("Started with --permission-mode {m}: it plans but doesn't change files."),
                    )
                }
                Some(m @ "acceptEdits") => {
                    return seen(
                        AccessLevel::Standard,
                        format!("Started with --permission-mode {m}: file edits are accepted without asking; commands still ask."),
                    )
                }
                _ => {}
            }
        }
        "codex" => {
            if let Some(f) = has_flag(
                args,
                &["--dangerously-bypass-approvals-and-sandbox", "--yolo"],
            ) {
                return auto(f);
            }
            if let Some(v @ "never") = flag_value(args, &["--ask-for-approval", "-a"]) {
                return auto(format!("--ask-for-approval {v}"));
            }
            if let Some(f) = has_flag(args, &["--full-auto"]) {
                return seen(
                    AccessLevel::Standard,
                    format!("Started with {f}: it works inside the workspace without asking and asks to go further."),
                );
            }
        }
        "gemini" => {
            if let Some(f) = has_flag(args, &["--yolo", "-y"]) {
                return auto(f);
            }
            match flag_value(args, &["--approval-mode"]) {
                Some(m @ "yolo") => return auto(format!("--approval-mode {m}")),
                Some(m @ "auto_edit") => {
                    return seen(
                        AccessLevel::Standard,
                        format!("Started with --approval-mode {m}: file edits are accepted without asking; other tools still ask."),
                    )
                }
                _ => {}
            }
        }
        "aider" => {
            if let Some(f) = has_flag(args, &["--yes-always"]) {
                return auto(f);
            }
        }
        "copilot-cli" => {
            if let Some(f) = has_flag(args, &["--allow-all", "--yolo", "--allow-all-tools"]) {
                return auto(f);
            }
        }
        "cursor-agent" => {
            if let Some(f) = has_flag(args, &["--force", "--yolo"]) {
                return auto(f);
            }
            if let Some(f) = has_flag(args, &["--approve-mcps"]) {
                return seen(
                    AccessLevel::Elevated,
                    format!("Started with {f}: MCP servers are accepted without asking."),
                );
            }
            if let Some(f) = has_flag(args, &["--trust"]) {
                return seen(
                    AccessLevel::Elevated,
                    format!("Started with {f}: the workspace is trusted without asking."),
                );
            }
        }
        "cline" | "goose" | "opencode" | "amp" | "crush" | "qwen" => {
            if let Some(f) = has_flag(
                args,
                &[
                    "--yolo",
                    "-y",
                    "--yes",
                    "--force",
                    "--auto-approve",
                    "--dangerously-skip-permissions",
                ],
            ) {
                return auto(f);
            }
        }
        _ => {}
    }
    fact(
        AccessTopic::Approvals,
        AccessLevel::Unknown,
        "No approval flags on its command line; its own settings decide (not read).".into(),
        Evidence::Unknown,
    )
}

/// Network: what it listens on and how many remote hosts it talks to.
pub fn network(ports: &[AgentPort], remote_hosts: usize) -> AccessFact {
    let remote = match remote_hosts {
        0 => String::new(),
        1 => " Talks to 1 remote host.".into(),
        n => format!(" Talks to {n} remote hosts."),
    };
    let shown: Vec<&AgentPort> = if ports.iter().any(|p| p.exposure != Exposure::Loopback) {
        ports
            .iter()
            .filter(|p| p.exposure != Exposure::Loopback)
            .collect()
    } else {
        ports.iter().collect()
    };
    let mut hinted = false;
    let mut labels: Vec<String> = shown
        .iter()
        .map(|p| {
            let (label, from_port) = listener_label(p);
            hinted |= from_port;
            label
        })
        .collect();
    labels.dedup();
    if labels.len() > 4 {
        let n = labels.len() - 4;
        labels.truncate(4);
        labels.push(format!("+{n}"));
    }
    let list = labels.join(", ");
    let (level, text) = if shown.iter().any(|p| p.exposure != Exposure::Loopback) {
        (
            AccessLevel::Elevated,
            format!("Listens on {list} beyond loopback: potentially reachable from your network."),
        )
    } else if ports.is_empty() {
        (AccessLevel::Standard, "No listening sockets.".to_string())
    } else {
        (
            AccessLevel::Standard,
            format!("Listens on {list} on this machine only."),
        )
    };
    fact(
        AccessTopic::Network,
        level,
        format!("{text}{remote}"),
        if hinted {
            Evidence::Inferred
        } else {
            Evidence::Observed
        },
    )
}

/// The macOS privacy-protected area (TCC "Files and Folders") `path` is in, if any. Pure path
/// logic: nothing is read.
pub fn privacy_area(path: &Path, home: Option<&Path>, platform: &str) -> Option<&'static str> {
    if platform != "macos" {
        return None;
    }
    if path.starts_with("/Volumes") {
        return Some("an external or network volume");
    }
    let rel = path.strip_prefix(home?).ok()?;
    const AREAS: &[(&str, &str)] = &[
        ("Desktop", "Desktop"),
        ("Documents", "Documents"),
        ("Downloads", "Downloads"),
        ("Library/Mobile Documents", "iCloud Drive"),
        ("Pictures", "Pictures"),
        ("Movies", "Movies"),
        ("Music", "Music"),
    ];
    AREAS
        .iter()
        .find(|(dir, _)| rel.starts_with(dir))
        .map(|(_, label)| *label)
}

/// Privacy: on macOS, whether it works in a protected folder (so it was allowed in); elsewhere,
/// that folder access follows the account.
pub fn privacy(platform: &str, folders: &[AgentFolder]) -> AccessFact {
    if platform != "macos" {
        let os = match platform {
            "linux" => "Linux",
            "windows" => "Windows",
            other => other,
        };
        return fact(
            AccessTopic::Privacy,
            AccessLevel::Standard,
            format!("{os} has no per-app folder permissions: file access follows the account (and any sandbox)."),
            Evidence::Inferred,
        );
    }
    let mut areas: Vec<&str> = folders
        .iter()
        .filter(|f| f.source != FolderSource::Recent)
        .filter_map(|f| f.privacy_area.as_deref())
        .collect();
    areas.sort_unstable();
    areas.dedup();
    if areas.is_empty() {
        return fact(
            AccessTopic::Privacy,
            AccessLevel::Unknown,
            "macOS privacy grants (Full Disk Access, Files and Folders) can't be read without Full Disk Access; holdmap doesn't ask for it.".into(),
            Evidence::Unknown,
        );
    }
    fact(
        AccessTopic::Privacy,
        AccessLevel::Standard,
        format!(
            "Works in {}, which macOS guards: it (or the terminal it runs in) has been allowed in. Other grants aren't readable.",
            areas.join(", ")
        ),
        Evidence::Inferred,
    )
}

/// Debug, notebook and model-server ports that are risky when reachable. A hint from the
/// number only: the process may be something else on a reused port.
pub fn sensitive_bind(port: u16) -> Option<&'static str> {
    Some(match port {
        5005 => "Java debug",
        5678 => "debugpy",
        5858 | 9229 | 9230 => "Node inspector",
        9222 => "Chrome DevTools",
        11434 => "Ollama",
        _ => return None,
    })
}

fn listener_label(p: &AgentPort) -> (String, bool) {
    if let Some(name) = p.framework.as_deref().filter(|name| {
        matches!(
            *name,
            "Node inspector" | "Chrome DevTools" | "Jupyter" | "Ollama" | "llama.cpp" | "LM Studio"
        )
    }) {
        return (format!(":{} ({name})", p.port), false);
    }
    if let Some(hint) = sensitive_bind(p.port) {
        return (format!(":{} ({hint})", p.port), true);
    }
    (format!(":{}", p.port), false)
}

/// A conventional service name for a well-known port (a hint, not a probe).
pub fn service_hint(port: u16) -> Option<&'static str> {
    sensitive_bind(port).or(match port {
        22 => Some("SSH"),
        53 => Some("DNS"),
        80 => Some("HTTP"),
        443 => Some("HTTPS"),
        3306 => Some("MySQL"),
        5432 => Some("PostgreSQL"),
        6379 => Some("Redis"),
        27017 => Some("MongoDB"),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::proc;

    #[test]
    fn approval_flags_are_reported_as_seen() {
        let p = proc(
            1,
            0,
            "claude",
            &["claude", "--dangerously-skip-permissions"],
        );
        let f = approvals("claude-code", &p);
        assert_eq!(
            (f.level, f.evidence),
            (AccessLevel::Elevated, Evidence::Observed)
        );
        let p = proc(1, 0, "claude", &["claude", "--permission-mode=plan"]);
        assert_eq!(approvals("claude-code", &p).level, AccessLevel::Restricted);
        let p = proc(1, 0, "codex", &["codex", "-a", "never"]);
        assert_eq!(approvals("codex", &p).level, AccessLevel::Elevated);
        let p = proc(1, 0, "gemini", &["gemini", "-y"]);
        assert_eq!(approvals("gemini", &p).level, AccessLevel::Elevated);
        let p = proc(1, 0, "gemini", &["gemini", "--approval-mode", "auto_edit"]);
        assert_eq!(approvals("gemini", &p).level, AccessLevel::Standard);
        let p = proc(1, 0, "copilot", &["copilot", "--yolo"]);
        assert_eq!(approvals("copilot-cli", &p).level, AccessLevel::Elevated);
        let p = proc(1, 0, "copilot", &["copilot", "--allow-all"]);
        assert_eq!(approvals("copilot-cli", &p).level, AccessLevel::Elevated);
        let p = proc(1, 0, "cursor-agent", &["cursor-agent", "--approve-mcps"]);
        let f = approvals("cursor-agent", &p);
        assert_eq!(f.level, AccessLevel::Elevated);
        assert!(f.summary.contains("--approve-mcps"), "{}", f.summary);
        let p = proc(1, 0, "cursor-agent", &["cursor-agent", "--trust"]);
        assert!(approvals("cursor-agent", &p).summary.contains("--trust"));
        let p = proc(1, 0, "cursor-agent", &["cursor-agent", "--force"]);
        assert_eq!(approvals("cursor-agent", &p).level, AccessLevel::Elevated);
        let p = proc(1, 0, "claude", &["claude"]);
        let f = approvals("claude-code", &p);
        assert_eq!(
            (f.level, f.evidence),
            (AccessLevel::Unknown, Evidence::Unknown)
        );
        // A flag means nothing for a product that doesn't define it.
        let p = proc(1, 0, "aider", &["aider", "--yolo"]);
        assert_eq!(approvals("aider", &p).level, AccessLevel::Unknown);
    }

    #[test]
    fn sandbox_comes_from_flags_and_wrapper_processes() {
        let codex = proc(1, 0, "codex", &["codex", "--sandbox", "workspace-write"]);
        assert_eq!(
            sandbox("codex", &codex, &[], &[]).level,
            AccessLevel::Restricted
        );
        let yolo = proc(1, 0, "codex", &["codex", "--yolo"]);
        assert_eq!(
            sandbox("codex", &yolo, &[], &[]).level,
            AccessLevel::Elevated
        );
        let claude = proc(1, 0, "claude", &["claude"]);
        let seatbelt = proc(
            2,
            1,
            "sandbox-exec",
            &["sandbox-exec", "-p", "…", "npm", "test"],
        );
        let f = sandbox("claude-code", &claude, &[], &[&seatbelt]);
        assert_eq!(
            (f.level, f.evidence),
            (AccessLevel::Restricted, Evidence::Observed)
        );
        assert!(f.summary.contains("Seatbelt"));
        let f = sandbox("claude-code", &claude, &[], &[]);
        assert_eq!(
            (f.level, f.evidence),
            (AccessLevel::Unknown, Evidence::Unknown)
        );
    }

    #[test]
    fn root_is_elevated() {
        let mut p = proc(1, 0, "claude", &[]);
        p.uid = Some(0);
        p.user = Some("root".into());
        assert_eq!(user(&p, false).level, AccessLevel::Elevated);
        let p = proc(1, 0, "claude", &[]);
        assert!(user(&p, true).summary.contains("you (dev)"));
    }

    #[test]
    fn privacy_areas_are_macos_only_and_path_based() {
        let home = Path::new("/Users/me");
        let at = |p: &str| privacy_area(Path::new(p), Some(home), "macos");
        assert_eq!(at("/Users/me/Documents/shop-web"), Some("Documents"));
        assert_eq!(
            at("/Users/me/Library/Mobile Documents/x"),
            Some("iCloud Drive")
        );
        assert_eq!(at("/Volumes/USB/x"), Some("an external or network volume"));
        assert_eq!(at("/Users/me/code/shop-web"), None);
        assert_eq!(
            privacy_area(
                Path::new("/home/me/Documents/x"),
                Some(Path::new("/home/me")),
                "linux"
            ),
            None
        );
    }

    #[test]
    fn prompt_arguments_after_separator_do_not_set_access_policy() {
        let p = proc(
            1,
            0,
            "codex",
            &["codex", "--", "--yolo", "--sandbox", "workspace-write"],
        );
        assert_eq!(approvals("codex", &p).evidence, Evidence::Unknown);
        assert_eq!(sandbox("codex", &p, &[], &[]).evidence, Evidence::Unknown);
        let p = proc(1, 0, "codex", &["codex", "--sandbox", "invalid-mode"]);
        assert_eq!(sandbox("codex", &p, &[], &[]).evidence, Evidence::Unknown);
    }

    #[test]
    fn a_specific_network_interface_is_not_described_as_machine_only() {
        let port = AgentPort {
            entry_id: "tcp:4000".into(),
            port: 4000,
            protocol: crate::model::Protocol::Tcp,
            exposure: Exposure::Specific,
            pid: Some(1),
            process: None,
            label: "service".into(),
            role: super::super::PortRole::Service,
            project: None,
            framework: None,
        };
        let fact = network(&[port], 0);
        assert_eq!(fact.level, AccessLevel::Elevated);
        assert!(fact.summary.contains("beyond loopback"));
        assert!(!fact.summary.contains("machine only"));
    }

    #[test]
    fn debug_and_model_ports_are_named_in_the_network_fact() {
        let port = |n: u16, exposure, framework: Option<&str>| AgentPort {
            entry_id: format!("tcp:{n}"),
            port: n,
            protocol: crate::model::Protocol::Tcp,
            exposure,
            pid: Some(1),
            process: None,
            label: "service".into(),
            role: super::super::PortRole::Service,
            project: None,
            framework: framework.map(str::to_string),
        };
        let loopback = crate::model::Exposure::Loopback;
        let all = crate::model::Exposure::AllInterfaces;
        let fact = network(&[port(9229, loopback, None)], 0);
        assert_eq!(fact.level, AccessLevel::Standard);
        assert_eq!(fact.evidence, Evidence::Inferred);
        assert!(
            fact.summary.contains(":9229 (Node inspector)"),
            "{}",
            fact.summary
        );
        let fact = network(&[port(9229, all, Some("Next.js"))], 0);
        assert_eq!(fact.level, AccessLevel::Elevated);
        assert_eq!(fact.evidence, Evidence::Inferred);
        assert!(fact.summary.contains("Node inspector"));
        let fact = network(&[port(8888, loopback, Some("Vite"))], 0);
        assert_eq!(fact.evidence, Evidence::Observed);
        assert!(fact.summary.contains(":8888"), "{}", fact.summary);
        assert!(!fact.summary.contains("Jupyter"), "{}", fact.summary);
        let fact = network(&[port(11434, all, Some("Ollama"))], 0);
        assert_eq!(fact.level, AccessLevel::Elevated);
        assert_eq!(fact.evidence, Evidence::Observed);
        assert!(fact.summary.contains(":11434 (Ollama)"), "{}", fact.summary);
        assert_eq!(sensitive_bind(11434), Some("Ollama"));
        assert_eq!(service_hint(9222), Some("Chrome DevTools"));
        assert_eq!(service_hint(5432), Some("PostgreSQL"));
        assert_eq!(sensitive_bind(8888), None);
    }
}

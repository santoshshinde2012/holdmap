//! Safety policy: which processes holdmap must never (or only with explicit consent) signal.

use crate::model::ProcessInfo;
use crate::process::ProcessTable;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

static PROTECT_OWN_TREE: AtomicBool = AtomicBool::new(true);

/// Whether processes *started by* this holdmap process (its descendants: the desktop app's
/// WebView helpers, the MCP server's children, `holdmap run`'s command…) are protected.
/// On by default for every frontend. Test harnesses that spawn their own fixture servers as
/// children can turn it off.
pub fn set_protect_own_tree(on: bool) {
    PROTECT_OWN_TREE.store(on, Ordering::Relaxed);
}

/// Whether holdmap protects its own process tree (on by default; see [`set_protect_own_tree`]).
pub fn protect_own_tree() -> bool {
    PROTECT_OWN_TREE.load(Ordering::Relaxed)
}

/// How strongly a process is protected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "level", content = "reason", rename_all = "snake_case")]
pub enum Protection {
    /// Not protected.
    None,
    /// System/desktop/IDE process: blocked unless `--allow-protected`.
    Soft(String),
    /// Never signalled: init/kernel/session processes, holdmap itself and its ancestors.
    Hard(String),
}

impl Protection {
    /// True for soft and hard protection.
    pub fn is_protected(&self) -> bool {
        !matches!(self, Protection::None)
    }
}

/// Never touched, whatever the flags.
const HARD: &[&str] = &[
    "init",
    "launchd",
    "kernel_task",
    "systemd",
    "system",
    "registry",
    "smss",
    "csrss",
    "wininit",
    "winlogon",
    "lsass",
    "services",
    "secure system",
    "memory compression",
    "loginwindow",
    "windowserver",
];

/// Protected by default: OS services, container daemons and desktop shells. Terminals, IDEs,
/// agents and interactive shells (and whatever hosts them) are covered by [`session_kind`].
const SOFT: &[&str] = &[
    // macOS
    "finder",
    "dock",
    "systemuiserver",
    "controlcenter",
    "mdnsresponder",
    "rapportd",
    "sharingd",
    "coreaudiod",
    "airplayxpchelper",
    "remoted",
    "configd",
    "notifyd",
    "cfprefsd",
    // Linux
    "sshd",
    "dbus-daemon",
    "dbus-broker",
    "networkmanager",
    "systemd-resolved",
    "systemd-networkd",
    "systemd-journald",
    "systemd-logind",
    "systemd-udevd",
    "xorg",
    "xwayland",
    "gnome-shell",
    "plasmashell",
    "kwin_wayland",
    "kwin_x11",
    "pipewire",
    "pulseaudio",
    "wireplumber",
    "cupsd",
    "avahi-daemon",
    "chronyd",
    "polkitd",
    "gdm",
    "sddm",
    "lightdm",
    "agetty",
    // Container runtimes (stop containers, not the daemon)
    "dockerd",
    "containerd",
    "containerd-shim",
    "containerd-shim-runc-v2",
    "com.docker.backend",
    "docker desktop",
    "com.docker.vpnkit",
    "orbstack",
    "orbstack helper",
    "podman",
    "colima",
    "limactl",
    // Windows
    "svchost",
    "explorer",
    "spoolsv",
    "dwm",
    "fontdrvhost",
    "sihost",
    "ctfmon",
    "conhost",
    "runtimebroker",
    "searchhost",
    "startmenuexperiencehost",
    "msmpeng",
];

/// Shells: protected (and their ancestors too) when they are interactive.
const SHELLS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "dash",
    "fish",
    "ash",
    "ksh",
    "tcsh",
    "csh",
    "nu",
    "elvish",
    "xonsh",
    "cmd",
    "powershell",
    "pwsh",
];

/// Path components that identify IDE remote servers and agent hosts running on a generic
/// runtime (`node /home/me/.vscode-server/...`, `node /exec-daemon/index.js`, `node …/claude-code/cli.js`).
const HOST_MARKERS: &[(&str, SessionKind)] = &[
    (".vscode-server", SessionKind::Editor),
    (".vscode-server-insiders", SessionKind::Editor),
    (".vscode-remote", SessionKind::Editor),
    (".cursor-server", SessionKind::Editor),
    (".windsurf-server", SessionKind::Editor),
    ("code-server", SessionKind::Editor),
    ("openvscode-server", SessionKind::Editor),
    ("remote-dev-server", SessionKind::Editor),
    ("exec-daemon", SessionKind::Agent),
    ("claude-code", SessionKind::Agent),
    ("gemini-cli", SessionKind::Agent),
];

const TERMINALS: &[&str] = &[
    "terminal",
    "iterm2",
    "wezterm",
    "wezterm-gui",
    "alacritty",
    "kitty",
    "ghostty",
    "warp",
    "gnome-terminal-server",
    "konsole",
    "xterm",
    "tilix",
    "windowsterminal",
    "openconsole",
    "tmux",
    "tmux: server",
    "screen",
];

const EDITORS: &[&str] = &[
    "code",
    "code helper",
    "code helper (plugin)",
    "code - insiders",
    "cursor",
    "cursor helper",
    "windsurf",
    "zed",
    "idea",
    "webstorm",
    "pycharm",
    "goland",
    "rustrover",
    "clion",
    "rider",
    "sublime_text",
];

/// Something the user is working *in*: stopping it, or anything hosting it, ends their session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    /// An interactive shell (bash, zsh, fish, PowerShell…).
    Shell,
    /// A terminal emulator or multiplexer.
    Terminal,
    /// An editor or IDE (including remote servers).
    Editor,
    /// An AI coding agent host.
    Agent,
}

impl SessionKind {
    /// Short human label, e.g. "interactive shell".
    pub fn label(self) -> &'static str {
        match self {
            SessionKind::Shell => "interactive shell",
            SessionKind::Terminal => "terminal",
            SessionKind::Editor => "editor/IDE",
            SessionKind::Agent => "AI agent",
        }
    }
}

/// An interactive shell: a login shell (`-zsh`), `bash -i`/`-l`, or a bare `bash`, but not
/// `sh -c "next dev"` or `bash script.sh`.
pub fn is_interactive_shell(p: &ProcessInfo) -> bool {
    let n = norm(&p.name);
    if !SHELLS.contains(&n.as_str()) {
        return false;
    }
    if p.cmdline.first().is_some_and(|a0| a0.starts_with('-')) {
        return true;
    }
    let mut interactive_flag = false;
    for a in p.cmdline.iter().skip(1) {
        let al = a.to_ascii_lowercase();
        if al == "-c" || al == "/c" || al == "/k" || al == "-command" || al == "-file" {
            return false;
        }
        if let Some(flags) = a.strip_prefix('-').filter(|f| !f.starts_with('-')) {
            if flags.contains('c') {
                return false;
            }
            if flags.contains('i') || flags.contains('l') {
                interactive_flag = true;
            }
            continue;
        }
        if a == "--login" || a == "--interactive" {
            interactive_flag = true;
            continue;
        }
        if a.starts_with("--") {
            continue;
        }
        // A positional argument is a script to run.
        return false;
    }
    interactive_flag || p.cmdline.len() <= 1
}

/// What kind of user session `p` is, if any.
pub fn session_kind(p: &ProcessInfo) -> Option<SessionKind> {
    let n = norm(&p.name);
    // AI coding agents that run your commands: stopping them (or their host) loses the session.
    if let Some(product) = crate::agents::catalog::identify(p) {
        match product.kind {
            crate::agents::AgentKind::Ide => return Some(SessionKind::Editor),
            crate::agents::AgentKind::Tool => {} // Runtime apps keep their existing OS-service rules.
            _ => return Some(SessionKind::Agent),
        }
    }
    if EDITORS.contains(&n.as_str()) {
        return Some(SessionKind::Editor);
    }
    if TERMINALS.contains(&n.as_str()) {
        return Some(SessionKind::Terminal);
    }
    // Only executable metadata and the parsed runtime entry identify a host. Option values
    // and arguments to an ordinary project command can contain agent/editor paths as data.
    let executable = p.exe.as_ref().map(|path| path.to_string_lossy());
    for a in executable
        .as_deref()
        .into_iter()
        .chain(p.cmdline.first().map(String::as_str))
        .chain(crate::agents::catalog::runtime_entry(p))
    {
        for comp in a.split(['/', '\\']) {
            if let Some((_, k)) = HOST_MARKERS.iter().find(|(m, _)| comp == *m) {
                return Some(*k);
            }
        }
    }
    if is_interactive_shell(p) {
        return Some(SessionKind::Shell);
    }
    None
}

fn norm(name: &str) -> String {
    let n = name.trim().to_ascii_lowercase();
    n.strip_suffix(".exe").map(str::to_owned).unwrap_or(n)
}

/// Which processes holdmap may signal. Engines, scanners and executors depend on this
/// abstraction, not on the concrete rule set, so frontends and tests can inject their own.
pub trait ProtectionPolicy: Send + Sync + std::fmt::Debug {
    /// Classify `p` (with its ancestry from `table`).
    fn protection(&self, p: &ProcessInfo, table: &ProcessTable) -> Protection;

    /// True when `p` is protected at any level.
    fn is_protected(&self, p: &ProcessInfo, table: &ProcessTable) -> bool {
        self.protection(p, table).is_protected()
    }
}

/// The built-in rule set shared by the CLI, TUI, desktop app and MCP server (see [`protection`]).
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultProtectionPolicy;

impl ProtectionPolicy for DefaultProtectionPolicy {
    fn protection(&self, p: &ProcessInfo, table: &ProcessTable) -> Protection {
        protection(p, table)
    }
}

/// Decide how protected `p` is.
pub fn protection(p: &ProcessInfo, table: &ProcessTable) -> Protection {
    if p.pid <= 1 || (cfg!(windows) && p.pid == 4) {
        return Protection::Hard(format!("PID {} is a core operating-system process", p.pid));
    }
    if p.pid == table.self_pid() {
        return Protection::Hard("this is holdmap itself".into());
    }
    if table.is_self_or_ancestor(p.pid) {
        return Protection::Hard(format!(
            "{} ({}) is running holdmap (your terminal, shell or editor)",
            p.name, p.pid
        ));
    }
    if protect_own_tree() && table.is_self_descendant(p.pid) {
        return Protection::Hard(format!(
            "{} ({}) was started by this holdmap app",
            p.name, p.pid
        ));
    }
    let n = norm(&p.name);
    if HARD.contains(&n.as_str()) {
        return Protection::Hard(format!("{} is a core operating-system process", p.name));
    }
    if SOFT.contains(&n.as_str()) || n.starts_with("systemd-") {
        return Protection::Soft(format!(
            "{} is a system, desktop, container-runtime or editor process",
            p.name
        ));
    }
    if let Some(kind) = session_kind(p) {
        return Protection::Soft(format!(
            "{} ({}) is your {}; stopping it ends that session",
            p.name,
            p.pid,
            kind.label()
        ));
    }
    if let Some((sp, kind)) = table.hosted_session(p.pid) {
        return Protection::Soft(format!(
            "{} ({}) hosts your {} {} ({}); stopping it would end that session",
            p.name,
            p.pid,
            kind.label(),
            sp.name,
            sp.pid
        ));
    }
    Protection::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::{proc, table};

    #[test]
    fn protects_core_self_and_ancestors() {
        let t = table(
            vec![
                proc(1, 0, "systemd", &[]),
                proc(10, 1, "zsh", &["-zsh"]),
                proc(30, 10, "holdmap", &["holdmap"]),
                proc(40, 1, "node", &["node", "server.js"]),
                proc(50, 1, "sshd", &["sshd"]),
                proc(60, 1, "Code Helper (Plugin)", &[]),
                proc(70, 1, "dockerd", &[]),
            ],
            30,
        );
        let get = |pid| protection(t.get(pid).unwrap(), &t);
        assert!(matches!(get(1), Protection::Hard(_)));
        assert!(matches!(get(10), Protection::Hard(_)), "our shell");
        assert!(matches!(get(30), Protection::Hard(_)), "ourselves");
        assert_eq!(get(40), Protection::None);
        assert!(matches!(get(50), Protection::Soft(_)));
        assert!(matches!(get(60), Protection::Soft(_)));
        assert!(matches!(get(70), Protection::Soft(_)));
    }

    #[test]
    fn exe_suffix_is_ignored() {
        let t = table(
            vec![
                proc(900, 1, "svchost.exe", &[]),
                proc(901, 1, "lsass.exe", &[]),
            ],
            999,
        );
        assert!(matches!(
            protection(t.get(900).unwrap(), &t),
            Protection::Soft(_)
        ));
        assert!(matches!(
            protection(t.get(901).unwrap(), &t),
            Protection::Hard(_)
        ));
    }

    #[test]
    fn interactive_shell_detection() {
        let sh = |cmd: &[&str]| is_interactive_shell(&proc(5, 1, "bash", cmd));
        assert!(sh(&["-bash"]), "login shell");
        assert!(sh(&["bash"]), "bare shell");
        assert!(sh(&["bash", "-i"]));
        assert!(sh(&["bash", "--login"]));
        assert!(!sh(&["bash", "-c", "next dev"]));
        assert!(!sh(&["bash", "-lc", "npm run dev"]));
        assert!(!sh(&["bash", "-O", "extglob", "-c", "x"]));
        assert!(!sh(&["bash", "scripts/demo.sh"]));
        assert!(!is_interactive_shell(&proc(5, 1, "node", &["node"])));
    }

    #[test]
    fn terminal_session_and_its_hosts_are_protected_but_not_the_dev_server() {
        // Desktop app launched from the OS (not from the terminal): ancestors don't help here.
        let t = table(
            vec![
                proc(1, 0, "launchd", &[]),
                proc(5, 1, "Terminal", &[]),
                proc(6, 5, "login", &["login", "-pf", "dev"]),
                proc(7, 6, "zsh", &["-zsh"]),
                proc(8, 7, "npm", &["npm", "run", "dev"]),
                proc(9, 8, "sh", &["sh", "-c", "next dev"]),
                proc(10, 9, "node", &["node", "next", "dev"]),
                proc(300, 1, "holdmap-desktop", &[]),
            ],
            300,
        );
        let get = |pid| protection(t.get(pid).unwrap(), &t);
        assert!(matches!(get(5), Protection::Soft(_)), "terminal");
        match get(6) {
            Protection::Soft(r) => assert!(r.contains("hosts your interactive shell"), "{r}"),
            other => panic!("login should host the shell: {other:?}"),
        }
        assert!(matches!(get(7), Protection::Soft(_)), "interactive shell");
        assert_eq!(get(8), Protection::None);
        assert_eq!(get(9), Protection::None);
        assert_eq!(get(10), Protection::None);
    }

    #[test]
    fn ide_and_agent_hosts_are_protected() {
        let t = table(
            vec![
                proc(1, 0, "init", &[]),
                proc(
                    40,
                    1,
                    "node",
                    &["/exec-daemon/node", "/exec-daemon/index.js", "serve"],
                ),
                proc(41, 40, "bash", &["bash", "-c", "npm run dev"]),
                proc(42, 41, "node", &["node", "server.js"]),
                proc(
                    50,
                    1,
                    "node",
                    &[
                        "node",
                        "/home/dev/.vscode-server/bin/abc/out/server-main.js",
                    ],
                ),
                proc(60, 1, "node", &["node", "/usr/local/bin/claude"]),
                proc(61, 60, "bash", &["bash", "-c", "vite"]),
                proc(62, 61, "node", &["node", "vite"]),
                proc(70, 1, "node", &["node", "/home/dev/claude/server.js"]),
                proc(80, 1, "claude", &["claude"]),
                proc(300, 1, "holdmap-desktop", &[]),
            ],
            300,
        );
        let get = |pid| protection(t.get(pid).unwrap(), &t);
        assert!(matches!(get(40), Protection::Soft(_)), "agent daemon");
        assert!(matches!(get(50), Protection::Soft(_)), "VS Code server");
        assert!(matches!(get(60), Protection::Soft(_)), "claude via node");
        assert!(matches!(get(80), Protection::Soft(_)), "claude binary");
        assert_eq!(get(42), Protection::None, "dev server started by the agent");
        assert_eq!(get(62), Protection::None);
        assert_eq!(
            get(70),
            Protection::None,
            "a project folder named claude is fine"
        );
    }

    #[test]
    fn protection_uses_the_same_runtime_entry_as_agent_discovery() {
        let cases = [
            (
                10,
                "node",
                vec![
                    "node",
                    "--conditions",
                    "development",
                    "/usr/local/bin/codex",
                ],
                Some(SessionKind::Agent),
            ),
            (
                11,
                "python3",
                vec!["python3", "-X", "dev", "-W", "default", "-m", "aider"],
                Some(SessionKind::Agent),
            ),
            (
                12,
                "node",
                vec![
                    "node",
                    "--conditions",
                    "development",
                    "/home/dev/.vscode-server/bin/abc/out/server-main.js",
                ],
                Some(SessionKind::Editor),
            ),
            (
                13,
                "node",
                vec!["node", "worker.js", "/home/dev/.vscode-server/input"],
                None,
            ),
            (
                14,
                "cp",
                vec!["cp", "/usr/local/bin/claude", "/tmp/backup"],
                None,
            ),
            (
                15,
                "node",
                vec!["node", "--conditions", "codex", "worker.js"],
                None,
            ),
            (16, "orb", vec!["orb"], None),
        ];
        let t = table(
            cases
                .iter()
                .map(|(pid, name, args, _)| proc(*pid, 1, name, args))
                .collect(),
            300,
        );
        for (pid, _, _, expected) in cases {
            let p = t.get(pid).unwrap();
            assert_eq!(session_kind(p), expected, "PID {pid}");
            match expected {
                Some(_) => assert!(
                    matches!(protection(p, &t), Protection::Soft(_)),
                    "PID {pid}"
                ),
                None => assert_eq!(protection(p, &t), Protection::None, "PID {pid}"),
            }
        }
    }

    #[test]
    fn own_process_tree_is_hard_protected() {
        let t = table(
            vec![
                proc(1, 0, "launchd", &[]),
                proc(300, 1, "holdmap-desktop", &[]),
                proc(301, 300, "WebKitWebProcess", &[]),
                proc(302, 301, "WebKitNetworkProcess", &[]),
            ],
            300,
        );
        assert!(protect_own_tree());
        for pid in [300, 301, 302] {
            assert!(
                matches!(protection(t.get(pid).unwrap(), &t), Protection::Hard(_)),
                "pid {pid}"
            );
        }
    }
}

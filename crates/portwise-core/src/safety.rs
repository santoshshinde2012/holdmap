//! Safety policy: which processes portwise must never (or only with explicit consent) signal.

use crate::model::ProcessInfo;
use crate::process::ProcessTable;
use serde::{Deserialize, Serialize};

/// How strongly a process is protected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "level", content = "reason", rename_all = "snake_case")]
pub enum Protection {
    /// Not protected.
    None,
    /// System/desktop/IDE process: blocked unless `--allow-protected`.
    Soft(String),
    /// Never signalled: init/kernel/session processes, portwise itself and its ancestors.
    Hard(String),
}

impl Protection {
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

/// Protected by default: OS services, container daemons, desktop shells, terminals and IDEs.
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
    // Terminals & editors (stopping them loses your work)
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
    "tmux",
    "tmux: server",
    "screen",
];

fn norm(name: &str) -> String {
    let n = name.trim().to_ascii_lowercase();
    n.strip_suffix(".exe").map(str::to_owned).unwrap_or(n)
}

/// Decide how protected `p` is.
pub fn protection(p: &ProcessInfo, table: &ProcessTable) -> Protection {
    if p.pid <= 1 || (cfg!(windows) && p.pid == 4) {
        return Protection::Hard(format!("PID {} is a core operating-system process", p.pid));
    }
    if p.pid == table.self_pid() {
        return Protection::Hard("this is portwise itself".into());
    }
    if table.is_self_or_ancestor(p.pid) {
        return Protection::Hard(format!(
            "{} ({}) is running portwise (your terminal, shell or editor)",
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
                proc(30, 10, "portwise", &["portwise"]),
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
            1,
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
}

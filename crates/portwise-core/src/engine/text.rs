//! Small text helpers used to build explanations.

use crate::model::{Exposure, PortEntry, ProcessInfo};
use crate::util::human_bytes;

pub(crate) fn fmt_ms(ms: u64) -> String {
    if ms.is_multiple_of(1000) {
        format!("{}s", ms / 1000)
    } else {
        format!("{ms}ms")
    }
}

pub(crate) fn is_root() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no preconditions.
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

pub(crate) fn sudo() -> &'static str {
    if cfg!(windows) {
        ""
    } else {
        "sudo "
    }
}

/// Replace the home directory prefix with `~`.
pub fn tilde(p: &std::path::Path) -> String {
    let s = p.display().to_string();
    if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        let h = home.to_string_lossy();
        if let Some(rest) = s.strip_prefix(h.as_ref()) {
            return format!("~{rest}");
        }
    }
    s
}

pub(crate) fn article(word: &str) -> &'static str {
    match word.chars().next().map(|c| c.to_ascii_lowercase()) {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

pub(crate) fn exposure_detail(e: &PortEntry) -> String {
    match e.exposure {
        Exposure::AllInterfaces => format!(
            "Listening on all interfaces ({}): reachable from other devices on your network.",
            e.addresses.join(", ")
        ),
        Exposure::Loopback => format!(
            "Listening on {} (this machine only).",
            e.addresses.join(", ")
        ),
        Exposure::Specific => format!("Listening on {}.", e.addresses.join(", ")),
    }
}

pub(crate) fn short_cmd(p: &ProcessInfo) -> String {
    let parts: Vec<String> = crate::redact::args(&p.cmdline)
        .iter()
        .take(4)
        .map(|a| {
            std::path::Path::new(a)
                .file_name()
                .filter(|_| a.contains('/') || a.contains('\\'))
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| a.clone())
        })
        .collect();
    if parts.is_empty() {
        p.name.clone()
    } else {
        parts.join(" ")
    }
}

pub(crate) fn process_details(p: &ProcessInfo, e: &PortEntry) -> Vec<String> {
    let mut d = vec![format!(
        "Process: {} (PID {}){}{}",
        p.name,
        p.pid,
        p.user
            .as_ref()
            .map(|u| format!(", user {u}"))
            .unwrap_or_default(),
        if p.memory_bytes > 0 {
            format!(", {} memory", human_bytes(p.memory_bytes))
        } else {
            String::new()
        }
    )];
    if !p.cmdline.is_empty() {
        let mut c = p.command();
        if c.chars().count() > 160 {
            c = c.chars().take(157).collect();
            c.push('…');
        }
        d.push(format!("Command: {c}"));
    }
    if let Some(pr) = &e.project {
        d.push(format!(
            "Project: {} ({}{})",
            pr.name,
            tilde(&pr.root),
            pr.git_branch
                .as_ref()
                .map(|b| format!(", branch {b}"))
                .unwrap_or_default()
        ));
    } else if let Some(cwd) = &p.cwd {
        d.push(format!("Working directory: {}", tilde(cwd)));
    }
    d
}

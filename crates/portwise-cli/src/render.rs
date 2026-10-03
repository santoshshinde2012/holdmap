//! Human-readable rendering of snapshots, explanations, plans and reports.

use crate::style::{self, bold, col, dim, flex, paint, rcol, Cell, S};
use portwise_core::util::{human_bytes, human_duration, now_secs};
use portwise_core::*;

pub fn category_style(e: &PortEntry) -> S {
    if e.process.is_none() && e.container.is_none() {
        return S::Dim;
    }
    match e.framework.as_ref().map(|f| f.category) {
        Some(FrameworkCategory::DevServer | FrameworkCategory::AppServer) => S::Green,
        Some(FrameworkCategory::Database | FrameworkCategory::Cache | FrameworkCategory::Queue) => {
            S::Magenta
        }
        Some(FrameworkCategory::Container) => S::Blue,
        Some(FrameworkCategory::WebServer | FrameworkCategory::Tool) => S::Cyan,
        Some(FrameworkCategory::System) => S::Dim,
        Some(FrameworkCategory::App) | None => {
            if e.is_dev {
                S::Green
            } else {
                S::Plain
            }
        }
    }
}

/// "localhost", "0.0.0.0, ::", "192.168.1.5".
pub fn address_label(e: &PortEntry) -> String {
    match e.exposure {
        Exposure::Loopback => {
            if e.addresses.len() > 1 {
                "localhost".into()
            } else {
                e.addresses.join(", ")
            }
        }
        _ => e
            .addresses
            .iter()
            .map(|a| {
                if a.contains(':') {
                    format!("[{a}]")
                } else {
                    a.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
    }
}

pub fn what_label(e: &PortEntry) -> String {
    let mut s = e.label.clone();
    if let Some(b) = e.project.as_ref().and_then(|p| p.git_branch.as_ref()) {
        s.push_str(&format!(" ({b})"));
    }
    if e.protected {
        s.push_str(" [protected]");
    }
    s
}

pub fn list_table(entries: &[&PortEntry], wide: bool, all_states: bool) -> String {
    list_table_with(entries, wide, all_states, &[], None)
}

/// [`list_table`] with pinned ports marked `★`, and an HTTP column when `http` is given.
pub fn list_table_with(
    entries: &[&PortEntry],
    wide: bool,
    all_states: bool,
    pins: &[u16],
    http: Option<&std::collections::BTreeMap<u16, portwise_core::http::HttpInfo>>,
) -> String {
    let mut cols = vec![rcol("PORT"), col("PROTO")];
    if all_states {
        cols.push(col("STATE"));
    }
    cols.extend([
        flex("ADDRESS", 9),
        rcol("PID"),
        flex("PROCESS", 8),
        flex("WHAT", 12),
    ]);
    if wide {
        cols.extend([
            col("USER"),
            rcol("UPTIME"),
            rcol("CPU"),
            rcol("MEM"),
            flex("COMMAND", 10),
        ]);
    }
    if all_states {
        cols.insert(4, flex("REMOTE", 8));
    }
    if http.is_some() {
        cols.push(flex("HTTP", 10));
    }
    let now = now_secs();
    let rows: Vec<Vec<Cell>> = entries
        .iter()
        .map(|e| {
            let port = if pins.contains(&e.port) {
                format!("★ {}", e.port)
            } else {
                e.port.to_string()
            };
            let mut r = vec![
                Cell::new(port, S::BoldCyan),
                Cell::new(e.protocol.to_string(), S::Dim),
            ];
            if all_states {
                let st = if e.state.is_listening() {
                    S::Green
                } else {
                    S::Dim
                };
                r.push(Cell::new(e.state.as_str(), st));
            }
            let addr_style = if e.exposure == Exposure::AllInterfaces {
                S::Yellow
            } else {
                S::Dim
            };
            r.push(Cell::new(address_label(e), addr_style));
            if all_states {
                r.push(Cell::new(e.remote.clone().unwrap_or_default(), S::Dim));
            }
            r.push(Cell::new(
                e.pid.map(|p| p.to_string()).unwrap_or_else(|| "–".into()),
                S::Plain,
            ));
            r.push(Cell::new(
                e.process
                    .as_ref()
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "–".into()),
                if e.protected { S::Dim } else { S::Plain },
            ));
            r.push(Cell::new(what_label(e), category_style(e)));
            if wide {
                let p = e.process.as_ref();
                r.push(Cell::new(e.user.clone().unwrap_or_default(), S::Dim));
                r.push(Cell::new(
                    p.map(|p| human_duration(now.saturating_sub(p.start_time)))
                        .unwrap_or_default(),
                    S::Dim,
                ));
                r.push(Cell::new(
                    p.filter(|p| p.cpu_percent >= 0.05)
                        .map(|p| format!("{:.1}%", p.cpu_percent))
                        .unwrap_or_default(),
                    S::Dim,
                ));
                r.push(Cell::new(
                    p.filter(|p| p.memory_bytes > 0)
                        .map(|p| human_bytes(p.memory_bytes))
                        .unwrap_or_default(),
                    S::Dim,
                ));
                r.push(Cell::new(
                    p.map(|p| p.command()).unwrap_or_default(),
                    S::Dim,
                ));
            }
            if let Some(h) = http {
                let info = (e.protocol == Protocol::Tcp)
                    .then(|| h.get(&e.port))
                    .flatten();
                r.push(match info {
                    Some(i) if i.healthy() => Cell::new(i.summary(), S::Plain),
                    Some(i) => Cell::new(i.summary(), S::Yellow),
                    None => Cell::new("", S::Dim),
                });
            }
            r
        })
        .collect();
    style::table(&cols, &rows, style::term_width())
}

pub fn summary_line(snap: &Snapshot, shown: &[&PortEntry]) -> String {
    let dev = shown.iter().filter(|e| e.is_dev).count();
    let exposed = shown
        .iter()
        .filter(|e| e.exposure == Exposure::AllInterfaces)
        .count();
    let mut parts = vec![format!(
        "{} port{}",
        shown.len(),
        if shown.len() == 1 { "" } else { "s" }
    )];
    if dev > 0 {
        parts.push(format!("{dev} dev"));
    }
    if exposed > 0 {
        parts.push(paint(
            format!("{exposed} exposed to the network"),
            S::Yellow,
        ));
    }
    if snap.hidden_sockets > 0 {
        parts.push(format!(
            "{} owned by other users (run with sudo for details)",
            snap.hidden_sockets
        ));
    }
    if snap.docker_available {
        parts.push("containers ✓".into());
    }
    parts.push(format!("{} ms", snap.scan_ms));
    dim(parts.join(" · "))
}

/// Wrap `text` to `width` with `indent` on continuation lines.
pub fn wrap(text: &str, width: usize, indent: &str) -> String {
    let mut out = String::new();
    let mut line = String::new();
    let max = width.saturating_sub(indent.len()).max(20);
    for word in text.split_whitespace() {
        if !line.is_empty() && style::width(&line) + 1 + style::width(word) > max {
            out.push_str(indent);
            out.push_str(&line);
            out.push('\n');
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push_str(indent);
        out.push_str(&line);
        out.push('\n');
    }
    out
}

pub fn status_dot(status: PortStatus) -> String {
    match status {
        PortStatus::Free => paint("●", S::BoldGreen),
        PortStatus::Busy => paint("●", S::BoldRed),
        PortStatus::Reserved => paint("●", S::BoldYellow),
    }
}

pub fn explanation(ex: &Explanation, show_plan: bool) -> String {
    let w = style::term_width().min(110);
    let mut out = String::new();
    let head = wrap(&ex.headline, w, "  ");
    out.push_str(&format!(
        "{} {}\n",
        status_dot(ex.status),
        bold(head.trim())
    ));
    if !ex.details.is_empty() {
        out.push('\n');
        for d in &ex.details {
            out.push_str(&dim(wrap(d, w, "  ").trim_end()));
            out.push('\n');
        }
    }
    out.push('\n');
    out.push_str(&format!(
        "{} {}\n",
        style::arrow(),
        paint(wrap(&ex.recommendation, w, "  ").trim(), S::Cyan)
    ));
    if show_plan {
        if let Some(plan) = &ex.plan {
            out.push('\n');
            out.push_str(&plan_text(plan));
        }
    }
    if !ex.commands.is_empty() {
        out.push('\n');
        out.push_str(&format!("{}\n", bold("Commands")));
        for c in &ex.commands {
            out.push_str(&format!("  {} {c}\n", dim("$")));
        }
    }
    out
}

pub fn plan_text(plan: &ActionPlan) -> String {
    let mut out = String::new();
    if let Some(b) = &plan.blocked {
        let (label, st) = match b.kind {
            BlockKind::NeedsElevation => ("Needs elevation", S::BoldYellow),
            BlockKind::NothingToStop => ("Nothing to stop", S::Dim),
            _ => ("Blocked", S::BoldRed),
        };
        out.push_str(&format!(
            "{} {}\n",
            paint(format!("{label}:"), st),
            b.message
        ));
        for w in &plan.warnings {
            out.push_str(&format!("  {} {}\n", style::warn_mark(), dim(w)));
        }
        return out;
    }
    let risk = match plan.risk {
        Risk::Low => paint("low risk", S::Green),
        Risk::Medium => paint("medium risk", S::Yellow),
        Risk::High => paint("HIGH RISK", S::BoldRed),
    };
    out.push_str(&format!(
        "{} {}\n",
        bold(format!("Plan for {}", plan.target)),
        dim(format!("({risk})"))
    ));
    if plan.target.starts_with("cluster ") {
        out.push_str(&format!("  {}\n", dim(&plan.summary)));
    }
    for (i, s) in plan.steps.iter().enumerate() {
        out.push_str(&format!("  {}. {}\n", i + 1, s.describe()));
    }
    for w in &plan.warnings {
        out.push_str(&format!(
            "  {} {}\n",
            style::warn_mark(),
            paint(w, S::Yellow)
        ));
    }
    out
}

pub fn report_line(r: &StopReport) -> String {
    let n = r.signalled.len();
    if r.success {
        let what = if n > 0 {
            format!(
                " (stopped {n} process{}{})",
                if n == 1 { "" } else { "es" },
                if r.escalated { ", needed SIGKILL" } else { "" }
            )
        } else {
            String::new()
        };
        format!(
            "{} {} {}",
            style::ok_mark(),
            bold(format!("{} is free{what}", r.target)),
            dim(format!("in {} ms", r.elapsed_ms))
        )
    } else {
        format!(
            "{} {} {}",
            style::err_mark(),
            paint(format!("Could not free {}", r.target), S::BoldRed),
            r.error.clone().unwrap_or_default()
        )
    }
}

pub fn inspect_entry(e: &PortEntry) -> String {
    let mut rows: Vec<(String, String)> = vec![
        ("Port".into(), format!("{} / {}", e.port, e.protocol)),
        ("State".into(), e.state.to_string()),
        ("Addresses".into(), e.addresses.join(", ")),
        (
            "Exposure".into(),
            match e.exposure {
                Exposure::Loopback => "this machine only".into(),
                Exposure::AllInterfaces => {
                    paint("all interfaces (reachable from the network)", S::Yellow)
                }
                Exposure::Specific => "specific interface".into(),
            },
        ),
    ];
    if let Some(p) = &e.process {
        rows.push((
            "PID".into(),
            if e.pids.len() > 1 {
                format!("{} (also {:?})", p.pid, e.pids)
            } else {
                p.pid.to_string()
            },
        ));
        rows.push(("Process".into(), p.name.clone()));
        if let Some(ppid) = p.ppid {
            rows.push(("Parent PID".into(), ppid.to_string()));
        }
        rows.push(("User".into(), e.user.clone().unwrap_or_else(|| "?".into())));
        rows.push(("Command".into(), p.command()));
        if let Some(exe) = &p.exe {
            rows.push(("Executable".into(), exe.display().to_string()));
        }
        if let Some(cwd) = &p.cwd {
            rows.push(("Working dir".into(), tilde(cwd)));
        }
        rows.push((
            "Uptime".into(),
            human_duration(now_secs().saturating_sub(p.start_time)),
        ));
        if p.memory_bytes > 0 {
            rows.push(("Memory".into(), human_bytes(p.memory_bytes)));
        }
    } else if e.container.is_none() {
        rows.push(("Owner".into(), dim("not visible (another user / root)")));
    }
    if let Some(pr) = &e.project {
        rows.push((
            "Project".into(),
            format!(
                "{} ({}, {}){}",
                pr.name,
                tilde(&pr.root),
                pr.kind,
                pr.git_branch
                    .as_ref()
                    .map(|b| format!(" · branch {b}"))
                    .unwrap_or_default()
            ),
        ));
    }
    if let Some(f) = &e.framework {
        rows.push(("Framework".into(), format!("{} ({:?})", f.name, f.category)));
    }
    if let Some(c) = &e.container {
        rows.push((
            "Container".into(),
            format!("{} · {} · {}", c.name, c.image, &c.id[..c.id.len().min(12)]),
        ));
        rows.push(("Runtime".into(), c.runtime.clone()));
        if let (Some(p), Some(s)) = (&c.compose_project, &c.compose_service) {
            rows.push(("Compose".into(), format!("{p} / {s}")));
        }
        rows.push(("Container port".into(), c.private_port.to_string()));
    }
    rows.push((
        "Dev server".into(),
        if e.is_dev { "yes" } else { "no" }.into(),
    ));
    rows.push((
        "Protected".into(),
        if e.protected {
            paint("yes", S::Yellow)
        } else {
            "no".into()
        },
    ));
    let kw = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    rows.iter()
        .map(|(k, v)| format!("  {}  {v}\n", dim(format!("{k:>kw$}"))))
        .collect()
}

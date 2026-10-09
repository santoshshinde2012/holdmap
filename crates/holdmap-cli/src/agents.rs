//! `holdmap agents`: the agents, tools and apps running here and their footprint: folders,
//! access, ports and the services they talk to. With `--stop-ports`, stop the unprotected
//! tools and apps they started (never the agent's own protected listeners).

use crate::style::{self, paint, S};
use anyhow::{bail, Context, Result};
use holdmap_core::agents::{
    AccessLevel, Agent, AgentsReport, Evidence, FolderSource, LinkKind, PortRole,
};
use holdmap_core::engine::tilde;
use holdmap_core::util::{count, human_bytes};
use holdmap_core::{execute, Engine, Exposure, ScanOptions, StopOptions, Target};

#[derive(clap::Args, Debug, Default)]
pub struct AgentsArgs {
    /// Only agents matching this (product such as `claude` or `cursor`, name, or PID).
    pub agent: Option<String>,
    /// List every process of each agent, with its command line (secrets hidden).
    #[arg(short, long)]
    pub wide: bool,
    /// Stop the unprotected ports each matching agent started (dev servers and services).
    #[arg(long)]
    pub stop_ports: bool,
    /// Show the stop plan only; don't send any signal.
    #[arg(long, requires = "stop_ports")]
    pub dry_run: bool,
    /// Don't ask for confirmation before stopping.
    #[arg(long, requires = "stop_ports")]
    pub yes: bool,
    /// Machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
}

/// Whether `a` matches the user's filter.
pub fn matches(a: &Agent, q: &str) -> bool {
    let q = q.trim().to_ascii_lowercase();
    q.is_empty()
        || a.pid.to_string() == q
        || a.product.contains(&q)
        || a.name.to_ascii_lowercase().contains(&q)
}

pub fn run(a: &AgentsArgs, docker: bool) -> Result<u8> {
    let e = Engine::new(&ScanOptions {
        all_states: false,
        docker,
    })
    .context("failed to scan processes and sockets")?;
    let mut report = e.agents();
    if let Some(q) = &a.agent {
        report.agents.retain(|x| matches(x, q));
    }
    if a.stop_ports {
        return stop_ports(&e, &report, a);
    }
    if a.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render(&report, a.wide));
    }
    Ok(if report.agents.is_empty() {
        crate::exit::BUSY
    } else {
        crate::exit::OK
    })
}

/// Stop every unprotected port the matching agents started.
fn stop_ports(e: &Engine, report: &AgentsReport, a: &AgentsArgs) -> Result<u8> {
    if a.json && !a.yes && !a.dry_run {
        bail!("--json needs --yes (or --dry-run): JSON mode never prompts");
    }
    let ports: Vec<(String, u16)> = report
        .agents
        .iter()
        .flat_map(|ag| ag.stoppable_ports().map(move |p| (ag.name.clone(), p.port)))
        .collect();
    if ports.is_empty() {
        if a.json {
            println!("{}", serde_json::json!({"stopped": [], "plans": []}));
        } else {
            println!(
                "{}",
                style::dim(
                    "No stoppable ports: matching agents haven't started a freeable service."
                )
            );
        }
        return Ok(crate::exit::BUSY);
    }
    let opts = StopOptions::default();
    let mut plans = Vec::new();
    for (_, port) in &ports {
        plans.push(e.plan(&Target::Port(*port), &opts));
    }
    if a.dry_run {
        if a.json {
            crate::commands::print_json(&plans)?;
        } else {
            for (i, plan) in plans.iter().enumerate() {
                let (name, port) = &ports[i];
                println!("{} :{port} ({name})", paint("plan", S::Cyan));
                print!("{}", crate::render::plan_text(plan));
            }
            println!("{}", style::dim("Dry run: nothing was changed."));
        }
        return Ok(crate::exit::OK);
    }
    let blocked = plans.iter().filter(|p| p.blocked.is_some()).count();
    let actionable: Vec<_> = plans
        .iter()
        .zip(ports.iter())
        .filter(|(p, _)| p.blocked.is_none())
        .collect();
    if actionable.is_empty() {
        bail!(
            "every stoppable port is blocked by the safety policy ({} protected)",
            blocked
        );
    }
    if !a.yes {
        let list = actionable
            .iter()
            .map(|(_, (name, port))| format!(":{port} ({name})"))
            .collect::<Vec<_>>()
            .join(", ");
        if !crate::commands::confirm(&format!("Stop {list}?"))? {
            bail!("cancelled");
        }
    }
    let mut reports = Vec::new();
    let mut code = crate::exit::OK;
    for (plan, (name, port)) in &actionable {
        if !a.json {
            println!("{} :{port} ({name})", paint("stop", S::Cyan));
        }
        let report = execute(plan, &mut |l| {
            if !a.json {
                println!("  {} {}", style::dim("·"), style::dim(l));
            }
        });
        if !report.freed {
            code = crate::exit::BUSY;
        }
        if !report.success {
            code = code.max(crate::exit::ERROR);
        }
        reports.push(report);
    }
    if a.json {
        crate::commands::print_json(&reports)?;
    }
    Ok(code)
}

fn level_mark(l: AccessLevel) -> String {
    match l {
        AccessLevel::Restricted => paint("▾", S::Green),
        AccessLevel::Standard => paint("·", S::Dim),
        AccessLevel::Elevated => paint("!", S::BoldYellow),
        AccessLevel::Unknown => paint("?", S::Dim),
    }
}

/// The human report.
pub fn render(r: &AgentsReport, wide: bool) -> String {
    let mut out = String::new();
    if r.agents.is_empty() {
        out.push_str(&style::dim("No agents or developer tools running.\n"));
        return out;
    }
    let label = |s: &str| style::dim(format!("  {s:<10}"));
    let pad = " ".repeat(12);
    for (i, a) in r.agents.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let who = match (&a.access.user, a.access.mine, a.access.root) {
            (_, _, true) => paint("root", S::BoldYellow),
            (Some(u), true, _) => format!("you ({u})"),
            (Some(u), false, _) => u.clone(),
            (None, _, _) => "unknown user".into(),
        };
        let procs = a.processes.len() + a.more_processes;
        let mut meta = vec![format!("pid {}", a.pid), a.kind.label().to_string()];
        if a.vendor != "unknown" {
            meta.push(a.vendor.clone());
        }
        meta.extend([
            who,
            count(procs, "process", "processes"),
            human_bytes(a.memory_bytes),
        ]);
        if a.cpu_percent > 0.05 {
            meta.push(format!("{:.1}% CPU", a.cpu_percent));
        }
        let stoppable = a.stoppable_ports().count();
        if stoppable > 0 {
            meta.push(count(stoppable, "stoppable port", "stoppable ports"));
        }
        out.push_str(&format!(
            "{}  {}\n",
            paint(&a.name, S::Bold),
            style::dim(meta.join(" · "))
        ));
        if let Some(p) = &a.parent {
            if let Some(host) = r.agents.iter().find(|x| &x.id == p) {
                out.push_str(&format!(
                    "{}{}\n",
                    label("from"),
                    style::dim(format!("started from {} (pid {})", host.name, host.pid))
                ));
            }
        }

        let mut first = true;
        let mut row = |out: &mut String, title: &str, text: String| {
            let head = if first { label(title) } else { pad.clone() };
            first = false;
            out.push_str(&format!("{head}{text}\n"));
        };
        if a.folders.is_empty() {
            row(&mut out, "folders", style::dim("none visible"));
        }
        for f in &a.folders {
            let mut tags = vec![match f.source {
                FolderSource::Agent => "working dir",
                FolderSource::Child => "child cwd",
                FolderSource::Recent => "recent",
            }
            .to_string()];
            if f.evidence == Evidence::Inferred {
                tags.push("inferred".into());
            }
            if let Some(area) = &f.privacy_area {
                tags.push(format!("in {area}"));
            }
            let branch = f
                .project
                .as_ref()
                .and_then(|p| p.git_branch.as_ref())
                .map(|b| format!(" {}", paint(format!("({b})"), S::Magenta)))
                .unwrap_or_default();
            row(
                &mut out,
                "folders",
                format!(
                    "{}{branch}  {}  {}",
                    paint(&f.label, S::Bold),
                    tilde(&f.path),
                    style::dim(tags.join(" · "))
                ),
            );
        }

        let mut first = true;
        let mut row = |out: &mut String, title: &str, text: String| {
            let head = if first { label(title) } else { pad.clone() };
            first = false;
            out.push_str(&format!("{head}{text}\n"));
        };
        for p in &a.ports {
            let role = match p.role {
                PortRole::Agent => "agent",
                PortRole::DevServer => "dev server",
                PortRole::Service => "service",
            };
            let exposure = match p.exposure {
                Exposure::AllInterfaces => paint("all interfaces", S::Yellow),
                Exposure::Loopback => style::dim("localhost"),
                Exposure::Specific => style::dim("specific address"),
            };
            row(
                &mut out,
                "ports",
                format!(
                    "{} {}  {}  {}",
                    paint(format!(":{}", p.port), S::BoldCyan),
                    p.label,
                    exposure,
                    style::dim(role)
                ),
            );
        }

        let mut first = true;
        let mut row = |out: &mut String, title: &str, text: String| {
            let head = if first { label(title) } else { pad.clone() };
            first = false;
            out.push_str(&format!("{head}{text}\n"));
        };
        for l in &a.links {
            let what = match l.kind {
                LinkKind::Local => format!(
                    "{} {}",
                    l.label,
                    style::dim(format!(":{} on this machine", l.port))
                ),
                LinkKind::Remote => format!(
                    "{}{}",
                    l.address,
                    l.service
                        .as_ref()
                        .map(|s| style::dim(format!(" {s}")))
                        .unwrap_or_default()
                ),
            };
            row(
                &mut out,
                "talks to",
                format!("{what}  {}", style::dim(format!("×{}", l.connections))),
            );
        }
        if a.more_links > 0 {
            row(
                &mut out,
                "talks to",
                style::dim(format!("+{} more", a.more_links)),
            );
        }

        let mut first = true;
        for f in &a.access.facts {
            let head = if first { label("access") } else { pad.clone() };
            first = false;
            let summary = if f.evidence == Evidence::Unknown {
                style::dim(&f.summary)
            } else {
                f.summary.clone()
            };
            out.push_str(&format!("{head}{} {summary}\n", level_mark(f.level)));
        }

        if wide {
            let mut first = true;
            for p in &a.processes {
                let head = if first {
                    label("processes")
                } else {
                    pad.clone()
                };
                first = false;
                out.push_str(&format!(
                    "{head}{} {}  {}\n",
                    paint(format!("{:>7}", p.pid), S::Cyan),
                    p.name,
                    style::dim(style::one_line(&p.command, 90))
                ));
            }
            if a.more_processes > 0 {
                out.push_str(&format!(
                    "{pad}{}\n",
                    style::dim(format!("+{} more", a.more_processes))
                ));
            }
        }
    }
    out.push('\n');
    out.push_str(&style::dim(format!(
        "{}  {}\n",
        paint("▾", S::Green),
        "narrower than your account · ! wider · ? unknown"
    )));
    for l in &r.limits {
        out.push_str(&style::dim(format!("note: {l}\n")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_core::agents::{AccessFact, AccessTopic, AgentAccess, AgentFolder, AgentKind};

    fn agent(pid: u32, product: &str, name: &str) -> Agent {
        Agent {
            id: format!("agent:{pid}"),
            product: product.into(),
            name: name.into(),
            vendor: "Vendor".into(),
            kind: AgentKind::Cli,
            pid,
            process_name: product.into(),
            command: product.into(),
            started_at: 0,
            parent: None,
            memory_bytes: 1 << 20,
            cpu_percent: 0.0,
            processes: vec![],
            more_processes: 0,
            folders: vec![AgentFolder {
                path: "/srv/shop-web".into(),
                label: "shop-web".into(),
                project: None,
                source: FolderSource::Agent,
                evidence: Evidence::Observed,
                pids: vec![pid],
                privacy_area: None,
                note: None,
            }],
            ports: vec![],
            links: vec![],
            more_links: 0,
            access: AgentAccess {
                user: Some("dev".into()),
                uid: Some(1000),
                root: false,
                mine: true,
                facts: vec![AccessFact {
                    topic: AccessTopic::Sandbox,
                    level: AccessLevel::Unknown,
                    summary: "No sandbox seen right now.".into(),
                    evidence: Evidence::Unknown,
                }],
            },
        }
    }

    #[test]
    fn filters_by_product_name_or_pid() {
        let a = agent(42, "claude-code", "Claude Code");
        assert!(matches(&a, "claude"));
        assert!(matches(&a, "Claude Code"));
        assert!(matches(&a, "42"));
        assert!(!matches(&a, "cursor"));
    }

    #[test]
    fn renders_each_agent_with_its_footprint() {
        style::init(style::ColorChoice::Never);
        let mut child = agent(7, "codex", "Codex CLI");
        child.parent = Some("agent:42".into());
        let r = AgentsReport {
            agents: vec![agent(42, "claude-code", "Claude Code"), child],
            platform: "linux".into(),
            taken_at_ms: 0,
            limits: vec!["Chats are never read.".into()],
        };
        let text = render(&r, false);
        assert!(text.contains("Claude Code  pid 42 · terminal agent · Vendor · you (dev)"));
        assert!(text.contains("folders   shop-web  /srv/shop-web  working dir"));
        assert!(text.contains("started from Claude Code (pid 42)"));
        assert!(text.contains("? No sandbox seen right now."));
        assert!(text.contains("note: Chats are never read."));
        let none = render(&AgentsReport::default(), false);
        assert!(none.contains("No agents or developer tools running."));
    }

    #[test]
    fn meta_mentions_stoppable_ports_and_cpu() {
        style::init(style::ColorChoice::Never);
        let mut a = agent(42, "claude-code", "Claude Code");
        a.cpu_percent = 3.4;
        a.ports.push(holdmap_core::agents::AgentPort {
            entry_id: "tcp:3001".into(),
            port: 3001,
            protocol: holdmap_core::Protocol::Tcp,
            exposure: Exposure::Loopback,
            pid: Some(42),
            process: Some("node".into()),
            label: "shop-api".into(),
            role: PortRole::DevServer,
            project: Some("shop-api".into()),
            framework: None,
        });
        let text = render(
            &AgentsReport {
                agents: vec![a],
                platform: "linux".into(),
                taken_at_ms: 0,
                limits: vec![],
            },
            false,
        );
        assert!(text.contains("3.4% CPU"));
        assert!(text.contains("1 stoppable port"));
    }
}

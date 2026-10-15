//! `holdmap agents`: the agents, tools and apps running here and their footprint: folders,
//! access, ports and the services they talk to. With `--stop-ports`, stop the unprotected
//! tools and apps they started (never the agent's own protected listeners).

use crate::style::{self, paint, S};
use anyhow::{bail, Context, Result};
use holdmap_core::agents::{AccessLevel, AgentsReport, Evidence, FolderSource, LinkKind, PortRole};
use holdmap_core::engine::tilde;
use holdmap_core::util::{count, human_bytes, printable};
use holdmap_core::{execute, Engine, Exposure, ScanOptions};

#[derive(clap::Args, Debug, Default)]
pub struct AgentsArgs {
    /// Only agents matching a product, name, vendor, folder, tool name, or exact agent/child PID.
    pub agent: Option<String>,
    /// Show full agent details, including processes and redacted command lines.
    #[arg(short, long)]
    pub wide: bool,
    /// Inspect bounded local MCP configuration and installed skill locations; no tool calls.
    #[arg(long, conflicts_with_all = ["flow", "stop_ports"])]
    pub capabilities: bool,
    /// Show observed process, listener and TCP relationships; no call or payload inference.
    #[arg(long, conflicts_with_all = ["stop_ports", "wide"])]
    pub flow: bool,
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

pub fn run(a: &AgentsArgs, docker: bool) -> Result<u8> {
    let e = Engine::new(&ScanOptions {
        all_states: false,
        docker,
    })
    .context("failed to scan processes and sockets")?;
    let mut report = if a.capabilities {
        e.agents_with_capabilities()
    } else {
        e.agents()
    };
    if let Some(q) = &a.agent {
        report.agents.retain(|x| x.matches(q));
    }
    if a.stop_ports {
        return stop_ports(&e, &report, a, docker);
    }
    if a.flow {
        let flow = holdmap_core::agents::flow::flow(&e.scan, &report);
        if a.json {
            println!("{}", serde_json::to_string_pretty(&flow)?);
        } else {
            print!("{}", crate::agent_flow::render(&flow));
        }
        return Ok(if flow.nodes.is_empty() {
            crate::exit::BUSY
        } else {
            crate::exit::OK
        });
    }
    if a.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!(
            "{}",
            render(&report, a.wide || a.capabilities, a.agent.as_deref())
        );
    }
    Ok(if report.agents.is_empty() {
        crate::exit::BUSY
    } else {
        crate::exit::OK
    })
}

/// Stop every unprotected port the matching agents started.
fn stop_ports(e: &Engine, report: &AgentsReport, a: &AgentsArgs, docker: bool) -> Result<u8> {
    if a.json && !a.yes && !a.dry_run {
        bail!("--json needs --yes (or --dry-run): JSON mode never prompts");
    }
    if report
        .agents
        .iter()
        .all(|ag| ag.stoppable_ports().next().is_none())
    {
        if a.json {
            println!("{}", serde_json::json!({"stopped": [], "plans": []}));
        } else {
            println!(
                "{}",
                style::dim(
                    "No stoppable services. Agent sessions and their own listeners stay running."
                )
            );
        }
        return Ok(crate::exit::BUSY);
    }
    let review = crate::agent_actions::ReviewedAgentStops::preview(e, &report.agents)?;
    if a.dry_run {
        if a.json {
            crate::commands::print_json(
                &review.plans().iter().map(|p| &p.plan).collect::<Vec<_>>(),
            )?;
        } else {
            for reviewed in review.plans() {
                println!(
                    "{} {} ({})",
                    paint("plan", S::Cyan),
                    reviewed.target(),
                    holdmap_core::util::printable(&reviewed.name)
                );
                print!("{}", crate::render::plan_text(&reviewed.plan));
            }
            println!("{}", style::dim("Dry run: nothing was changed."));
        }
        return Ok(if review.plans().iter().any(|p| p.plan.is_blocked()) {
            crate::exit::BLOCKED
        } else {
            crate::exit::OK
        });
    }
    if !a.json {
        for reviewed in review.plans() {
            println!(
                "{} {} ({})",
                paint("plan", S::Cyan),
                reviewed.target(),
                holdmap_core::util::printable(&reviewed.name)
            );
            print!("{}", crate::render::plan_text(&reviewed.plan));
        }
        println!("{}", style::dim("Agent sessions will stay running."));
    }
    if review.plans().iter().any(|p| p.plan.is_blocked()) {
        if a.json {
            crate::commands::print_json(
                &review.plans().iter().map(|p| &p.plan).collect::<Vec<_>>(),
            )?;
        }
        return Ok(crate::exit::BLOCKED);
    }
    if !a.yes {
        let list = review
            .plans()
            .iter()
            .map(|p| {
                format!(
                    "{} ({})",
                    p.target(),
                    holdmap_core::util::printable(&p.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        if !crate::commands::confirm(&format!("Stop {list}?"))? {
            bail!("cancelled");
        }
    }
    let fresh = Engine::new(&ScanOptions {
        all_states: false,
        docker,
    })
    .context("failed to refresh service ownership; nothing was stopped")?;
    let outcome = review.execute_with(
        &fresh,
        |reviewed| {
            if let Err(error) = crate::project_policy::check_before_execute(docker, &reviewed.plan)
            {
                return holdmap_core::StopReport {
                    target: reviewed.target(),
                    error: Some(error.to_string()),
                    ..Default::default()
                };
            }
            let mut report = execute(&reviewed.plan, &mut |l| {
                if !a.json {
                    println!("  {} {}", style::dim("·"), style::dim(l));
                }
            });
            report.target = reviewed.target();
            if !a.json {
                println!("{}", crate::render::report_line(&report));
            }
            report
        },
        |plan, report| {
            crate::state::record_checked(&fresh, plan, report).map_err(|e| e.to_string())
        },
    )?;
    for error in &outcome.history_errors {
        eprintln!("{}", style::terminal_text(error));
    }
    if a.json {
        crate::commands::print_json(&outcome.reports)?;
        if outcome.remaining > 0 {
            eprintln!(
                "{} remaining service(s) were not attempted after a failed stop.",
                outcome.remaining
            );
        }
    } else if outcome.remaining > 0 {
        println!("{}", style::dim(format!("{} remaining service(s) were not attempted. Refresh and review before trying again.", outcome.remaining)));
    }
    Ok(if outcome.succeeded() {
        crate::exit::OK
    } else if !outcome.history_errors.is_empty() || outcome.reports.iter().any(|r| !r.success) {
        crate::exit::ERROR
    } else {
        crate::exit::BUSY
    })
}

fn level_mark(l: AccessLevel) -> String {
    match l {
        AccessLevel::Restricted => paint("▾", S::Green),
        AccessLevel::Standard => paint("·", S::Dim),
        AccessLevel::Elevated => paint("!", S::BoldYellow),
        AccessLevel::Unknown => paint("?", S::Dim),
    }
}

fn render_overview(r: &AgentsReport) -> String {
    let mut out = format!(
        "{}\n\n",
        paint(
            count(
                r.agents.len(),
                "running agent or tool",
                "running agents and tools"
            ),
            S::Bold
        )
    );
    for a in &r.agents {
        let mut meta = vec![
            format!("pid {}", a.pid),
            a.kind.label().to_string(),
            human_bytes(a.memory_bytes),
            format!("{:.1}% CPU", a.cpu_percent),
        ];
        if a.access.root {
            meta.push("root account".into());
        } else if !a.access.mine {
            meta.push(a.access.user.as_deref().unwrap_or("unknown account").into());
        }
        out.push_str(&format!(
            "{}  {}\n",
            paint(printable(&a.name), S::Bold),
            style::dim(printable(&meta.join(" · ")))
        ));
        let folder = a
            .folders
            .iter()
            .find(|f| f.source != FolderSource::Recent)
            .or_else(|| a.folders.first());
        let folder = folder.map_or_else(
            || "working folder not visible".into(),
            |f| {
                let source = match (f.source, f.evidence) {
                    (FolderSource::Recent, _) => "recent project; not current activity",
                    (_, Evidence::Inferred) => "inferred folder",
                    (FolderSource::Agent, _) => "working dir",
                    _ => "child working dir",
                };
                let more = a.folders.len().saturating_sub(1) + a.more_folders;
                format!(
                    "{} ({source}){}",
                    style::one_line(&printable(&f.label), 44),
                    if more > 0 {
                        format!(" +{more} folders")
                    } else {
                        String::new()
                    }
                )
            },
        );
        let mut targets: Vec<_> = a.stoppable_ports().map(|p| (p.port, p.protocol)).collect();
        targets.sort();
        targets.dedup();
        let services = if targets.is_empty() {
            "no stoppable services".to_string()
        } else {
            let listed = targets
                .iter()
                .take(4)
                .map(|(port, protocol)| format!("{port}/{protocol}"))
                .collect::<Vec<_>>()
                .join(", ");
            let more = targets.len().saturating_sub(4);
            format!(
                "{}: {listed}{}",
                count(targets.len(), "service", "services"),
                if more > 0 {
                    format!(" +{more} more")
                } else {
                    String::new()
                }
            )
        };
        let tools = a.tools.len() + a.more_tools;
        out.push_str(&format!(
            "  {folder}\n  {services} · {}",
            count(tools, "child tool", "child tools")
        ));
        let exposed = a
            .ports
            .iter()
            .filter(|p| p.exposure == Exposure::AllInterfaces)
            .count();
        if exposed > 0 {
            out.push_str(&format!(
                " · {}",
                paint(
                    count(
                        exposed,
                        "network-exposed listener",
                        "network-exposed listeners"
                    ),
                    S::Yellow
                )
            ));
        }
        out.push_str("\n\n");
    }
    out.push_str(&style::dim("Details: holdmap agents <PID> · Processes: holdmap agents --wide\nStop services: holdmap agents <PID> --stop-ports --dry-run\nAgent sessions stay running; chats and tool invocations are never read.\n"));
    for limit in &r.limits {
        out.push_str(&style::dim(format!("note: {}\n", printable(limit))));
    }
    style::terminal_text(&out)
}

/// The human report.
pub fn render(r: &AgentsReport, wide: bool, query: Option<&str>) -> String {
    let mut out = String::new();
    if r.agents.is_empty() {
        let message = match query.filter(|q| !q.trim().is_empty()) {
            Some(q) => format!("No agents or developer tools match {q:?}.\n"),
            None => "No agents or developer tools running.\n".to_string(),
        };
        out.push_str(&style::dim(message));
        for limit in &r.limits {
            out.push_str(&style::dim(format!("note: {}\n", printable(limit))));
        }
        out.push_str(&style::dim(
            if query.is_some_and(|q| !q.trim().is_empty()) {
                "Run holdmap agents to show every running agent and tool.\n"
            } else {
                "Start an agent or editor, then run holdmap agents again.\n"
            },
        ));
        return style::terminal_text(&out);
    }
    if !wide && query.is_none_or(|q| q.trim().is_empty()) {
        return render_overview(r);
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
            format!("{:.1}% CPU", a.cpu_percent),
        ]);
        let stoppable = a.stoppable_ports().count();
        if stoppable > 0 {
            meta.push(count(stoppable, "stoppable port", "stoppable ports"));
        }
        out.push_str(&format!(
            "{}  {}\n",
            paint(printable(&a.name), S::Bold),
            style::dim(printable(&meta.join(" · ")))
        ));
        if let Some(p) = &a.parent {
            if let Some(host) = r.agents.iter().find(|x| &x.id == p) {
                out.push_str(&format!(
                    "{}{}\n",
                    label("from"),
                    style::dim(format!(
                        "started from {} (pid {})",
                        printable(&host.name),
                        host.pid
                    ))
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
                .map(|b| format!(" {}", paint(format!("({})", printable(b)), S::Magenta)))
                .unwrap_or_default();
            row(
                &mut out,
                "folders",
                format!(
                    "{}{branch}  {}  {}",
                    paint(printable(&f.label), S::Bold),
                    holdmap_core::util::printable(&tilde(&f.path)),
                    style::dim(printable(&tags.join(" · ")))
                ),
            );
        }
        if a.more_folders > 0 {
            row(
                &mut out,
                "folders",
                style::dim(format!(
                    "+{} not listed",
                    count(a.more_folders, "folder", "folders")
                )),
            );
        }

        let mut first = true;
        let mut row = |out: &mut String, text: String| {
            let head = if first { label("tools") } else { pad.clone() };
            first = false;
            out.push_str(&format!("{head}{text}\n"));
        };
        for tool in &a.tools {
            let evidence = match tool.evidence {
                Evidence::Observed => "observed",
                Evidence::Inferred => "inferred",
                Evidence::Unknown => "unknown",
            };
            let ports = if tool.ports.is_empty() {
                "no listening ports".to_string()
            } else {
                tool.ports
                    .iter()
                    .map(|p| format!(":{p}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            row(
                &mut out,
                format!(
                    "{} (pid {})  {}",
                    paint(printable(&tool.name), S::Bold),
                    tool.pid,
                    style::dim(format!("{} · {evidence} · {ports}", tool.kind.label())),
                ),
            );
        }
        if a.more_tools > 0 {
            row(
                &mut out,
                style::dim(format!(
                    "+{} not listed",
                    count(a.more_tools, "tool", "tools")
                )),
            );
        }

        if a.capabilities.inspected {
            out.push_str(&format!(
                "{}{}\n",
                label("MCP/skills"),
                style::dim("Configuration / installation only; loading and use are unknown.")
            ));
            for server in &a.capabilities.mcp_servers {
                let enabled = match server.enabled {
                    Some(true) => "enabled in configuration",
                    Some(false) => "disabled in configuration",
                    None => "enablement unknown",
                };
                out.push_str(&format!(
                    "{}{} · {} · {} · {}\n{}{}\n",
                    pad,
                    paint(printable(&server.name), S::Bold),
                    server.transport.label(),
                    server.scope.label(),
                    enabled,
                    pad,
                    style::dim(printable(&tilde(&server.source_path))),
                ));
            }
            for skill in &a.capabilities.skills {
                out.push_str(&format!(
                    "{}{} · skill folder · {}\n{}{}\n",
                    pad,
                    paint(printable(&skill.name), S::Bold),
                    skill.scope.label(),
                    pad,
                    style::dim(printable(&tilde(&skill.source_path))),
                ));
                if let Some(name) = skill
                    .declared_name
                    .as_ref()
                    .filter(|name| *name != &skill.name)
                {
                    out.push_str(&format!("{}Declared name: {}\n", pad, printable(name)));
                }
                if let Some(description) = &skill.description {
                    out.push_str(&format!(
                        "{}Purpose metadata: {}\n",
                        pad,
                        printable(description)
                    ));
                }
            }
            if a.capabilities.mcp_servers.is_empty() && a.capabilities.skills.is_empty() {
                out.push_str(&format!(
                    "{}{}\n",
                    pad,
                    style::dim(
                        "No MCP definitions or skill files found in the inspected locations."
                    )
                ));
            }
            if a.capabilities.more_mcp_servers > 0 || a.capabilities.more_skills > 0 {
                out.push_str(&format!(
                    "{}{}\n",
                    pad,
                    style::dim(format!(
                        "Not listed: {} MCP definitions, {} skills.",
                        a.capabilities.more_mcp_servers, a.capabilities.more_skills
                    ))
                ));
            }
            for limit in &a.capabilities.limits {
                out.push_str(&format!("{}{}\n", pad, style::dim(printable(limit))));
            }
        } else {
            out.push_str(&format!(
                "{}{}\n",
                label("MCP/skills"),
                style::dim("Not inspected. Use holdmap agents --capabilities for local inventory.")
            ));
            for limit in a
                .capabilities
                .limits
                .iter()
                .filter(|limit| !limit.contains("were not inspected"))
            {
                out.push_str(&format!("{}{}\n", pad, style::dim(printable(limit))));
            }
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
                    paint(format!("{}/{}", p.port, p.protocol), S::BoldCyan),
                    printable(&p.label),
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
                    printable(&l.label),
                    style::dim(format!(":{} on this machine", l.port))
                ),
                LinkKind::Remote => format!(
                    "{}{}",
                    printable(&l.address),
                    l.service
                        .as_ref()
                        .map(|s| style::dim(format!(" {}", printable(s))))
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
                style::dim(format!(
                    "+{} not listed",
                    count(a.more_links, "connection target", "connection targets")
                )),
            );
        }

        let mut first = true;
        for f in &a.access.facts {
            let head = if first { label("access") } else { pad.clone() };
            first = false;
            let summary = if f.evidence == Evidence::Unknown {
                style::dim(printable(&f.summary))
            } else {
                printable(&f.summary).into_owned()
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
                    printable(&p.name),
                    style::dim(style::one_line(
                        &printable(&holdmap_core::redact::line(&p.command)),
                        90
                    ))
                ));
            }
            if a.more_processes > 0 {
                out.push_str(&format!(
                    "{pad}{}\n",
                    style::dim(format!(
                        "+{} not listed",
                        count(a.more_processes, "process", "processes")
                    ))
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
        out.push_str(&style::dim(format!("note: {}\n", printable(l))));
    }
    out.push_str(&style::dim("Stop services: holdmap agents <PID> --stop-ports --dry-run\nAgent sessions stay running.\n"));
    style::terminal_text(&out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_core::agents::{
        AccessFact, AccessTopic, Agent, AgentAccess, AgentFolder, AgentKind, AgentTool, ToolKind,
    };

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
            process_ids: vec![pid],
            tools: vec![],
            more_tools: 0,
            capabilities: Default::default(),
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
            more_folders: 0,
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
        let text = render(&r, false, Some("claude-code"));
        assert!(text.contains("Claude Code  pid 42 · terminal agent · Vendor · you (dev)"));
        assert!(text.contains("folders   shop-web  /srv/shop-web  working dir"));
        assert!(text.contains("started from Claude Code (pid 42)"));
        assert!(text.contains("? No sandbox seen right now."));
        assert!(text.contains("note: Chats are never read."));
        let none = render(&AgentsReport::default(), false, None);
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
            None,
        );
        assert!(text.contains("3.4% CPU"));
        assert!(text.contains("1 service: 3001/TCP"));
    }

    #[test]
    fn renders_stdio_tools_resources_and_omitted_items() {
        style::init(style::ColorChoice::Never);
        let mut a = agent(42, "claude-code", "Claude Code");
        a.cpu_percent = 25.5;
        a.tools = vec![
            AgentTool {
                pid: 43,
                ppid: Some(42),
                name: "filesystem-mcp".into(),
                kind: ToolKind::McpServer,
                command: "node filesystem-mcp --token=[redacted]".into(),
                cwd: Some("/srv/shop-web".into()),
                evidence: Evidence::Inferred,
                ports: vec![],
                memory_bytes: 1024,
                cpu_percent: 1.0,
            },
            AgentTool {
                pid: 44,
                ppid: Some(42),
                name: "Vite".into(),
                kind: ToolKind::DevServer,
                command: "node vite".into(),
                cwd: Some("/srv/shop-web".into()),
                evidence: Evidence::Observed,
                ports: vec![5173],
                memory_bytes: 1024,
                cpu_percent: 1.0,
            },
        ];
        a.more_tools = 2;
        a.more_folders = 3;
        a.more_processes = 4;
        a.more_links = 5;
        let r = AgentsReport {
            agents: vec![a],
            ..AgentsReport::default()
        };
        let text = render(&r, true, None);
        assert!(text.contains("25.5% CPU"));
        assert!(
            text.contains("filesystem-mcp (pid 43)  MCP server · inferred · no listening ports")
        );
        assert!(text.contains("Vite (pid 44)  dev server · observed · :5173"));
        for omission in [
            "+2 tools not listed",
            "+3 folders not listed",
            "+4 processes not listed",
            "+5 connection targets not listed",
        ] {
            assert!(text.contains(omission), "missing {omission}: {text}");
        }
    }

    #[test]
    fn empty_reports_preserve_collection_limits_and_filter_context() {
        style::init(style::ColorChoice::Never);
        let report = AgentsReport {
            limits: vec!["The process table could not be collected.".into()],
            ..AgentsReport::default()
        };
        let text = render(&report, false, None);
        assert!(text.starts_with("No agents or developer tools running."));
        assert!(text.contains("note: The process table could not be collected."));
        let filtered = render(&report, false, Some("shop-web"));
        assert!(filtered.starts_with("No agents or developer tools match \"shop-web\"."));
        assert!(filtered.contains("note: The process table could not be collected."));
    }

    #[test]
    fn overview_is_compact_and_keeps_recent_projects_distinct_from_activity() {
        style::init(style::ColorChoice::Never);
        let mut a = agent(42, "claude-code", "Claude Code");
        a.folders[0].source = FolderSource::Recent;
        a.command = "claude --token=[redacted]".into();
        let report = AgentsReport {
            agents: vec![a],
            limits: vec!["Process and socket metadata only.".into()],
            ..AgentsReport::default()
        };
        let compact = render(&report, false, None);
        assert!(compact.contains("1 running agent or tool"));
        assert!(compact.contains("shop-web (recent project; not current activity)"));
        assert!(compact.contains("no stoppable services · 0 child tools"));
        assert!(compact.contains("Details: holdmap agents <PID>"));
        assert!(compact.contains("note: Process and socket metadata only."));
        assert!(!compact.contains("--token"));
        assert!(!compact.contains("No sandbox seen right now."));
        assert!(render(&report, false, Some("42")).contains("No sandbox seen right now."));
    }

    #[test]
    fn capability_inspection_is_explicit_and_cannot_be_combined_with_stop_or_flow() {
        use clap::Parser;
        #[derive(Parser)]
        struct Args {
            #[command(flatten)]
            agents: AgentsArgs,
        }
        assert!(
            !Args::try_parse_from(["holdmap"])
                .unwrap()
                .agents
                .capabilities
        );
        assert!(
            Args::try_parse_from(["holdmap", "--capabilities", "--json"])
                .unwrap()
                .agents
                .capabilities
        );
        for option in ["--stop-ports", "--flow"] {
            assert!(Args::try_parse_from(["holdmap", "--capabilities", option]).is_err());
        }
    }

    #[test]
    fn capability_details_keep_configured_disabled_and_installed_distinct_from_use() {
        style::init(style::ColorChoice::Never);
        let mut a = agent(42, "claude-code", "Claude Code");
        a.capabilities = serde_json::from_value(serde_json::json!({
            "inspected": true,
            "mcp_servers": [{"id":"design","name":"design","scope":"project","source_path":"/srv/shop-web/.mcp.json","project_path":"/srv/shop-web","transport":"http","enabled":false,"evidence":"observed"}],
            "skills": [{"id":"review","name":"review","declared_name":"review","description":"Review UI behavior.","scope":"user","source_path":"/home/dev/.claude/skills/review/SKILL.md","project_path":null,"evidence":"observed"}],
            "more_mcp_servers": 1, "more_skills": 2, "limits":["Coverage is partial."]
        })).unwrap();
        let text = render(
            &AgentsReport {
                agents: vec![a],
                ..Default::default()
            },
            true,
            None,
        );
        assert!(text.contains("disabled in configuration"));
        assert!(text.contains("review · skill folder · user"));
        assert!(text.contains("Purpose metadata: Review UI behavior."));
        assert!(text.contains("/srv/shop-web/.mcp.json"));
        assert!(
            text.contains("Loading and use are unknown.")
                || text.contains("loading and use are unknown.")
        );
        assert!(text.contains("Not listed: 1 MCP definitions, 2 skills."));
        assert!(text.contains("Coverage is partial."));
    }

    #[test]
    fn human_reports_neutralize_terminal_control_sequences() {
        style::init(style::ColorChoice::Never);
        let mut a = agent(
            42,
            "claude-code",
            "Claude Code\x1b]52;c;injected\x07\nforged row",
        );
        a.folders[0].label = "project\x1b[2J".into();
        let report = AgentsReport {
            agents: vec![a],
            ..AgentsReport::default()
        };
        for (wide, query) in [(false, None), (true, None), (false, Some("42"))] {
            let text = render(&report, wide, query);
            assert!(!text.contains('\x1b'));
            assert!(!text.contains('\x07'));
            assert!(!text.contains("\nforged row"));
        }
    }
}

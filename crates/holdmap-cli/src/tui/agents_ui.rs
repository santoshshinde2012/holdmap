//! Agents presentation: a stable list, a pinned summary and scrollable observed details.

use super::agents_view::{access_attention, safe_text, working_folders, wrap_lines};
use super::app::App;
use super::{theme, ui};
use holdmap_core::agents::*;
use holdmap_core::engine::tilde;
use holdmap_core::util::human_bytes;
use holdmap_core::Exposure;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Table,
};
use ratatui::Frame;

fn panel(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::muted())
        .title(title)
}

pub(super) fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let indices = app.visible_agent_indices();
    let total = app.agents.as_ref().map_or(0, |report| report.agents.len());
    let count = if indices.len() == total {
        format!("{total} agents")
    } else {
        format!("{}/{} agents", indices.len(), total)
    };
    let status = if app.busy.is_some() {
        Span::styled("stopping services…", theme::title())
    } else if app.error.is_some() {
        Span::styled("scan failed · r retry", Style::new().fg(Color::Yellow))
    } else if app.paused {
        Span::styled("paused · p resume", theme::muted())
    } else if app.scanning {
        Span::styled("refreshing…", theme::muted())
    } else if app.inspect_capabilities {
        Span::styled("inspecting config · every 2s", theme::muted())
    } else {
        Span::styled("live · every 2s", theme::muted())
    };
    let [left, right] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(if area.width >= 70 { 32 } else { 0 }),
    ])
    .areas(area);
    let mut spans = vec![Span::styled(" ◉ holdmap ", theme::title())];
    if area.width >= 90 {
        spans.extend([
            Span::styled(" Ports  Graph ", theme::muted()),
            Span::styled(
                " Agents ",
                Style::new().fg(Color::Black).bg(ui::ACCENT).bold(),
            ),
            Span::styled("  Tab ", theme::muted()),
        ]);
    } else {
        spans.push(Span::styled("· Agents ", theme::heading()));
    }
    spans.push(Span::styled(count, theme::muted()));
    f.render_widget(Paragraph::new(Line::from(spans)), left);
    f.render_widget(
        Paragraph::new(Line::from(status)).alignment(Alignment::Right),
        right,
    );
}

pub(super) fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let line = if app.agent_searching {
        Line::from(vec![
            Span::styled(" / ", theme::title()),
            Span::raw(safe_text(&app.agent_query)),
            Span::styled("▏", theme::title()),
            Span::styled("  Enter keep · Esc clear", theme::muted()),
        ])
    } else if !app.agent_query.is_empty() {
        Line::from(vec![
            Span::styled(" search ", theme::muted()),
            Span::styled(safe_text(&app.agent_query), theme::title()),
            Span::styled("  / edit · Esc clear", theme::muted()),
        ])
    } else {
        Line::from(Span::styled(
            format!(
                " / Search name, PID, folder, tool or inventory · c {} · PgUp/PgDn scroll",
                if app.inspect_capabilities {
                    "stop inspection"
                } else {
                    "inspect MCP/skills"
                },
            ),
            theme::muted(),
        ))
    };
    f.render_widget(Paragraph::new(line), area);
}

pub(super) fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    if let Some(busy) = &app.busy {
        f.render_widget(
            Paragraph::new(Span::styled(
                format!(
                    " {} · Another stop is unavailable until this finishes.",
                    safe_text(busy)
                ),
                theme::title(),
            )),
            area,
        );
        return;
    }
    let actions: &[(&str, &str)] = if app.agent_searching {
        &[("Enter", "keep"), ("Esc", "clear"), ("↑↓", "agent")]
    } else if area.width >= 110 {
        &[
            ("↑↓", "agent"),
            ("/", "search"),
            ("PgUp/Dn", "details"),
            (
                "c",
                if app.inspect_capabilities {
                    "inventory on"
                } else {
                    "MCP/skills"
                },
            ),
            ("x", "review stop"),
            ("Tab", "ports"),
            ("r", "refresh"),
            ("p", "pause"),
            ("?", "help"),
            ("q", "quit"),
        ]
    } else if area.width >= 70 {
        &[
            ("↑↓", "agent"),
            ("/", "search"),
            ("c", "MCP/skills"),
            ("PgDn", "more"),
            ("x", "stop"),
            ("?", "help"),
            ("q", "quit"),
        ]
    } else {
        &[
            ("/", "search"),
            ("c", "MCP/skills"),
            ("PgDn", "more"),
            ("x", "stop"),
            ("?", "help"),
            ("q", "quit"),
        ]
    };
    let mut spans = Vec::new();
    let mut used = 0;
    for (key, label) in actions {
        let key = ui::key(key);
        let label = Span::styled(format!(" {label} "), theme::muted());
        let width = key.width() + label.width();
        if used + width > usize::from(area.width) {
            break;
        }
        used += width;
        spans.extend([key, label]);
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub(super) fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let body = if let Some(error) = &app.error {
        let [warning, body] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(area);
        let prefix = if app.agents.is_some() {
            "Scan failed. Showing the last successful snapshot."
        } else {
            "Agent visibility unavailable."
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(prefix, Style::new().fg(Color::Yellow))),
                Line::from(Span::styled(
                    format!("{} · r to retry", safe_text(error)),
                    theme::muted(),
                )),
            ]),
            warning,
        );
        body
    } else {
        area
    };
    let [list, detail] = if body.width >= 100 {
        Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)]).areas(body)
    } else {
        Layout::vertical([
            Constraint::Length((body.height / 3).clamp(4, 9)),
            Constraint::Min(0),
        ])
        .areas(body)
    };
    app.table_area = list;
    app.agent_detail_area = detail;
    draw_list(f, app, list);
    draw_details(f, app, detail);
}

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    let indices = app.visible_agent_indices();
    let total = app.agents.as_ref().map_or(0, |report| report.agents.len());
    let block = panel(Line::from(vec![
        Span::styled(" Agents ", theme::heading()),
        Span::styled(format!("{} / {total} ", indices.len()), theme::muted()),
    ]));
    let Some(report) = &app.agents else {
        f.render_widget(
            Paragraph::new(if app.error.is_some() {
                "Press r to retry the scan."
            } else {
                "Looking for supported agents…"
            })
            .style(theme::muted())
            .block(block),
            area,
        );
        return;
    };
    if indices.is_empty() {
        let lines = if report.agents.is_empty() {
            vec![
                Line::from(Span::styled(
                    "No supported agents detected",
                    theme::heading(),
                )),
                Line::from(Span::styled(
                    "Start an agent, then press r to refresh.",
                    theme::muted(),
                )),
            ]
        } else {
            vec![
                Line::from(Span::styled(
                    "No agents match this search",
                    theme::heading(),
                )),
                Line::from(Span::styled(
                    "Esc clears the search; / edits it.",
                    theme::muted(),
                )),
            ]
        };
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(Paragraph::new(wrap_lines(lines, inner.width)), inner);
        app.agent_state.select(None);
        return;
    }
    let selected = indices.iter().position(|index| *index == app.agent_idx);
    app.agent_state.select(selected);
    let compact = area.width < 54;
    let rows = indices.iter().map(|index| {
        let agent = &report.agents[*index];
        let attention = if access_attention(agent) { " !" } else { "" };
        let name = Line::from(vec![
            Span::styled(safe_text(&agent.name), theme::heading()),
            Span::styled(attention, Style::new().fg(Color::Yellow)),
        ]);
        let subtitle = Line::from(Span::styled(
            format!(
                "PID {} · {} ports · {} tools",
                agent.pid,
                agent.ports.len(),
                agent.tools.len() + agent.more_tools
            ),
            theme::muted(),
        ));
        let mut cells = vec![Cell::from(vec![name, subtitle])];
        if !compact {
            cells.push(Cell::from(vec![
                Line::raw(human_bytes(agent.memory_bytes)),
                Line::from(Span::styled(
                    format!("{:.1}% CPU", agent.cpu_percent),
                    theme::muted(),
                )),
            ]));
        }
        Row::new(cells).height(2)
    });
    let widths = if compact {
        vec![Constraint::Min(0)]
    } else {
        vec![Constraint::Min(0), Constraint::Length(10)]
    };
    let table = Table::new(rows, widths)
        .block(block)
        .row_highlight_style(Style::new().bg(Color::Indexed(237)))
        .highlight_symbol(Span::styled("› ", theme::title()));
    f.render_stateful_widget(table, area, &mut app.agent_state);
}

fn draw_details(f: &mut Frame, app: &mut App, area: Rect) {
    let block = panel(Span::styled(" Agent details ", theme::heading()));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(agent) = app.selected_agent() else {
        let mut lines = vec![
            Line::from(Span::styled("Visibility", theme::heading())),
            Line::raw("Inspect running processes, services, tools and access signals."),
            Line::raw(
                "Search names, PIDs, folders or tool names. Commands and prompts are not searched.",
            ),
        ];
        if let Some(report) = &app.agents {
            lines.extend(limit_lines(report));
        }
        let lines = wrap_lines(lines, inner.width.saturating_sub(1));
        app.agent_detail_max_scroll = lines
            .len()
            .saturating_sub(usize::from(inner.height))
            .min(usize::from(u16::MAX)) as u16;
        app.agent_scroll = app.agent_scroll.min(app.agent_detail_max_scroll);
        f.render_widget(Paragraph::new(lines).scroll((app.agent_scroll, 0)), inner);
        return;
    };
    let stoppable = agent.stoppable_ports().count();
    let folders = working_folders(agent);
    let tools = agent.tools.len() + agent.more_tools;
    let mut summary = vec![
        Line::from(Span::styled(safe_text(&agent.name), theme::heading())),
        Line::from(Span::styled(
            format!(
                "{} · {} · PID {}",
                agent.kind.label(),
                safe_text(&agent.vendor),
                agent.pid
            ),
            theme::muted(),
        )),
        Line::from(Span::styled(
            format!(
                "{} · {:.1}% CPU · {folders} working folder{} · {tools} tool{}",
                human_bytes(agent.memory_bytes),
                agent.cpu_percent,
                if folders == 1 { "" } else { "s" },
                if tools == 1 { "" } else { "s" }
            ),
            theme::muted(),
        )),
        Line::from(if stoppable > 0 {
            Span::styled(
                format!(
                    "x Review stop for {stoppable} service{} · Agent stays running",
                    if stoppable == 1 { "" } else { "s" }
                ),
                theme::title(),
            )
        } else {
            Span::styled(
                "Agent stays running · No stoppable services",
                theme::muted(),
            )
        }),
    ];
    if access_attention(agent) {
        summary.push(Line::from(Span::styled(
            "! Elevated access signals · inspect Access below",
            Style::new().fg(Color::Yellow),
        )));
    }
    let summary = wrap_lines(summary, inner.width);
    let summary_height = (summary.len() as u16).min(inner.height.saturating_sub(2));
    let [summary_area, content] =
        Layout::vertical([Constraint::Length(summary_height), Constraint::Min(0)]).areas(inner);
    f.render_widget(Paragraph::new(summary), summary_area);
    let report = app
        .agents
        .as_ref()
        .expect("a selected agent belongs to a report");
    let lines = wrap_lines(
        detail_lines(agent, report, app.inspect_capabilities),
        content.width.saturating_sub(1),
    );
    let max_scroll = lines
        .len()
        .saturating_sub(usize::from(content.height))
        .min(usize::from(u16::MAX)) as u16;
    app.agent_detail_max_scroll = max_scroll;
    app.agent_scroll = app.agent_scroll.min(max_scroll);
    let line_count = lines.len();
    f.render_widget(Paragraph::new(lines).scroll((app.agent_scroll, 0)), content);
    if max_scroll > 0 && content.width > 1 {
        let mut state = ScrollbarState::new(line_count)
            .position(usize::from(app.agent_scroll))
            .viewport_content_length(usize::from(content.height));
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            content,
            &mut state,
        );
    }
}

fn heading(text: &'static str) -> Line<'static> {
    Line::from(Span::styled(text, theme::heading()))
}

fn muted(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(text.into(), theme::muted()))
}

fn evidence(value: Evidence) -> &'static str {
    match value {
        Evidence::Observed => "observed",
        Evidence::Inferred => "inferred",
        Evidence::Unknown => "unknown",
    }
}

fn detail_lines(
    agent: &Agent,
    report: &AgentsReport,
    inspection_enabled: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(parent) = agent
        .parent
        .as_ref()
        .and_then(|id| report.agents.iter().find(|other| &other.id == id))
    {
        lines.push(muted(format!(
            "Started from {} · PID {}",
            safe_text(&parent.name),
            parent.pid
        )));
    }
    lines.push(Line::raw(""));
    lines.push(heading("Services & ports"));
    if agent.ports.is_empty() {
        lines.push(muted("No listening ports observed."));
    }
    for port in &agent.ports {
        let role = match port.role {
            PortRole::Agent => "agent session · kept running",
            PortRole::DevServer => "dev service · review stop",
            PortRole::Service => "service · review stop",
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {}/{}", port.port, port.protocol), theme::port()),
            Span::raw(format!(" · {role}")),
        ]));
        lines.push(Line::raw(format!(
            "    {}",
            safe_text(
                port.project
                    .as_deref()
                    .or(port.framework.as_deref())
                    .unwrap_or(&port.label)
            )
        )));
        if port.exposure == Exposure::AllInterfaces {
            lines.push(Line::from(Span::styled(
                "    ! Listening on all interfaces",
                Style::new().fg(Color::Yellow),
            )));
        }
    }
    lines.push(Line::raw(""));
    lines.push(heading("Running tools"));
    lines.push(muted(
        "Processes under this agent; this does not prove tool calls or outputs.",
    ));
    if agent.tools.is_empty() {
        lines.push(muted("No child tools observed."));
    }
    for tool in &agent.tools {
        lines.push(Line::raw(format!(
            "  {} · {} · PID {} · {}",
            safe_text(&tool.name),
            tool.kind.label(),
            tool.pid,
            evidence(tool.evidence)
        )));
        if !tool.ports.is_empty() {
            lines.push(muted(format!(
                "    Listening ports: {}",
                tool.ports
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        } else if tool.kind == ToolKind::McpServer {
            lines.push(muted("    No listening port; may communicate over stdio."));
        }
        if let Some(cwd) = &tool.cwd {
            lines.push(muted(format!("    {}", safe_text(&cwd.to_string_lossy()))));
        }
    }
    if agent.more_tools > 0 {
        lines.push(muted(format!(
            "+ {} tools omitted from the scan report",
            agent.more_tools
        )));
    }
    lines.push(Line::raw(""));
    lines.extend(capability_lines(agent, inspection_enabled));
    lines.push(Line::raw(""));
    lines.push(heading("Working folders & recent projects"));
    if agent.folders.is_empty() {
        lines.push(muted("No working folders visible."));
    }
    for folder in &agent.folders {
        let source = match folder.source {
            FolderSource::Agent => "agent working directory",
            FolderSource::Child => "child working directory",
            FolderSource::Recent => "recent project · not proof of current work",
        };
        lines.push(Line::raw(format!(
            "  {} · {}",
            safe_text(&folder.label),
            source
        )));
        lines.push(muted(format!(
            "    {} · {}",
            safe_text(&folder.path.to_string_lossy()),
            evidence(folder.evidence)
        )));
        if let Some(note) = &folder.note {
            lines.push(muted(format!("    {}", safe_text(note))));
        }
    }
    if agent.more_folders > 0 {
        lines.push(muted(format!(
            "+ {} working folders omitted from the scan report",
            agent.more_folders
        )));
    }
    lines.push(Line::raw(""));
    lines.push(heading("Live connections"));
    if agent.links.is_empty() {
        lines.push(muted("No live TCP connections observed."));
    }
    for link in &agent.links {
        lines.push(Line::raw(format!(
            "  {} · {} connection{} · {}",
            safe_text(&link.label),
            link.connections,
            if link.connections == 1 { "" } else { "s" },
            match link.kind {
                LinkKind::Local => "local service",
                LinkKind::Remote => "remote endpoint",
            }
        )));
        lines.push(muted(format!(
            "    {}{}",
            safe_text(&link.address),
            link.service
                .as_ref()
                .map(|service| format!(" · {} (port hint)", safe_text(service)))
                .unwrap_or_default()
        )));
    }
    if agent.more_links > 0 {
        lines.push(muted(format!(
            "+ {} connection groups omitted from the scan report",
            agent.more_links
        )));
    }
    lines.push(Line::raw(""));
    lines.push(heading("Access · observed / inferred / unknown"));
    if agent.access.facts.is_empty() {
        lines.push(muted("Access policy unknown; no signals were available."));
    }
    for fact in &agent.access.facts {
        let topic = match fact.topic {
            AccessTopic::User => "Account",
            AccessTopic::Sandbox => "Sandbox",
            AccessTopic::Approvals => "Approvals",
            AccessTopic::Network => "Network",
            AccessTopic::Privacy => "Privacy",
        };
        let style = if fact.level == AccessLevel::Elevated {
            Style::new().fg(Color::Yellow)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!(
                "  {topic} · {} · {}",
                safe_text(&fact.summary),
                evidence(fact.evidence)
            ),
            style,
        )));
    }
    if agent.more_processes > 0 {
        lines.push(muted(format!(
            "{} further processes are represented in aggregate usage",
            agent.more_processes
        )));
    }
    lines.extend(limit_lines(report));
    lines
}

fn capability_lines(agent: &Agent, inspection_enabled: bool) -> Vec<Line<'static>> {
    let mut lines = vec![heading("MCP & skills inventory")];
    if agent.capabilities.inspected {
        lines.push(muted(
            "Configuration / installation only; loading and use are unknown.",
        ));
        for server in &agent.capabilities.mcp_servers {
            let enabled = match server.enabled {
                Some(true) => "enabled in configuration",
                Some(false) => "disabled in configuration",
                None => "enablement unknown",
            };
            lines.push(Line::raw(format!(
                "  {} · {} · {} · {}",
                safe_text(&server.name),
                server.transport.label(),
                server.scope.label(),
                enabled
            )));
            lines.push(muted(format!(
                "    {}",
                safe_text(&tilde(&server.source_path))
            )));
            if let Some(project) = &server.project_path {
                lines.push(muted(format!(
                    "    Project: {}",
                    safe_text(&tilde(project))
                )));
            }
        }
        for skill in &agent.capabilities.skills {
            lines.push(Line::raw(format!(
                "  {} · skill folder · {}",
                safe_text(&skill.name),
                skill.scope.label()
            )));
            lines.push(muted(format!(
                "    {}",
                safe_text(&tilde(&skill.source_path))
            )));
            if let Some(project) = &skill.project_path {
                lines.push(muted(format!(
                    "    Project: {}",
                    safe_text(&tilde(project))
                )));
            }
            if let Some(name) = skill
                .declared_name
                .as_ref()
                .filter(|name| *name != &skill.name)
            {
                lines.push(muted(format!("    Declared name: {}", safe_text(name))));
            }
            if let Some(description) = &skill.description {
                lines.push(muted(format!(
                    "    Purpose metadata: {}",
                    safe_text(description)
                )));
            }
        }
        if agent.capabilities.mcp_servers.is_empty() && agent.capabilities.skills.is_empty() {
            lines.push(muted(
                "No MCP definitions or skill files listed; missing entries do not prove absence.",
            ));
        }
        if agent.capabilities.more_mcp_servers > 0 || agent.capabilities.more_skills > 0 {
            lines.push(muted(format!(
                "Not listed: {} MCP definitions, {} skills.",
                agent.capabilities.more_mcp_servers, agent.capabilities.more_skills
            )));
        }
        for limit in &agent.capabilities.limits {
            lines.push(muted(format!("• {}", safe_text(limit))));
        }
    } else {
        lines.push(muted(if inspection_enabled {
            "Inspection enabled; no inventory reported for this agent. See coverage below."
        } else {
            "Not inspected. Press c to inspect local MCP definitions and skill locations."
        }));
        lines.push(muted(
            "Process scans do not establish configuration, skill loading or use.",
        ));
        for limit in agent
            .capabilities
            .limits
            .iter()
            .filter(|limit| !limit.contains("were not inspected"))
        {
            lines.push(muted(format!("• {}", safe_text(limit))));
        }
    }
    lines
}

fn limit_lines(report: &AgentsReport) -> Vec<Line<'static>> {
    let mut lines = vec![Line::raw(""), heading("Visibility limits")];
    lines.push(muted(
        "Prompts, chats, tool invocations and tool outputs are not read.",
    ));
    for limit in &report.limits {
        lines.push(muted(format!("• {}", safe_text(limit))));
    }
    lines
}

pub(super) fn draw_help(f: &mut Frame) {
    let area = f.area();
    let width = area.width.min(84);
    let height = area.height.min(24);
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    f.render_widget(Clear, popup);
    let block = panel(Span::styled(" Agents · keys ", theme::title()));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let rows = [
        ("↑ ↓ / j k", "Select agent"),
        ("Home / End", "First / last matching agent"),
        ("/", "Search name, PID, folder or tool"),
        ("Enter", "Keep search"),
        ("Esc", "Clear search; then return to Ports"),
        ("PgUp / PgDn", "Scroll agent details"),
        (
            "mouse wheel",
            "Scroll the list or details under the pointer",
        ),
        ("c", "Toggle MCP/skill inspection (Agents only)"),
        ("x", "Review a stop for this agent's services and ports"),
        ("y", "Confirm a reviewed stop; Enter / n cancels"),
        ("Tab / v", "Switch Ports → Graph → Agents"),
        ("r / F5", "Refresh now (auto every 2 seconds)"),
        ("p / space", "Pause / resume refresh"),
        ("q", "Quit Holdmap"),
        ("? / F1", "Show this help"),
    ];
    let mut lines = rows
        .into_iter()
        .map(|(key, label)| {
            Line::from(vec![
                Span::styled(format!("{key:>13}  "), theme::key()),
                Span::raw(label),
            ])
        })
        .collect::<Vec<_>>();
    lines.extend([
        Line::raw(""),
        muted("Stopping services keeps the agent session and its own ports running."),
        muted("MCP labels are inferred from processes, not a history of tool calls."),
        muted("c inspects configuration files; loading and use stay unknown."),
        muted("Leaving Agents clears inventory and stops inspection."),
        muted("Any key closes help."),
    ]);
    f.render_widget(Paragraph::new(wrap_lines(lines, inner.width)), inner);
}

#[cfg(test)]
mod tests {
    use super::super::agents_view::test_agent;
    use super::super::app::Tab;
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| super::super::ui::draw(frame, app))
            .unwrap();
        terminal
    }

    fn text(terminal: &Terminal<TestBackend>, area: Rect) -> String {
        let buffer = terminal.backend().buffer();
        (area.y..area.bottom())
            .map(|y| {
                (area.x..area.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn app(report: AgentsReport) -> App {
        let mut app = App::empty(false);
        app.tab = Tab::Agents;
        app.agents = Some(report);
        app
    }

    #[test]
    fn selected_agent_stays_visible_when_the_list_is_long() {
        let mut app = app(AgentsReport {
            agents: (1..=20)
                .map(|pid| test_agent(pid, &format!("Agent {pid:02}")))
                .collect(),
            ..Default::default()
        });
        app.agent_idx = 19;
        let terminal = render(&mut app, 80, 24);
        assert_eq!(app.agent_state.selected(), Some(19));
        assert!(app.agent_state.offset() > 0);
        assert!(text(&terminal, app.table_area).contains("Agent 20"));
        assert!(!text(&terminal, app.table_area).contains("Agent 01"));
    }

    #[test]
    fn narrow_details_scroll_to_all_visibility_notes_without_losing_identity() {
        let mut app = app(AgentsReport {
            agents: vec![test_agent(42, "Codex")],
            limits: vec!["Unique final visibility note".into()],
            ..Default::default()
        });
        render(&mut app, 80, 24);
        assert!(app.agent_detail_max_scroll > 0);
        app.agent_scroll = u16::MAX;
        let terminal = render(&mut app, 80, 24);
        let details = text(&terminal, app.agent_detail_area);
        assert_eq!(app.agent_scroll, app.agent_detail_max_scroll);
        assert!(details.contains("Codex"));
        assert!(details.contains("Unique final visibility note"));
        assert!(!details.contains("super-secret"));
    }

    #[test]
    fn tools_include_stdio_mcp_evidence_and_omitted_counts_without_commands() {
        let mut agent = test_agent(42, "Codex\x1b[31m\u{202e}");
        agent.tools = vec![AgentTool {
            pid: 43,
            ppid: Some(42),
            name: "example-mcp".into(),
            kind: ToolKind::McpServer,
            command: "secret tool prompt".into(),
            cwd: None,
            evidence: Evidence::Inferred,
            ports: Vec::new(),
            memory_bytes: 0,
            cpu_percent: 0.0,
        }];
        agent.more_tools = 2;
        agent.folders = vec![AgentFolder {
            path: "/workspace/project\nsecond-line".into(),
            label: "project".into(),
            project: None,
            source: FolderSource::Recent,
            evidence: Evidence::Observed,
            pids: Vec::new(),
            privacy_area: None,
            note: None,
        }];
        let report = AgentsReport {
            agents: vec![agent],
            ..Default::default()
        };
        let lines = detail_lines(&report.agents[0], &report, false)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(lines.contains("example-mcp · MCP server · PID 43 · inferred"));
        assert!(lines.contains("may communicate over stdio"));
        assert!(lines.contains("2 tools omitted"));
        assert!(lines.contains("recent project · not proof of current work"));
        assert!(!lines.contains("secret tool prompt"));
        assert!(!lines.contains("project\nsecond-line"));
        let mut app = app(report);
        let terminal = render(&mut app, 120, 30);
        let visible = text(&terminal, Rect::new(0, 0, 120, 30));
        assert!(visible.contains("example-mcp"));
        assert!(visible.contains("inferred"));
        assert!(!visible.contains('\x1b'));
        assert!(!visible.contains('\u{202e}'));
        assert!(!visible.contains("super-secret"));
    }

    #[test]
    fn stale_scan_and_filtered_empty_states_are_explicit() {
        let mut app = app(AgentsReport {
            agents: vec![test_agent(42, "Codex")],
            ..Default::default()
        });
        app.error = Some("permission denied\x1b".into());
        let terminal = render(&mut app, 80, 24);
        assert!(text(&terminal, Rect::new(0, 0, 80, 24)).contains("last successful snapshot"));
        app.error = None;
        app.agent_query = "nonexistent".into();
        let terminal = render(&mut app, 80, 24);
        let visible = text(&terminal, Rect::new(0, 0, 80, 24));
        assert!(visible.contains("No agents match this search"));
        assert!(visible.contains("Esc clears"));
        assert!(!visible.contains("TCP+UDP"));
        assert!(!visible.contains("sorted by"));
    }

    #[test]
    fn inspected_inventory_stays_separate_from_running_tools() {
        let mut agent = test_agent(42, "Codex");
        agent.tools = vec![AgentTool {
            pid: 99,
            ppid: Some(42),
            name: "docs-mcp".into(),
            kind: ToolKind::McpServer,
            command: "private-command".into(),
            cwd: None,
            evidence: Evidence::Inferred,
            ports: Vec::new(),
            memory_bytes: 0,
            cpu_percent: 0.0,
        }];
        agent.capabilities = AgentCapabilities {
            inspected: true,
            mcp_servers: vec![McpServerCapability {
                id: "mcp-docs".into(),
                name: "docs".into(),
                scope: CapabilityScope::User,
                source_path: "/Users/dev/.codex/config.toml".into(),
                project_path: Some("/fixture/first-project".into()),
                transport: McpTransport::Stdio,
                enabled: Some(true),
                evidence: Evidence::Observed,
                launch: None,
                remote_host: None,
                env_names: Vec::new(),
                header_names: Vec::new(),
                runtime: Default::default(),
            }],
            skills: vec![SkillCapability {
                id: "skill-review".into(),
                name: "review".into(),
                declared_name: Some("Review diffs".into()),
                description: Some("Review local diffs.".into()),
                scope: CapabilityScope::User,
                source_path: "/Users/dev/.agents/skills/review/SKILL.md".into(),
                project_path: Some("/fixture/second-project".into()),
                evidence: Evidence::Observed,
            }],
            more_mcp_servers: 1,
            more_skills: 0,
            limits: vec!["Configuration and skill file presence do not prove loading.".into()],
            ..Default::default()
        };
        let report = AgentsReport {
            agents: vec![agent],
            ..Default::default()
        };
        let lines = detail_lines(&report.agents[0], &report, false)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(lines.contains("Configuration / installation only; loading and use are unknown."));
        assert!(lines.contains("docs · stdio · user · enabled in configuration"));
        assert!(lines.contains("review · skill folder · user"));
        assert!(lines.contains("Declared name: Review diffs"));
        assert!(lines.contains("Purpose metadata: Review local diffs."));
        assert!(lines.contains("Project: /fixture/first-project"));
        assert!(lines.contains("Project: /fixture/second-project"));
        assert!(lines.contains("Not listed: 1 MCP definitions, 0 skills."));
        assert!(lines.contains("docs-mcp · MCP server · PID 99 · inferred"));
        assert!(!lines.contains("private-command"));
        let tools = lines.find("Running tools").unwrap();
        let inventory = lines.find("MCP & skills inventory").unwrap();
        assert!(tools < inventory);
        assert!(!lines.contains("Run holdmap agents --capabilities"));
        let mut app = app(report);
        app.inspect_capabilities = true;
        let terminal = render(&mut app, 120, 36);
        let visible = text(&terminal, Rect::new(0, 0, 120, 36));
        assert!(visible.contains("inspecting config"));
        assert!(visible.contains("docs"));
        assert!(visible.contains("review"));
        assert!(!visible.contains("private-command"));
    }

    #[test]
    fn inspection_controls_and_unknown_inventory_are_honest_at_small_widths() {
        let mut agent = test_agent(42, "Unsupported agent");
        agent.capabilities.limits = vec!["This product's configuration is not supported.".into()];
        let unavailable = capability_lines(&agent, true)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(unavailable.contains("Inspection enabled; no inventory reported"));
        assert!(unavailable.contains("not supported"));
        assert!(!unavailable.contains("Press c to inspect"));
        assert!(!unavailable.contains("No MCP definitions or skill files listed"));
        let not_requested = capability_lines(&agent, false)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(not_requested.contains("Press c to inspect"));
        for width in [40, 80] {
            let mut app = app(AgentsReport {
                agents: vec![agent.clone()],
                ..Default::default()
            });
            app.inspect_capabilities = true;
            let terminal = render(&mut app, width, 24);
            let footer = text(&terminal, Rect::new(0, 23, width, 1))
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                footer.contains("c MCP/skills"),
                "inspection is discoverable at {width} columns: {footer}"
            );
        }
    }

    #[test]
    fn omitted_inventory_and_project_contexts_do_not_imply_absence_or_alias_identity() {
        let mut agent = test_agent(42, "Claude Code");
        agent.capabilities.inspected = true;
        agent.capabilities.more_mcp_servers = 2;
        let lines = capability_lines(&agent, true)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(lines.contains("missing entries do not prove absence"));
        assert!(lines.contains("Not listed: 2 MCP definitions"));
        for project in ["/fixture/project-a", "/fixture/project-b"] {
            agent.capabilities.mcp_servers.push(McpServerCapability {
                id: format!("mcp:{project}:docs"),
                name: "docs".into(),
                scope: CapabilityScope::Project,
                source_path: "/fixture/.claude.json".into(),
                project_path: Some(project.into()),
                transport: McpTransport::Http,
                enabled: None,
                evidence: Evidence::Observed,
                launch: None,
                remote_host: None,
                env_names: Vec::new(),
                header_names: Vec::new(),
                runtime: Default::default(),
            });
        }
        let lines = capability_lines(&agent, true)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            lines
                .matches("docs · HTTP · project · enablement unknown")
                .count(),
            2
        );
        assert_eq!(lines.matches("/fixture/.claude.json").count(), 2);
        assert!(lines.contains("Project: /fixture/project-a"));
        assert!(lines.contains("Project: /fixture/project-b"));
    }

    #[test]
    fn empty_and_tiny_terminals_render_without_panicking() {
        for (width, height) in [(120, 30), (80, 24), (40, 10), (12, 5), (1, 1)] {
            let mut app = app(AgentsReport::default());
            render(&mut app, width, height);
        }
    }
}

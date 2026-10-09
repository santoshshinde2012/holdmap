//! TUI rendering.

use super::app::{App, Modal, ProtoFilter, Tab};
use super::theme;
use crate::render::{address_label, what_label};
use portwise_core::util::{human_bytes, human_duration, now_secs};
use portwise_core::*;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, Padding, Paragraph, Row, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Table, Wrap,
};
use ratatui::Frame;

pub(super) const ACCENT: Color = Color::Cyan;
pub(super) const MUTED: Color = Color::DarkGray;
const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn category_color(e: &PortEntry) -> Color {
    if e.process.is_none() && e.container.is_none() {
        return MUTED;
    }
    match e.framework.as_ref().map(|f| f.category) {
        Some(FrameworkCategory::DevServer | FrameworkCategory::AppServer) => Color::Green,
        Some(FrameworkCategory::Database | FrameworkCategory::Cache | FrameworkCategory::Queue) => {
            Color::Magenta
        }
        Some(FrameworkCategory::Container) => Color::Blue,
        Some(FrameworkCategory::WebServer | FrameworkCategory::Tool) => Color::Cyan,
        Some(FrameworkCategory::System) => MUTED,
        _ if e.is_dev => Color::Green,
        _ => Color::Reset,
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let [header, search, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(f.area());
    draw_header(f, app, header);
    draw_search(f, app, search);
    let wide = body.width >= 120;
    let (list_area, detail_area) = if wide {
        let [l, d] = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
            .areas(body);
        (l, d)
    } else {
        let [l, d] =
            Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(body);
        (l, d)
    };
    if app.tab == Tab::Graph {
        super::graph_ui::draw_graph(f, app, list_area, detail_area);
    } else if app.tab == Tab::Agents {
        draw_agents(f, app, list_area, detail_area);
    } else {
        draw_table(f, app, list_area);
        draw_details(f, app, detail_area);
    }
    draw_footer(f, app, footer);
    match &app.modal {
        Modal::Confirm { .. } => draw_confirm(f, app),
        Modal::Explain { .. } => draw_explain(f, app),
        Modal::Help => draw_help(f),
        Modal::None => {}
    }
}

fn chip(label: &str, on: bool) -> Span<'static> {
    if on {
        Span::styled(
            format!(" {label} "),
            Style::new().fg(Color::Black).bg(ACCENT).bold(),
        )
    } else {
        Span::styled(format!(" {label} "), theme::muted())
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(
            " ◉ portwise ",
            Style::new().fg(Color::Black).bg(ACCENT).bold(),
        ),
        Span::raw(" "),
        chip("Ports", app.tab == Tab::Ports),
        chip("Graph", app.tab == Tab::Graph),
        chip("Agents", app.tab == Tab::Agents),
        Span::styled(" ⇥  ", theme::muted()),
    ];
    let mut stats: Vec<Span> = Vec::new();
    if let Some(e) = &app.engine {
        let s = e.snapshot();
        let dev = app.rows.iter().filter(|r| r.is_dev).count();
        let exposed = app
            .rows
            .iter()
            .filter(|r| r.exposure == Exposure::AllInterfaces)
            .count();
        stats.push(Span::styled(
            portwise_core::util::count(app.rows.len(), "port", "ports"),
            theme::heading(),
        ));
        stats.push(Span::styled(
            format!(" · {dev} dev"),
            Style::new().fg(Color::Green),
        ));
        if exposed > 0 {
            stats.push(Span::styled(
                format!(" · {exposed} exposed"),
                Style::new().fg(Color::Yellow),
            ));
        }
        if s.hidden_sockets > 0 {
            stats.push(Span::styled(
                format!(" · {} hidden", s.hidden_sockets),
                theme::muted(),
            ));
        }
        stats.push(Span::styled(format!(" · {} ms", s.scan_ms), theme::muted()));
    } else {
        stats.push(Span::styled(
            format!("{} scanning…", SPIN[app.spinner % SPIN.len()]),
            theme::muted(),
        ));
    }
    let right = Line::from(vec![
        chip(
            if app.show_all {
                "All sockets"
            } else {
                "Listening"
            },
            true,
        ),
        Span::raw(" "),
        chip(
            match app.proto {
                ProtoFilter::Both => "TCP+UDP",
                ProtoFilter::Tcp => "TCP",
                ProtoFilter::Udp => "UDP",
            },
            app.proto != ProtoFilter::Both,
        ),
        Span::raw(" "),
        chip("Dev", app.dev_only),
        Span::raw(" "),
        chip("Mine", app.mine_only),
        Span::raw(" "),
        Span::styled(
            if app.paused {
                "⏸ paused "
            } else if app.scanning {
                "⟳ "
            } else {
                "  "
            },
            theme::muted(),
        ),
    ])
    .alignment(Alignment::Right);
    // Stats are listed most-important first; on a narrow terminal the tail is dropped whole
    // rather than cut mid-word under the filter chips.
    let fixed: usize = spans.iter().map(Span::width).sum();
    let budget = usize::from(area.width).saturating_sub(fixed + right.width() + 1);
    spans.extend(fit_spans(stats, budget));
    let left = Line::from(spans);
    f.render_widget(Paragraph::new(left), area);
    f.render_widget(Paragraph::new(right), area);
}

fn draw_search(f: &mut Frame, app: &App, area: Rect) {
    let line = if app.searching {
        Line::from(vec![
            Span::styled(" / ", theme::title()),
            Span::raw(app.query.clone()),
            Span::styled(
                "▏",
                Style::new().fg(ACCENT).add_modifier(Modifier::SLOW_BLINK),
            ),
            Span::styled(
                "   Enter to keep · Esc to clear · try :3000, 3000-3999, proto:udp, vite",
                theme::muted(),
            ),
        ])
    } else if !app.query.is_empty() {
        Line::from(vec![
            Span::styled(" filter ", theme::muted()),
            Span::styled(app.query.clone(), theme::title()),
            Span::styled("  (/ to edit, Esc to clear)", theme::muted()),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Press ", theme::muted()),
            Span::styled("/", theme::title()),
            Span::styled(" to search · sorted by ", theme::muted()),
            Span::styled(
                format!(
                    "{}{}",
                    app.sort.label(),
                    if app.reverse { " ↓" } else { " ↑" }
                ),
                Style::new().fg(Color::Reset),
            ),
        ])
    };
    f.render_widget(Paragraph::new(line), area);
}

fn draw_agents(f: &mut Frame, app: &App, list_area: Rect, detail_area: Rect) {
    let n = app.agents.as_ref().map(|r| r.agents.len()).unwrap_or(0);
    let list_block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::muted())
        .title(Line::from(vec![
            Span::styled(" Agents ", theme::heading()),
            Span::styled(format!("{n} "), theme::muted()),
        ]));
    let detail_block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::muted())
        .title(Span::styled(" Footprint ", theme::heading()));

    let Some(report) = &app.agents else {
        f.render_widget(
            Paragraph::new(Span::styled("Looking for agents…", theme::muted())).block(list_block),
            list_area,
        );
        f.render_widget(Paragraph::new("").block(detail_block), detail_area);
        return;
    };
    if report.agents.is_empty() {
        f.render_widget(
            Paragraph::new(Text::from(vec![
                Line::from(Span::styled(
                    "No agents or developer tools running",
                    theme::heading(),
                )),
                Line::from(Span::styled(
                    "Start Claude Code, Cursor, Docker Desktop…",
                    theme::muted(),
                )),
            ]))
            .block(list_block)
            .wrap(Wrap { trim: true }),
            list_area,
        );
        f.render_widget(Paragraph::new("").block(detail_block), detail_area);
        return;
    }

    let rows: Vec<Row> = report
        .agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let stoppable = a.stoppable_ports().count();
            let sel = i == app.agent_idx;
            let style = if sel {
                Style::new().bg(Color::Indexed(237)).fg(ACCENT).bold()
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(a.name.clone()),
                Cell::from(a.kind.label()),
                Cell::from(format!("{}", a.pid)),
                Cell::from(format!(
                    "{}f {}p {}l",
                    a.folders.iter().filter(|f| f.source != portwise_core::agents::FolderSource::Recent).count(),
                    a.ports.len(),
                    a.links.len() + a.more_links
                )),
                Cell::from(if stoppable > 0 {
                    format!("{stoppable} stoppable")
                } else {
                    "—".into()
                }),
            ])
            .style(style)
        })
        .collect();
    let widths = [
        Constraint::Percentage(32),
        Constraint::Percentage(18),
        Constraint::Length(8),
        Constraint::Percentage(22),
        Constraint::Percentage(18),
    ];
    let table = Table::new(rows, widths)
        .header(
            Row::new(["Name", "Kind", "PID", "Footprint", "Control"])
                .style(theme::muted())
                .bottom_margin(0),
        )
        .block(list_block)
        .row_highlight_style(Style::new().fg(ACCENT));
    f.render_widget(table, list_area);

    let Some(a) = app.selected_agent() else {
        f.render_widget(Paragraph::new("").block(detail_block), detail_area);
        return;
    };
    let mut lines = vec![
        Line::from(Span::styled(a.name.clone(), theme::heading())),
        Line::from(Span::styled(
            format!(
                "{} · {} · pid {}",
                a.kind.label(),
                a.vendor,
                a.pid
            ),
            theme::muted(),
        )),
        Line::from(""),
    ];
    for f in &a.access.facts {
        let topic = match f.topic {
            portwise_core::agents::AccessTopic::User => "account",
            portwise_core::agents::AccessTopic::Sandbox => "sandbox",
            portwise_core::agents::AccessTopic::Approvals => "approvals",
            portwise_core::agents::AccessTopic::Network => "network",
            portwise_core::agents::AccessTopic::Privacy => "privacy",
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{topic:<10}"), theme::muted()),
            Span::raw(f.summary.clone()),
        ]));
    }
    if !a.folders.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Folders", theme::heading())));
        for folder in a.folders.iter().take(8) {
            lines.push(Line::from(format!(
                "  {}  {}",
                folder.label,
                folder.path.display()
            )));
        }
    }
    if !a.ports.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Ports", theme::heading())));
        for p in &a.ports {
            let role = match p.role {
                portwise_core::agents::PortRole::DevServer => "dev",
                portwise_core::agents::PortRole::Service => "svc",
                portwise_core::agents::PortRole::Agent => "own",
            };
            lines.push(Line::from(format!(
                "  :{}  {}  {}",
                p.port,
                role,
                p.project.as_deref().or(p.framework.as_deref()).unwrap_or(&p.label)
            )));
        }
    }
    if !a.links.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Talks to", theme::heading())));
        for l in a.links.iter().take(8) {
            lines.push(Line::from(format!(
                "  {}  ×{}",
                l.label, l.connections
            )));
        }
    }
    for note in &report.limits {
        lines.push(Line::from(Span::styled(format!("note: {note}"), theme::muted())));
    }
    f.render_widget(
        Paragraph::new(Text::from(lines))
            .block(detail_block)
            .wrap(Wrap { trim: false }),
        detail_area,
    );
}

fn draw_table(f: &mut Frame, app: &mut App, area: Rect) {
    app.table_area = area;
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::muted())
        .title(Line::from(vec![
            Span::styled(" Ports ", theme::heading()),
            Span::styled(format!("{} ", app.rows.len()), theme::muted()),
        ]));
    if let Some(err) = &app.error {
        let p = Paragraph::new(Text::from(vec![
            Line::from(Span::styled(
                "Couldn't scan sockets",
                Style::new().fg(Color::Red).bold(),
            )),
            Line::from(err.clone()),
            Line::from(Span::styled("Press r to retry.", theme::muted())),
        ]))
        .block(block)
        .wrap(Wrap { trim: true });
        f.render_widget(p, area);
        return;
    }
    if app.engine.is_none() {
        let p = Paragraph::new(Line::from(Span::styled(
            format!(
                "{} Scanning sockets and processes…",
                SPIN[app.spinner % SPIN.len()]
            ),
            theme::muted(),
        )))
        .block(block);
        f.render_widget(p, area);
        return;
    }
    if app.rows.is_empty() {
        let msg = if app.query.is_empty() && !app.dev_only && !app.mine_only {
            "Nothing is listening. Enjoy the quiet."
        } else {
            "No ports match your filters. Esc clears the search; d/m/t toggle filters."
        };
        let p = Paragraph::new(Text::from(vec![
            Line::raw(""),
            Line::from(Span::styled(msg, theme::muted())),
        ]))
        .alignment(Alignment::Center)
        .block(block);
        f.render_widget(p, area);
        return;
    }
    let header = Row::new(
        ["PORT", "PROTO", "ADDRESS", "PID", "PROCESS", "WHAT"]
            .map(|h| Cell::from(h).style(theme::label())),
    );
    let rows: Vec<Row> = app
        .rows
        .iter()
        .map(|e| {
            let marker = if e.exposure == Exposure::AllInterfaces {
                Span::styled("●", Style::new().fg(Color::Yellow))
            } else {
                Span::styled("●", Style::new().fg(category_color(e)))
            };
            Row::new(vec![
                Cell::from(Line::from(vec![
                    marker,
                    Span::raw(" "),
                    Span::styled(format!(":{}", e.port), theme::port()),
                ])),
                Cell::from(Span::styled(e.protocol.to_string(), theme::muted())),
                Cell::from(Span::styled(
                    address_label(e),
                    if e.exposure == Exposure::AllInterfaces {
                        Style::new().fg(Color::Yellow)
                    } else {
                        theme::muted()
                    },
                )),
                Cell::from(e.pid.map(|p| p.to_string()).unwrap_or_else(|| "–".into())),
                Cell::from(Span::styled(
                    e.process
                        .as_ref()
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "–".into()),
                    if e.protected {
                        theme::muted()
                    } else {
                        Style::new()
                    },
                )),
                Cell::from(Span::styled(
                    what_label(e),
                    Style::new().fg(category_color(e)),
                )),
            ])
            .height(1)
        })
        .collect();
    let widths = [
        Constraint::Length(9),
        Constraint::Length(5),
        Constraint::Length(16),
        Constraint::Length(8),
        Constraint::Length(16),
        Constraint::Min(10),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .column_spacing(1)
        .row_highlight_style(
            Style::new()
                .bg(Color::Indexed(236))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(Span::styled("▌", Style::new().fg(ACCENT)));
    f.render_stateful_widget(table, area, &mut app.state);
    let mut sb = ScrollbarState::new(app.rows.len()).position(app.state.selected().unwrap_or(0));
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(theme::muted()),
        area.inner(ratatui::layout::Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut sb,
    );
}

/// Keeps the leading spans that fit in `budget` columns, dropping the rest whole.
pub(super) fn fit_spans(spans: Vec<Span<'static>>, budget: usize) -> Vec<Span<'static>> {
    let mut used = 0;
    spans
        .into_iter()
        .take_while(|s| {
            used += s.width();
            used <= budget
        })
        .collect()
}

/// Section heading with a rule that fills the rest of the panel's inner `width` (never wraps).
pub(super) fn section(name: &str, width: u16) -> Line<'static> {
    let label = format!("{} ", name.to_uppercase());
    let rule = usize::from(width).saturating_sub(label.chars().count());
    Line::from(vec![
        Span::styled(label, theme::port()),
        Span::styled("─".repeat(rule), Style::new().fg(Color::Indexed(238))),
    ])
}

pub(super) fn kv(k: &str, v: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{k:>9}  "), theme::muted()),
        Span::raw(v.into()),
    ])
}

fn plan_lines(plan: &ActionPlan) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    if let Some(b) = &plan.blocked {
        let (label, color) = match b.kind {
            BlockKind::NeedsElevation => ("Needs elevation", Color::Yellow),
            BlockKind::NothingToStop => ("Nothing to stop", MUTED),
            _ => ("Blocked", Color::Red),
        };
        out.push(Line::from(vec![
            Span::styled(format!("{label}: "), Style::new().fg(color).bold()),
            Span::raw(b.message.clone()),
        ]));
        return out;
    }
    for (i, s) in plan.steps.iter().enumerate() {
        out.push(Line::from(vec![
            Span::styled(format!(" {}. ", i + 1), Style::new().fg(ACCENT)),
            Span::raw(s.describe()),
        ]));
    }
    for w in &plan.warnings {
        out.push(Line::from(Span::styled(
            format!(" ! {w}"),
            Style::new().fg(Color::Yellow),
        )));
    }
    out
}

fn draw_details(f: &mut Frame, app: &mut App, area: Rect) {
    let title = match app.selected() {
        Some(e) => format!(
            " :{} · {} ",
            e.port,
            e.framework
                .as_ref()
                .map(|f| f.name.clone())
                .unwrap_or_else(|| e.protocol.to_string())
        ),
        None => " Details ".into(),
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::muted())
        .title(Span::styled(title, theme::title()))
        .padding(Padding::horizontal(1));
    let Some(e) = app.selected().cloned() else {
        f.render_widget(
            Paragraph::new(Span::styled(
                "Select a port to see who owns it and why.",
                theme::muted(),
            ))
            .block(block),
            area,
        );
        return;
    };
    let w = block.inner(area).width;
    let ex = app.explanation();
    let mut lines: Vec<Line> = Vec::new();
    if let Some(ex) = &ex {
        lines.push(Line::from(Span::styled(
            ex.headline.clone(),
            theme::heading(),
        )));
        lines.push(Line::raw(""));
    }
    if let Some(p) = &e.process {
        lines.push(section("Process", w));
        lines.push(kv("Process", format!("{} (PID {})", p.name, p.pid)));
        if let Some(u) = &e.user {
            lines.push(kv("User", u.clone()));
        }
        let cmd = p.command();
        let cmd = if cmd.chars().count() > 140 {
            format!("{}…", cmd.chars().take(139).collect::<String>())
        } else {
            cmd
        };
        lines.push(kv("Command", cmd));
        lines.push(kv(
            "Uptime",
            human_duration(now_secs().saturating_sub(p.start_time)),
        ));
        if p.memory_bytes > 0 {
            lines.push(kv("Memory", human_bytes(p.memory_bytes)));
        }
        lines.push(Line::raw(""));
        if let Some(pr) = &e.project {
            lines.push(section("Project", w));
            lines.push(kv(
                "Project",
                format!(
                    "{}{}",
                    pr.name,
                    pr.git_branch
                        .as_ref()
                        .map(|b| format!("  on branch {b}"))
                        .unwrap_or_default()
                ),
            ));
            lines.push(kv("Path", tilde(&pr.root)));
            lines.push(Line::raw(""));
        } else if let Some(cwd) = &p.cwd {
            lines.push(section("Project", w));
            lines.push(kv("Cwd", tilde(cwd)));
            lines.push(Line::raw(""));
        }
    }
    if let Some(c) = &e.container {
        lines.push(section("Container", w));
        lines.push(kv("Container", format!("{} ({})", c.name, c.runtime)));
        lines.push(kv("Image", c.image.clone()));
        lines.push(kv("Mapping", format!("{} → {}", e.port, c.private_port)));
        lines.push(Line::raw(""));
    }
    lines.push(section("Network", w));
    lines.push(Line::from(vec![
        Span::styled(format!("{:>9}  ", "Address"), theme::muted()),
        Span::styled(
            e.addresses.join(", "),
            if e.exposure == Exposure::AllInterfaces {
                Style::new().fg(Color::Yellow)
            } else {
                Style::new()
            },
        ),
        Span::styled(
            if e.exposure == Exposure::AllInterfaces {
                "  reachable from your network"
            } else {
                ""
            },
            Style::new().fg(Color::Yellow),
        ),
    ]));
    if let Some(ex) = &ex {
        lines.push(Line::raw(""));
        lines.push(section("Plan", w));
        lines.push(Line::from(vec![
            Span::styled("→ ", Style::new().fg(ACCENT)),
            Span::styled(ex.recommendation.clone(), Style::new().fg(ACCENT)),
        ]));
        if let Some(plan) = &ex.plan {
            lines.extend(plan_lines(plan));
        }
    }
    if let Some(b) = &app.busy {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("{} {b}", SPIN[app.spinner % SPIN.len()]),
            theme::title(),
        )));
        for l in app.stop_log.iter().rev().take(6).rev() {
            lines.push(Line::from(Span::styled(format!("  · {l}"), theme::muted())));
        }
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

pub(super) fn key(k: &str) -> Span<'static> {
    Span::styled(
        format!(" {k} "),
        Style::new()
            .fg(Color::White)
            .bg(Color::Indexed(238))
            .add_modifier(Modifier::BOLD),
    )
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    if let Some(t) = &app.toast {
        let style = if t.ok {
            Style::new().fg(Color::Green).bold()
        } else {
            Style::new().fg(Color::Red).bold()
        };
        f.render_widget(
            Paragraph::new(Span::styled(format!(" {}", t.text), style)),
            area,
        );
        return;
    }
    let groups: &[&[(&str, &str)]] = if app.searching {
        &[
            &[("Enter", "keep filter"), ("Esc", "clear")],
            &[("↑↓", "move")],
        ]
    } else if app.tab == Tab::Graph {
        &[
            &[("↑↓", "move"), ("Tab", "agents")],
            &[("x", "stop"), ("C", "stop cluster"), ("o", "open")],
            &[("h", "external"), ("a", "all")],
            &[("?", "help"), ("q", "quit")],
        ]
    } else if app.tab == Tab::Agents {
        &[
            &[("↑↓", "move"), ("Tab", "ports")],
            &[("r", "refresh"), ("space", "pause")],
            &[("?", "help"), ("q", "quit")],
        ]
    } else {
        &[
            &[
                ("↑↓", "move"),
                ("/", "search"),
                ("e", "explain"),
                ("Tab", "graph"),
            ],
            &[("x", "stop"), ("X", "kill"), ("o", "open")],
            &[("s", "sort"), ("t", "proto"), ("d", "dev"), ("a", "all")],
            &[("?", "help"), ("q", "quit")],
        ]
    };
    let mut spans = vec![Span::raw(" ")];
    for (gi, g) in groups.iter().enumerate() {
        if gi > 0 {
            spans.push(Span::styled(" │ ", Style::new().fg(Color::Indexed(238))));
        }
        for (i, (k, label)) in g.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(key(k));
            spans.push(Span::styled(format!(" {label}"), theme::muted()));
        }
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let [v] = Layout::vertical([Constraint::Length(h.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    let [r] = Layout::horizontal([Constraint::Length(w.min(area.width))])
        .flex(Flex::Center)
        .areas(v);
    r
}

fn draw_confirm(f: &mut Frame, app: &App) {
    let Modal::Confirm { plan, headline } = &app.modal else {
        return;
    };
    let blocked = plan.is_blocked();
    let title = if blocked {
        " Can't stop this safely "
    } else if plan
        .steps
        .iter()
        .any(|s| matches!(s, Step::SignalProcesses { force: true, .. }))
    {
        " Force kill? "
    } else if plan.target.starts_with("cluster ") {
        " Stop cluster? "
    } else {
        " Stop? "
    };
    let color = if blocked { Color::Yellow } else { Color::Red };
    let mut lines = vec![
        Line::from(Span::styled(headline.clone(), theme::heading())),
        Line::raw(""),
    ];
    lines.extend(plan_lines(plan));
    lines.push(Line::raw(""));
    if blocked {
        lines.push(Line::from(vec![
            key("Enter"),
            Span::styled(" close", theme::muted()),
        ]));
    } else {
        let risk = match plan.risk {
            Risk::Low => Span::styled("low risk", Style::new().fg(Color::Green)),
            Risk::Medium => Span::styled("medium risk", Style::new().fg(Color::Yellow)),
            Risk::High => Span::styled("HIGH RISK", Style::new().fg(Color::Red).bold()),
        };
        lines.push(Line::from(vec![
            Span::styled(" y ", Style::new().fg(Color::White).bg(Color::Red).bold()),
            Span::raw(" stop   "),
            key("n"),
            Span::styled(" cancel     ", theme::muted()),
            risk,
        ]));
    }
    let height = (lines.len() as u16 + 6).clamp(18, f.area().height.saturating_sub(2));
    let area = centered(f.area(), 92, height);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(color))
        .title(Span::styled(title, Style::new().fg(color).bold()))
        .padding(Padding::uniform(1));
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_explain(f: &mut Frame, app: &mut App) {
    let scroll = match &app.modal {
        Modal::Explain { scroll } => *scroll,
        _ => 0,
    };
    let Some(ex) = app.explanation() else { return };
    let area = centered(f.area(), 100, f.area().height.saturating_sub(4));
    f.render_widget(Clear, area);
    let mut lines = vec![
        Line::from(Span::styled(ex.headline.clone(), theme::heading())),
        Line::raw(""),
    ];
    for d in &ex.details {
        lines.push(Line::from(Span::styled(
            format!("• {d}"),
            Style::new().fg(Color::Gray),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("Recommended: ", theme::title()),
        Span::raw(ex.recommendation.clone()),
    ]));
    if let Some(plan) = &ex.plan {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled("Plan", theme::heading())));
        lines.extend(plan_lines(plan));
    }
    if !ex.commands.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled("Commands", theme::heading())));
        for c in &ex.commands {
            lines.push(Line::from(vec![
                Span::styled(" $ ", theme::muted()),
                Span::styled(c.clone(), Style::new().fg(Color::Green)),
            ]));
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        key("x"),
        Span::styled(" stop   ", theme::muted()),
        key("↑↓"),
        Span::styled(" scroll   ", theme::muted()),
        key("any"),
        Span::styled(" close", theme::muted()),
    ]));
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT))
        .title(Span::styled(
            format!(" Why is :{} busy? ", ex.port),
            theme::title(),
        ))
        .padding(Padding::uniform(1));
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
}

fn draw_help(f: &mut Frame) {
    let area = centered(f.area(), 84, 27);
    f.render_widget(Clear, area);
    let rows = [
        ("↑ ↓ / j k", "Move selection (PgUp/PgDn, g/G)"),
        ("/", "Search: text, :3000, 3000-3999, proto:udp, pid:123"),
        ("Enter / e", "Explain why the port is busy"),
        (
            "x / Del",
            "Stop gracefully (SIGTERM → SIGKILL) with confirmation",
        ),
        ("X", "Force kill (SIGKILL) with confirmation"),
        ("Tab / v", "Switch Ports ↔ Graph (selection follows)"),
        (
            "C",
            "Stop the selected service's cluster in dependency order",
        ),
        ("h", "Graph: show/hide external hosts"),
        ("o", "Open http://localhost:<port> in the browser"),
        ("c", "Copy URL to clipboard (OSC 52)"),
        ("s / S", "Cycle sort key / reverse"),
        ("t", "Protocol: TCP+UDP → TCP → UDP"),
        ("d", "Only dev servers"),
        ("m", "Only my processes"),
        ("a", "Listening only ↔ all sockets"),
        ("r / F5", "Refresh now (auto every 2 s)"),
        ("p", "Pause auto-refresh"),
        ("q / Esc", "Quit"),
    ];
    let mut lines: Vec<Line> = rows
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("{k:>11}  "), theme::key()),
                Span::raw(*v),
            ])
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Colours: green dev · magenta data · blue container · yellow exposed",
        theme::muted(),
    )));
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(ACCENT))
        .title(Span::styled(" Keys ", theme::title()))
        .padding(Padding::uniform(1));
    f.render_widget(Paragraph::new(lines).block(block), area);
}

#[cfg(test)]
mod tests {
    use super::{fit_spans, section};
    use ratatui::text::Span;

    #[test]
    fn header_stats_drop_whole_items_when_narrow() {
        let stats = || {
            vec![
                Span::raw("12 ports"),
                Span::raw(" · 3 dev"),
                Span::raw(" · 24 ms"),
            ]
        };
        let w = |v: Vec<Span<'static>>| v.iter().map(Span::width).sum::<usize>();
        assert_eq!(w(fit_spans(stats(), 100)), 8 + 8 + 8);
        assert_eq!(w(fit_spans(stats(), 20)), 16);
        assert_eq!(fit_spans(stats(), 10).len(), 1);
        assert!(fit_spans(stats(), 3).is_empty());
    }

    #[test]
    fn section_rule_fills_the_panel_width_exactly() {
        for w in [12u16, 30, 57, 120] {
            assert_eq!(section("Network", w).width(), usize::from(w));
        }
    }

    #[test]
    fn section_rule_never_overflows_a_tiny_panel() {
        let l = section("Container", 4);
        assert_eq!(l.width(), "CONTAINER ".len());
    }
}

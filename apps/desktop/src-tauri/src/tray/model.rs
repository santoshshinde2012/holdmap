//! What the menu-bar / tray widget shows, as plain data: the readings beside the icon, the
//! native menu, the tooltip, the attention badge and the data for the popup panel. Pure, so
//! every platform's output is unit-tested; `mod.rs` applies it with Tauri.
//!
//! Platforms differ in what a tray can show, and the model only asks for what each one draws:
//! macOS draws stacked mini readings into the menu-bar image and has a tooltip; Linux
//! (AppIndicator) shows the readings as a panel label but no tooltip and opens the menu on any
//! click; Windows shows a tooltip but no text, so its figures live in the tooltip and the badge.

use holdmap_core::agents::AgentsReport;
use holdmap_core::attention::{AttentionItem, Target, Urgency};
use holdmap_core::model::Exposure;
use holdmap_core::util::{count, human_bytes, printable};
use holdmap_core::{PortEntry, Snapshot};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// Samples kept per trend: the most recent scans (about half a minute at the default 4 s,
/// longer while the window is hidden and scans slow down).
const TREND_LEN: usize = 8;
/// Scans the panel's history chart and attention grid keep.
const HISTORY_LEN: usize = 60;
/// Bars in the menu bar's mini CPU chart.
const BAR_CHART_LEN: usize = 16;
/// Agents the panel's table lists, busiest first.
const PANEL_AGENTS: usize = 5;
/// Rows each panel section lists before "+N more".
const MAX_NEEDS: usize = 5;
const MAX_DEV: usize = 10;
const MAX_AGENTS: usize = 6;
/// Longest label a panel row gets, so one long project name can't widen the whole menu.
const MAX_LABEL: usize = 60;
/// Windows copies at most 128 UTF-16 units into the tooltip, including the terminator.
const MAX_TOOLTIP_UNITS: usize = 127;
/// Sparkline bars, lowest to highest.
const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// A sparkline's full height is at least this much CPU, so idle noise stays flat.
const CPU_FLOOR: f32 = 10.0;

/// Which desktop the widget draws for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    pub const CURRENT: Platform = if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    };

    /// Windows tray icons are square with no text beside them, so the style choice changes
    /// nothing there.
    fn has_style(self) -> bool {
        self != Platform::Windows
    }
}

/// What sits beside the icon (`Config::tray_style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// The icon and its badge only.
    Icon,
    /// Adds the readings: agents, their CPU, dev servers and items that need you.
    Count,
    /// Also adds a mini chart of the agents' CPU.
    Trend,
}

impl Style {
    pub const ALL: [Style; 3] = [Style::Icon, Style::Count, Style::Trend];

    /// Reads a `Config::tray_style()` value (already validated there).
    pub fn parse(s: &str) -> Style {
        match s {
            "icon" => Style::Icon,
            "trend" => Style::Trend,
            _ => Style::Count,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Style::Icon => "icon",
            Style::Count => "count",
            Style::Trend => "trend",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Style::Icon => "Icon only",
            Style::Count => "Readings",
            Style::Trend => "Readings and agent CPU chart",
        }
    }
}

/// One dev server row.
#[derive(Debug, Clone, PartialEq)]
pub struct DevServer {
    pub port: u16,
    pub label: String,
    pub stoppable: bool,
    /// Reachable from the network, not only this computer.
    pub exposed: bool,
}

/// One agent row.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRow {
    pub id: String,
    pub name: String,
    /// What tells it apart from others of the same product: its first folder's name, or its
    /// PID when it shares a name and has no folder.
    pub place: Option<String>,
    pub cpu: f32,
    pub memory: u64,
}

/// One "needs you" row.
#[derive(Debug, Clone, PartialEq)]
pub struct NeedRow {
    pub urgency: Urgency,
    pub title: String,
    pub target: Target,
}

/// The facts one scan gives the widget, reduced to what it shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Readings {
    pub ports: usize,
    pub exposed: usize,
    pub dev: Vec<DevServer>,
    /// `None` until the agents report is read.
    pub agents: Option<Vec<AgentRow>>,
    /// `None` until both the scan and the agents report are read.
    pub needs: Option<Vec<NeedRow>>,
}

fn dev_label(e: &PortEntry) -> String {
    match (&e.framework, &e.project) {
        (Some(f), Some(p)) => format!("{} · {}", f.name, p.name),
        (Some(f), None) => f.name.clone(),
        (None, Some(p)) => p.name.clone(),
        _ => e.label.clone(),
    }
}

impl Readings {
    pub fn of(
        snapshot: &Snapshot,
        report: Option<&AgentsReport>,
        needs: Option<&[AttentionItem]>,
    ) -> Readings {
        let entries = &snapshot.entries;
        Readings {
            ports: entries.len(),
            exposed: entries
                .iter()
                .filter(|e| e.exposure != Exposure::Loopback)
                .count(),
            dev: entries
                .iter()
                .filter(|e| e.is_dev && e.is_mine)
                .map(|e| DevServer {
                    port: e.port,
                    label: dev_label(e),
                    stoppable: !e.protected,
                    exposed: e.exposure != Exposure::Loopback,
                })
                .collect(),
            agents: report.map(|r| {
                r.agents
                    .iter()
                    .map(|a| AgentRow {
                        id: a.id.clone(),
                        name: a.name.clone(),
                        place: a.folders.first().map(|f| f.label.clone()).or_else(|| {
                            let twins = r.agents.iter().filter(|b| b.name == a.name).count();
                            (twins > 1).then(|| format!("PID {}", a.pid))
                        }),
                        cpu: a.cpu_percent,
                        memory: a.memory_bytes,
                    })
                    .collect()
            }),
            needs: needs.map(|items| {
                items
                    .iter()
                    .map(|i| NeedRow {
                        urgency: i.severity,
                        title: i.title.clone(),
                        target: i.target.clone(),
                    })
                    .collect()
            }),
        }
    }

    fn cpu(&self) -> Option<f32> {
        // Folded from +0.0: an empty float `sum()` is -0.0, which would print as "-0%".
        self.agents
            .as_ref()
            .map(|a| a.iter().fold(0.0, |total, a| total + a.cpu))
    }
}

/// Recent CPU samples: all agents together, and each agent by id.
#[derive(Debug, Default)]
///
/// CPU is measured between scans, so the first reading after start-up, and each agent's first
/// reading, is always 0: it primes the trend instead of becoming a sample, and the widget shows
/// no CPU figure until there is one.
pub struct Trends {
    total: VecDeque<f32>,
    agents: HashMap<String, VecDeque<f32>>,
    /// The most urgent "needs you" item per scan: 0 none, 1 worth a look, 2 look now.
    attention: VecDeque<u8>,
    /// Agents in the previous scan (`None` before the first one).
    seen: Option<HashSet<String>>,
}

fn push<T>(series: &mut VecDeque<T>, v: T, len: usize) {
    if series.len() == len {
        series.pop_front();
    }
    series.push_back(v);
}

/// The last `n` samples.
fn recent(series: &VecDeque<f32>, n: usize) -> VecDeque<f32> {
    series
        .iter()
        .skip(series.len().saturating_sub(n))
        .copied()
        .collect()
}

fn urgency_level(needs: &[NeedRow]) -> u8 {
    match needs.iter().map(|n| n.urgency).max() {
        Some(Urgency::High) => 2,
        Some(Urgency::Medium) => 1,
        None => 0,
    }
}

impl Trends {
    /// Add one scan's samples; agents that stopped are forgotten.
    pub fn record(&mut self, readings: &Readings) {
        if let Some(needs) = &readings.needs {
            push(&mut self.attention, urgency_level(needs), HISTORY_LEN);
        }
        let (Some(agents), Some(total)) = (&readings.agents, readings.cpu()) else {
            return;
        };
        self.agents
            .retain(|id, _| agents.iter().any(|a| &a.id == id));
        if let Some(seen) = &self.seen {
            push(&mut self.total, total.max(0.0), HISTORY_LEN);
            for a in agents.iter().filter(|a| seen.contains(&a.id)) {
                push(
                    self.agents.entry(a.id.clone()).or_default(),
                    a.cpu.max(0.0),
                    TREND_LEN,
                );
            }
        }
        self.seen = Some(agents.iter().map(|a| a.id.clone()).collect());
    }

    /// The agents' CPU together, once it has been measured.
    fn total_cpu(&self, readings: &Readings) -> Option<f32> {
        readings.cpu().filter(|_| !self.total.is_empty())
    }
}

/// `▁▂▅▇` for the samples, scaled to the highest (or [`CPU_FLOOR`], so idle stays flat). `None`
/// for fewer than two samples (one point isn't a trend) and when every bar is the lowest: a row
/// of `▁` reads as an empty `____` field in a menu.
pub fn sparkline(samples: &VecDeque<f32>) -> Option<String> {
    if samples.len() < 2 {
        return None;
    }
    let top = samples.iter().copied().fold(CPU_FLOOR, f32::max);
    let bars: String = samples
        .iter()
        .map(|v| {
            let level = (v / top * (BARS.len() - 1) as f32).round() as usize;
            BARS[level.min(BARS.len() - 1)]
        })
        .collect();
    bars.chars().any(|b| b != BARS[0]).then_some(bars)
}

/// "3.7% ▃▅▇", or "3.7%" while the trend is flat.
fn cpu_with_trend(cpu: f32, samples: &VecDeque<f32>) -> String {
    match sparkline(samples) {
        Some(bars) => format!("{} {bars}", cpu_text(cpu)),
        None => cpu_text(cpu),
    }
}

fn cpu_text(cpu: f32) -> String {
    if cpu < 9.95 {
        format!("{cpu:.1}%")
    } else {
        format!("{cpu:.0}%")
    }
}

/// Printable, at most [`MAX_LABEL`] characters, with `&` escaped: native menus read a single
/// `&` as a keyboard-mnemonic marker (so `R&D` would show as `RD`).
fn clip(s: &str) -> String {
    let s = printable(s);
    let shown: String = if s.chars().count() <= MAX_LABEL {
        s.into_owned()
    } else {
        s.chars().take(MAX_LABEL - 1).chain(['…']).collect()
    };
    shown.replace('&', "&&")
}

/// One row of the panel. Plain data so a refresh can tell whether only the text changed (then
/// the open menu updates in place) or the rows did (then the menu is rebuilt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayItem {
    Item {
        id: String,
        label: String,
        enabled: bool,
        accel: Option<&'static str>,
    },
    Check {
        id: String,
        label: String,
        checked: bool,
    },
    Submenu {
        id: String,
        label: String,
        items: Vec<TrayItem>,
    },
    Separator,
}

impl TrayItem {
    pub fn label(&self) -> &str {
        match self {
            TrayItem::Item { label, .. }
            | TrayItem::Check { label, .. }
            | TrayItem::Submenu { label, .. } => label,
            TrayItem::Separator => "",
        }
    }
}

pub fn item(id: impl Into<String>, label: impl Into<String>, enabled: bool) -> TrayItem {
    TrayItem::Item {
        id: id.into(),
        label: label.into(),
        enabled,
        accel: None,
    }
}

/// A row that only reads out a value.
fn reading(id: &str, label: impl Into<String>) -> TrayItem {
    item(id, label, false)
}

/// True when `a` and `b` have the same rows (ids, kinds, enabled state and shortcuts), so `b`
/// can be applied by changing text and check marks only.
pub fn same_rows(a: &[TrayItem], b: &[TrayItem]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|pair| match pair {
            (TrayItem::Separator, TrayItem::Separator) => true,
            (
                TrayItem::Item {
                    id: i,
                    enabled: e,
                    accel: k,
                    ..
                },
                TrayItem::Item {
                    id: j,
                    enabled: f,
                    accel: l,
                    ..
                },
            ) => i == j && e == f && k == l,
            (TrayItem::Check { id: i, .. }, TrayItem::Check { id: j, .. }) => i == j,
            (
                TrayItem::Submenu {
                    id: i, items: x, ..
                },
                TrayItem::Submenu {
                    id: j, items: y, ..
                },
            ) => i == j && same_rows(x, y),
            _ => false,
        })
}

/// Every row except separators, depth first (a submenu, then its rows): the order `mod.rs`
/// keeps the native handles in.
pub fn flatten(items: &[TrayItem]) -> Vec<&TrayItem> {
    let mut out = Vec::new();
    for i in items {
        match i {
            TrayItem::Separator => {}
            TrayItem::Submenu { items, .. } => {
                out.push(i);
                out.extend(flatten(items));
            }
            _ => out.push(i),
        }
    }
    out
}

/// What a panel row does when chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Show,
    /// The popup panel (macOS and Windows open it with a click; Linux trays only from the menu).
    ShowPanel,
    HidePanel,
    Quit,
    Refresh,
    /// Opens the window's reviewed stop plan; nothing is stopped from the menu itself.
    StopAllDev,
    FocusPort(u16),
    OpenAgent(String),
    OpenEntry(String),
    Style(Style),
}

impl Action {
    pub fn parse(id: &str) -> Option<Action> {
        let action = match id {
            "show" | "more-needs" | "more-dev" | "more-agents" => Action::Show,
            "show-panel" => Action::ShowPanel,
            "hide-panel" => Action::HidePanel,
            "quit" => Action::Quit,
            "refresh" => Action::Refresh,
            "stop-all-dev" => Action::StopAllDev,
            _ => {
                if let Some(port) = id.strip_prefix("focus:") {
                    return port.parse().ok().map(Action::FocusPort);
                }
                if let Some(agent) = id.strip_prefix("agent:") {
                    return Some(Action::OpenAgent(agent.into()));
                }
                // `need:<row>:<agent|entry>:<id>`: the row number keeps ids unique when two
                // items lead to the same place.
                if let Some(rest) = id.strip_prefix("need:") {
                    let (_, target) = rest.split_once(':')?;
                    if let Some(agent) = target.strip_prefix("agent:") {
                        return Some(Action::OpenAgent(agent.into()));
                    }
                    return target
                        .strip_prefix("entry:")
                        .map(|e| Action::OpenEntry(e.into()));
                }
                let style = id.strip_prefix("style:")?;
                return Style::ALL
                    .into_iter()
                    .find(|s| s.as_str() == style)
                    .map(Action::Style);
            }
        };
        Some(action)
    }
}

fn need_id(row: usize, target: &Target) -> String {
    match target {
        Target::Agents { agent_id } => format!("need:{row}:agent:{agent_id}"),
        Target::List { entry_id } => format!("need:{row}:entry:{entry_id}"),
    }
}

fn need_mark(u: Urgency) -> &'static str {
    match u {
        Urgency::High => "⚠",
        Urgency::Medium => "•",
    }
}

/// The attention badge: how many items need you and the most urgent one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Badge {
    pub count: usize,
    pub urgency: Option<Urgency>,
}

/// Everything the widget shows for one state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayView {
    pub items: Vec<TrayItem>,
    /// Readings drawn beside the icon (macOS).
    pub bar: MenuBar,
    /// The panel label beside the icon (Linux); `None` shows the icon alone.
    pub title: Option<String>,
    /// Hover text (macOS, Windows).
    pub tooltip: String,
    pub badge: Badge,
}

fn more(id: &str, n: usize, what: &str) -> Option<TrayItem> {
    (n > 0).then(|| item(id, format!("+{n} more {what} in holdmap…"), true))
}

fn panel(r: Option<&Readings>, trends: &Trends, style: Style, platform: Platform) -> Vec<TrayItem> {
    let mut out = Vec::new();
    let Some(r) = r else {
        out.push(reading("reading-ports", "Scanning…"));
        out.push(TrayItem::Separator);
        out.extend(controls(style, platform));
        return out;
    };

    // Readings, like the header of a system monitor's popup.
    out.push(reading(
        "reading-ports",
        format!(
            "{} in use · {} network-exposed",
            count(r.ports, "port", "ports"),
            r.exposed
        ),
    ));
    if let Some(agents) = &r.agents {
        let label = if agents.is_empty() {
            "No agents running".to_string()
        } else {
            let memory: u64 = agents.iter().map(|a| a.memory).sum();
            let cpu = trends.total_cpu(r).map(|cpu| {
                format!(
                    " · CPU {}",
                    cpu_with_trend(cpu, &recent(&trends.total, TREND_LEN))
                )
            });
            format!(
                "{}{} · {}",
                count(agents.len(), "agent", "agents"),
                cpu.unwrap_or_default(),
                human_bytes(memory)
            )
        };
        out.push(reading("reading-agents", label));
    }

    if let Some(needs) = &r.needs {
        out.push(TrayItem::Separator);
        if needs.is_empty() {
            out.push(reading("needs", "Nothing needs you"));
        } else {
            out.push(reading("needs", format!("Needs you · {}", needs.len())));
            for (row, n) in needs.iter().take(MAX_NEEDS).enumerate() {
                out.push(item(
                    need_id(row, &n.target),
                    clip(&format!("{}  {}", need_mark(n.urgency), n.title)),
                    true,
                ));
            }
            out.extend(more(
                "more-needs",
                needs.len().saturating_sub(MAX_NEEDS),
                "items",
            ));
        }
    }

    out.push(TrayItem::Separator);
    if r.dev.is_empty() {
        out.push(reading("dev", "No dev servers running"));
    } else {
        out.push(reading("dev", format!("Dev servers · {}", r.dev.len())));
        for d in r.dev.iter().take(MAX_DEV) {
            out.push(item(
                format!("focus:{}", d.port),
                clip(&format!("● :{}   {}", d.port, d.label)),
                true,
            ));
        }
        out.extend(more(
            "more-dev",
            r.dev.len().saturating_sub(MAX_DEV),
            "dev servers",
        ));
        if r.dev.iter().any(|d| d.stoppable) {
            out.push(item("stop-all-dev", "Stop all dev servers…", true));
        }
    }

    if let Some(agents) = r.agents.as_ref().filter(|a| !a.is_empty()) {
        out.push(TrayItem::Separator);
        out.push(reading("agents", format!("Agents · {}", agents.len())));
        for a in agents.iter().take(MAX_AGENTS) {
            let name = match &a.place {
                Some(place) => format!("{} · {place}", a.name),
                None => a.name.clone(),
            };
            let cpu = trends
                .agents
                .get(&a.id)
                .map(|s| format!("   {}", cpu_with_trend(a.cpu, s)));
            out.push(item(
                format!("agent:{}", a.id),
                clip(&format!(
                    "{name}{} · {}",
                    cpu.unwrap_or_default(),
                    human_bytes(a.memory)
                )),
                true,
            ));
        }
        out.extend(more(
            "more-agents",
            agents.len().saturating_sub(MAX_AGENTS),
            "agents",
        ));
    }

    out.push(TrayItem::Separator);
    out.extend(controls(style, platform));
    out
}

fn controls(style: Style, platform: Platform) -> Vec<TrayItem> {
    let mut out = vec![
        item("show-panel", "Show panel", true),
        item("show", "Open holdmap", true),
    ];
    if platform.has_style() {
        out.push(TrayItem::Submenu {
            id: "style".into(),
            label: if platform == Platform::MacOs {
                "Menu bar shows".into()
            } else {
                "Panel shows".into()
            },
            items: Style::ALL
                .into_iter()
                .map(|s| TrayItem::Check {
                    id: format!("style:{}", s.as_str()),
                    label: s.label().into(),
                    checked: s == style,
                })
                .collect(),
        });
    }
    out.push(item("refresh", "Refresh", true));
    out.push(TrayItem::Item {
        id: "quit".into(),
        label: "Quit holdmap".into(),
        enabled: true,
        accel: Some("CmdOrCtrl+Q"),
    });
    out
}

fn needs_text(n: usize) -> String {
    format!("{n} need you")
}

/// One stacked reading beside the icon, like a system monitor's: a small label over a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mini {
    pub label: &'static str,
    pub value: String,
    /// How the reading reads in a sentence (the Linux label), e.g. "8 agents".
    pub words: String,
}

/// What macOS draws beside the icon: stacked readings and, with [`Style::Trend`], a mini chart
/// of the agents' CPU. Empty shows the icon alone.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MenuBar {
    pub readings: Vec<Mini>,
    pub chart: Vec<f32>,
}

impl Eq for MenuBar {}

/// "7%", or "<1%" for a little but not nothing.
fn bar_cpu(cpu: f32) -> String {
    if cpu > 0.0 && cpu < 0.5 {
        "<1%".into()
    } else {
        format!("{cpu:.0}%")
    }
}

fn menu_bar(r: Option<&Readings>, trends: &Trends, style: Style) -> MenuBar {
    let Some(r) = r.filter(|_| style != Style::Icon) else {
        return MenuBar::default();
    };
    let mut readings = Vec::new();
    if let Some(agents) = &r.agents {
        readings.push(Mini {
            label: "AGENTS",
            value: agents.len().to_string(),
            words: count(agents.len(), "agent", "agents"),
        });
    }
    if let Some(cpu) = trends.total_cpu(r) {
        readings.push(Mini {
            label: "CPU",
            value: bar_cpu(cpu),
            words: format!("CPU {}", bar_cpu(cpu)),
        });
    }
    readings.push(Mini {
        label: "DEV",
        value: r.dev.len().to_string(),
        words: format!("{} dev", r.dev.len()),
    });
    if let Some(n) = r.needs.as_ref().map(Vec::len).filter(|n| *n > 0) {
        readings.push(Mini {
            label: "NEEDS",
            value: n.to_string(),
            words: needs_text(n),
        });
    }
    let chart = if style == Style::Trend && trends.total.len() >= 2 {
        recent(&trends.total, BAR_CHART_LEN).into_iter().collect()
    } else {
        Vec::new()
    };
    MenuBar { readings, chart }
}

/// The Linux panel label: the same readings as a sentence.
fn label_text(bar: &MenuBar) -> Option<String> {
    if bar.readings.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = bar.readings.iter().map(|m| m.words.clone()).collect();
    if let Some(spark) = sparkline(&bar.chart.iter().copied().collect()) {
        parts.push(spark);
    }
    Some(parts.join(" · "))
}

/// The hover text, at most [`MAX_TOOLTIP_UNITS`] UTF-16 units (whole lines only).
fn tooltip(r: Option<&Readings>, trends: &Trends) -> String {
    let mut lines = vec!["holdmap".to_string()];
    if let Some(r) = r {
        lines.push(format!(
            "{} · {} in use",
            count(r.dev.len(), "dev server", "dev servers"),
            count(r.ports, "port", "ports")
        ));
        if let Some(agents) = r.agents.as_ref().filter(|a| !a.is_empty()) {
            let cpu = trends
                .total_cpu(r)
                .map(|cpu| format!(" · CPU {}", cpu_text(cpu)));
            lines.push(format!(
                "{}{}",
                count(agents.len(), "agent", "agents"),
                cpu.unwrap_or_default()
            ));
        }
        match r.needs.as_ref().map(Vec::len) {
            Some(0) => lines.push("Nothing needs you".into()),
            Some(n) => lines.push(needs_text(n)),
            None => {}
        }
    }
    fit_lines(lines)
}

fn fit_lines(lines: Vec<String>) -> String {
    let mut out = String::new();
    for line in lines {
        let next = if out.is_empty() {
            line
        } else {
            format!("{out}\n{line}")
        };
        if next.encode_utf16().count() > MAX_TOOLTIP_UNITS {
            break;
        }
        out = next;
    }
    out
}

fn badge(r: Option<&Readings>) -> Badge {
    let needs = r.and_then(|r| r.needs.as_deref()).unwrap_or_default();
    Badge {
        count: needs.len(),
        urgency: needs.iter().map(|n| n.urgency).max(),
    }
}

/// The widget for `readings` (`None` before the first scan).
pub fn view(
    readings: Option<&Readings>,
    trends: &Trends,
    style: Style,
    platform: Platform,
) -> TrayView {
    let bar = if platform.has_style() {
        menu_bar(readings, trends, style)
    } else {
        MenuBar::default()
    };
    TrayView {
        items: panel(readings, trends, style, platform),
        title: if platform == Platform::Linux {
            label_text(&bar)
        } else {
            None
        },
        bar: if platform == Platform::MacOs {
            bar
        } else {
            MenuBar::default()
        },
        tooltip: tooltip(readings, trends),
        badge: badge(readings),
    }
}

/// One "needs you" row in the popup panel.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelNeed {
    /// `high` (look now) or `medium` (worth a look).
    pub urgency: &'static str,
    pub title: String,
    /// The [`Action`] id that opens it in the window.
    pub action: String,
}

/// One dev server row in the popup panel.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelDev {
    pub port: u16,
    pub label: String,
    pub exposed: bool,
    pub action: String,
}

/// One agent row in the popup panel's table.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelAgent {
    pub name: String,
    pub place: Option<String>,
    /// Percent of one core, once measured between two scans.
    pub cpu: Option<f32>,
    pub memory: u64,
    pub action: String,
}

/// Everything the popup panel shows. Rows carry the [`Action`] id they trigger, so the panel can
/// only ask for what the native menu offers (and a stop still opens the window's reviewed plan).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelData {
    /// False until the first scan.
    pub ready: bool,
    pub ports: usize,
    pub exposed: usize,
    /// `None` until the agents report is read.
    pub agents: Option<usize>,
    /// The agents' CPU together, once measured.
    pub cpu: Option<f32>,
    pub memory: Option<u64>,
    /// The agents' CPU per scan, oldest first.
    pub cpu_history: Vec<f32>,
    /// The most urgent item per scan, oldest first: 0 none, 1 worth a look, 2 look now.
    pub attention_history: Vec<u8>,
    /// `None` until both the scan and the agents report are read.
    pub needs: Option<Vec<PanelNeed>>,
    pub dev: Vec<PanelDev>,
    /// Whether "Stop all dev servers…" has anything to stop.
    pub stop_dev: bool,
    /// The busiest agents first.
    pub top_agents: Vec<PanelAgent>,
    pub more_agents: usize,
    pub style: &'static str,
}

/// The popup panel's data for `readings` (`None` before the first scan).
pub fn panel_data(readings: Option<&Readings>, trends: &Trends, style: Style) -> PanelData {
    let Some(r) = readings else {
        return PanelData {
            ready: false,
            ports: 0,
            exposed: 0,
            agents: None,
            cpu: None,
            memory: None,
            cpu_history: Vec::new(),
            attention_history: Vec::new(),
            needs: None,
            dev: Vec::new(),
            stop_dev: false,
            top_agents: Vec::new(),
            more_agents: 0,
            style: style.as_str(),
        };
    };
    let mut agents: Vec<&AgentRow> = r.agents.iter().flatten().collect();
    agents.sort_by(|a, b| {
        b.cpu
            .total_cmp(&a.cpu)
            .then_with(|| b.memory.cmp(&a.memory))
    });
    PanelData {
        ready: true,
        ports: r.ports,
        exposed: r.exposed,
        agents: r.agents.as_ref().map(Vec::len),
        cpu: trends.total_cpu(r),
        memory: r.agents.as_ref().map(|a| a.iter().map(|a| a.memory).sum()),
        cpu_history: trends.total.iter().copied().collect(),
        attention_history: trends.attention.iter().copied().collect(),
        needs: r.needs.as_ref().map(|needs| {
            needs
                .iter()
                .enumerate()
                .map(|(row, n)| PanelNeed {
                    urgency: if n.urgency == Urgency::High {
                        "high"
                    } else {
                        "medium"
                    },
                    title: printable(&n.title).into_owned(),
                    action: need_id(row, &n.target),
                })
                .collect()
        }),
        dev: r
            .dev
            .iter()
            .map(|d| PanelDev {
                port: d.port,
                label: printable(&d.label).into_owned(),
                exposed: d.exposed,
                action: format!("focus:{}", d.port),
            })
            .collect(),
        stop_dev: r.dev.iter().any(|d| d.stoppable),
        more_agents: agents.len().saturating_sub(PANEL_AGENTS),
        top_agents: agents
            .into_iter()
            .take(PANEL_AGENTS)
            .map(|a| PanelAgent {
                name: printable(&a.name).into_owned(),
                place: a.place.as_deref().map(|p| printable(p).into_owned()),
                cpu: trends.agents.get(&a.id).map(|_| a.cpu),
                memory: a.memory,
                action: format!("agent:{}", a.id),
            })
            .collect(),
        style: style.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readings() -> Readings {
        Readings {
            ports: 12,
            exposed: 2,
            dev: vec![
                DevServer {
                    port: 3000,
                    label: "Next.js · web".into(),
                    stoppable: true,
                    exposed: false,
                },
                DevServer {
                    port: 5173,
                    label: "Vite".into(),
                    stoppable: true,
                    exposed: true,
                },
            ],
            agents: Some(vec![AgentRow {
                id: "agent:42".into(),
                name: "Claude Code".into(),
                place: Some("holdmap".into()),
                cpu: 12.4,
                memory: 300 * 1024 * 1024,
            }]),
            needs: Some(vec![
                NeedRow {
                    urgency: Urgency::High,
                    title: "Claude Code listens on all interfaces".into(),
                    target: Target::Agents {
                        agent_id: "agent:42".into(),
                    },
                },
                NeedRow {
                    urgency: Urgency::Medium,
                    title: ":5173 is open to the network".into(),
                    target: Target::List {
                        entry_id: "tcp:5173".into(),
                    },
                },
            ]),
        }
    }

    /// Trends after three scans of `r`: the first only primes them, the next two are samples.
    fn measured(r: &Readings) -> Trends {
        let mut trends = Trends::default();
        for _ in 0..3 {
            trends.record(r);
        }
        trends
    }

    fn ids(items: &[TrayItem]) -> Vec<String> {
        flatten(items)
            .into_iter()
            .map(|i| match i {
                TrayItem::Item { id, .. }
                | TrayItem::Check { id, .. }
                | TrayItem::Submenu { id, .. } => id.clone(),
                TrayItem::Separator => unreachable!(),
            })
            .collect()
    }

    fn label_of<'a>(items: &'a [TrayItem], want: &str) -> &'a str {
        flatten(items)
            .into_iter()
            .find(|i| {
                matches!(i, TrayItem::Item { id, .. }
                    | TrayItem::Check { id, .. }
                    | TrayItem::Submenu { id, .. } if id == want)
            })
            .unwrap_or_else(|| panic!("no row {want}"))
            .label()
    }

    #[test]
    fn the_panel_reads_like_a_monitor_and_every_row_has_an_action() {
        let trends = measured(&readings());
        let v = view(Some(&readings()), &trends, Style::Count, Platform::MacOs);
        assert_eq!(
            ids(&v.items),
            [
                "reading-ports",
                "reading-agents",
                "needs",
                "need:0:agent:agent:42",
                "need:1:entry:tcp:5173",
                "dev",
                "focus:3000",
                "focus:5173",
                "stop-all-dev",
                "agents",
                "agent:agent:42",
                "show-panel",
                "show",
                "style",
                "style:icon",
                "style:count",
                "style:trend",
                "refresh",
                "quit",
            ]
        );
        assert_eq!(
            label_of(&v.items, "reading-ports"),
            "12 ports in use · 2 network-exposed"
        );
        assert_eq!(
            label_of(&v.items, "reading-agents"),
            "1 agent · CPU 12% ██ · 300.0 MB"
        );
        assert_eq!(
            label_of(&v.items, "need:0:agent:agent:42"),
            "⚠  Claude Code listens on all interfaces"
        );
        assert_eq!(label_of(&v.items, "focus:3000"), "● :3000   Next.js · web");
        assert_eq!(
            label_of(&v.items, "agent:agent:42"),
            "Claude Code · holdmap   12% ██ · 300.0 MB"
        );
        // Read-only rows are disabled; everything else does something.
        for i in flatten(&v.items) {
            if let TrayItem::Item { id, enabled, .. } = i {
                let action = Action::parse(id);
                assert_eq!(*enabled, action.is_some(), "{id}");
            }
        }
        assert_eq!(
            Action::parse("need:0:agent:agent:42"),
            Some(Action::OpenAgent("agent:42".into()))
        );
        assert_eq!(
            Action::parse("need:1:entry:tcp:5173"),
            Some(Action::OpenEntry("tcp:5173".into()))
        );
        assert_eq!(
            Action::parse("agent:agent:42"),
            Some(Action::OpenAgent("agent:42".into()))
        );
        assert_eq!(Action::parse("focus:3000"), Some(Action::FocusPort(3000)));
        assert_eq!(
            Action::parse("style:trend"),
            Some(Action::Style(Style::Trend))
        );
        assert_eq!(Action::parse("style:rainbow"), None);
        assert_eq!(Action::parse("focus:99999"), None);
    }

    #[test]
    fn styles_choose_the_readings_beside_the_icon() {
        let r = readings();
        let mut trends = Trends::default();
        let bar = |trends: &Trends, style| view(Some(&r), trends, style, Platform::MacOs).bar;
        let cells = |b: &MenuBar| -> Vec<String> {
            b.readings
                .iter()
                .map(|m| format!("{} {}", m.label, m.value))
                .collect()
        };
        assert_eq!(bar(&trends, Style::Icon), MenuBar::default());
        // CPU waits for its first measurement; no chart without the Trend style.
        assert_eq!(
            cells(&bar(&trends, Style::Count)),
            ["AGENTS 1", "DEV 2", "NEEDS 2"]
        );
        trends.record(&r);
        trends.record(&r);
        assert_eq!(
            cells(&bar(&trends, Style::Count)),
            ["AGENTS 1", "CPU 12%", "DEV 2", "NEEDS 2"]
        );
        assert!(bar(&trends, Style::Count).chart.is_empty());
        // A chart needs two samples.
        assert!(bar(&trends, Style::Trend).chart.is_empty());
        trends.record(&r);
        assert_eq!(bar(&trends, Style::Trend).chart, [12.4, 12.4]);
        // macOS draws the readings; Linux shows them as a panel label; Windows has neither.
        let mac = view(Some(&r), &trends, Style::Trend, Platform::MacOs);
        assert_eq!(mac.title, None);
        let linux = view(Some(&r), &trends, Style::Trend, Platform::Linux);
        assert_eq!(linux.bar, MenuBar::default());
        assert_eq!(
            linux.title.as_deref(),
            Some("1 agent · CPU 12% · 2 dev · 2 need you · ██")
        );
        let windows = view(Some(&r), &trends, Style::Trend, Platform::Windows);
        assert_eq!((windows.title, windows.bar), (None, MenuBar::default()));
        // Nothing needs you: no NEEDS reading.
        let quiet = Readings {
            agents: Some(vec![]),
            needs: Some(vec![]),
            dev: vec![],
            ..Readings::default()
        };
        assert_eq!(
            cells(&view(Some(&quiet), &trends, Style::Count, Platform::MacOs).bar),
            ["AGENTS 0", "CPU 0%", "DEV 0"]
        );
        assert_eq!(bar_cpu(0.2), "<1%");
        assert_eq!(bar_cpu(0.0), "0%");
        assert_eq!(bar_cpu(7.4), "7%");
    }

    #[test]
    fn the_style_menu_checks_the_current_style_and_is_absent_on_windows() {
        let trends = Trends::default();
        let v = view(Some(&readings()), &trends, Style::Trend, Platform::Linux);
        let checked: Vec<&str> = flatten(&v.items)
            .into_iter()
            .filter_map(|i| match i {
                TrayItem::Check {
                    id, checked: true, ..
                } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(checked, ["style:trend"]);
        assert_eq!(label_of(&v.items, "style"), "Panel shows");
        let win = view(Some(&readings()), &trends, Style::Trend, Platform::Windows);
        assert!(!ids(&win.items).iter().any(|id| id.starts_with("style")));
    }

    #[test]
    fn before_the_first_scan_it_says_so_and_still_offers_the_controls() {
        let v = view(None, &Trends::default(), Style::Count, Platform::MacOs);
        assert_eq!(label_of(&v.items, "reading-ports"), "Scanning…");
        assert_eq!(v.title, None);
        assert_eq!(v.tooltip, "holdmap");
        assert_eq!(v.badge, Badge::default());
        assert!(ids(&v.items).contains(&"quit".to_string()));
    }

    #[test]
    fn unknown_agents_and_needs_are_left_out_not_shown_as_zero() {
        let r = Readings {
            agents: None,
            needs: None,
            ..readings()
        };
        let v = view(Some(&r), &Trends::default(), Style::Count, Platform::MacOs);
        let ids = ids(&v.items);
        assert!(!ids.contains(&"reading-agents".to_string()));
        assert!(!ids.contains(&"needs".to_string()));
        let labels: Vec<&str> = v.bar.readings.iter().map(|m| m.label).collect();
        assert_eq!(labels, ["DEV"]);
        assert_eq!(
            view(Some(&r), &Trends::default(), Style::Count, Platform::Linux)
                .title
                .as_deref(),
            Some("2 dev")
        );
        assert_eq!(v.tooltip, "holdmap\n2 dev servers · 12 ports in use");
        assert_eq!(v.badge, Badge::default());
        let data = panel_data(Some(&r), &Trends::default(), Style::Count);
        assert_eq!((data.agents, data.cpu, data.memory), (None, None, None));
        assert!(data.needs.is_none());
    }

    #[test]
    fn badge_counts_needs_at_the_highest_urgency() {
        let v = view(
            Some(&readings()),
            &Trends::default(),
            Style::Icon,
            Platform::Windows,
        );
        assert_eq!(
            v.badge,
            Badge {
                count: 2,
                urgency: Some(Urgency::High)
            }
        );
        assert_eq!(
            v.tooltip,
            "holdmap\n2 dev servers · 12 ports in use\n1 agent\n2 need you"
        );
        let measured = view(
            Some(&readings()),
            &measured(&readings()),
            Style::Icon,
            Platform::Windows,
        );
        assert_eq!(
            measured.tooltip,
            "holdmap\n2 dev servers · 12 ports in use\n1 agent · CPU 12%\n2 need you"
        );
    }

    #[test]
    fn cpu_waits_for_a_measurement_instead_of_showing_zero() {
        let r = readings();
        let mut trends = Trends::default();
        trends.record(&r);
        let primed = view(Some(&r), &trends, Style::Trend, Platform::MacOs);
        assert_eq!(
            label_of(&primed.items, "reading-agents"),
            "1 agent · 300.0 MB"
        );
        assert_eq!(
            label_of(&primed.items, "agent:agent:42"),
            "Claude Code · holdmap · 300.0 MB"
        );
        // An agent that appears later has no CPU figure until its second scan.
        let mut later = r.clone();
        later.agents.as_mut().unwrap().push(AgentRow {
            id: "agent:7".into(),
            name: "Codex CLI".into(),
            place: None,
            cpu: 0.0,
            memory: 1024 * 1024,
        });
        trends.record(&later);
        let v = view(Some(&later), &trends, Style::Count, Platform::MacOs);
        assert_eq!(
            label_of(&v.items, "agent:agent:42"),
            "Claude Code · holdmap   12% · 300.0 MB"
        );
        assert_eq!(label_of(&v.items, "agent:agent:7"), "Codex CLI · 1.0 MB");
        trends.record(&later);
        let v = view(Some(&later), &trends, Style::Count, Platform::MacOs);
        assert_eq!(
            label_of(&v.items, "agent:agent:7"),
            "Codex CLI   0.0% · 1.0 MB"
        );
    }

    #[test]
    fn long_lists_and_labels_are_capped() {
        let mut r = readings();
        r.dev = (3000..3020)
            .map(|port| DevServer {
                port,
                label: "x".repeat(200),
                stoppable: false,
                exposed: false,
            })
            .collect();
        let v = view(Some(&r), &Trends::default(), Style::Count, Platform::MacOs);
        let ids = ids(&v.items);
        assert_eq!(ids.iter().filter(|i| i.starts_with("focus:")).count(), 10);
        assert!(ids.contains(&"more-dev".to_string()));
        // Nothing stoppable: no stop row.
        assert!(!ids.contains(&"stop-all-dev".to_string()));
        assert_eq!(label_of(&v.items, "focus:3000").chars().count(), MAX_LABEL);
        assert!(label_of(&v.items, "focus:3000").ends_with('…'));
        assert_eq!(
            label_of(&v.items, "more-dev"),
            "+10 more dev servers in holdmap…"
        );
    }

    #[test]
    fn labels_keep_a_literal_ampersand_and_drop_control_characters() {
        let mut r = readings();
        r.dev[0].label = "R&D\u{1b}[31m api".into();
        let v = view(Some(&r), &Trends::default(), Style::Count, Platform::MacOs);
        let label = label_of(&v.items, "focus:3000");
        assert!(label.starts_with("● :3000   R&&D"), "{label}");
        assert!(!label.contains('\u{1b}'), "{label}");
    }

    #[test]
    fn the_tooltip_fits_windows_limit_by_whole_lines() {
        let line = "x".repeat(50);
        let tip = fit_lines(vec!["holdmap".into(), line.clone(), line.clone(), line]);
        assert_eq!(tip, format!("holdmap\n{0}\n{0}", "x".repeat(50)));
        assert!(tip.encode_utf16().count() <= MAX_TOOLTIP_UNITS);
    }

    #[test]
    fn sparklines_scale_to_the_peak_but_keep_idle_flat() {
        let s = |v: &[f32]| sparkline(&v.iter().copied().collect());
        assert_eq!(s(&[0.0, 50.0, 100.0]).as_deref(), Some("▁▅█"));
        // Idle is left out rather than drawn as a flat line.
        assert_eq!(s(&[0.1, 0.4, 0.2]), None);
        assert_eq!(s(&[]), None);
        let mut trends = Trends::default();
        for _ in 0..100 {
            trends.record(&readings());
        }
        // The panel's history is longer than a row's sparkline.
        assert_eq!(trends.total.len(), HISTORY_LEN);
        assert_eq!(trends.agents["agent:42"].len(), TREND_LEN);
        assert_eq!(trends.attention.len(), HISTORY_LEN);
        // An agent that stopped is forgotten.
        trends.record(&Readings {
            agents: Some(vec![]),
            ..readings()
        });
        assert!(trends.agents.is_empty());
    }

    #[test]
    fn the_popup_panel_lists_busiest_agents_first_with_their_actions() {
        let mut r = readings();
        r.agents.as_mut().unwrap().extend((0..6).map(|i| AgentRow {
            id: format!("agent:{i}"),
            name: format!("Codex {i}"),
            place: None,
            cpu: i as f32,
            memory: 1024,
        }));
        let mut trends = Trends::default();
        assert!(!panel_data(None, &trends, Style::Count).ready);
        trends.record(&r);
        let mut quiet = r.clone();
        quiet.needs = Some(vec![]);
        trends.record(&quiet);
        let data = panel_data(Some(&r), &trends, Style::Trend);
        assert!(data.ready);
        assert_eq!((data.ports, data.exposed, data.agents), (12, 2, Some(7)));
        assert_eq!(data.style, "trend");
        // Busiest first, five rows, and each opens its agent.
        let names: Vec<&str> = data.top_agents.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            ["Claude Code", "Codex 5", "Codex 4", "Codex 3", "Codex 2"]
        );
        assert_eq!(data.more_agents, 2);
        assert_eq!(data.top_agents[0].action, "agent:agent:42");
        assert_eq!(data.top_agents[0].cpu, Some(12.4));
        // Needs keep their urgency and the action the menu uses.
        let needs = data.needs.unwrap();
        assert_eq!(needs[0].urgency, "high");
        assert_eq!(needs[0].action, "need:0:agent:agent:42");
        assert_eq!(needs[1].urgency, "medium");
        // Dev servers say whether they're reachable from the network.
        let dev: Vec<(u16, bool, &str)> = data
            .dev
            .iter()
            .map(|d| (d.port, d.exposed, d.action.as_str()))
            .collect();
        assert_eq!(
            dev,
            [(3000, false, "focus:3000"), (5173, true, "focus:5173")]
        );
        assert!(data.stop_dev);
        // History: one CPU sample so far (the first scan primes); attention per scan.
        assert_eq!(data.cpu_history.len(), 1);
        assert_eq!(data.attention_history, [2, 0]);
        // Every row's action is one the tray menu accepts.
        for action in needs
            .iter()
            .map(|n| &n.action)
            .chain(data.dev.iter().map(|d| &d.action))
            .chain(data.top_agents.iter().map(|a| &a.action))
        {
            assert!(Action::parse(action).is_some(), "{action}");
        }
        assert_eq!(Action::parse("show-panel"), Some(Action::ShowPanel));
        assert_eq!(Action::parse("hide-panel"), Some(Action::HidePanel));
    }

    #[test]
    fn text_changes_keep_the_rows_so_the_open_menu_updates_in_place() {
        let a = view(
            Some(&readings()),
            &Trends::default(),
            Style::Count,
            Platform::MacOs,
        );
        let trends = measured(&readings());
        let mut busier = readings();
        busier.ports = 13;
        busier.agents.as_mut().unwrap()[0].cpu = 80.0;
        let b = view(Some(&busier), &trends, Style::Trend, Platform::MacOs);
        assert_ne!(a.items, b.items);
        assert!(same_rows(&a.items, &b.items));
        // A new dev server is a new row.
        let mut more = readings();
        more.dev.push(DevServer {
            port: 8080,
            label: "api".into(),
            stoppable: true,
            exposed: false,
        });
        let c = view(Some(&more), &trends, Style::Count, Platform::MacOs);
        assert!(!same_rows(&a.items, &c.items));
    }

    #[test]
    fn readings_come_from_the_scan() {
        let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
            "entries": [
                {
                    "id": "tcp:3000", "port": 3000, "protocol": "tcp", "state": "listen",
                    "addresses": ["0.0.0.0"], "families": [], "remote": null,
                    "exposure": "all_interfaces", "pid": 1, "pids": [1], "uid": null, "user": null,
                    "process": null, "project": null, "framework": null, "container": null,
                    "label": "svc", "is_dev": true, "is_mine": true, "protected": true, "tunnel": null
                }
            ],
            "hidden_sockets": 0, "platform": "linux", "taken_at_ms": 1, "scan_ms": 1,
            "docker_available": false, "warnings": []
        }))
        .expect("fixture snapshot");
        let r = Readings::of(&snapshot, None, None);
        assert_eq!(
            r,
            Readings {
                ports: 1,
                exposed: 1,
                dev: vec![DevServer {
                    port: 3000,
                    label: "svc".into(),
                    stoppable: false,
                    exposed: true,
                }],
                agents: None,
                needs: None,
            }
        );
    }
}

//! What the menu-bar / tray widget shows, as plain data: the readings beside the icon, the
//! native menu, the tooltip, the attention badge and the data for the popup panel. Pure, so
//! every platform's output is unit-tested; `mod.rs` applies it with Tauri.
//!
//! Platforms differ in what a tray can show, and the model only asks for what each one draws:
//! macOS draws stacked mini readings into the menu-bar image and has a tooltip; Linux
//! (AppIndicator) shows the readings as a panel label but no tooltip and opens the menu on any
//! click; Windows shows a tooltip but no text, so its figures live in the tooltip and the badge.

use holdmap_core::agents::{Agent, AgentKind, AgentsReport};
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
/// Agents the menu bar's bar chart shows.
const BAR_AGENTS: usize = 8;
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
}

/// One menu-bar item, as `Config::TRAY_ITEMS` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Item {
    /// holdmap's icon, with a dot when something needs you.
    Icon,
    Agents,
    Needs,
    /// The agents' CPU together.
    Cpu,
    /// The agents' RAM together.
    Ram,
    Dev,
    /// A line of the agents' CPU over recent scans.
    CpuHistory,
    /// One bar per running agent.
    AgentBars,
}

impl Item {
    pub const ALL: [Item; 8] = [
        Item::Icon,
        Item::Agents,
        Item::Needs,
        Item::Cpu,
        Item::Ram,
        Item::Dev,
        Item::CpuHistory,
        Item::AgentBars,
    ];

    pub fn parse(id: &str) -> Option<Item> {
        Self::ALL.into_iter().find(|i| i.id() == id)
    }

    /// Its id in `Config::tray_items`.
    pub fn id(self) -> &'static str {
        match self {
            Item::Icon => "icon",
            Item::Agents => "agents",
            Item::Needs => "needs",
            Item::Cpu => "cpu",
            Item::Ram => "ram",
            Item::Dev => "dev",
            Item::CpuHistory => "cpu_history",
            Item::AgentBars => "agent_bars",
        }
    }

    /// Its name in Settings and the menu.
    pub fn name(self) -> &'static str {
        match self {
            Item::Icon => "holdmap icon",
            Item::Agents => "Agents",
            Item::Needs => "Alerts",
            Item::Cpu => "Agent CPU",
            Item::Ram => "Agent RAM",
            Item::Dev => "Dev servers",
            Item::CpuHistory => "CPU history",
            Item::AgentBars => "Agent bars",
        }
    }

    /// The panel section a click on it opens.
    pub fn section(self) -> &'static str {
        match self {
            Item::Icon => "top",
            Item::Agents | Item::AgentBars | Item::Ram => "agents",
            Item::Needs => "needs",
            Item::Cpu | Item::CpuHistory => "cpu",
            Item::Dev => "dev",
        }
    }

    /// Whether this desktop can show it: Linux's panel label holds text only, and Windows tray
    /// icons hold no text at all.
    pub fn shown_on(self, platform: Platform) -> bool {
        match platform {
            Platform::MacOs => true,
            Platform::Linux => !matches!(self, Item::Icon | Item::AgentBars),
            Platform::Windows => false,
        }
    }
}

/// What the menu bar shows: which items, whether as one item, and whether compact (macOS).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarChoice {
    pub items: Vec<Item>,
    pub combined: bool,
    /// One item: the icon (its dot stands for alerts) and at most [`COMPACT_READINGS`] of the
    /// chosen readings. The full choice is kept for when compact is turned off.
    pub compact: bool,
}

/// Readings a compact menu bar shows beside the icon (`Config::COMPACT_READINGS`).
pub const COMPACT_READINGS: usize = holdmap_core::store::Config::COMPACT_READINGS;

impl BarChoice {
    /// From `Config::tray_items()` (already validated there), `Config::tray_combined` and
    /// `Config::tray_compact`.
    pub fn from_config(items: &[&str], combined: bool, compact: bool) -> BarChoice {
        BarChoice {
            items: items.iter().filter_map(|id| Item::parse(id)).collect(),
            combined,
            compact,
        }
    }

    /// The readings a compact item shows beside the icon, in menu-bar order. The icon's dot
    /// already says when something needs you, so Alerts is not repeated as a number.
    pub fn compact_readings(&self) -> Vec<Item> {
        self.items
            .iter()
            .copied()
            .filter(|i| !matches!(i, Item::Icon | Item::Needs))
            .take(COMPACT_READINGS)
            .collect()
    }
}

impl Default for BarChoice {
    fn default() -> BarChoice {
        BarChoice {
            items: vec![Item::Agents, Item::Needs],
            combined: false,
            compact: true,
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
    /// A developer tool that sits beside agents (Docker Desktop), not an AI agent: listed on its
    /// own and left out of the agents' count, CPU and RAM.
    pub tool: bool,
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
    /// Every listener's entry id, to count what opened and closed since the last scan.
    pub port_ids: Vec<String>,
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
            port_ids: entries.iter().map(|e| e.id.clone()).collect(),
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
                        place: agent_place(a, &r.agents),
                        cpu: a.cpu_percent,
                        memory: a.memory_bytes,
                        tool: a.kind == AgentKind::Tool,
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

    /// The AI agents (developer tools left out), once the agents report is read.
    pub fn ai_agents(&self) -> Option<Vec<&AgentRow>> {
        self.agents
            .as_ref()
            .map(|a| a.iter().filter(|a| !a.tool).collect())
    }

    fn cpu(&self) -> Option<f32> {
        // Folded from +0.0: an empty float `sum()` is -0.0, which would print as "-0%".
        self.ai_agents()
            .map(|a| a.iter().fold(0.0, |total, a| total + a.cpu))
    }

    /// The AI agents' RAM together, as the Agents view counts it.
    fn ram(&self) -> Option<u64> {
        self.ai_agents().map(|a| a.iter().map(|a| a.memory).sum())
    }
}

/// What tells an agent apart in a row: its first folder, else its own working folder's name
/// (not the home folder or `/`), else the agent it was started from, else its PID when another
/// agent shares its name.
fn agent_place(a: &Agent, all: &[Agent]) -> Option<String> {
    if let Some(folder) = a.folders.first() {
        return Some(folder.label.clone());
    }
    let home = dirs_home();
    let cwd = a
        .processes
        .iter()
        .find(|p| p.pid == a.pid)
        .and_then(|p| p.cwd.as_deref())
        .filter(|cwd| cwd.parent().is_some() && home.as_deref() != Some(*cwd))
        .and_then(|cwd| cwd.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    if cwd.is_some() {
        return cwd;
    }
    if let Some(parent) = a
        .parent
        .as_ref()
        .and_then(|id| all.iter().find(|b| &b.id == id))
    {
        return Some(format!("in {}", parent.name));
    }
    let twins = all.iter().filter(|b| b.name == a.name).count();
    (twins > 1).then(|| format!("PID {}", a.pid))
}

/// The account's home folder, so a working folder that is just `~` isn't shown as a project.
fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
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
    /// Listeners opened and closed per scan.
    churn: VecDeque<[u32; 2]>,
    /// Listeners in the previous scan (`None` before the first one).
    ports_seen: Option<HashSet<String>>,
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
        let now: HashSet<String> = readings.port_ids.iter().cloned().collect();
        if let Some(before) = &self.ports_seen {
            let opened = now.difference(before).count() as u32;
            let closed = before.difference(&now).count() as u32;
            push(&mut self.churn, [opened, closed], HISTORY_LEN);
        }
        self.ports_seen = Some(now);
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
    /// Show or hide one menu-bar item.
    ToggleItem(Item),
    /// Show the menu-bar items as one item, or each on its own (macOS).
    ToggleCombined,
    /// The compact menu bar (icon and two readings), or every chosen item (macOS).
    ToggleCompact,
    /// Keep the panel open when it loses focus, or let it close again.
    PinPanel,
    /// Keep holdmap in the Dock, or run it from the menu bar only (macOS).
    ToggleDock,
    /// Turn desktop notifications on or off.
    ToggleNotify,
    /// Open the window's Settings.
    OpenSettings,
    /// The app's theme: `system`, `light` or `dark`.
    SetTheme(&'static str),
}

impl Action {
    pub fn parse(id: &str) -> Option<Action> {
        let action = match id {
            "show" | "more-needs" | "more-dev" | "more-agents" => Action::Show,
            "show-panel" => Action::ShowPanel,
            "hide-panel" => Action::HidePanel,
            "pin-panel" => Action::PinPanel,
            "combined" => Action::ToggleCombined,
            "compact" => Action::ToggleCompact,
            "dock" => Action::ToggleDock,
            "notify" => Action::ToggleNotify,
            "settings" => Action::OpenSettings,
            "theme:system" => Action::SetTheme("system"),
            "theme:light" => Action::SetTheme("light"),
            "theme:dark" => Action::SetTheme("dark"),
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
                let item = id.strip_prefix("item:")?;
                return Item::parse(item).map(Action::ToggleItem);
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
    /// The menu-bar items (macOS), each drawn on its own.
    pub bar: Vec<BarItem>,
    /// The panel label beside the icon (Linux); `None` shows the icon alone.
    pub title: Option<String>,
    /// Hover text (macOS, Windows).
    pub tooltip: String,
    pub badge: Badge,
}

fn more(id: &str, n: usize, what: &str) -> Option<TrayItem> {
    (n > 0).then(|| item(id, format!("+{n} more {what} in holdmap…"), true))
}

fn panel(
    r: Option<&Readings>,
    trends: &Trends,
    choice: &BarChoice,
    platform: Platform,
) -> Vec<TrayItem> {
    let mut out = Vec::new();
    let Some(r) = r else {
        out.push(reading("reading-ports", "Scanning…"));
        out.push(TrayItem::Separator);
        out.extend(controls(choice, platform));
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
    if let Some(agents) = r.ai_agents() {
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
            let largest = (agents.len() > 1)
                .then(|| agents.iter().max_by_key(|agent| agent.memory))
                .flatten()
                .map(|agent| format!(" · {} uses the most", agent.name))
                .unwrap_or_default();
            format!(
                "{}{} · {}{largest}",
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

    let tools: Vec<&AgentRow> = r.agents.iter().flatten().filter(|a| a.tool).collect();
    if let Some(agents) = r.ai_agents().filter(|a| !a.is_empty()) {
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
    for t in tools.iter().take(MAX_AGENTS) {
        out.push(item(
            format!("agent:{}", t.id),
            clip(&format!(
                "{} (developer tool) · {}",
                t.name,
                human_bytes(t.memory)
            )),
            true,
        ));
    }

    out.push(TrayItem::Separator);
    out.extend(controls(choice, platform));
    out
}

fn controls(choice: &BarChoice, platform: Platform) -> Vec<TrayItem> {
    let mut out = vec![
        item("show-panel", "Show panel", true),
        item("show", "Open holdmap", true),
        item("settings", "Settings…", true),
    ];
    if platform != Platform::Windows {
        let mut items: Vec<TrayItem> = Item::ALL
            .into_iter()
            .filter(|i| i.shown_on(platform))
            .map(|i| TrayItem::Check {
                id: format!("item:{}", i.id()),
                label: i.name().into(),
                checked: choice.items.contains(&i),
            })
            .collect();
        if platform == Platform::MacOs {
            items.push(TrayItem::Separator);
            items.push(TrayItem::Check {
                id: "compact".into(),
                label: "Compact (icon and two readings)".into(),
                checked: choice.compact,
            });
            items.push(TrayItem::Check {
                id: "combined".into(),
                label: "As one item".into(),
                checked: choice.combined,
            });
        }
        out.push(TrayItem::Submenu {
            id: "bar".into(),
            label: if platform == Platform::MacOs {
                "Menu bar shows".into()
            } else {
                "Panel shows".into()
            },
            items,
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
    if n == 1 {
        "1 alert needs you".into()
    } else {
        format!("{n} alerts need you")
    }
}

/// How a reading is coloured in the menu bar: plain ink, or a state worth seeing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Warning,
    Critical,
}

/// One part of a menu-bar item.
#[derive(Debug, Clone, PartialEq)]
pub enum Draw {
    /// holdmap's icon, with a dot when something needs you.
    Icon(Badge),
    /// A small label over a value, as system monitors show readings.
    Reading {
        label: &'static str,
        value: String,
        tone: Tone,
    },
    /// The agents' CPU over recent scans, oldest first.
    Line(Vec<f32>),
    /// One bar per running agent.
    Bars(Vec<f32>),
}

impl Draw {
    /// Whether it needs colour (otherwise it's drawn as a template that follows the menu bar).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn coloured(&self) -> bool {
        match self {
            Draw::Icon(badge) => badge.count > 0,
            Draw::Reading { tone, .. } => *tone != Tone::Plain,
            Draw::Line(_) | Draw::Bars(_) => false,
        }
    }
}

/// One menu-bar item (macOS): its parts, its hover text and the panel section it opens.
#[derive(Debug, Clone, PartialEq)]
pub struct BarItem {
    /// An [`Item::id`], or `combined`.
    pub id: &'static str,
    pub parts: Vec<Draw>,
    pub tooltip: String,
    pub section: &'static str,
}

impl Eq for BarItem {}

/// "7%", or "<1%" for a little but not nothing.
fn bar_cpu(cpu: f32) -> String {
    if cpu > 0.0 && cpu < 0.5 {
        "<1%".into()
    } else {
        format!("{cpu:.0}%")
    }
}

/// "2.4G", "512M": bytes short enough for a menu-bar reading.
fn bar_bytes(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= 10.0 * GB {
        format!("{:.0}G", b / GB)
    } else if b >= GB {
        format!("{:.1}G", b / GB)
    } else {
        format!("{:.0}M", b / MB)
    }
}

/// A reading's value, or "…" until it's known.
fn known(value: Option<String>) -> String {
    value.unwrap_or_else(|| "…".into())
}

/// One item's part, its hover text and, for the Linux label, its words (`None`: no words).
fn part(item: Item, r: Option<&Readings>, trends: &Trends) -> (Draw, String, Option<String>) {
    let agents = r.and_then(Readings::ai_agents);
    let agents = agents.as_ref();
    let needs = r.and_then(|r| r.needs.as_ref());
    let cpu = r.and_then(|r| trends.total_cpu(r));
    match item {
        Item::Icon => (Draw::Icon(badge(r)), "holdmap".into(), None),
        Item::Agents => (
            Draw::Reading {
                label: "AGENTS",
                value: known(agents.map(|a| a.len().to_string())),
                tone: Tone::Plain,
            },
            known(agents.map(|a| count(a.len(), "agent", "agents") + " running")),
            agents.map(|a| count(a.len(), "agent", "agents")),
        ),
        Item::Needs => {
            let tone = match needs.and_then(|n| n.iter().map(|n| n.urgency).max()) {
                Some(Urgency::High) => Tone::Critical,
                Some(Urgency::Medium) => Tone::Warning,
                None => Tone::Plain,
            };
            (
                Draw::Reading {
                    label: "ALERTS",
                    value: known(needs.map(|n| n.len().to_string())),
                    tone,
                },
                match needs.map(Vec::len) {
                    Some(0) => "Nothing needs you".into(),
                    Some(n) => needs_text(n),
                    None => "Checking what needs you".into(),
                },
                needs.map(|n| needs_text(n.len())),
            )
        }
        Item::Cpu => (
            Draw::Reading {
                label: "CPU",
                value: known(cpu.map(bar_cpu)),
                tone: match cpu {
                    Some(c) if c >= 80.0 => Tone::Critical,
                    Some(c) if c >= 60.0 => Tone::Warning,
                    _ => Tone::Plain,
                },
            },
            known(cpu.map(|c| format!("Agent CPU {}", cpu_text(c)))),
            cpu.map(|c| format!("CPU {}", bar_cpu(c))),
        ),
        Item::Ram => {
            let ram = r.and_then(Readings::ram);
            (
                Draw::Reading {
                    label: "RAM",
                    value: known(ram.map(bar_bytes)),
                    tone: Tone::Plain,
                },
                known(ram.map(|b| format!("Agent RAM {}", human_bytes(b)))),
                ram.map(|b| format!("RAM {}", human_bytes(b))),
            )
        }
        Item::Dev => {
            let dev = r.map(|r| r.dev.len());
            (
                Draw::Reading {
                    label: "SERVERS",
                    value: known(dev.map(|n| n.to_string())),
                    tone: Tone::Plain,
                },
                known(dev.map(|n| count(n, "dev server", "dev servers") + " running")),
                dev.map(|n| format!("{n} dev")),
            )
        }
        Item::CpuHistory => {
            let samples: VecDeque<f32> = trends.total.iter().copied().collect();
            (
                Draw::Line(samples.iter().copied().collect()),
                "Agent CPU over recent scans".into(),
                sparkline(&recent(&samples, TREND_LEN)),
            )
        }
        Item::AgentBars => {
            let values: Vec<f32> = agents
                .map(|a| {
                    a.iter()
                        .take(BAR_AGENTS)
                        .map(|a| trends.agents.get(&a.id).map_or(0.0, |_| a.cpu))
                        .collect()
                })
                .unwrap_or_default();
            (Draw::Bars(values), "CPU of each running agent".into(), None)
        }
    }
}

/// The menu-bar items (macOS): one per chosen item, or all of them as one.
fn bar_items(r: Option<&Readings>, trends: &Trends, choice: &BarChoice) -> Vec<BarItem> {
    if choice.compact {
        let readings = choice.compact_readings();
        let mut parts = vec![Draw::Icon(badge(r))];
        let mut lines = vec!["holdmap".to_string()];
        for &item in &readings {
            let (draw, tooltip, _) = part(item, r, trends);
            parts.push(draw);
            lines.push(tooltip);
        }
        // The dot's meaning, in words: it never shows a count.
        lines.push(match r.and_then(|r| r.needs.as_ref()).map(Vec::len) {
            Some(0) => "Nothing needs you".into(),
            Some(n) => needs_text(n),
            None => "Checking what needs you".into(),
        });
        return vec![BarItem {
            id: "compact",
            parts,
            tooltip: fit_lines(lines),
            section: "top",
        }];
    }
    let made: Vec<(Item, Draw, String)> = choice
        .items
        .iter()
        .map(|&item| {
            let (draw, tooltip, _) = part(item, r, trends);
            (item, draw, tooltip)
        })
        .collect();
    if choice.combined {
        let lines: Vec<String> = std::iter::once("holdmap".to_string())
            .chain(
                made.iter()
                    .filter(|(i, ..)| *i != Item::Icon)
                    .map(|(_, _, t)| t.clone()),
            )
            .collect();
        return vec![BarItem {
            id: "combined",
            parts: made.into_iter().map(|(_, d, _)| d).collect(),
            tooltip: fit_lines(lines),
            section: "top",
        }];
    }
    made.into_iter()
        .map(|(item, draw, tooltip)| BarItem {
            id: item.id(),
            parts: vec![draw],
            tooltip: if item == Item::Icon {
                tooltip
            } else {
                format!("holdmap · {tooltip}")
            },
            section: item.section(),
        })
        .collect()
}

/// The Linux panel label: the chosen readings as a sentence.
fn label_text(r: Option<&Readings>, trends: &Trends, choice: &BarChoice) -> Option<String> {
    let parts: Vec<String> = choice
        .items
        .iter()
        .filter(|i| i.shown_on(Platform::Linux))
        .filter_map(|&i| part(i, r, trends).2)
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
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
        if let Some(agents) = r.ai_agents().filter(|a| !a.is_empty()) {
            let cpu = trends
                .total_cpu(r)
                .map(|cpu| format!(" · CPU {}", cpu_text(cpu)));
            let ram = r
                .ram()
                .map(|b| format!(" · RAM {}", human_bytes(b)))
                .unwrap_or_default();
            lines.push(format!(
                "{}{}{ram}",
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
    choice: &BarChoice,
    platform: Platform,
) -> TrayView {
    TrayView {
        items: panel(readings, trends, choice, platform),
        title: if platform == Platform::Linux {
            label_text(readings, trends, choice)
        } else {
            None
        },
        bar: if platform == Platform::MacOs {
            bar_items(readings, trends, choice)
        } else {
            Vec::new()
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

/// The panel's quick settings: what the menu bar shows and the switches Settings also offers.
/// Each one carries the [`Action`] id that flips it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelSettings {
    /// What holds the readings: "Menu bar" (macOS) or "Panel" (Linux); `None` on Windows,
    /// whose tray icon holds no readings.
    pub bar: Option<&'static str>,
    /// The items this desktop can show, in menu-bar order.
    pub items: Vec<PanelToggle>,
    /// The items as one menu-bar item (macOS only).
    pub combined: Option<bool>,
    /// The compact menu bar: the icon and at most two readings (macOS only).
    pub compact: Option<bool>,
    /// holdmap's icon in the Dock (macOS only).
    pub dock: Option<bool>,
    pub notify: bool,
}

/// One menu-bar item in the quick settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelToggle {
    pub name: &'static str,
    pub on: bool,
    pub action: String,
}

/// The quick settings for this desktop.
pub fn panel_settings(
    choice: &BarChoice,
    dock: bool,
    notify: bool,
    platform: Platform,
) -> PanelSettings {
    let mac = platform == Platform::MacOs;
    PanelSettings {
        bar: match platform {
            Platform::MacOs => Some("Menu bar"),
            Platform::Linux => Some("Panel"),
            Platform::Windows => None,
        },
        items: Item::ALL
            .into_iter()
            .filter(|i| i.shown_on(platform))
            .map(|i| PanelToggle {
                name: i.name(),
                on: choice.items.contains(&i),
                action: format!("item:{}", i.id()),
            })
            .collect(),
        combined: mac.then_some(choice.combined),
        compact: mac.then_some(choice.compact),
        dock: mac.then_some(dock),
        notify,
    }
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
    /// The AI agents using the most RAM first (a steady order: CPU changes every scan).
    pub top_agents: Vec<PanelAgent>,
    pub more_agents: usize,
    /// Developer tools running beside the agents (Docker Desktop), largest first. Not agents:
    /// left out of the count, CPU and RAM.
    pub tools: Vec<PanelAgent>,
    /// Listeners opened and closed per scan, oldest first.
    pub churn: Vec<[u32; 2]>,
    /// The section to bring into view (the menu-bar item clicked), and a count that changes
    /// with every click so the same section can be brought back.
    pub focus: Option<&'static str>,
    pub focus_seq: u64,
    /// The panel stays open when it loses focus.
    pub pinned: bool,
    pub settings: PanelSettings,
}

/// What the panel window adds to the data: the clicked item's section and the pin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PanelState {
    pub focus: Option<&'static str>,
    pub focus_seq: u64,
    pub pinned: bool,
}

/// The popup panel's data for `readings` (`None` before the first scan).
pub fn panel_data(
    readings: Option<&Readings>,
    trends: &Trends,
    state: PanelState,
    settings: PanelSettings,
) -> PanelData {
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
            tools: Vec::new(),
            churn: Vec::new(),
            focus: state.focus,
            focus_seq: state.focus_seq,
            pinned: state.pinned,
            settings,
        };
    };
    let by_ram = |a: &&AgentRow, b: &&AgentRow| {
        b.memory
            .cmp(&a.memory)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    };
    let mut agents: Vec<&AgentRow> = r.ai_agents().unwrap_or_default();
    agents.sort_by(by_ram);
    let mut tools: Vec<&AgentRow> = r.agents.iter().flatten().filter(|a| a.tool).collect();
    tools.sort_by(by_ram);
    let row = |a: &AgentRow| PanelAgent {
        name: printable(&a.name).into_owned(),
        place: a.place.as_deref().map(|p| printable(p).into_owned()),
        cpu: trends.agents.get(&a.id).map(|_| a.cpu),
        memory: a.memory,
        action: format!("agent:{}", a.id),
    };
    PanelData {
        ready: true,
        ports: r.ports,
        exposed: r.exposed,
        agents: r.ai_agents().map(|a| a.len()),
        cpu: trends.total_cpu(r),
        memory: r.ram(),
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
        top_agents: agents.into_iter().take(PANEL_AGENTS).map(&row).collect(),
        tools: tools.into_iter().take(PANEL_AGENTS).map(row).collect(),
        churn: trends.churn.iter().copied().collect(),
        focus: state.focus,
        focus_seq: state.focus_seq,
        pinned: state.pinned,
        settings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readings() -> Readings {
        Readings {
            ports: 12,
            port_ids: vec!["tcp:3000".into(), "tcp:5173".into()],
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
                tool: false,
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
        let v = view(
            Some(&readings()),
            &trends,
            &BarChoice::default(),
            Platform::MacOs,
        );
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
                "settings",
                "bar",
                "item:icon",
                "item:agents",
                "item:needs",
                "item:cpu",
                "item:ram",
                "item:dev",
                "item:cpu_history",
                "item:agent_bars",
                "compact",
                "combined",
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
            Action::parse("item:cpu_history"),
            Some(Action::ToggleItem(Item::CpuHistory))
        );
        assert_eq!(Action::parse("combined"), Some(Action::ToggleCombined));
        assert_eq!(Action::parse("pin-panel"), Some(Action::PinPanel));
        assert_eq!(Action::parse("settings"), Some(Action::OpenSettings));
        assert_eq!(Action::parse("dock"), Some(Action::ToggleDock));
        assert_eq!(Action::parse("notify"), Some(Action::ToggleNotify));
        assert_eq!(Action::parse("theme:dark"), Some(Action::SetTheme("dark")));
        assert_eq!(Action::parse("theme:sepia"), None);
        assert_eq!(Action::parse("item:rainbow"), None);
        assert_eq!(Action::parse("focus:99999"), None);
    }

    #[test]
    fn each_chosen_item_is_its_own_menu_bar_item_like_a_system_monitor_s() {
        let r = readings();
        let mut trends = Trends::default();
        let choice = |items: &[Item], combined| BarChoice {
            items: items.to_vec(),
            combined,
            compact: false,
        };
        let bar = |trends: &Trends, c: &BarChoice| view(Some(&r), trends, c, Platform::MacOs).bar;
        let reading = |b: &BarItem| match &b.parts[..] {
            [Draw::Reading { label, value, tone }] => format!("{label} {value} {tone:?}"),
            other => format!("{other:?}"),
        };
        // Not compact: Agents and Alerts, one item each, each opening its own section.
        let items = bar(&trends, &choice(&[Item::Agents, Item::Needs], false));
        assert_eq!(
            items.iter().map(reading).collect::<Vec<_>>(),
            ["AGENTS 1 Plain", "ALERTS 2 Critical"]
        );
        assert_eq!(
            items.iter().map(|b| (b.id, b.section)).collect::<Vec<_>>(),
            [("agents", "agents"), ("needs", "needs")]
        );
        assert_eq!(items[1].tooltip, "holdmap · 2 alerts need you");
        // CPU waits for its first measurement, then is coloured by load.
        let cpu = choice(&[Item::Cpu], false);
        assert_eq!(reading(&bar(&trends, &cpu)[0]), "CPU … Plain");
        trends.record(&r);
        trends.record(&r);
        assert_eq!(reading(&bar(&trends, &cpu)[0]), "CPU 12% Plain");
        let mut busy = r.clone();
        busy.agents.as_mut().unwrap()[0].cpu = 85.0;
        assert_eq!(
            reading(&view(Some(&busy), &trends, &cpu, Platform::MacOs).bar[0]),
            "CPU 85% Critical"
        );
        // Charts: the CPU history and one bar per agent.
        let charts = bar(
            &trends,
            &choice(&[Item::CpuHistory, Item::AgentBars], false),
        );
        assert_eq!(charts[0].parts, [Draw::Line(vec![12.4])]);
        assert_eq!(charts[1].parts, [Draw::Bars(vec![12.4])]);
        // As one item: every part side by side under one hover text.
        let one = bar(
            &trends,
            &choice(&[Item::Icon, Item::Agents, Item::Dev], true),
        );
        assert_eq!(one.len(), 1);
        assert_eq!((one[0].id, one[0].section), ("combined", "top"));
        assert_eq!(one[0].parts.len(), 3);
        assert_eq!(
            one[0].tooltip,
            "holdmap\n1 agent running\n2 dev servers running"
        );
        // Linux shows the readings as its label (a trend once there are two samples);
        // Windows has neither.
        trends.record(&r);
        let all = choice(&Item::ALL, false);
        let linux = view(Some(&r), &trends, &all, Platform::Linux);
        assert!(linux.bar.is_empty());
        assert_eq!(
            linux.title.as_deref(),
            Some("1 agent · 2 alerts need you · CPU 12% · RAM 300.0 MB · 2 dev · ██")
        );
        let windows = view(Some(&r), &trends, &all, Platform::Windows);
        assert_eq!((windows.title, windows.bar.len()), (None, 0));
        assert_eq!(bar_cpu(0.2), "<1%");
        assert_eq!(bar_cpu(0.0), "0%");
        assert_eq!(bar_cpu(7.4), "7%");
    }

    #[test]
    fn the_compact_menu_bar_is_one_item_the_icon_and_two_readings() {
        let r = readings();
        let mut trends = Trends::default();
        trends.record(&r);
        trends.record(&r);
        // The default: the icon (its dot says something needs you) and the agents' count.
        let one = view(Some(&r), &trends, &BarChoice::default(), Platform::MacOs).bar;
        assert_eq!(one.len(), 1);
        assert_eq!((one[0].id, one[0].section), ("compact", "top"));
        assert!(matches!(
            one[0].parts[0],
            Draw::Icon(Badge { count: 2, .. })
        ));
        assert!(matches!(
            &one[0].parts[1..],
            [Draw::Reading {
                label: "AGENTS",
                ..
            }]
        ));
        assert_eq!(
            one[0].tooltip,
            "holdmap\n1 agent running\n2 alerts need you"
        );
        // Everything chosen still fits: the first two readings, Alerts folded into the dot.
        let all = BarChoice {
            items: Item::ALL.to_vec(),
            combined: false,
            compact: true,
        };
        let labels: Vec<&str> = view(Some(&r), &trends, &all, Platform::MacOs).bar[0]
            .parts
            .iter()
            .filter_map(|p| match p {
                Draw::Reading { label, .. } => Some(*label),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["AGENTS", "CPU"]);
        assert_eq!(all.compact_readings(), [Item::Agents, Item::Cpu]);
        // Only the icon chosen: the icon alone.
        let icon = BarChoice {
            items: vec![Item::Icon],
            combined: false,
            compact: true,
        };
        assert_eq!(
            view(Some(&r), &trends, &icon, Platform::MacOs).bar[0]
                .parts
                .len(),
            1
        );
        // RAM reads short enough for the menu bar.
        assert_eq!(bar_bytes(300 * 1024 * 1024), "300M");
        assert_eq!(bar_bytes(2_576_980_378), "2.4G");
        assert_eq!(bar_bytes(12 * 1024 * 1024 * 1024), "12G");
        assert_eq!(Action::parse("compact"), Some(Action::ToggleCompact));
    }

    #[test]
    fn before_the_first_scan_the_items_hold_their_places_with_dots() {
        let v = view(
            None,
            &Trends::default(),
            &BarChoice {
                compact: false,
                ..BarChoice::default()
            },
            Platform::MacOs,
        );
        assert_eq!(
            v.bar.len(),
            2,
            "claimed at launch, filled by the first scan"
        );
        assert!(v.bar.iter().all(|b| matches!(
            &b.parts[..],
            [Draw::Reading { value, .. }] if value == "…"
        )));
    }

    #[test]
    fn the_menu_ticks_the_chosen_items_and_is_absent_on_windows() {
        let trends = Trends::default();
        let checked = |v: &TrayView| -> Vec<String> {
            flatten(&v.items)
                .into_iter()
                .filter_map(|i| match i {
                    TrayItem::Check {
                        id, checked: true, ..
                    } => Some(id.clone()),
                    _ => None,
                })
                .collect()
        };
        let mac = view(
            Some(&readings()),
            &trends,
            &BarChoice {
                items: vec![Item::Agents, Item::CpuHistory],
                combined: true,
                compact: false,
            },
            Platform::MacOs,
        );
        assert_eq!(
            checked(&mac),
            ["item:agents", "item:cpu_history", "combined"]
        );
        assert_eq!(label_of(&mac.items, "bar"), "Menu bar shows");
        // Linux: text items only, no "as one item" (it always has one indicator).
        let linux = view(
            Some(&readings()),
            &trends,
            &BarChoice::default(),
            Platform::Linux,
        );
        assert_eq!(label_of(&linux.items, "bar"), "Panel shows");
        let linux_ids = ids(&linux.items);
        assert!(!linux_ids.contains(&"item:icon".to_string()));
        assert!(!linux_ids.contains(&"item:agent_bars".to_string()));
        assert!(!linux_ids.contains(&"combined".to_string()));
        let win = view(
            Some(&readings()),
            &trends,
            &BarChoice::default(),
            Platform::Windows,
        );
        assert!(!ids(&win.items).iter().any(|id| id.starts_with("item:")));
    }

    #[test]
    fn before_the_first_scan_it_says_so_and_still_offers_the_controls() {
        let v = view(
            None,
            &Trends::default(),
            &BarChoice::default(),
            Platform::MacOs,
        );
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
        let v = view(
            Some(&r),
            &Trends::default(),
            &BarChoice {
                compact: false,
                ..BarChoice::default()
            },
            Platform::MacOs,
        );
        let ids = ids(&v.items);
        assert!(!ids.contains(&"reading-agents".to_string()));
        assert!(!ids.contains(&"needs".to_string()));
        // Unknown agents and needs read "…", never 0.
        assert!(v.bar.iter().all(|b| matches!(
            &b.parts[..],
            [Draw::Reading { value, .. }] if value == "…"
        )));
        assert_eq!(
            view(
                Some(&r),
                &Trends::default(),
                &BarChoice {
                    items: vec![Item::Agents, Item::Dev],
                    combined: false,
                    compact: false,
                },
                Platform::Linux
            )
            .title
            .as_deref(),
            Some("2 dev")
        );
        assert_eq!(v.tooltip, "holdmap\n2 dev servers · 12 ports in use");
        assert_eq!(v.badge, Badge::default());
        let data = panel_data(Some(&r), &Trends::default(), PanelState::default(), quick());
        assert_eq!((data.agents, data.cpu, data.memory), (None, None, None));
        assert!(data.needs.is_none());
    }

    #[test]
    fn badge_counts_needs_at_the_highest_urgency() {
        let v = view(
            Some(&readings()),
            &Trends::default(),
            &BarChoice::default(),
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
            "holdmap\n2 dev servers · 12 ports in use\n1 agent · RAM 300.0 MB\n2 alerts need you"
        );
        let measured = view(
            Some(&readings()),
            &measured(&readings()),
            &BarChoice::default(),
            Platform::Windows,
        );
        assert_eq!(
            measured.tooltip,
            "holdmap\n2 dev servers · 12 ports in use\n1 agent · CPU 12% · RAM 300.0 MB\n2 alerts need you"
        );
    }

    #[test]
    fn cpu_waits_for_a_measurement_instead_of_showing_zero() {
        let r = readings();
        let mut trends = Trends::default();
        trends.record(&r);
        let primed = view(Some(&r), &trends, &BarChoice::default(), Platform::MacOs);
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
            tool: false,
        });
        trends.record(&later);
        let v = view(
            Some(&later),
            &trends,
            &BarChoice::default(),
            Platform::MacOs,
        );
        assert_eq!(
            label_of(&v.items, "agent:agent:42"),
            "Claude Code · holdmap   12% · 300.0 MB"
        );
        assert!(
            label_of(&v.items, "reading-agents").contains("Claude Code uses the most"),
            "{}",
            label_of(&v.items, "reading-agents")
        );
        assert_eq!(label_of(&v.items, "agent:agent:7"), "Codex CLI · 1.0 MB");
        trends.record(&later);
        let v = view(
            Some(&later),
            &trends,
            &BarChoice::default(),
            Platform::MacOs,
        );
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
        let v = view(
            Some(&r),
            &Trends::default(),
            &BarChoice::default(),
            Platform::MacOs,
        );
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
        let v = view(
            Some(&r),
            &Trends::default(),
            &BarChoice::default(),
            Platform::MacOs,
        );
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
    fn the_popup_panel_lists_agents_by_ram_with_their_actions_and_tools_apart() {
        let mut r = readings();
        r.agents.as_mut().unwrap().extend((0..6).map(|i| AgentRow {
            id: format!("agent:{i}"),
            name: format!("Codex {i}"),
            place: None,
            cpu: 6.0 - i as f32,
            memory: 1024 * (i + 1),
            tool: false,
        }));
        r.agents.as_mut().unwrap().push(AgentRow {
            id: "agent:900".into(),
            name: "Docker Desktop".into(),
            place: Some("Data".into()),
            cpu: 30.0,
            memory: 600 * 1024 * 1024,
            tool: true,
        });
        let mut trends = Trends::default();
        assert!(!panel_data(None, &trends, PanelState::default(), quick()).ready);
        trends.record(&r);
        let mut quiet = r.clone();
        quiet.needs = Some(vec![]);
        trends.record(&quiet);
        let data = panel_data(Some(&r), &trends, PanelState::default(), quick());
        assert!(data.ready);
        assert_eq!((data.ports, data.exposed, data.agents), (12, 2, Some(7)));
        assert_eq!(data.focus, None);
        // Most RAM first (a steady order), five rows, and each opens its agent. Docker Desktop
        // is a developer tool: listed on its own, not counted as an agent.
        let names: Vec<&str> = data.top_agents.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            ["Claude Code", "Codex 5", "Codex 4", "Codex 3", "Codex 2"]
        );
        assert_eq!(data.tools.len(), 1);
        assert_eq!(data.tools[0].name, "Docker Desktop");
        assert_eq!(data.memory, Some(300 * 1024 * 1024 + 1024 * 21));
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

    fn quick() -> PanelSettings {
        panel_settings(&BarChoice::default(), true, true, Platform::MacOs)
    }

    #[test]
    fn quick_settings_offer_what_this_desktop_can_change() {
        let mac = panel_settings(&BarChoice::default(), false, true, Platform::MacOs);
        assert_eq!(mac.bar, Some("Menu bar"));
        let shown: Vec<(&str, bool, &str)> = mac
            .items
            .iter()
            .map(|i| (i.name, i.on, i.action.as_str()))
            .collect();
        assert_eq!(shown.len(), Item::ALL.len());
        assert_eq!(shown[1], ("Agents", true, "item:agents"));
        assert_eq!(shown[3], ("Agent CPU", false, "item:cpu"));
        assert_eq!(
            (mac.combined, mac.dock, mac.notify),
            (Some(false), Some(false), true)
        );
        // Every switch is an action the panel may run.
        for toggle in &mac.items {
            assert!(Action::parse(&toggle.action).is_some(), "{}", toggle.action);
        }
        // Linux panels hold text only, and have no Dock or "as one item".
        let linux = panel_settings(&BarChoice::default(), true, false, Platform::Linux);
        assert_eq!(linux.bar, Some("Panel"));
        assert!(linux
            .items
            .iter()
            .all(|i| i.name != "holdmap icon" && i.name != "Agent bars"));
        assert_eq!(
            (linux.combined, linux.dock, linux.notify),
            (None, None, false)
        );
        // Windows tray icons hold no readings.
        let windows = panel_settings(&BarChoice::default(), true, true, Platform::Windows);
        assert_eq!((windows.bar, windows.items.len()), (None, 0));
    }

    #[test]
    fn text_changes_keep_the_rows_so_the_open_menu_updates_in_place() {
        let a = view(
            Some(&readings()),
            &Trends::default(),
            &BarChoice::default(),
            Platform::MacOs,
        );
        let trends = measured(&readings());
        let mut busier = readings();
        busier.ports = 13;
        busier.agents.as_mut().unwrap()[0].cpu = 80.0;
        let b = view(
            Some(&busier),
            &trends,
            &BarChoice::default(),
            Platform::MacOs,
        );
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
        let c = view(Some(&more), &trends, &BarChoice::default(), Platform::MacOs);
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
                port_ids: vec!["tcp:3000".into()],
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

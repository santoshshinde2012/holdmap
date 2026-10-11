//! TUI state and key handling. Scans and stops run on background threads so the UI never blocks.

use super::graph::GraphView;
use crate::agent_actions::{AgentStopOutcome, ReviewedAgentStops};
use holdmap_core::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::widgets::TableState;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

const REFRESH: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Port,
    Process,
    Pid,
    Memory,
    Uptime,
}

impl Sort {
    pub fn next(self) -> Sort {
        match self {
            Sort::Port => Sort::Process,
            Sort::Process => Sort::Pid,
            Sort::Pid => Sort::Memory,
            Sort::Memory => Sort::Uptime,
            Sort::Uptime => Sort::Port,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Sort::Port => "port",
            Sort::Process => "process",
            Sort::Pid => "pid",
            Sort::Memory => "memory",
            Sort::Uptime => "uptime",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtoFilter {
    Both,
    Tcp,
    Udp,
}

/// Top-level tab: the port table, the service graph or the agents map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Ports,
    Graph,
    Agents,
}

pub enum Modal {
    None,
    Confirm {
        plan: Box<ActionPlan>,
        headline: String,
        /// Combined text is displayed; each original reviewed plan retains authority.
        agent_stops: Option<Box<ReviewedAgentStops>>,
        reviewed_target: Option<(Target, StopOptions)>,
        scroll: u16,
        max_scroll: u16,
    },
    Explain {
        scroll: u16,
    },
    Help,
}

pub enum Msg {
    Scanned(Box<Result<(Engine, holdmap_core::agents::AgentsReport), String>>),
    StopLine(String),
    Stopped(Box<StopReport>),
    AgentStopped(Box<Result<AgentStopOutcome, String>>),
}

pub struct Toast {
    pub text: String,
    pub ok: bool,
    pub at: Instant,
}

pub struct App {
    pub docker: bool,
    pub engine: Option<Arc<Engine>>,
    pub error: Option<String>,
    pub rows: Vec<PortEntry>,
    pub state: TableState,
    pub query: String,
    pub searching: bool,
    pub show_all: bool,
    pub proto: ProtoFilter,
    pub dev_only: bool,
    pub mine_only: bool,
    pub sort: Sort,
    pub reverse: bool,
    pub modal: Modal,
    pub tab: Tab,
    pub graph: GraphView,
    /// Agent attribution collected on the worker alongside the current engine snapshot.
    pub agents: Option<holdmap_core::agents::AgentsReport>,
    /// Selected agent index in `agents.agents`.
    pub agent_idx: usize,
    pub agent_query: String,
    pub agent_searching: bool,
    pub agent_state: TableState,
    pub agent_scroll: u16,
    pub agent_detail_area: ratatui::layout::Rect,
    pub agent_detail_max_scroll: u16,
    pub toast: Option<Toast>,
    pub busy: Option<String>,
    pub stop_log: Vec<String>,
    pub quit: bool,
    pub last_scan: Option<Instant>,
    pub scanning: bool,
    scan_again: bool,
    pub paused: bool,
    explain_cache: HashMap<String, Arc<Explanation>>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub spinner: usize,
    pub table_area: ratatui::layout::Rect,
}

impl App {
    pub fn new(docker: bool) -> App {
        let mut app = Self::empty(docker);
        app.start_scan();
        app
    }

    /// Construction is separate from collection so state and rendering tests need no OS scan.
    pub(super) fn empty(docker: bool) -> App {
        let (tx, rx) = channel();
        App {
            docker,
            engine: None,
            error: None,
            rows: Vec::new(),
            state: TableState::default().with_selected(Some(0)),
            query: String::new(),
            searching: false,
            show_all: false,
            proto: ProtoFilter::Both,
            dev_only: false,
            mine_only: false,
            sort: Sort::Port,
            reverse: false,
            modal: Modal::None,
            tab: Tab::Ports,
            graph: GraphView::new(),
            agents: None,
            agent_idx: 0,
            agent_query: String::new(),
            agent_searching: false,
            agent_state: TableState::default(),
            agent_scroll: 0,
            agent_detail_area: Default::default(),
            agent_detail_max_scroll: 0,
            toast: None,
            busy: None,
            stop_log: Vec::new(),
            quit: false,
            last_scan: None,
            scanning: false,
            scan_again: false,
            paused: false,
            explain_cache: HashMap::new(),
            tx,
            rx,
            spinner: 0,
            table_area: Default::default(),
        }
    }

    fn start_scan(&mut self) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        let tx = self.tx.clone();
        let opts = ScanOptions {
            all_states: self.show_all,
            docker: self.docker,
        };
        std::thread::spawn(move || {
            let r = Engine::new(&opts)
                .map(|engine| {
                    // Attribution can read bounded, allowlisted metadata. Keep that work on
                    // the collector thread and give every view one consistent scan.
                    let agents = engine.agents();
                    (engine, agents)
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Scanned(Box::new(r)));
        });
    }

    /// Process background messages and schedule refreshes.
    pub fn tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Scanned(r) => {
                    self.scanning = false;
                    self.last_scan = Some(Instant::now());
                    if self.scan_again {
                        // An options change or explicit refresh requested a newer scan.
                        // Discard this old response instead of briefly changing ownership.
                        self.scan_again = false;
                        self.start_scan();
                        continue;
                    }
                    match *r {
                        Ok((e, agents)) => {
                            self.error = None;
                            self.engine = Some(Arc::new(e));
                            self.explain_cache.clear();
                            self.rebuild();
                            self.rebuild_graph();
                            self.set_agents_report(agents);
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Msg::StopLine(l) => {
                    if self.stop_log.len() >= 256 {
                        self.stop_log.remove(0);
                    }
                    self.stop_log.push(l);
                }
                Msg::Stopped(r) => {
                    self.busy = None;
                    let n = r.signalled.len();
                    let text = if r.success && r.freed {
                        format!(
                            "✔ {} is free{} in {} ms",
                            r.target,
                            if n > 0 {
                                format!(" (stopped {n} process{})", if n == 1 { "" } else { "es" })
                            } else {
                                String::new()
                            },
                            r.elapsed_ms
                        )
                    } else if r.freed {
                        format!(
                            "{} is free; {}",
                            r.target,
                            r.error
                                .clone()
                                .unwrap_or_else(|| "the stop did not complete cleanly".into())
                        )
                    } else {
                        format!(
                            "✖ Could not free {}: {}",
                            r.target,
                            r.error.clone().unwrap_or_else(|| format!(
                                "ports still busy: {:?}",
                                r.ports_still_busy
                            ))
                        )
                    };
                    self.toast = Some(Toast {
                        text,
                        ok: r.success && r.freed,
                        at: Instant::now(),
                    });
                    self.start_scan_force();
                }
                Msg::AgentStopped(result) => {
                    self.busy = None;
                    let (text, ok) = match *result {
                        Ok(outcome) if outcome.succeeded() => (
                            format!(
                                "Stopped {} reviewed services; agent session stays running",
                                outcome.reports.len()
                            ),
                            true,
                        ),
                        Ok(outcome) => {
                            let completed = outcome
                                .reports
                                .iter()
                                .filter(|r| r.success && r.freed)
                                .count();
                            let detail = outcome
                                .history_errors
                                .first()
                                .cloned()
                                .or_else(|| outcome.reports.last().and_then(|r| r.error.clone()))
                                .unwrap_or_else(|| "a reviewed port is still busy".into());
                            (format!("{completed} services stopped; {} left unattempted: {detail}. Review again; no retry.", outcome.remaining), false)
                        }
                        Err(error) => (format!("No service was stopped: {error}"), false),
                    };
                    self.toast = Some(Toast {
                        text,
                        ok,
                        at: Instant::now(),
                    });
                    self.start_scan_force();
                }
            }
        }
        let due = self.last_scan.is_none_or(|t| t.elapsed() >= REFRESH);
        if due
            && !self.paused
            && self.busy.is_none()
            && !matches!(self.modal, Modal::Confirm { .. })
        {
            self.start_scan();
        }
        if self
            .toast
            .as_ref()
            .is_some_and(|t| t.at.elapsed() > Duration::from_secs(6))
        {
            self.toast = None;
        }
    }

    pub fn filter(&self) -> Filter {
        Filter {
            query: self.query.clone(),
            protocol: match self.proto {
                ProtoFilter::Both => None,
                ProtoFilter::Tcp => Some(Protocol::Tcp),
                ProtoFilter::Udp => Some(Protocol::Udp),
            },
            listening_only: !self.show_all,
            dev_only: self.dev_only,
            mine_only: self.mine_only,
            ..Default::default()
        }
    }

    /// Re-apply filter and sort, keeping the selection on the same row id.
    pub fn rebuild(&mut self) {
        let selected_id = self.selected().map(|e| e.id.clone());
        let Some(engine) = &self.engine else { return };
        let f = self.filter();
        let mut rows: Vec<PortEntry> = f
            .apply(&engine.snapshot().entries)
            .into_iter()
            .cloned()
            .collect();
        match self.sort {
            Sort::Port => rows.sort_by_key(|e| (e.port, e.protocol)),
            Sort::Process => rows.sort_by_key(|e| {
                (
                    e.process
                        .as_ref()
                        .map(|p| p.name.to_lowercase())
                        .unwrap_or_else(|| "~".into()),
                    e.port,
                )
            }),
            Sort::Pid => rows.sort_by_key(|e| (e.pid.unwrap_or(u32::MAX), e.port)),
            Sort::Memory => rows.sort_by_key(|e| {
                std::cmp::Reverse(if e.app_memory_bytes > 0 {
                    e.app_memory_bytes
                } else {
                    e.process.as_ref().map(|p| p.memory_bytes).unwrap_or(0)
                })
            }),
            Sort::Uptime => {
                rows.sort_by_key(|e| e.process.as_ref().map(|p| p.start_time).unwrap_or(u64::MAX))
            }
        }
        if self.reverse {
            rows.reverse();
        }
        self.rows = rows;
        let idx = selected_id
            .and_then(|id| self.rows.iter().position(|r| r.id == id))
            .or(self.state.selected())
            .map(|i| i.min(self.rows.len().saturating_sub(1)));
        self.state.select(if self.rows.is_empty() {
            None
        } else {
            idx.or(Some(0))
        });
    }

    /// Recompute the topology from the current scan (dev services unless "all" is on).
    pub fn rebuild_graph(&mut self) {
        let Some(engine) = &self.engine else { return };
        if let Ok(g) = crate::graph::build(engine, self.show_all, None, true) {
            self.graph.set_graph(g);
        }
    }

    /// Switch tabs, carrying the selection across (entry ↔ node).
    pub fn toggle_tab(&mut self) {
        self.searching = false;
        self.agent_searching = false;
        match self.tab {
            Tab::Ports => {
                if let Some(id) = self.selected().map(|e| e.id.clone()) {
                    self.graph.select_entry(&id);
                }
                self.tab = Tab::Graph;
            }
            Tab::Graph => {
                self.rebuild_agents();
                self.tab = Tab::Agents;
            }
            Tab::Agents => {
                self.tab = Tab::Ports;
            }
        }
    }

    fn previous_tab(&mut self) {
        self.searching = false;
        self.agent_searching = false;
        match self.tab {
            Tab::Ports => {
                self.rebuild_agents();
                self.tab = Tab::Agents;
            }
            Tab::Agents => self.tab = Tab::Graph,
            Tab::Graph => {
                self.sync_from_graph();
                self.tab = Tab::Ports;
            }
        }
    }

    /// Point the port table at the selected graph node's first port.
    fn sync_from_graph(&mut self) -> bool {
        let Some(entry) = self
            .graph
            .selected_node()
            .and_then(|n| n.ports.first())
            .map(|p| p.entry_id.clone())
        else {
            return false;
        };
        if !self.rows.iter().any(|r| r.id == entry) && !self.query.is_empty() {
            self.query.clear();
            self.rebuild();
        }
        if let Some(i) = self.rows.iter().position(|r| r.id == entry) {
            self.state.select(Some(i));
            true
        } else {
            false
        }
    }

    /// Ask to stop every service in the selected node's cluster, in dependency order.
    fn request_cluster_stop(&mut self) {
        if self.busy.is_some() {
            return;
        }
        let Some(name) = self.graph.selected_cluster().map(str::to_string) else {
            self.toast = Some(Toast {
                text: "This service isn't part of a cluster".into(),
                ok: false,
                at: Instant::now(),
            });
            return;
        };
        let Some(engine) = self.engine.clone() else {
            return;
        };
        let target = Target::Cluster(name);
        let options = StopOptions::default();
        let plan = engine.plan(&target, &options);
        self.modal = Modal::Confirm {
            headline: plan.summary.clone(),
            plan: Box::new(plan),
            agent_stops: None,
            reviewed_target: Some((target, options)),
            scroll: 0,
            max_scroll: 0,
        };
    }

    fn on_graph_key(&mut self, k: KeyEvent) -> bool {
        match k.code {
            KeyCode::Down | KeyCode::Char('j') => self.graph.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.graph.move_by(-1),
            KeyCode::PageDown => self.graph.move_by(10),
            KeyCode::PageUp => self.graph.move_by(-10),
            KeyCode::Home | KeyCode::Char('g') => self.graph.move_by(-10_000),
            KeyCode::End | KeyCode::Char('G') => self.graph.move_by(10_000),
            KeyCode::Char('C') => self.request_cluster_stop(),
            KeyCode::Char('h') => self.graph.toggle_external(),
            KeyCode::Char('x')
            | KeyCode::Char('X')
            | KeyCode::Char('e')
            | KeyCode::Enter
            | KeyCode::Char('o')
            | KeyCode::Char('c') => {
                // Act on the node's port through the regular port actions.
                if self.sync_from_graph() {
                    return false;
                }
                self.toast = Some(Toast {
                    text: "This graph node has no visible local listener. Choose its port in Ports and adjust filters to act on it.".into(),
                    ok: false,
                    at: Instant::now(),
                });
            }
            _ => return false,
        }
        true
    }

    pub fn selected(&self) -> Option<&PortEntry> {
        self.state.selected().and_then(|i| self.rows.get(i))
    }

    pub fn explanation(&mut self) -> Option<Arc<Explanation>> {
        let entry = self.selected()?.clone();
        if let Some(e) = self.explain_cache.get(&entry.id) {
            return Some(e.clone());
        }
        let engine = self.engine.clone()?;
        let opts = StopOptions {
            protocol: Some(entry.protocol),
            ..Default::default()
        };
        let ex = Arc::new(engine.explain(entry.port, &opts));
        self.explain_cache.insert(entry.id.clone(), ex.clone());
        Some(ex)
    }

    fn move_by(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let cur = self.state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, self.rows.len() as isize - 1);
        self.state.select(Some(next as usize));
    }

    fn request_stop(&mut self, force: bool) {
        if self.busy.is_some() {
            return;
        }
        let Some(entry) = self.selected().cloned() else {
            return;
        };
        let Some(engine) = self.engine.clone() else {
            return;
        };
        let opts = StopOptions {
            force,
            protocol: Some(entry.protocol),
            ..Default::default()
        };
        let ex = engine.explain(entry.port, &opts);
        let plan = ex
            .plan
            .clone()
            .unwrap_or_else(|| engine.plan(&Target::Port(entry.port), &opts));
        self.modal = Modal::Confirm {
            plan: Box::new(plan),
            headline: ex.headline.clone(),
            agent_stops: None,
            reviewed_target: Some((Target::Port(entry.port), opts)),
            scroll: 0,
            max_scroll: 0,
        };
    }

    fn confirm_stop(&mut self) {
        if self.busy.is_some() {
            return;
        }
        let Modal::Confirm {
            plan,
            agent_stops,
            reviewed_target,
            ..
        } = std::mem::replace(&mut self.modal, Modal::None)
        else {
            return;
        };
        if plan.is_blocked() {
            return;
        }
        self.busy = Some(format!("Stopping {}…", plan.target));
        self.stop_log.clear();
        let tx = self.tx.clone();
        let docker = self.docker;
        std::thread::spawn(move || {
            let fresh = Engine::new(&ScanOptions {
                all_states: false,
                docker,
            })
            .map_err(|e| e.to_string());
            if let Some(reviewed) = agent_stops {
                let outcome = fresh.and_then(|engine| {
                    reviewed
                        .execute_with(
                            &engine,
                            |reviewed| {
                                let _ = tx.send(Msg::StopLine(format!(
                                    "→ {} ({})",
                                    reviewed.target(),
                                    reviewed.name
                                )));
                                execute(&reviewed.plan, &mut |line| {
                                    let _ = tx.send(Msg::StopLine(line.into()));
                                })
                            },
                            |plan, report| {
                                crate::state::record_checked(&engine, plan, report)
                                    .map_err(|e| e.to_string())
                            },
                        )
                        .map_err(|e| e.to_string())
                });
                let _ = tx.send(Msg::AgentStopped(Box::new(outcome)));
                return;
            }
            let mut report = match fresh {
                Ok(engine) => {
                    let Some((target, options)) = reviewed_target else {
                        let _ = tx.send(Msg::Stopped(Box::new(StopReport {
                            target: plan.target.clone(),
                            error: Some(
                                "The reviewed target is missing. No action was taken.".into(),
                            ),
                            ..Default::default()
                        })));
                        return;
                    };
                    let current = engine.plan(&target, &options);
                    if current.is_blocked()
                        || current.risk > plan.risk
                        || !plan.same_effects(&current)
                    {
                        StopReport { target: plan.target.clone(), error: Some("The target changed since review. Refresh and review the plan again. No action was taken.".into()), ..Default::default() }
                    } else {
                        let mut result = execute(&plan, &mut |line| {
                            let _ = tx.send(Msg::StopLine(line.into()));
                        });
                        if let Err(error) = crate::state::record_checked(&engine, &plan, &result) {
                            result.success = false;
                            result.error = Some(format!(
                                "{} Stop history could not be saved: {error}",
                                result.error.unwrap_or_default()
                            ));
                        }
                        result
                    }
                }
                Err(error) => StopReport {
                    target: plan.target.clone(),
                    success: false,
                    freed: false,
                    signalled: vec![],
                    ports_still_busy: vec![],
                    elapsed_ms: 0,
                    error: Some(format!(
                        "Could not check the reviewed target: {error}. No action was taken."
                    )),
                    ..Default::default()
                },
            };
            // A refused preflight is reported through the same read-only status channel.
            report.target = plan.target.clone();
            let _ = tx.send(Msg::Stopped(Box::new(report)));
        });
    }

    fn open_selected(&mut self) {
        let Some(e) = self.selected() else { return };
        let url = format!("http://localhost:{}", e.port);
        let r = holdmap_core::util::open_url(&url);
        self.toast = Some(match r {
            Ok(_) => Toast {
                text: format!("Opened {url}"),
                ok: true,
                at: Instant::now(),
            },
            Err(e) => Toast {
                text: format!("Couldn't open browser: {e}"),
                ok: false,
                at: Instant::now(),
            },
        });
    }

    fn copy_selected(&mut self) {
        let Some(e) = self.selected() else { return };
        let text = format!("http://localhost:{}", e.port);
        // OSC 52: ask the terminal to put text on the clipboard (works over SSH too).
        let b64 = base64(text.as_bytes());
        print!("\x1b]52;c;{b64}\x07");
        self.toast = Some(Toast {
            text: format!("Copied {text}"),
            ok: true,
            at: Instant::now(),
        });
    }

    pub fn on_mouse(&mut self, m: MouseEvent) {
        match &mut self.modal {
            Modal::Confirm {
                scroll, max_scroll, ..
            } => {
                match m.kind {
                    MouseEventKind::ScrollDown => {
                        *scroll = scroll.saturating_add(3).min(*max_scroll)
                    }
                    MouseEventKind::ScrollUp => *scroll = scroll.saturating_sub(3),
                    _ => {}
                }
                return;
            }
            Modal::Explain { scroll } => {
                match m.kind {
                    MouseEventKind::ScrollDown => *scroll = scroll.saturating_add(3),
                    MouseEventKind::ScrollUp => *scroll = scroll.saturating_sub(3),
                    _ => {}
                }
                return;
            }
            Modal::Help => return,
            Modal::None => {}
        }
        if self.tab == Tab::Agents {
            self.on_agent_mouse(m);
            return;
        }
        let graph = self.tab == Tab::Graph;
        match m.kind {
            MouseEventKind::ScrollDown if graph => self.graph.move_by(1),
            MouseEventKind::ScrollUp if graph => self.graph.move_by(-1),
            MouseEventKind::ScrollDown => self.move_by(1),
            MouseEventKind::ScrollUp => self.move_by(-1),
            _ => {}
        }
    }

    fn request_quit(&mut self) {
        if self.busy.is_some() {
            self.toast = Some(Toast {
                text: "A reviewed stop is finishing. Wait for its result, then press q to quit."
                    .into(),
                ok: false,
                at: Instant::now(),
            });
        } else {
            self.quit = true;
        }
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        if k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('c')) {
            self.request_quit();
            return;
        }
        match &mut self.modal {
            Modal::Confirm {
                plan,
                scroll,
                max_scroll,
                ..
            } => {
                match k.code {
                    KeyCode::Char('y') if !plan.is_blocked() => self.confirm_stop(),
                    KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                        self.modal = Modal::None
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        *scroll = scroll.saturating_add(1).min(*max_scroll)
                    }
                    KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                    KeyCode::PageDown => *scroll = scroll.saturating_add(8).min(*max_scroll),
                    KeyCode::PageUp => *scroll = scroll.saturating_sub(8),
                    KeyCode::Home => *scroll = 0,
                    KeyCode::End => *scroll = *max_scroll,
                    _ => {}
                }
                return;
            }
            Modal::Explain { scroll } => {
                match k.code {
                    KeyCode::Down | KeyCode::Char('j') => *scroll = scroll.saturating_add(1),
                    KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                    KeyCode::Char('x') => {
                        self.modal = Modal::None;
                        self.request_stop(false);
                    }
                    _ => self.modal = Modal::None,
                }
                return;
            }
            Modal::Help => {
                self.modal = Modal::None;
                return;
            }
            Modal::None => {}
        }
        if self.tab == Tab::Agents && self.agent_searching {
            self.search_agents(k);
            return;
        }
        if self.searching {
            match k.code {
                KeyCode::Esc => {
                    self.searching = false;
                    self.query.clear();
                }
                KeyCode::Enter => self.searching = false,
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Down => self.move_by(1),
                KeyCode::Up => self.move_by(-1),
                KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.clear()
                }
                KeyCode::Char(c) => self.query.push(c),
                _ => {}
            }
            self.rebuild();
            return;
        }
        if matches!(k.code, KeyCode::BackTab) {
            self.previous_tab();
            return;
        }
        if matches!(k.code, KeyCode::Tab | KeyCode::Char('v')) {
            self.toggle_tab();
            return;
        }
        if self.tab == Tab::Graph && self.on_graph_key(k) {
            return;
        }
        if self.tab == Tab::Agents {
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => self.move_agent(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_agent(-1),
                KeyCode::Home | KeyCode::Char('g') => self.move_agent(-10_000),
                KeyCode::End | KeyCode::Char('G') => self.move_agent(10_000),
                KeyCode::PageDown => self.scroll_agent(8),
                KeyCode::PageUp => self.scroll_agent(-8),
                KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.scroll_agent(8)
                }
                KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.scroll_agent(-8)
                }
                KeyCode::Char('/') => self.agent_searching = true,
                KeyCode::Char('x') => self.request_stop_agent_ports(),
                KeyCode::Char('X') => {
                    self.toast = Some(Toast {
                        text: "Use x to review this agent's services with their normal stop plan."
                            .into(),
                        ok: false,
                        at: Instant::now(),
                    })
                }
                KeyCode::Char('q') => self.request_quit(),
                KeyCode::Esc => {
                    if !self.agent_query.is_empty() {
                        self.agent_query.clear();
                        self.agent_scroll = 0;
                        self.rebuild_agents();
                    } else {
                        self.tab = Tab::Ports;
                    }
                }
                KeyCode::Char('?') | KeyCode::F(1) => self.modal = Modal::Help,
                KeyCode::Char('r') | KeyCode::F(5) => self.start_scan_force(),
                KeyCode::Char(' ') | KeyCode::Char('p') => self.paused = !self.paused,
                _ => {}
            }
            return;
        }
        match k.code {
            KeyCode::Char('q') => self.request_quit(),
            KeyCode::Esc => {
                if !self.query.is_empty() {
                    self.query.clear();
                    self.rebuild();
                } else {
                    self.request_quit();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::PageDown => self.move_by(10),
            KeyCode::PageUp => self.move_by(-10),
            KeyCode::Home | KeyCode::Char('g') => self.move_by(-(self.rows.len() as isize)),
            KeyCode::End | KeyCode::Char('G') => self.move_by(self.rows.len() as isize),
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
                self.rebuild();
            }
            KeyCode::Char('S') => {
                self.reverse = !self.reverse;
                self.rebuild();
            }
            KeyCode::Char('t') => {
                self.proto = match self.proto {
                    ProtoFilter::Both => ProtoFilter::Tcp,
                    ProtoFilter::Tcp => ProtoFilter::Udp,
                    ProtoFilter::Udp => ProtoFilter::Both,
                };
                self.rebuild();
            }
            KeyCode::Char('d') => {
                self.dev_only = !self.dev_only;
                self.rebuild();
            }
            KeyCode::Char('C') => {
                if let Some(id) = self.selected().map(|e| e.id.clone()) {
                    self.graph.select_entry(&id);
                    self.request_cluster_stop();
                }
            }
            KeyCode::Char('m') => {
                self.mine_only = !self.mine_only;
                self.rebuild();
            }
            KeyCode::Char('a') => {
                self.show_all = !self.show_all;
                self.last_scan = None;
                self.start_scan_force();
            }
            KeyCode::Char('p') => self.paused = !self.paused,
            KeyCode::Char('r') | KeyCode::F(5) => self.start_scan_force(),
            KeyCode::Char('x') | KeyCode::Delete | KeyCode::Backspace => self.request_stop(false),
            KeyCode::Char('X') => self.request_stop(true),
            KeyCode::Char('e') | KeyCode::Enter if self.selected().is_some() => {
                self.modal = Modal::Explain { scroll: 0 };
            }
            KeyCode::Char('o') => self.open_selected(),
            KeyCode::Char('c') => self.copy_selected(),
            KeyCode::Char('?') | KeyCode::F(1) => self.modal = Modal::Help,
            _ => {}
        }
    }

    fn start_scan_force(&mut self) {
        if self.scanning {
            self.scan_again = true;
        } else {
            self.start_scan();
        }
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encodes() {
        assert_eq!(base64(b"hi"), "aGk=");
        assert_eq!(
            base64(b"http://localhost:3000"),
            "aHR0cDovL2xvY2FsaG9zdDozMDAw"
        );
    }

    #[test]
    fn sort_cycles() {
        let mut s = Sort::Port;
        for _ in 0..5 {
            s = s.next();
        }
        assert_eq!(s, Sort::Port);
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    /// Background-message tests must never collect the host or execute a stop.
    fn quiet_app() -> App {
        let mut app = App::empty(false);
        app.paused = true;
        app.scanning = true;
        app
    }

    fn report(success: bool, freed: bool, error: Option<&str>) -> StopReport {
        StopReport {
            target: "3000/tcp".into(),
            success,
            freed,
            error: error.map(str::to_owned),
            ..Default::default()
        }
    }

    fn deliver(app: &mut App, message: Msg) {
        app.tx.send(message).unwrap();
        app.tick();
        assert!(app.scanning, "existing collector stays active");
        assert!(
            app.scan_again,
            "a fresh read-only scan is queued after status delivery"
        );
        assert!(matches!(app.modal, Modal::None));
        assert!(app.rx.try_recv().is_err());
    }

    #[test]
    fn refresh_during_a_scan_queues_one_followup_without_starting_parallel_collection() {
        for tab in [Tab::Ports, Tab::Graph, Tab::Agents] {
            let mut app = quiet_app();
            app.tab = tab;
            for key in [KeyCode::F(5), KeyCode::Char('r'), KeyCode::F(5)] {
                app.on_key(KeyEvent::new(key, KeyModifiers::NONE));
                assert!(app.scanning);
                assert!(app.scan_again);
                assert!(app.last_scan.is_none());
                assert!(app.engine.is_none());
                assert!(app.rx.try_recv().is_err());
            }
        }
    }

    #[test]
    fn progress_logs_keep_only_the_latest_bounded_lines() {
        let mut app = quiet_app();
        for line in 0..300 {
            app.tx
                .send(Msg::StopLine(format!("progress {line}")))
                .unwrap();
        }
        app.tick();
        assert_eq!(app.stop_log.len(), 256);
        assert_eq!(app.stop_log.first().unwrap(), "progress 44");
        assert_eq!(app.stop_log.last().unwrap(), "progress 299");
        assert!(app.scanning);
        assert!(!app.scan_again);
        assert!(app.toast.is_none());
    }

    #[test]
    fn a_freed_port_with_history_failure_is_reported_truthfully_without_success() {
        let mut app = quiet_app();
        app.busy = Some("Stopping 3000/tcp…".into());
        deliver(
            &mut app,
            Msg::Stopped(Box::new(report(
                false,
                true,
                Some("history could not be saved"),
            ))),
        );
        let toast = app.toast.as_ref().unwrap();
        assert!(!toast.ok);
        assert!(toast.text.contains("3000/tcp is free;"));
        assert!(toast.text.contains("history could not be saved"));
        assert!(!toast.text.contains("Could not free"));
        assert!(app.busy.is_none());
    }

    #[test]
    fn a_success_flag_does_not_report_success_if_the_port_remains_busy() {
        let mut app = quiet_app();
        let mut report = report(true, false, None);
        report.ports_still_busy = vec![3000];
        deliver(&mut app, Msg::Stopped(Box::new(report)));
        let toast = app.toast.as_ref().unwrap();
        assert!(!toast.ok);
        assert!(toast.text.contains("Could not free 3000/tcp"));
        assert!(toast.text.contains("ports still busy: [3000]"));
    }

    #[test]
    fn a_partial_agent_stop_counts_completed_and_unattempted_services_without_retrying() {
        let mut app = quiet_app();
        app.busy = Some("Stopping agent services…".into());
        deliver(
            &mut app,
            Msg::AgentStopped(Box::new(Ok(AgentStopOutcome {
                reports: vec![
                    report(true, true, None),
                    report(false, false, Some("service still busy")),
                ],
                remaining: 2,
                history_errors: Vec::new(),
            }))),
        );
        let toast = app.toast.as_ref().unwrap();
        assert!(!toast.ok);
        assert!(toast.text.starts_with("1 service"));
        assert!(toast.text.contains("2 left unattempted:"));
        assert!(toast.text.contains("service still busy"));
        assert!(toast.text.contains("Review again; no retry"));
        assert!(app.busy.is_none());
        let text = toast.text.clone();
        app.tick();
        assert_eq!(app.toast.as_ref().unwrap().text, text);
        assert!(app.busy.is_none());
        assert!(app.scan_again);
        assert!(matches!(app.modal, Modal::None));
        assert!(app.rx.try_recv().is_err());
    }

    #[test]
    fn agent_status_rejects_incomplete_or_history_failed_results() {
        for outcome in [
            AgentStopOutcome {
                reports: vec![report(true, false, None)],
                remaining: 0,
                history_errors: Vec::new(),
            },
            AgentStopOutcome {
                reports: vec![report(true, true, None)],
                remaining: 0,
                history_errors: vec!["history write refused".into()],
            },
            AgentStopOutcome {
                reports: Vec::new(),
                remaining: 0,
                history_errors: Vec::new(),
            },
        ] {
            let mut app = quiet_app();
            deliver(&mut app, Msg::AgentStopped(Box::new(Ok(outcome))));
            let toast = app.toast.as_ref().unwrap();
            assert!(!toast.ok);
            assert!(!toast.text.contains("agent session stays running"));
            assert!(toast.text.contains("Review again; no retry"));
        }
    }

    #[test]
    fn an_agent_preflight_refusal_clears_busy_without_claiming_any_effect_or_retry() {
        let mut app = quiet_app();
        app.busy = Some("Checking reviewed services…".into());
        deliver(
            &mut app,
            Msg::AgentStopped(Box::new(Err("ownership changed since review".into()))),
        );
        let toast = app.toast.as_ref().unwrap();
        assert!(!toast.ok);
        assert!(toast.text.starts_with("No service was stopped:"));
        assert!(toast.text.contains("ownership changed since review"));
        assert!(app.busy.is_none());
    }

    #[test]
    fn all_verified_agent_services_report_completion_and_session_preservation() {
        let mut app = quiet_app();
        deliver(
            &mut app,
            Msg::AgentStopped(Box::new(Ok(AgentStopOutcome {
                reports: vec![report(true, true, None), report(true, true, None)],
                remaining: 0,
                history_errors: Vec::new(),
            }))),
        );
        let toast = app.toast.as_ref().unwrap();
        assert!(toast.ok);
        assert!(toast.text.contains("Stopped 2 reviewed services"));
        assert!(toast.text.contains("agent session stays running"));
    }

    #[test]
    fn quit_keys_wait_for_an_active_reviewed_stop_and_work_after_its_result() {
        for key in [
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            let mut app = quiet_app();
            app.busy = Some("Stopping reviewed services…".into());
            app.on_key(key);
            assert!(!app.quit);
            assert!(app.busy.is_some());
            assert!(app
                .toast
                .as_ref()
                .unwrap()
                .text
                .contains("Wait for its result"));
            deliver(&mut app, Msg::Stopped(Box::new(report(true, true, None))));
            assert!(app.busy.is_none());
            app.on_key(key);
            assert!(app.quit);
        }
    }

    #[test]
    fn a_graph_node_without_a_visible_listener_cannot_act_on_the_old_port_selection() {
        use super::super::graph::tests::node;
        use holdmap_core::topology::{Graph, EXTERNAL_ID};

        for selected_node in [
            node(EXTERNAL_ID, None, None),
            node("filtered-service", Some(5000), None),
        ] {
            let mut app = quiet_app();
            app.tab = Tab::Graph;
            app.rows = vec![PortEntry {
                id: "tcp:3000".into(),
                port: 3000,
                protocol: Protocol::Tcp,
                state: SocketState::Listen,
                addresses: vec!["127.0.0.1".into()],
                families: Vec::new(),
                remote: None,
                exposure: Exposure::Loopback,
                pid: None,
                pids: Vec::new(),
                uid: None,
                user: None,
                process: None,
                project: None,
                framework: None,
                container: None,
                label: "Previous port selection".into(),
                is_dev: true,
                is_mine: true,
                protected: false,
                tunnel: None,
                app_memory_bytes: 0,
                helper_count: 0,
            }];
            app.state.select(Some(0));
            app.graph.set_graph(Graph {
                nodes: vec![selected_node],
                ..Default::default()
            });
            assert!(!app.sync_from_graph());
            for key in [
                KeyCode::Char('x'),
                KeyCode::Char('X'),
                KeyCode::Char('e'),
                KeyCode::Enter,
            ] {
                app.on_key(KeyEvent::new(key, KeyModifiers::NONE));
                assert_eq!(app.selected().unwrap().id, "tcp:3000");
                assert!(matches!(app.modal, Modal::None));
                assert!(app.busy.is_none());
                let toast = app.toast.as_ref().unwrap();
                assert!(!toast.ok);
                assert!(toast.text.contains("no visible local listener"));
            }
        }
    }
}

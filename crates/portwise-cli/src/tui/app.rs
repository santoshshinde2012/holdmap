//! TUI state and key handling. Scans and stops run on background threads so the UI never blocks.

use portwise_core::*;
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

pub enum Modal {
    None,
    Confirm { plan: ActionPlan, headline: String },
    Explain { scroll: u16 },
    Help,
}

pub enum Msg {
    Scanned(Box<Result<Engine, String>>),
    StopLine(String),
    Stopped(Box<StopReport>),
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
    pub toast: Option<Toast>,
    pub busy: Option<String>,
    pub stop_log: Vec<String>,
    pub quit: bool,
    pub last_scan: Option<Instant>,
    pub scanning: bool,
    pub paused: bool,
    explain_cache: HashMap<String, Arc<Explanation>>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub spinner: usize,
    pub table_area: ratatui::layout::Rect,
}

impl App {
    pub fn new(docker: bool) -> App {
        let (tx, rx) = channel();
        let mut app = App {
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
            toast: None,
            busy: None,
            stop_log: Vec::new(),
            quit: false,
            last_scan: None,
            scanning: false,
            paused: false,
            explain_cache: HashMap::new(),
            tx,
            rx,
            spinner: 0,
            table_area: Default::default(),
        };
        app.start_scan();
        app
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
            let r = Engine::new(&opts).map_err(|e| e.to_string());
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
                    match *r {
                        Ok(e) => {
                            self.error = None;
                            self.engine = Some(Arc::new(e));
                            self.explain_cache.clear();
                            self.rebuild();
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Msg::StopLine(l) => self.stop_log.push(l),
                Msg::Stopped(r) => {
                    self.busy = None;
                    let n = r.signalled.len();
                    let text = if r.success {
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
                    } else {
                        format!(
                            "✖ Could not free {}: {}",
                            r.target,
                            r.error.clone().unwrap_or_default()
                        )
                    };
                    self.toast = Some(Toast {
                        text,
                        ok: r.success,
                        at: Instant::now(),
                    });
                    self.start_scan();
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
                std::cmp::Reverse(e.process.as_ref().map(|p| p.memory_bytes).unwrap_or(0))
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
            plan,
            headline: ex.headline.clone(),
        };
    }

    fn confirm_stop(&mut self) {
        let Modal::Confirm { plan, .. } = std::mem::replace(&mut self.modal, Modal::None) else {
            return;
        };
        if plan.is_blocked() {
            return;
        }
        self.busy = Some(format!("Stopping {}…", plan.target));
        self.stop_log.clear();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let report = execute(&plan, &mut |l| {
                let _ = tx.send(Msg::StopLine(l.to_string()));
            });
            let _ = tx.send(Msg::Stopped(Box::new(report)));
        });
    }

    fn open_selected(&mut self) {
        let Some(e) = self.selected() else { return };
        let url = format!("http://localhost:{}", e.port);
        let r = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(&url).spawn()
        } else if cfg!(windows) {
            std::process::Command::new("cmd")
                .args(["/C", "start", "", &url])
                .spawn()
        } else {
            std::process::Command::new("xdg-open")
                .arg(&url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
        };
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
        match m.kind {
            MouseEventKind::ScrollDown => self.move_by(1),
            MouseEventKind::ScrollUp => self.move_by(-1),
            _ => {}
        }
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        if k.modifiers.contains(KeyModifiers::CONTROL) && matches!(k.code, KeyCode::Char('c')) {
            self.quit = true;
            return;
        }
        match &mut self.modal {
            Modal::Confirm { plan, .. } => {
                match k.code {
                    KeyCode::Char('y') | KeyCode::Enter if !plan.is_blocked() => {
                        self.confirm_stop()
                    }
                    KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                        self.modal = Modal::None
                    }
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
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                if !self.query.is_empty() {
                    self.query.clear();
                    self.rebuild();
                } else {
                    self.quit = true;
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
            KeyCode::Char('e') | KeyCode::Enter => {
                if self.selected().is_some() {
                    self.modal = Modal::Explain { scroll: 0 };
                }
            }
            KeyCode::Char('o') => self.open_selected(),
            KeyCode::Char('c') => self.copy_selected(),
            KeyCode::Char('?') | KeyCode::F(1) => self.modal = Modal::Help,
            _ => {}
        }
    }

    fn start_scan_force(&mut self) {
        self.scanning = false;
        self.start_scan();
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

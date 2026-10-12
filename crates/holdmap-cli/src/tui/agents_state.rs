//! Agent navigation and reviewed service-stop requests, independent of terminal layout.

use super::app::{App, Modal, Toast};
use super::input::append_search_text;
use crate::agent_actions::ReviewedAgentStops;
use holdmap_core::agents::Agent;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::time::Instant;

impl App {
    pub fn visible_agent_indices(&self) -> Vec<usize> {
        super::agents_view::filtered_indices(self.agents.as_ref(), &self.agent_query)
    }

    /// Keep the same agent selected across collection reorderings and filtered results.
    pub fn rebuild_agents(&mut self) {
        let selected = self.selected_agent().map(|a| a.id.clone());
        self.reconcile_agent_selection(selected.as_deref());
    }

    pub(super) fn set_agents_report(&mut self, report: holdmap_core::agents::AgentsReport) {
        let selected = self.selected_agent().map(|a| a.id.clone());
        self.agents = Some(report);
        self.reconcile_agent_selection(selected.as_deref());
    }

    fn reconcile_agent_selection(&mut self, preferred: Option<&str>) {
        let visible = self.visible_agent_indices();
        let same = preferred
            .and_then(|id| {
                self.agents
                    .as_ref()?
                    .agents
                    .iter()
                    .position(|agent| agent.id == id)
            })
            .filter(|index| visible.contains(index));
        let chosen = same
            .or_else(|| visible.contains(&self.agent_idx).then_some(self.agent_idx))
            .or_else(|| visible.first().copied());
        if preferred.is_some_and(|id| {
            chosen
                .and_then(|i| self.agents.as_ref()?.agents.get(i))
                .is_none_or(|a| a.id != id)
        }) {
            self.agent_scroll = 0;
        }
        self.agent_idx = chosen.unwrap_or(0);
        self.agent_state
            .select(chosen.and_then(|i| visible.iter().position(|x| *x == i)));
    }

    pub fn selected_agent(&self) -> Option<&Agent> {
        if !self.visible_agent_indices().contains(&self.agent_idx) {
            return None;
        }
        self.agents.as_ref()?.agents.get(self.agent_idx)
    }

    pub(super) fn move_agent(&mut self, delta: isize) {
        let visible = self.visible_agent_indices();
        let Some(position) = visible.iter().position(|i| *i == self.agent_idx) else {
            return;
        };
        let next =
            (position as isize + delta).clamp(0, visible.len().saturating_sub(1) as isize) as usize;
        if self.agent_idx != visible[next] {
            self.agent_scroll = 0;
        }
        self.agent_idx = visible[next];
        self.agent_state.select(Some(next));
    }

    pub(super) fn scroll_agent(&mut self, delta: i32) {
        self.agent_scroll = (i32::from(self.agent_scroll) + delta)
            .clamp(0, i32::from(self.agent_detail_max_scroll)) as u16;
    }

    pub(super) fn search_agents(&mut self, key: KeyEvent) {
        let selected = self.selected_agent().map(|a| a.id.clone());
        let previous = self.agent_query.clone();
        match key.code {
            KeyCode::Esc => {
                self.agent_searching = false;
                self.agent_query.clear();
            }
            KeyCode::Enter => self.agent_searching = false,
            KeyCode::Backspace => {
                self.agent_query.pop();
            }
            KeyCode::Down => self.move_agent(1),
            KeyCode::Up => self.move_agent(-1),
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.agent_query.clear()
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                append_search_text(&mut self.agent_query, c.encode_utf8(&mut [0; 4]))
            }
            _ => {}
        }
        if previous != self.agent_query {
            self.agent_scroll = 0;
            *self.agent_state.offset_mut() = 0;
            self.reconcile_agent_selection(selected.as_deref());
        }
    }

    pub(super) fn on_agent_mouse(&mut self, event: MouseEvent) {
        let delta = match event.kind {
            MouseEventKind::ScrollDown => 1,
            MouseEventKind::ScrollUp => -1,
            _ => return,
        };
        if self
            .agent_detail_area
            .contains((event.column, event.row).into())
        {
            self.scroll_agent(delta * 3);
        } else if self.table_area.contains((event.column, event.row).into()) {
            self.move_agent(delta as isize);
        }
    }

    pub(super) fn request_stop_agent_ports(&mut self) {
        if self.busy.is_some() {
            return;
        }
        let Some(agent) = self.selected_agent().cloned() else {
            return;
        };
        let Some(engine) = &self.engine else { return };
        let reviewed = match ReviewedAgentStops::preview(engine, std::slice::from_ref(&agent)) {
            Ok(reviewed) if !reviewed.plans().is_empty() => reviewed,
            Ok(_) => {
                self.toast = Some(Toast {
                    text: format!("{} has no stoppable services", agent.name),
                    ok: false,
                    at: Instant::now(),
                });
                return;
            }
            Err(error) => {
                self.toast = Some(Toast {
                    text: format!("Could not review services: {error}"),
                    ok: false,
                    at: Instant::now(),
                });
                return;
            }
        };
        let targets = reviewed
            .plans()
            .iter()
            .map(|p| p.target())
            .collect::<Vec<_>>()
            .join(", ");
        let headline = format!(
            "Stop {} services started by {}: {targets}",
            reviewed.plans().len(),
            agent.name
        );
        // This combined plan is presentation only; execution consumes the retained review.
        let mut display = reviewed
            .plans()
            .iter()
            .find(|p| p.plan.is_blocked())
            .unwrap_or(&reviewed.plans()[0])
            .plan
            .clone();
        display.target = format!("{} services", agent.name);
        display.summary = headline.clone();
        if !display.is_blocked() {
            display.steps = reviewed
                .plans()
                .iter()
                .flat_map(|p| p.plan.steps.clone())
                .collect();
        }
        display.risk = reviewed
            .plans()
            .iter()
            .map(|p| p.plan.risk)
            .max()
            .unwrap_or(display.risk);
        self.modal = Modal::Confirm {
            plan: Box::new(display),
            headline,
            agent_stops: Some(Box::new(reviewed)),
            reviewed_target: None,
            scroll: 0,
            max_scroll: 0,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::super::agents_view::test_agent;
    use super::super::app::Tab;
    use super::*;
    use holdmap_core::agents::AgentsReport;
    use holdmap_core::{ActionPlan, Risk};
    use ratatui::layout::Rect;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn app() -> App {
        let mut app = App::empty(false);
        app.tab = Tab::Agents;
        app.paused = true;
        let mut claude = test_agent(111, "Claude Code");
        claude.product = "claude".into();
        claude.vendor = "Anthropic".into();
        claude.process_name = "claude".into();
        app.set_agents_report(AgentsReport {
            agents: vec![claude, test_agent(222, "Codex")],
            ..Default::default()
        });
        app
    }

    #[test]
    fn refreshed_reports_preserve_identity_when_order_changes() {
        let mut app = app();
        app.move_agent(1);
        app.agent_scroll = 4;
        app.set_agents_report(AgentsReport {
            agents: vec![test_agent(222, "Codex"), test_agent(111, "Claude Code")],
            ..Default::default()
        });
        assert_eq!(app.selected_agent().unwrap().pid, 222);
        assert_eq!(app.agent_state.selected(), Some(0));
        assert_eq!(app.agent_scroll, 4);
        app.set_agents_report(AgentsReport {
            agents: vec![test_agent(111, "Claude Code")],
            ..Default::default()
        });
        assert_eq!(app.selected_agent().unwrap().pid, 111);
        assert_eq!(app.agent_scroll, 0);
    }

    #[test]
    fn search_has_its_own_state_and_never_dispatches_action_keys() {
        let mut app = app();
        app.query = "port-only".into();
        app.on_key(key(KeyCode::Char('/')));
        for ch in "codex".chars() {
            app.on_key(key(KeyCode::Char(ch)));
        }
        assert!(app.agent_searching);
        assert_eq!(app.query, "port-only");
        assert_eq!(app.selected_agent().unwrap().pid, 222);
        assert!(matches!(app.modal, Modal::None));
        app.on_key(key(KeyCode::Enter));
        assert!(!app.agent_searching);
        assert_eq!(app.agent_query, "codex");
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.tab, Tab::Agents);
        assert!(app.agent_query.is_empty());
        assert!(!app.quit);
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.tab, Tab::Ports);
        assert!(!app.quit);
    }

    #[test]
    fn empty_search_has_no_selected_agent_or_stop_target() {
        let mut app = app();
        app.agent_query = "missing-agent".into();
        app.rebuild_agents();
        app.move_agent(100);
        app.on_key(key(KeyCode::Char('x')));
        assert!(app.selected_agent().is_none());
        assert_eq!(app.agent_state.selected(), None);
        assert!(matches!(app.modal, Modal::None));
        assert!(app.busy.is_none());
    }

    #[test]
    fn filtered_navigation_and_detail_scroll_are_bounded_and_independent() {
        let mut app = app();
        app.agent_detail_max_scroll = 12;
        app.on_key(key(KeyCode::PageDown));
        assert_eq!(app.agent_scroll, 8);
        assert_eq!(app.selected_agent().unwrap().pid, 111);
        app.on_key(key(KeyCode::PageDown));
        assert_eq!(app.agent_scroll, 12);
        app.on_key(key(KeyCode::End));
        assert_eq!(app.selected_agent().unwrap().pid, 222);
        assert_eq!(app.agent_scroll, 0);
        app.move_agent(100);
        assert_eq!(app.agent_state.selected(), Some(1));
        app.move_agent(-100);
        assert_eq!(app.agent_state.selected(), Some(0));
        app.agent_query = "pid:222".into();
        app.rebuild_agents();
        app.move_agent(-100);
        assert_eq!(app.selected_agent().unwrap().pid, 222);
    }

    #[test]
    fn mouse_scroll_targets_the_hovered_agent_pane() {
        let mut app = app();
        app.table_area = Rect::new(0, 3, 35, 15);
        app.agent_detail_area = Rect::new(35, 3, 45, 15);
        app.agent_detail_max_scroll = 20;
        let mut event = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 50,
            row: 8,
            modifiers: KeyModifiers::NONE,
        };
        app.on_mouse(event);
        assert_eq!(app.agent_scroll, 3);
        assert_eq!(app.selected_agent().unwrap().pid, 111);
        event.column = 10;
        app.on_mouse(event);
        assert_eq!(app.selected_agent().unwrap().pid, 222);
        assert_eq!(app.agent_scroll, 0);
        event.column = 90;
        app.on_mouse(event);
        assert_eq!(app.selected_agent().unwrap().pid, 222);
    }

    #[test]
    fn reverse_tab_opens_agents_directly_and_closes_search_entry() {
        let mut app = app();
        app.tab = Tab::Ports;
        app.searching = false;
        app.on_key(key(KeyCode::BackTab));
        assert_eq!(app.tab, Tab::Agents);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.tab, Tab::Ports);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.tab, Tab::Graph);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.tab, Tab::Agents);
        app.on_key(key(KeyCode::Char('p')));
        assert!(!app.paused);
    }

    fn review() -> Modal {
        Modal::Confirm {
            plan: Box::new(ActionPlan {
                target: "review-only".into(),
                owners: Vec::new(),
                summary: "review".into(),
                steps: Vec::new(),
                blocked: None,
                warnings: Vec::new(),
                risk: Risk::Low,
                allow_protected: false,
            }),
            headline: "review".into(),
            agent_stops: None,
            reviewed_target: None,
            scroll: 0,
            max_scroll: 20,
        }
    }

    #[test]
    fn enter_escape_and_no_cancel_without_any_effect() {
        for cancel in [KeyCode::Enter, KeyCode::Esc, KeyCode::Char('n')] {
            let mut app = app();
            app.modal = review();
            app.on_key(key(cancel));
            assert!(matches!(app.modal, Modal::None));
            assert!(app.busy.is_none());
        }
    }

    #[test]
    fn pending_effect_prevents_another_confirmation_and_modal_scroll_is_bounded() {
        let mut app = app();
        app.busy = Some("Stopping reviewed services".into());
        app.on_key(key(KeyCode::Char('x')));
        assert!(matches!(app.modal, Modal::None));
        app.modal = review();
        app.on_key(key(KeyCode::End));
        app.on_key(key(KeyCode::PageDown));
        assert!(matches!(app.modal, Modal::Confirm { scroll: 20, .. }));
        app.on_key(key(KeyCode::Home));
        app.on_key(key(KeyCode::PageUp));
        assert!(matches!(app.modal, Modal::Confirm { scroll: 0, .. }));
        app.on_key(key(KeyCode::Char('y')));
        assert!(matches!(app.modal, Modal::Confirm { .. }));
        assert!(app.busy.is_some());
    }
}

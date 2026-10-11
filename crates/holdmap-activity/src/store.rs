use crate::{
    dto::visibility_limits, ActivitySnapshot, ActivitySpan, ActivityStatus, COLLECTOR_LIMITS,
};
use std::collections::{HashMap, VecDeque};

pub(crate) struct ActivityStore {
    spans: HashMap<String, ActivitySpan>,
    order: VecDeque<String>,
    pub received: u64,
    pub dropped: u64,
}

impl ActivityStore {
    pub fn new() -> Self {
        Self {
            spans: HashMap::new(),
            order: VecDeque::new(),
            received: 0,
            dropped: 0,
        }
    }

    pub fn clear(&mut self) {
        self.spans.clear();
        self.order.clear();
        self.received = 0;
        self.dropped = 0;
    }

    pub fn ingest(&mut self, incoming: Vec<ActivitySpan>, mut rejected: u64) -> (usize, u64) {
        self.dropped = self.dropped.saturating_add(rejected);
        let mut accepted = 0;
        for span in incoming {
            self.received = self.received.saturating_add(1);
            if let Some(existing) = self.spans.get_mut(&span.id) {
                if compatible(existing, &span) {
                    merge(existing, span);
                    accepted += 1;
                } else {
                    self.dropped = self.dropped.saturating_add(1);
                    rejected = rejected.saturating_add(1);
                }
                continue;
            }
            if self.spans.len() >= COLLECTOR_LIMITS.max_spans {
                if let Some(oldest) = self.order.pop_front() {
                    self.spans.remove(&oldest);
                    self.dropped = self.dropped.saturating_add(1);
                }
            }
            self.order.push_back(span.id.clone());
            self.spans.insert(span.id.clone(), span);
            accepted += 1;
        }
        (accepted, rejected)
    }

    pub fn snapshot(&self) -> ActivitySnapshot {
        ActivitySnapshot {
            spans: self
                .order
                .iter()
                .filter_map(|id| self.spans.get(id).cloned())
                .collect(),
            received: self.received,
            dropped: self.dropped,
            limits: visibility_limits(),
        }
    }

    pub fn finish_pending(&mut self) {
        for span in self.spans.values_mut() {
            if span.end_at_ms.is_none()
                && matches!(
                    span.status,
                    ActivityStatus::Requested | ActivityStatus::Running
                )
            {
                span.status = ActivityStatus::Unknown;
            }
        }
    }
}

fn compatible(existing: &ActivitySpan, incoming: &ActivitySpan) -> bool {
    let agrees =
        |old: &Option<String>, new: &Option<String>| old.is_none() || new.is_none() || old == new;
    existing.provider == incoming.provider
        && existing.source == incoming.source
        && existing.kind == incoming.kind
        && existing.name == incoming.name
        && (existing.trace_id.is_empty()
            || incoming.trace_id.is_empty()
            || existing.trace_id == incoming.trace_id)
        && agrees(&existing.parent_id, &incoming.parent_id)
        && agrees(&existing.session_id, &incoming.session_id)
        && agrees(&existing.agent_id, &incoming.agent_id)
        && agrees(&existing.parent_agent_id, &incoming.parent_agent_id)
        && (existing.pid.is_none() || incoming.pid.is_none() || existing.pid == incoming.pid)
        && (existing.process_started_at.is_none()
            || incoming.process_started_at.is_none()
            || existing.process_started_at == incoming.process_started_at)
}

fn terminal(span: &ActivitySpan) -> bool {
    span.end_at_ms.is_some()
        || matches!(
            span.status,
            ActivityStatus::Ok | ActivityStatus::Error | ActivityStatus::Cancelled
        )
}

fn merge(existing: &mut ActivitySpan, incoming: ActivitySpan) {
    // A late start cannot erase a finish. A late explicit outcome can enrich Unknown.
    let updates_completion =
        !terminal(existing) || (terminal(&incoming) && existing.status == ActivityStatus::Unknown);
    if updates_completion {
        existing.status = incoming.status;
        existing.end_at_ms = incoming.end_at_ms.or(existing.end_at_ms);
        existing.duration_ms = incoming.duration_ms.or(existing.duration_ms);
        if matches!((existing.start_at_ms, existing.end_at_ms), (Some(start), Some(end)) if start > end)
        {
            existing.start_at_ms = None;
        }
    }
    if let Some(start) = incoming
        .start_at_ms
        .filter(|start| existing.end_at_ms.is_none_or(|end| *start <= end))
    {
        existing.start_at_ms = Some(existing.start_at_ms.map_or(start, |old| old.min(start)));
    }
    if updates_completion {
        existing.input_bytes = incoming.input_bytes.or(existing.input_bytes);
        existing.output_bytes = incoming.output_bytes.or(existing.output_bytes);
    } else {
        existing.input_bytes = existing.input_bytes.or(incoming.input_bytes);
        existing.output_bytes = existing.output_bytes.or(incoming.output_bytes);
    }
    existing.parent_id = existing.parent_id.take().or(incoming.parent_id);
    existing.session_id = existing.session_id.take().or(incoming.session_id);
    existing.agent_id = existing.agent_id.take().or(incoming.agent_id);
    existing.parent_agent_id = existing.parent_agent_id.take().or(incoming.parent_agent_id);
    existing.pid = existing.pid.or(incoming.pid);
    existing.process_started_at = existing.process_started_at.or(incoming.process_started_at);
    if existing.trace_id.is_empty() {
        existing.trace_id = incoming.trace_id;
    }
}

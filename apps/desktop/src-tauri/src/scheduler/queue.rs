//! Bounded queue, native timing policy, and cancellation/dispatch ordering.

use super::model::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_ACTIVE: usize = 32;
const MAX_HISTORY: usize = 64;
const MIN_DELAY_SECS: u64 = 60;
const MAX_DELAY_SECS: u64 = 86_400;
const MAX_LATE_MS: u64 = 60_000;
const MAX_CLOCK_DRIFT_MS: u64 = 30_000;
const MISSED: &str = "The scheduled time was missed by more than 60 seconds, or the system clock changed by more than 30 seconds. Review and create a new schedule. No action was taken.";

#[derive(Clone, Copy)]
pub(super) struct ClockTime {
    pub(super) mono: Instant,
    pub(super) wall_ms: u64,
}

impl ClockTime {
    pub(super) fn now() -> Self {
        Self {
            mono: Instant::now(),
            wall_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
        }
    }
}

pub(super) struct Job {
    pub(super) public: ScheduledAction,
    created: ClockTime,
    delay: Duration,
    operation: Operation,
}

impl Job {
    fn timing_problem(&self, now: ClockTime) -> bool {
        let monotonic_elapsed: u64 = now
            .mono
            .saturating_duration_since(self.created.mono)
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        let expected_wall = self.created.wall_ms.saturating_add(monotonic_elapsed);
        now.wall_ms.abs_diff(expected_wall) > MAX_CLOCK_DRIFT_MS
            || now.wall_ms.saturating_sub(self.public.due_at_ms) > MAX_LATE_MS
            || monotonic_elapsed.saturating_sub(self.delay.as_millis() as u64) > MAX_LATE_MS
    }

    fn due(&self, now: ClockTime) -> bool {
        now.wall_ms >= self.public.due_at_ms
            && now.mono.saturating_duration_since(self.created.mono) >= self.delay
    }

    fn status(&mut self, status: ScheduleStatus, message: Option<String>) {
        self.public.status = status;
        self.public.cancellable = status.cancellable();
        self.public.message = message;
    }
}

#[derive(Default)]
pub(super) struct Queue {
    next_id: u64,
    pub(super) jobs: VecDeque<Job>,
}

impl Queue {
    pub(super) fn capacity(&self) -> Result<(), String> {
        if self
            .jobs
            .iter()
            .filter(|j| j.public.status.active())
            .count()
            >= MAX_ACTIVE
        {
            Err("There are already 32 active schedules. Cancel one before adding another.".into())
        } else {
            Ok(())
        }
    }

    pub(super) fn insert(
        &mut self,
        operation: Operation,
        label: String,
        targets: Vec<String>,
        delay_secs: u64,
        now: ClockTime,
    ) -> Result<ScheduledAction, String> {
        validate_delay(delay_secs)?;
        self.capacity()?;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("Schedule IDs exhausted. Restart Holdmap.")?;
        let due_at_ms = now
            .wall_ms
            .checked_add(delay_secs * 1000)
            .ok_or("Invalid scheduled time.")?;
        let kind = match &operation {
            Operation::Shutdown(_) => "shutdown",
            Operation::Stops { agent: Some(_), .. } => "agent",
            Operation::Stops { .. } => "port",
        };
        let public = ScheduledAction {
            id: format!("schedule-{}", self.next_id),
            kind,
            label,
            targets,
            created_at_ms: now.wall_ms,
            due_at_ms,
            status: ScheduleStatus::Pending,
            message: None,
            cancellable: true,
        };
        self.jobs.push_back(Job {
            public: public.clone(),
            created: now,
            delay: Duration::from_secs(delay_secs),
            operation,
        });
        Ok(public)
    }

    pub(super) fn prune(&mut self) {
        while self
            .jobs
            .iter()
            .filter(|j| !j.public.status.active())
            .count()
            > MAX_HISTORY
        {
            if let Some(index) = self.jobs.iter().position(|j| !j.public.status.active()) {
                self.jobs.remove(index);
            }
        }
    }
}

#[derive(Default)]
pub struct SchedulerState {
    pub(super) queue: Mutex<Queue>,
}

impl SchedulerState {
    pub(super) fn list(&self) -> Vec<ScheduledAction> {
        self.queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .jobs
            .iter()
            .map(|j| j.public.clone())
            .collect()
    }

    pub(super) fn cancel(
        &self,
        id: &str,
        release: impl FnOnce(&Operation),
    ) -> Result<ScheduledAction, String> {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let job = queue
            .jobs
            .iter_mut()
            .find(|j| j.public.id == id)
            .ok_or("Schedule not found.")?;
        if !job.public.status.cancellable() {
            return Err(
                "This action has already started or finished and cannot be cancelled.".into(),
            );
        }
        // The queue lock orders cancellation before dispatch. Power authority is released
        // inside the same boundary, so neither immediate nor delayed shutdown can race it.
        release(&job.operation);
        job.status(
            ScheduleStatus::Cancelled,
            Some("Cancelled before any action was dispatched.".into()),
        );
        let result = job.public.clone();
        queue.prune();
        Ok(result)
    }

    /// Dependency injection keeps timing, preflight, and dispatch testable with no OS effects.
    /// A single worker calls this method: every batch is preflighted before its first effect.
    pub(super) fn tick<T>(
        &self,
        clock: impl Fn() -> ClockTime,
        prepare: impl FnOnce(&Operation) -> Result<T, String>,
        dispatch: impl FnOnce(&Operation, T) -> Result<Option<String>, String>,
        release: impl Fn(&Operation),
        changed: impl Fn(),
    ) {
        let now = clock();
        let claimed = {
            let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            let next = queue.jobs.iter_mut().find(|j| {
                j.public.status == ScheduleStatus::Pending && (j.due(now) || j.timing_problem(now))
            });
            let Some(job) = next else { return };
            if job.timing_problem(now) {
                release(&job.operation);
                job.status(ScheduleStatus::NeedsReview, Some(MISSED.into()));
                queue.prune();
                None
            } else {
                job.status(
                    ScheduleStatus::Checking,
                    Some("Checking the originally reviewed targets before dispatch.".into()),
                );
                Some((job.public.id.clone(), job.operation.clone()))
            }
        };
        changed();
        let Some((id, operation)) = claimed else {
            return;
        };
        let prepared = prepare(&operation);
        let dispatch_allowed = {
            let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            let Some(job) = queue.jobs.iter_mut().find(|j| j.public.id == id) else {
                return;
            };
            if job.public.status != ScheduleStatus::Checking {
                // A user cancelled while the fresh scan was in progress.
                return;
            }
            let checked_at = clock();
            if job.timing_problem(checked_at) {
                release(&job.operation);
                job.status(ScheduleStatus::NeedsReview, Some(MISSED.into()));
                false
            } else if let Err(error) = &prepared {
                release(&job.operation);
                job.status(
                    ScheduleStatus::NeedsReview,
                    Some(format!(
                        "No action was taken. {error} Review and create a new schedule."
                    )),
                );
                false
            } else if !job.due(checked_at) {
                // A small backwards clock adjustment is within the drift tolerance, but it
                // cannot authorize dispatch before the reviewed wall-clock due time.
                job.status(
                    ScheduleStatus::Pending,
                    Some("Waiting for the reviewed local clock time before checking again.".into()),
                );
                false
            } else {
                // Cancellation and the accepted dispatch have one atomic ordering point.
                job.status(
                    ScheduleStatus::Running,
                    Some("Action dispatched. It can no longer be cancelled.".into()),
                );
                true
            }
        };
        changed();
        if !dispatch_allowed {
            self.queue.lock().unwrap_or_else(|e| e.into_inner()).prune();
            return;
        }
        let outcome = dispatch(&operation, prepared.expect("successful preflight"));
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(job) = queue.jobs.iter_mut().find(|j| j.public.id == id) {
            match outcome {
                Ok(message) => job.status(ScheduleStatus::Completed, message),
                Err(error) => job.status(ScheduleStatus::Failed, Some(error)),
            }
        }
        queue.prune();
        drop(queue);
        changed();
    }
}

pub(super) fn validate_delay(delay_secs: u64) -> Result<(), String> {
    if !(MIN_DELAY_SECS..=MAX_DELAY_SECS).contains(&delay_secs) {
        Err("Choose a delay from 1 minute to 24 hours.".into())
    } else {
        Ok(())
    }
}

//! Local, in-memory delayed actions. Native time and retained plans grant authority;
//! frontend timers, process names, remote hosts, and persisted files never do.

mod model;
mod queue;
mod runner;
mod stop_authorization;
#[cfg(test)]
mod tests;

pub use model::{ScheduledAction, ScheduledStopRequest};
pub use queue::SchedulerState;
pub use runner::spawn;

use crate::power::{self, PowerState};
use crate::state::{scan_now, AppState};
use model::{Operation, PinnedStop};
use queue::{validate_delay, ClockTime};
use runner::{publish, release};
use std::time::Instant;
use stop_authorization::{agent_identity, validate_requests, validate_stops};
use tauri::{Manager, WebviewWindow};

#[tauri::command]
pub async fn schedule_shutdown(
    window: WebviewWindow,
    confirmation_id: String,
    delay_secs: u64,
) -> Result<ScheduledAction, String> {
    power::authorize(&window)?;
    // The reviewed delay starts when IPC is accepted, before any fresh scan or queue wait.
    let requested_at = ClockTime::now();
    validate_delay(delay_secs)?;
    if confirmation_id.is_empty() || confirmation_id.len() > 64 {
        return Err("Review shutdown again before scheduling.".into());
    }
    let app = window.app_handle().clone();
    let scheduler = app.state::<SchedulerState>();
    let mut queue = scheduler.queue.lock().unwrap_or_else(|e| e.into_inner());
    queue.capacity()?;
    let reserved = app
        .state::<PowerState>()
        .reserve(&confirmation_id, Instant::now())?;
    let result = queue.insert(
        Operation::Shutdown(reserved.clone()),
        "Shut down this computer".into(),
        vec![],
        delay_secs,
        requested_at,
    );
    if result.is_err() {
        app.state::<PowerState>().release(&reserved);
    }
    drop(queue);
    publish(&app);
    result
}

#[tauri::command]
pub async fn schedule_stops(
    window: WebviewWindow,
    requests: Vec<ScheduledStopRequest>,
    delay_secs: u64,
    agent_id: Option<String>,
) -> Result<ScheduledAction, String> {
    power::authorize(&window)?;
    // The reviewed delay starts when IPC is accepted, before any fresh scan or queue wait.
    let requested_at = ClockTime::now();
    validate_delay(delay_secs)?;
    validate_requests(&requests)?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Fail fast before consuming a preview when the queue is already full. Insertion
        // checks again after the fresh scan because another request may reserve capacity.
        app.state::<SchedulerState>()
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .capacity()?;
        let state = app.state::<AppState>();
        let plans = {
            let mut confirmations = state
                .confirmations
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            requests
                .iter()
                .map(|r| {
                    confirmations
                        .take(&r.confirmation_id, &r.target, false, false)
                        .map(|plan| PinnedStop {
                            target: r.target.clone(),
                            plan,
                        })
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let engine = scan_now(false, state.docker())?;
        let (agent, label) = match agent_id {
            Some(id) => {
                let (identity, label) = agent_identity(&engine, &id, &plans)?;
                (Some(identity), label)
            }
            None => (
                None,
                if plans.len() == 1 {
                    format!("Stop {}", plans[0].target)
                } else {
                    format!("Stop {} listeners", plans.len())
                },
            ),
        };
        validate_stops(&engine, &plans, agent.as_ref())?;
        let targets = plans.iter().map(|p| p.target.clone()).collect();
        let result = app
            .state::<SchedulerState>()
            .queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                Operation::Stops { plans, agent },
                label,
                targets,
                delay_secs,
                requested_at,
            )?;
        publish(&app);
        Ok(result)
    })
    .await
    .map_err(|_| "Couldn't create the schedule. Review the targets again.".to_owned())?
}

#[tauri::command]
pub fn list_schedules(window: WebviewWindow) -> Result<Vec<ScheduledAction>, String> {
    power::authorize(&window)?;
    Ok(window.app_handle().state::<SchedulerState>().list())
}

#[tauri::command]
pub fn cancel_schedule(window: WebviewWindow, id: String) -> Result<ScheduledAction, String> {
    power::authorize(&window)?;
    if id.is_empty() || id.len() > 64 {
        return Err("Invalid schedule ID.".into());
    }
    let app = window.app_handle();
    let result = app
        .state::<SchedulerState>()
        .cancel(&id, |operation| release(app, operation))?;
    publish(app);
    Ok(result)
}

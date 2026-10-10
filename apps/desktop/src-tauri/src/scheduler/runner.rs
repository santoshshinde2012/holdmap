//! Native worker and execution adapters. Effects are isolated from queue policy.

use super::model::*;
use super::queue::{ClockTime, SchedulerState};
use super::stop_authorization::validate_stops;
use crate::power::{self, PowerState};
use crate::state::{scan_now, AppState};
use holdmap_core::{execute, history, ActionPlan, StopReport};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub(super) fn publish(app: &AppHandle) {
    let _ = app.emit("schedules-changed", app.state::<SchedulerState>().list());
}

pub(super) fn release(app: &AppHandle, operation: &Operation) {
    if let Operation::Shutdown(reserved) = operation {
        app.state::<PowerState>().release(reserved);
    }
}

/// The execution adapter must report the listener as free, in addition to successful
/// steps. Every attempted plan gets a history record, including partial failures.
pub(super) fn execute_stops(
    plans: &[PinnedStop],
    mut execute_plan: impl FnMut(&ActionPlan) -> StopReport,
    mut record: impl FnMut(&ActionPlan, &StopReport) -> Result<(), String>,
) -> Result<Option<String>, String> {
    let mut completed = 0;
    for pinned in plans {
        let report = execute_plan(&pinned.plan);
        let history_error = record(&pinned.plan, &report).err();
        if !report.success || !report.freed {
            let detail = report.error.clone().unwrap_or_else(|| {
                if report.ports_still_busy.is_empty() {
                    "the listener did not become free".into()
                } else {
                    format!("ports still busy: {:?}", report.ports_still_busy)
                }
            });
            let history_note = history_error
                .map(|error| format!(" Stop history could not be saved: {error}."))
                .unwrap_or_default();
            return Err(format!("{} of {} targets completed; {} was not fully stopped: {}. Remaining targets were not dispatched. No automatic retry.{}", completed, plans.len(), pinned.target, detail, history_note));
        }
        completed += 1;
        if let Some(error) = history_error {
            return Err(format!("{} of {} targets stopped, but stop history could not be saved: {}. Remaining targets were not dispatched. No automatic retry.", completed, plans.len(), error));
        }
    }
    Ok(Some(format!(
        "Stopped {completed} listener targets. Stop history was saved."
    )))
}

/// Independent of scan/watch cadence and window visibility. Delays are not persisted: a
/// full app quit drops them, so reopening cannot execute an old destructive action.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || loop {
        app.state::<SchedulerState>().tick(
            ClockTime::now,
            |operation| match operation {
                Operation::Shutdown(_) => Ok(None),
                Operation::Stops { plans, agent } => {
                    let engine = scan_now(false, app.state::<AppState>().docker())?;
                    validate_stops(&engine, plans, agent.as_ref())?;
                    Ok(Some(engine))
                }
            },
            |operation, engine| match operation {
                Operation::Shutdown(reserved) => {
                    app.state::<PowerState>()
                        .request_reserved(reserved, power::execute_system)?;
                    Ok(Some(
                        "The operating system accepted the shutdown request.".into(),
                    ))
                }
                Operation::Stops { plans, .. } => {
                    let engine = engine.ok_or("Missing checked stop snapshot.")?;
                    execute_stops(
                        plans,
                        |plan| execute(plan, &mut |_| {}),
                        |plan, report| {
                            let entries = history::entries_from_plan(&engine.scan, plan, report);
                            app.state::<AppState>()
                                .store
                                .record(&entries)
                                .map_err(|error| error.to_string())
                        },
                    )
                }
            },
            |operation| release(&app, operation),
            || publish(&app),
        );
        std::thread::sleep(Duration::from_millis(250));
    });
}

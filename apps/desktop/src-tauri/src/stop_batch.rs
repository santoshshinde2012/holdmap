//! Preflight the whole reviewed agent batch, then run its retained plans. Replanning after
//! each stop would reject a second listener released by the same process's first stop.

use crate::commands::Stopped;
use crate::confirmation::Confirmations;
use crate::scheduler::{
    validate_agent_batch, validate_batch_requests, PinnedStop, ScheduledStopRequest,
};
use holdmap_core::exec::CompletedSupervisorStops;
use holdmap_core::{ActionPlan, Engine};

pub(crate) fn take(
    confirmations: &mut Confirmations,
    requests: &[ScheduledStopRequest],
) -> Result<Vec<PinnedStop>, String> {
    validate_batch_requests(requests)?;
    requests
        .iter()
        .map(|request| {
            confirmations
                .take(&request.confirmation_id, &request.target, false, false)
                .map(|plan| PinnedStop {
                    target: request.target.clone(),
                    plan,
                })
        })
        .collect()
}

pub(crate) fn run(
    engine: &Engine,
    agent_id: &str,
    plans: &[PinnedStop],
    mut dispatch: impl FnMut(&PinnedStop, &ActionPlan) -> Stopped,
) -> Result<Vec<Stopped>, String> {
    validate_agent_batch(engine, agent_id, plans)?;
    let mut results = Vec::with_capacity(plans.len());
    let mut completed_commands = CompletedSupervisorStops::default();
    for pinned in plans {
        let execution = completed_commands.remaining(&pinned.plan);
        let mut result = dispatch(pinned, &execution);
        if execution.steps.len() < pinned.plan.steps.len() {
            result.report.log.insert(
                0,
                "The same reviewed supervisor stop completed earlier in this batch; verifying this listener without repeating that command.".into(),
            );
        }
        let complete =
            result.report.success && result.report.freed && result.history_error.is_none();
        if complete {
            completed_commands.remember(&execution, &result.report);
        }
        results.push(result);
        if !complete {
            break;
        }
    }
    Ok(results)
}

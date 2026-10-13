//! Serializable UI metadata and private retained authorization.

use crate::power::ScheduledShutdown;
use holdmap_core::ActionPlan;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct ScheduledStopRequest {
    pub target: String,
    pub confirmation_id: String,
    pub force: bool,
    pub allow_protected: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleStatus {
    Pending,
    Checking,
    Running,
    Completed,
    Cancelled,
    NeedsReview,
    Failed,
}

impl ScheduleStatus {
    pub(super) fn active(self) -> bool {
        matches!(self, Self::Pending | Self::Checking | Self::Running)
    }
    pub(super) fn cancellable(self) -> bool {
        matches!(self, Self::Pending | Self::Checking)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ScheduledAction {
    pub id: String,
    pub kind: &'static str,
    pub label: String,
    pub targets: Vec<String>,
    pub created_at_ms: u64,
    pub due_at_ms: u64,
    pub status: ScheduleStatus,
    pub message: Option<String>,
    pub cancellable: bool,
}

#[derive(Clone)]
pub(crate) struct PinnedStop {
    pub(crate) target: String,
    pub(crate) plan: ActionPlan,
}

#[derive(Clone)]
pub(super) struct AgentIdentity {
    pub(super) id: String,
    pub(super) product: String,
    pub(super) pid: u32,
    pub(super) start_token: u64,
    /// Bind exact listener membership, including protocol, instead of only a port number.
    pub(super) listeners: Vec<(String, Vec<String>)>,
}

#[derive(Clone)]
pub(super) enum Operation {
    Shutdown(ScheduledShutdown),
    Stops {
        plans: Vec<PinnedStop>,
        agent: Option<AgentIdentity>,
    },
}

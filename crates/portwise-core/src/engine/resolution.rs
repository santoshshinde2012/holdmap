//! [`Resolution`]: what one [`StopStrategy`](super::StopStrategy) concluded about one port entry.

use super::types::{ActionPlan, BlockKind, Blocked, Owner, Risk, Step};

/// Per-entry resolution result produced by a stop strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub owner: Owner,
    pub headline: String,
    pub details: Vec<String>,
    pub recommendation: String,
    pub commands: Vec<String>,
    pub steps: Vec<Step>,
    pub blocked: Option<Blocked>,
    pub warnings: Vec<String>,
    pub risk: Risk,
}

impl Resolution {
    pub fn new(owner: Owner, headline: impl Into<String>) -> Self {
        Self {
            owner,
            headline: headline.into(),
            details: Vec::new(),
            recommendation: String::new(),
            commands: Vec::new(),
            steps: Vec::new(),
            blocked: None,
            warnings: Vec::new(),
            risk: Risk::Low,
        }
    }

    /// Block the plan; no override can unblock it.
    pub fn block(mut self, kind: BlockKind, msg: impl Into<String>) -> Self {
        self.blocked = Some(Blocked {
            kind,
            message: msg.into(),
            overridable: false,
        });
        self
    }

    /// Block the plan unless the user explicitly overrides (`--allow-protected`).
    pub fn block_overridable(mut self, kind: BlockKind, msg: impl Into<String>) -> Self {
        self = self.block(kind, msg);
        if let Some(b) = &mut self.blocked {
            b.overridable = true;
        }
        self
    }

    pub fn with_details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }
}

/// A plan that does nothing and explains why.
pub fn blocked_plan(
    target: String,
    owners: Vec<Owner>,
    kind: BlockKind,
    msg: impl Into<String>,
) -> ActionPlan {
    let msg = msg.into();
    ActionPlan {
        target,
        owners,
        summary: msg.clone(),
        steps: vec![],
        blocked: Some(Blocked {
            kind,
            message: msg,
            overridable: false,
        }),
        warnings: vec![],
        risk: Risk::Low,
    }
}

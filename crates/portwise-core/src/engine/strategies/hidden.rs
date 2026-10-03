//! Sockets whose owning process isn't visible to us (another user / root).

use super::{Resolution, ResolveCtx, StopStrategy};
use crate::engine::text::sudo;
use crate::engine::types::{BlockKind, Owner};
use crate::model::PortEntry;

pub struct HiddenOwnerStrategy;

impl StopStrategy for HiddenOwnerStrategy {
    fn name(&self) -> &'static str {
        "hidden-owner"
    }

    fn resolve(&self, _ctx: &ResolveCtx, e: &PortEntry, details: &[String]) -> Option<Resolution> {
        if e.process.is_some() {
            return None;
        }
        let port = e.port;
        let who = e
            .user
            .clone()
            .map(|u| format!("user {u}"))
            .or(e.uid.map(|u| format!("uid {u}")))
            .unwrap_or_else(|| "another user or root".into());
        let mut r = Resolution::new(
            Owner::Hidden {
                uid: e.uid,
                user: e.user.clone(),
            },
            format!("Port {port} is held by a process owned by {who}, which portwise can't inspect without elevated privileges."),
        )
        .with_details(details.to_vec());
        r.recommendation =
            "Re-run portwise with elevated privileges to identify and stop it.".into();
        r.commands
            .push(format!("{}portwise explain {port}", sudo()));
        Some(r.block(
            BlockKind::NeedsElevation,
            format!("The owner of port {port} belongs to {who}; elevation is required."),
        ))
    }
}

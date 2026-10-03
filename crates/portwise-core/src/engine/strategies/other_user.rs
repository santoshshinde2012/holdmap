//! Processes owned by another user: only they (or an administrator) can stop them.

use super::{Resolution, ResolveCtx, StopStrategy};
use crate::engine::text::{is_root, process_details, sudo};
use crate::engine::types::{BlockKind, Owner};
use crate::model::PortEntry;

/// Blocks processes owned by another user and explains the elevation needed.
pub struct OtherUserStrategy;

impl StopStrategy for OtherUserStrategy {
    fn name(&self) -> &'static str {
        "other-user"
    }

    fn resolve(&self, ctx: &ResolveCtx, e: &PortEntry, details: &[String]) -> Option<Resolution> {
        let p = e.process.as_ref()?;
        if ctx.table().is_mine(p.pid) || is_root() {
            return None;
        }
        let (port, pid) = (e.port, p.pid);
        let who = p.user.clone().unwrap_or_else(|| "another user".into());
        let mut r = Resolution::new(
            Owner::Process {
                pid,
                name: p.name.clone(),
            },
            format!(
                "Port {port} is held by {} (PID {pid}), owned by {who}.",
                p.name
            ),
        );
        let mut d = details.to_vec();
        d.extend(process_details(p, e));
        r.details = d;
        r.recommendation = "Only the owner or an administrator can stop it.".into();
        r.commands.push(format!("{}portwise stop {port}", sudo()));
        Some(r.block(
            BlockKind::NeedsElevation,
            format!("{} belongs to {who}; elevation is required.", p.name),
        ))
    }
}

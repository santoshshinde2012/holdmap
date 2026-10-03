//! Processes the [`ProtectionPolicy`](crate::safety::ProtectionPolicy) refuses to stop.

use super::{Resolution, ResolveCtx, StopStrategy};
use crate::engine::text::process_details;
use crate::engine::types::{BlockKind, Owner};
use crate::model::PortEntry;
use crate::safety::Protection;

/// Blocks protected processes (system, editor, terminal, agent hosts, portwise itself).
pub struct ProtectedStrategy;

impl StopStrategy for ProtectedStrategy {
    fn name(&self) -> &'static str {
        "protected"
    }

    fn resolve(&self, ctx: &ResolveCtx, e: &PortEntry, details: &[String]) -> Option<Resolution> {
        let p = e.process.as_ref()?;
        let (port, pid) = (e.port, p.pid);
        let prot = ctx.protection(p);
        let (Protection::Hard(reason) | Protection::Soft(reason)) = &prot else {
            return None;
        };
        let soft = matches!(prot, Protection::Soft(_));
        if soft && ctx.opts.allow_protected {
            return None;
        }
        let mut r = Resolution::new(
            Owner::Protected {
                pid,
                name: p.name.clone(),
                reason: reason.clone(),
            },
            format!("Port {port} is held by {} (PID {pid}): {reason}.", p.name),
        );
        let mut d = details.to_vec();
        d.extend(process_details(p, e));
        r.details = d;
        r.recommendation = if soft {
            "It's protected by default. If you're sure, stop it with --allow-protected, or use another port.".into()
        } else {
            "portwise will never stop this process. Use another port.".into()
        };
        r.commands.push(format!("portwise free-port --near {port}"));
        if soft {
            r.commands
                .push(format!("portwise stop {port} --allow-protected"));
        }
        let msg = format!("{} is protected: {reason}.", p.name);
        Some(if soft {
            r.block_overridable(BlockKind::Protected, msg)
        } else {
            r.block(BlockKind::Protected, msg)
        })
    }
}

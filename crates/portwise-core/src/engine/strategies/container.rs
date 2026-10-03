//! Containers: stop the container through its runtime API, never the host-side forwarder.

use super::{Resolution, ResolveCtx, StopStrategy};
use crate::docker;
use crate::engine::types::{BlockKind, Owner, Step};
use crate::model::PortEntry;

/// Stops containers through their runtime API instead of killing the port forwarder.
pub struct ContainerStrategy;

pub(crate) fn endpoint_string(ep: &docker::Endpoint) -> String {
    match ep {
        docker::Endpoint::Unix(p) => format!("unix://{}", p.display()),
        docker::Endpoint::Tcp(a) => format!("tcp://{a}"),
        docker::Endpoint::Pipe(p) => format!("npipe://{}", p.replace('\\', "/")),
    }
}

impl StopStrategy for ContainerStrategy {
    fn name(&self) -> &'static str {
        "container"
    }

    fn resolve(&self, ctx: &ResolveCtx, e: &PortEntry, details: &[String]) -> Option<Resolution> {
        let c = e.container.as_ref()?;
        let port = e.port;
        let mut details = details.to_vec();
        let pp = ctx.scan.published.iter().find(|pp| pp.container.id == c.id);
        let svc = match (&c.compose_project, &c.compose_service) {
            (Some(p), Some(s)) => format!(", compose project {p} / service {s}"),
            _ => String::new(),
        };
        let mut r = Resolution::new(
            Owner::Container {
                container: c.clone(),
                forwarder_pid: e.pid,
            },
            format!(
                "Port {port} is published by {} container {} ({}{svc}), mapped to port {} inside the container.",
                c.runtime, c.name, c.image, c.private_port
            ),
        );
        if let Some(p) = &e.process {
            details.push(format!(
                "The host-side listener {} (PID {}) is the runtime's port forwarder: killing it would not stop the container and can leave the runtime in a bad state.",
                p.name, p.pid
            ));
        }
        r.details = details;
        r.recommendation = format!(
            "Stop the container {} (portwise does this via the {} API).",
            c.name, c.runtime
        );
        r.commands.push(format!("docker stop {}", c.name));
        if let (Some(p), Some(s)) = (&c.compose_project, &c.compose_service) {
            r.commands.push(format!("docker compose -p {p} stop {s}"));
        }
        Some(match pp {
            Some(pp) => {
                r.steps.push(Step::StopContainer {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    runtime: c.runtime.clone(),
                    endpoint: endpoint_string(&pp.runtime.endpoint),
                    timeout_s: (ctx.opts.timeout_ms / 1000).clamp(1, 60),
                });
                r
            }
            None => r.block(
                BlockKind::NothingToStop,
                "Container runtime endpoint unknown.",
            ),
        })
    }
}

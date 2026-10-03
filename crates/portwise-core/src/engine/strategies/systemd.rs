//! systemd (Linux): socket-activated units and services with `Restart=`; stop the unit.

use super::{Resolution, ResolveCtx, StopStrategy};
use crate::model::PortEntry;

pub struct SystemdStrategy;

/// Parse `systemctl list-sockets --no-legend` lines: `LISTEN UNIT ACTIVATES`.
pub fn parse_list_sockets(text: &str, port: u16) -> Option<(String, Option<String>)> {
    let suffix = format!(":{port}");
    text.lines().find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        let listen = *f.first()?;
        (listen.ends_with(&suffix) || listen == port.to_string()).then(|| {
            let unit = f
                .iter()
                .find(|x| x.ends_with(".socket"))
                .map(|s| s.to_string())
                .unwrap_or_default();
            let service = f
                .iter()
                .find(|x| x.ends_with(".service"))
                .map(|s| s.to_string());
            (unit, service)
        })
    })
}

impl StopStrategy for SystemdStrategy {
    fn name(&self) -> &'static str {
        "systemd"
    }

    #[cfg(not(target_os = "linux"))]
    fn resolve(&self, _: &ResolveCtx, _: &PortEntry, _: &[String]) -> Option<Resolution> {
        None
    }

    #[cfg(target_os = "linux")]
    fn resolve(&self, ctx: &ResolveCtx, e: &PortEntry, details: &[String]) -> Option<Resolution> {
        let p = e.process.as_ref()?;
        linux::resolve(ctx, e, p, details)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::super::{Resolution, ResolveCtx};
    use super::parse_list_sockets;
    use crate::engine::text::{is_root, process_details};
    use crate::engine::types::{BlockKind, Owner, Step, Supervisor};
    use crate::model::{PortEntry, ProcessInfo};
    use crate::sys;
    use crate::util::run_with_timeout;
    use std::time::Duration;

    fn systemd_socket_unit(port: u16, user: bool) -> Option<(String, Option<String>)> {
        let mut args = vec!["list-sockets", "--all", "--no-legend", "--no-pager"];
        if user {
            args.insert(0, "--user");
        }
        let out = run_with_timeout("systemctl", &args, Duration::from_secs(3))?;
        parse_list_sockets(&out.stdout, port)
    }

    fn systemd_main_pid(unit: &str, user: bool) -> Option<u32> {
        let mut args = vec!["show", "-p", "MainPID", "--value", unit];
        if user {
            args.insert(0, "--user");
        }
        let out = run_with_timeout("systemctl", &args, Duration::from_secs(3))?;
        out.stdout.trim().parse().ok().filter(|p| *p != 0)
    }

    pub(super) fn resolve(
        ctx: &ResolveCtx,
        e: &PortEntry,
        p: &ProcessInfo,
        details: &[String],
    ) -> Option<Resolution> {
        let port = e.port;
        let t = ctx.table();
        // Socket activation: the listening socket is held by PID 1 / `systemd --user`.
        let is_systemd =
            p.pid == 1 || (p.name == "systemd" && p.cmdline.iter().any(|a| a == "--user"));
        if is_systemd {
            let user = p.pid != 1;
            let (unit, service) = systemd_socket_unit(port, user)
                .unwrap_or_else(|| (format!("<unit>.socket (port {port})"), None));
            let mut r = Resolution::new(
                Owner::Supervised {
                    supervisor: Supervisor::SystemdSocket { unit: unit.clone(), user, service: service.clone() },
                    pid: p.pid,
                    name: p.name.clone(),
                },
                format!("Port {port} is held by systemd socket activation ({unit}). systemd itself owns the socket, so killing the service wouldn't free it: systemd would start it again on the next connection."),
            )
            .with_details(details.to_vec());
            let mut units = vec![unit.clone()];
            units.extend(service);
            let cmd = format!(
                "{}systemctl {}stop {}",
                if user { "" } else { "sudo " },
                if user { "--user " } else { "" },
                units.join(" ")
            );
            r.recommendation = format!("Stop the socket unit (and its service): `{cmd}`.");
            r.commands.push(cmd);
            if unit.contains('<') {
                return Some(r.block(
                    BlockKind::NothingToStop,
                    "Couldn't determine the socket unit; see `systemctl list-sockets`.",
                ));
            }
            if user || is_root() {
                let mut args: Vec<String> = if user { vec!["--user".into()] } else { vec![] };
                args.push("stop".into());
                args.extend(units);
                r.steps.push(Step::RunCommand {
                    program: "systemctl".into(),
                    args,
                    reason: "stop the socket-activated unit".into(),
                });
            } else {
                r = r.block(
                    BlockKind::NeedsElevation,
                    "Stopping a system unit requires root.",
                );
            }
            return Some(r);
        }
        let cg = sys::linux::cgroup_unit(p.pid)?;
        // Our own unit (e.g. the terminal or agent service portwise runs in) is not a supervisor
        // of the dev servers started from it.
        if sys::linux::cgroup_unit(t.self_pid()).as_ref() == Some(&cg) {
            return None;
        }
        let main = systemd_main_pid(&cg.unit, cg.user)?;
        let anc = t.ancestors(p.pid);
        if !(main == p.pid || (anc.contains(&main) && !t.is_self_or_ancestor(main))) {
            return None;
        }
        let mut r = Resolution::new(
            Owner::Supervised {
                supervisor: Supervisor::SystemdService { unit: cg.unit.clone(), user: cg.user },
                pid: p.pid,
                name: p.name.clone(),
            },
            format!(
                "Port {port} is held by {} (PID {}), run by the systemd {}service {}. Killing it would let systemd restart it (Restart=), so stop the unit instead.",
                p.name,
                p.pid,
                if cg.user { "user " } else { "" },
                cg.unit
            ),
        );
        let mut d = details.to_vec();
        d.extend(process_details(p, e));
        r.details = d;
        let cmd = if cg.user {
            format!("systemctl --user stop {}", cg.unit)
        } else {
            format!("sudo systemctl stop {}", cg.unit)
        };
        r.recommendation = format!("Stop the unit: `{cmd}`.");
        r.commands.push(cmd);
        if cg.user || is_root() {
            let mut args: Vec<String> = if cg.user {
                vec!["--user".into()]
            } else {
                vec![]
            };
            args.extend(["stop".to_string(), cg.unit.clone()]);
            r.steps.push(Step::RunCommand {
                program: "systemctl".into(),
                args,
                reason: "stop the systemd unit".into(),
            });
        } else {
            r = r.block(
                BlockKind::NeedsElevation,
                format!(
                    "{} is a system service; stopping it requires root.",
                    cg.unit
                ),
            );
        }
        Some(r)
    }
}

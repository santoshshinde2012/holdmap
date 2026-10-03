//! OS features that must never be killed: macOS AirPlay Receiver, Windows HTTP.sys, the WSL relay.

use super::{Resolution, ResolveCtx, StopStrategy};
use crate::engine::types::{BlockKind, Owner};
use crate::model::PortEntry;

pub struct OsServiceStrategy;

impl StopStrategy for OsServiceStrategy {
    fn name(&self) -> &'static str {
        "os-service"
    }

    fn resolve(&self, _ctx: &ResolveCtx, e: &PortEntry, details: &[String]) -> Option<Resolution> {
        let p = e.process.as_ref()?;
        let (port, pid) = (e.port, p.pid);
        let name_l = p.name.to_ascii_lowercase();
        let mut details = details.to_vec();
        if cfg!(target_os = "macos") && name_l == "controlcenter" {
            let mut r = Resolution::new(
                Owner::OsService {
                    service: "AirPlay Receiver".into(),
                    pid: Some(pid),
                },
                format!(
                    "Port {port} is held by macOS AirPlay Receiver (ControlCenter, PID {pid})."
                ),
            );
            details.push("Since macOS Monterey, AirPlay Receiver listens on 5000 and 7000, the defaults for Flask and others.".into());
            r.details = details;
            r.recommendation = "Turn off AirPlay Receiver (System Settings → General → AirDrop & Handoff → AirPlay Receiver) or use another port.".into();
            r.commands.push(
                "open \"x-apple.systempreferences:com.apple.AirDrop-Handoff-Settings.extension\""
                    .into(),
            );
            r.commands.push(format!("portwise free-port --near {port}"));
            return Some(r.block(
                BlockKind::OsService,
                "AirPlay Receiver is part of macOS; disable it in System Settings instead of killing it.",
            ));
        }
        if cfg!(windows) && (pid == 4 || name_l == "system") {
            let mut r = Resolution::new(
                Owner::OsService { service: "HTTP.sys".into(), pid: Some(pid) },
                format!("Port {port} is owned by the Windows kernel HTTP server (HTTP.sys, PID 4) on behalf of a service such as IIS, WinRM, SSRS or Windows Media sharing."),
            )
            .with_details(details);
            r.recommendation = "Find the registered URL/service and stop or reconfigure it (elevated), or use another port.".into();
            r.commands
                .push("netsh http show servicestate view=requestq verbose=no".into());
            r.commands.push("netsh http show urlacl".into());
            return Some(r.block(
                BlockKind::OsService,
                "HTTP.sys is part of Windows; stop the service that registered the URL instead.",
            ));
        }
        if name_l.starts_with("wslrelay") {
            let mut r = Resolution::new(
                Owner::OsService { service: "WSL relay".into(), pid: Some(pid) },
                format!("Port {port} is forwarded from WSL by wslrelay.exe (PID {pid}): a server inside your WSL distro is listening."),
            )
            .with_details(details);
            r.recommendation =
                "Stop the server inside WSL (e.g. `wsl -- portwise stop <port>`), or restart WSL."
                    .into();
            r.commands.push(format!("wsl -- portwise stop {port}"));
            r.commands.push("wsl --shutdown".into());
            return Some(r.block(
                BlockKind::OsService,
                "This is the WSL localhost relay; stop the server inside WSL instead.",
            ));
        }
        None
    }
}

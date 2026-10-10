//! Plan executor: graceful ladder (SIGTERM → wait → SIGKILL), container/supervisor stops, and a
//! final "is the port actually free?" verification.

use crate::docker;
use crate::engine::{port_busy, ActionPlan, Owner, ProcRef, Step, StopReport};
use crate::process::ProcessTable;
use crate::safety::{protection, Protection};
use crate::sys::{self, Sig, SignalError};
use crate::util::{count, run_with_timeout};
use std::time::{Duration, Instant};

fn wait_exit(pids: &[&ProcRef], dur: Duration) -> Vec<u32> {
    let deadline = Instant::now() + dur;
    loop {
        let left: Vec<u32> = pids
            .iter()
            .filter(|p| sys::is_alive(p.pid, p.start_token))
            .map(|p| p.pid)
            .collect();
        if left.is_empty() || Instant::now() >= deadline {
            return left;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

struct Run<'a> {
    report: StopReport,
    progress: &'a mut dyn FnMut(&str),
    ok: bool,
}

impl Run<'_> {
    fn log(&mut self, msg: String) {
        let msg = crate::redact::line(&msg);
        (self.progress)(&msg);
        self.report.log.push(msg);
    }

    fn signal_step(
        &mut self,
        processes: &[ProcRef],
        force: bool,
        timeout_ms: u64,
        allow_protected: bool,
    ) {
        // Re-check protection against a fresh process table: never trust a stale plan.
        let fresh = ProcessTable::capture();
        let mut alive: Vec<&ProcRef> = Vec::new();
        for pr in processes {
            if !signal_authorized(pr, &fresh, allow_protected) {
                if sys::is_alive(pr.pid, pr.start_token) {
                    self.ok = false;
                    self.log(format!("skipped {} ({}): current identity or protection does not authorize signalling", pr.name, pr.pid));
                } else {
                    self.log(format!(
                        "{} ({}) had already exited or changed identity",
                        pr.name, pr.pid
                    ));
                }
                continue;
            }
            alive.push(pr);
        }
        let first = if force { Sig::Kill } else { Sig::Term };
        let mut failed_graceful = 0;
        for pr in &alive {
            match sys::signal(pr.pid, pr.start_token, first) {
                Ok(()) => {
                    self.report.signalled.push(pr.pid);
                    self.log(format!("sent {first} to {} ({})", pr.name, pr.pid));
                }
                Err(SignalError::NotFound) => {
                    self.log(format!("{} ({}) had already exited", pr.name, pr.pid))
                }
                Err(SignalError::IdentityChanged) => self.log(format!(
                    "skipped PID {}: it now belongs to a different process (PID reuse guard)",
                    pr.pid
                )),
                Err(SignalError::PermissionDenied) => {
                    self.ok = false;
                    self.log(format!(
                        "permission denied signalling {} ({})",
                        pr.name, pr.pid
                    ));
                }
                Err(SignalError::Other(e)) => {
                    failed_graceful += 1;
                    self.log(format!("{first} to {} ({}) failed: {e}", pr.name, pr.pid));
                }
            }
        }
        // If no graceful request could be delivered at all (e.g. Windows console apps),
        // don't make the user wait for the full timeout.
        let grace = if !force && failed_graceful > 0 && failed_graceful == alive.len() {
            Duration::ZERO
        } else if force {
            Duration::from_millis(2000)
        } else {
            Duration::from_millis(timeout_ms)
        };
        let mut left = wait_exit(&alive, grace);
        if !left.is_empty() && !force {
            self.report.escalated = true;
            self.log(format!(
                "{} still running after {} ms; sending SIGKILL",
                count(left.len(), "process", "processes"),
                grace.as_millis()
            ));
            let fresh = ProcessTable::capture();
            for pr in alive.iter().filter(|p| left.contains(&p.pid)) {
                if !signal_authorized(pr, &fresh, allow_protected) {
                    self.ok = false;
                    self.log(format!("skipped SIGKILL to {} ({}): current identity or protection does not authorize signalling", pr.name, pr.pid));
                    continue;
                }
                match sys::signal(pr.pid, pr.start_token, Sig::Kill) {
                    Ok(()) => {
                        if !self.report.signalled.contains(&pr.pid) {
                            self.report.signalled.push(pr.pid);
                        }
                        self.log(format!("sent SIGKILL to {} ({})", pr.name, pr.pid))
                    }
                    Err(SignalError::NotFound) => {}
                    Err(e) => self.log(format!("SIGKILL to {} ({}) failed: {e}", pr.name, pr.pid)),
                }
            }
            let still: Vec<&ProcRef> = alive
                .iter()
                .copied()
                .filter(|p| left.contains(&p.pid))
                .collect();
            left = wait_exit(&still, Duration::from_millis(3000));
        }
        if !left.is_empty() {
            self.ok = false;
            self.log(format!("still running: {left:?}"));
            self.report.survivors.extend(left);
        }
    }
}

/// Execute a plan, reporting progress lines through `progress`. Blocked plans do nothing.
pub fn execute(plan: &ActionPlan, progress: &mut dyn FnMut(&str)) -> StopReport {
    let started = Instant::now();
    let report = StopReport {
        target: plan.target.clone(),
        success: false,
        freed: false,
        ports_still_busy: vec![],
        signalled: vec![],
        escalated: false,
        survivors: vec![],
        elapsed_ms: 0,
        log: vec![],
        error: None,
    };
    let mut run = Run {
        report,
        progress,
        ok: true,
    };
    if let Some(b) = &plan.blocked {
        run.report.error = Some(b.message.clone());
        return run.report;
    }
    let mut verify = Vec::new();
    for step in &plan.steps {
        match step {
            Step::SignalProcesses {
                processes,
                force,
                timeout_ms,
            } => run.signal_step(processes, *force, *timeout_ms, plan.allow_protected),
            Step::StopContainer {
                id,
                name,
                runtime,
                endpoint,
                timeout_s,
            } => {
                run.log(format!("stopping {runtime} container {name}…"));
                match docker::parse_docker_host(endpoint) {
                    Some(ep) => match docker::stop_container(&ep, id, *timeout_s) {
                        Ok(()) => run.log(format!("container {name} stopped")),
                        Err(e) => {
                            run.ok = false;
                            run.log(format!("failed to stop container {name}: {e}"));
                        }
                    },
                    None => {
                        run.ok = false;
                        run.log(format!("unknown runtime endpoint {endpoint}"));
                    }
                }
            }
            Step::RunCommand {
                program,
                args,
                guard,
                ..
            } => {
                if !command_owners_authorized(
                    &plan.owners,
                    guard.as_ref(),
                    program,
                    args,
                    &ProcessTable::capture(),
                    plan.allow_protected,
                ) {
                    run.ok = false;
                    run.log(format!("skipped `{program}`: current service ownership or protection does not authorize stopping"));
                    continue;
                }
                let a: Vec<&str> = args.iter().map(String::as_str).collect();
                run.log(format!("running `{program} {}`", args.join(" ")));
                match run_with_timeout(program, &a, Duration::from_secs(60)) {
                    Some(o) if o.success => run.log(format!("`{program}` succeeded")),
                    Some(o) => {
                        run.ok = false;
                        run.log(format!("`{program}` failed: {}", o.stderr.trim()));
                    }
                    None => {
                        run.ok = false;
                        run.log(format!("`{program}` could not be run or timed out"));
                    }
                }
            }
            Step::VerifyFree {
                port,
                protocol,
                timeout_ms,
            } => verify.push((*port, *protocol, *timeout_ms)),
        }
    }
    let mut all_free = true;
    for (port, proto, timeout_ms) in verify {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut busy = port_busy(port, proto);
        while busy && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
            busy = port_busy(port, proto);
        }
        if busy {
            all_free = false;
            run.report.ports_still_busy.push(port);
            run.log(format!("{proto} port {port} is STILL in use"));
        } else {
            run.log(format!("{proto} port {port} is free"));
        }
    }
    let ok = run.ok;
    let mut report = run.report;
    report.freed = all_free;
    report.success = ok && all_free;
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    if !report.success {
        report.error = Some(if !all_free {
            let ports: Vec<String> = report
                .ports_still_busy
                .iter()
                .map(|p| format!(":{p}"))
                .collect();
            let (what, it) = match ports.as_slice() {
                [one] => (format!("port {one} is"), "it"),
                many => (format!("ports {} are", many.join(", ")), "them"),
            };
            format!("{what} still in use: something else may hold {it}, or a supervisor restarted the process")
        } else {
            "some steps failed (see log)".into()
        });
    }
    report
}

fn signal_authorized(process: &ProcRef, table: &ProcessTable, allow_protected: bool) -> bool {
    let Some(current) = table
        .get(process.pid)
        .filter(|p| process.start_token != 0 && p.start_token == process.start_token)
    else {
        return false;
    };
    match protection(current, table) {
        Protection::Hard(_) => false,
        Protection::Soft(_) => allow_protected,
        Protection::None => true,
    }
}

fn command_owners_authorized(
    owners: &[Owner],
    guard: Option<&ProcRef>,
    program: &str,
    args: &[String],
    table: &ProcessTable,
    allow_protected: bool,
) -> bool {
    if let Some(guard) = guard {
        return owners.iter().any(|owner| matches!(owner, Owner::Supervised { pid, .. } | Owner::OsService { pid: Some(pid), .. } if *pid == guard.pid))
            && signal_authorized(guard, table, allow_protected);
    }
    for owner in owners {
        if let Owner::Supervised { supervisor, .. } = owner {
            if !supervisor.socket_command_matches(program, args) {
                continue;
            }
            let crate::engine::Supervisor::SystemdSocket {
                unit,
                user,
                service,
            } = supervisor
            else {
                continue;
            };
            // The command stops a specific unit, not its shared protected PID1 manager.
            return socket_unit_authorized(unit, *user, service.as_deref(), table, allow_protected);
        }
    }
    false
}

fn socket_unit_authorized(
    _unit: &str,
    _user: bool,
    _service: Option<&str>,
    _table: &ProcessTable,
    _allow_protected: bool,
) -> bool {
    #[cfg(target_os = "linux")]
    {
        socket_unit_processes_authorized(_unit, _user, _service, _table, _allow_protected, |pid| {
            crate::sys::linux::cgroup_unit(pid).map(|group| (group.unit, group.user))
        })
    }
    #[cfg(not(target_os = "linux"))]
    false
}

#[cfg(any(target_os = "linux", test))]
fn socket_unit_processes_authorized(
    unit: &str,
    user: bool,
    service: Option<&str>,
    table: &ProcessTable,
    allow_protected: bool,
    group_for: impl Fn(u32) -> Option<(String, bool)>,
) -> bool {
    for process in table.iter() {
        let Some((group_unit, group_user)) = group_for(process.pid) else {
            continue;
        };
        if group_user == user && (group_unit == unit || Some(group_unit.as_str()) == service) {
            match protection(process, table) {
                Protection::Hard(_) => return false,
                Protection::Soft(_) if !allow_protected => return false,
                _ => {}
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::{proc, table};

    #[test]
    fn security_execution_rechecks_identity_and_new_session_protection() {
        let p = proc(420, 1, "node", &["node", "worker.js"]);
        let reference = ProcRef {
            pid: p.pid,
            name: p.name.clone(),
            start_token: p.start_token,
            command: p.command(),
        };
        let regular = table(vec![p.clone()], 9000);
        assert!(signal_authorized(&reference, &regular, false));
        let mut session = p.clone();
        session.name = "codex".into();
        session.cmdline = vec!["codex".into()];
        let session_table = table(vec![session], 9000);
        assert!(!signal_authorized(&reference, &session_table, false));
        assert!(signal_authorized(&reference, &session_table, true));
        let hard = table(vec![p.clone()], p.pid);
        assert!(!signal_authorized(&reference, &hard, true));
        let mut changed = p;
        changed.start_token += 1;
        assert!(!signal_authorized(
            &reference,
            &table(vec![changed], 9000),
            true
        ));
        assert!(!signal_authorized(&reference, &table(vec![], 9000), true));
    }

    #[test]
    fn security_supervisor_stop_rechecks_current_session_protection() {
        let p = proc(420, 1, "node", &["node", "worker.js"]);
        let reference = ProcRef {
            pid: p.pid,
            name: p.name.clone(),
            start_token: p.start_token,
            command: p.command(),
        };
        let owner = Owner::Supervised {
            supervisor: crate::engine::Supervisor::Pm2 {
                name: "app".into(),
                id: "1".into(),
            },
            pid: p.pid,
            name: p.name.clone(),
        };
        assert!(command_owners_authorized(
            std::slice::from_ref(&owner),
            Some(&reference),
            "pm2",
            &["stop".into(), "1".into()],
            &table(vec![p.clone()], 9000),
            false
        ));
        let mut session = p;
        session.name = "codex".into();
        session.cmdline = vec!["codex".into()];
        let current = table(vec![session], 9000);
        assert!(!command_owners_authorized(
            std::slice::from_ref(&owner),
            Some(&reference),
            "pm2",
            &["stop".into(), "1".into()],
            &current,
            false
        ));
        assert!(command_owners_authorized(
            std::slice::from_ref(&owner),
            Some(&reference),
            "pm2",
            &["stop".into(), "1".into()],
            &current,
            true
        ));
        assert!(!command_owners_authorized(
            std::slice::from_ref(&owner),
            Some(&reference),
            "pm2",
            &["stop".into(), "1".into()],
            &table(vec![], 9000),
            true
        ));
        assert!(!command_owners_authorized(
            &[],
            Some(&reference),
            "pm2",
            &[],
            &current,
            true
        ));
        assert!(!command_owners_authorized(
            std::slice::from_ref(&owner),
            None,
            "pm2",
            &["stop".into(), "1".into()],
            &current,
            true
        ));
        let mut reused = reference.clone();
        reused.start_token += 1;
        assert!(!command_owners_authorized(
            std::slice::from_ref(&owner),
            Some(&reused),
            "pm2",
            &["stop".into(), "1".into()],
            &current,
            true
        ));
    }

    #[test]
    fn security_socket_units_keep_logical_identity_and_protect_associated_sessions() {
        let supervisor = crate::engine::Supervisor::SystemdSocket {
            unit: "fixture.socket".into(),
            user: false,
            service: Some("fixture.service".into()),
        };
        assert!(supervisor.socket_command_matches(
            "systemctl",
            &[
                "stop".into(),
                "fixture.socket".into(),
                "fixture.service".into()
            ]
        ));
        assert!(!supervisor.socket_command_matches(
            "systemctl",
            &[
                "stop".into(),
                "other.socket".into(),
                "fixture.service".into()
            ]
        ));
        assert!(!supervisor.socket_command_matches(
            "sh",
            &[
                "stop".into(),
                "fixture.socket".into(),
                "fixture.service".into()
            ]
        ));
        let manager = proc(1, 0, "systemd", &["systemd"]);
        let service = proc(420, 1, "node", &["node", "worker.js"]);
        let groups = |pid| (pid == 420).then(|| ("fixture.service".into(), false));
        assert!(socket_unit_processes_authorized(
            "fixture.socket",
            false,
            Some("fixture.service"),
            &table(vec![manager.clone(), service.clone()], 9000),
            false,
            groups
        ));
        let mut session = service;
        session.name = "codex".into();
        session.cmdline = vec!["codex".into()];
        let current = table(vec![manager, session], 9000);
        assert!(!socket_unit_processes_authorized(
            "fixture.socket",
            false,
            Some("fixture.service"),
            &current,
            false,
            groups
        ));
        assert!(socket_unit_processes_authorized(
            "fixture.socket",
            false,
            Some("fixture.service"),
            &current,
            true,
            groups
        ));
    }
}

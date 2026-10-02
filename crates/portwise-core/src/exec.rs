//! Plan executor: graceful ladder (SIGTERM → wait → SIGKILL), container/supervisor stops, and a
//! final "is the port actually free?" verification.

use crate::docker;
use crate::engine::{port_busy, ActionPlan, ProcRef, Step, StopReport};
use crate::process::ProcessTable;
use crate::safety::{protection, Protection};
use crate::sys::{self, Sig, SignalError};
use crate::util::run_with_timeout;
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
        (self.progress)(&msg);
        self.report.log.push(msg);
    }

    fn signal_step(&mut self, processes: &[ProcRef], force: bool, timeout_ms: u64) {
        // Re-check protection against a fresh process table: never trust a stale plan.
        let fresh = ProcessTable::capture();
        let mut alive: Vec<&ProcRef> = Vec::new();
        for pr in processes {
            if let Some(p) = fresh
                .get(pr.pid)
                .filter(|p| p.start_token == pr.start_token)
            {
                if matches!(protection(p, &fresh), Protection::Hard(_)) {
                    self.log(format!("skipped {} ({}): protected", pr.name, pr.pid));
                    continue;
                }
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
                "{} process(es) still running after {} ms; sending SIGKILL",
                left.len(),
                grace.as_millis()
            ));
            for pr in alive.iter().filter(|p| left.contains(&p.pid)) {
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
            } => run.signal_step(processes, *force, *timeout_ms),
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
            Step::RunCommand { program, args, .. } => {
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
            format!(
                "port(s) {:?} still in use: something else may hold them, or a supervisor restarted the process",
                report.ports_still_busy
            )
        } else {
            "some steps failed (see log)".into()
        });
    }
    report
}

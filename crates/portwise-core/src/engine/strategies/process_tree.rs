//! Fallback: a plain process or dev-server tree (npm → sh -c → node) stopped gracefully.

use super::{Resolution, ResolveCtx, StopStrategy};
use crate::engine::text::{article, fmt_ms, process_details, short_cmd, tilde};
use crate::engine::types::{Owner, Risk};
use crate::model::{FrameworkCategory, PortEntry};
use crate::util::{human_duration, now_secs};

/// Stops a dev-server process tree from its launcher root (the default strategy).
pub struct ProcessTreeStrategy;

impl StopStrategy for ProcessTreeStrategy {
    fn name(&self) -> &'static str {
        "process-tree"
    }

    fn resolve(&self, ctx: &ResolveCtx, e: &PortEntry, details: &[String]) -> Option<Resolution> {
        let p = e.process.as_ref()?;
        let (port, pid) = (e.port, p.pid);
        let t = ctx.table();
        let opts = ctx.opts;
        let root = if opts.tree { ctx.tree_root(pid) } else { pid };
        let mut pids = vec![root];
        if opts.tree {
            pids.extend(ctx.safe_descendants(root));
        }
        for extra in &e.pids {
            if !pids.contains(extra)
                && t.get(*extra)
                    .is_some_and(|x| !ctx.protection(x).is_protected())
            {
                pids.push(*extra);
            }
        }
        let root_p = t.get(root).cloned().unwrap_or_else(|| p.clone());
        let what_s = match &e.framework {
            Some(f) if f.category == FrameworkCategory::DevServer => {
                format!("{} {} dev server", article(&f.name), f.name)
            }
            Some(f) => format!("{} {} process", article(&f.name), f.name),
            None => p.name.clone(),
        };
        let where_s = e
            .project
            .as_ref()
            .map(|pr| {
                format!(
                    " in {}{}",
                    tilde(&pr.root),
                    pr.git_branch
                        .as_ref()
                        .map(|b| format!(" (branch {b})"))
                        .unwrap_or_default()
                )
            })
            .unwrap_or_default();
        let started_by = if root != pid {
            format!(", started by `{}`", short_cmd(&root_p))
        } else {
            String::new()
        };
        let age = now_secs().saturating_sub(p.start_time);
        let owner = if pids.len() > 1 {
            Owner::ProcessTree {
                root_pid: root,
                root_name: root_p.name.clone(),
                pids: pids.clone(),
            }
        } else {
            Owner::Process {
                pid,
                name: p.name.clone(),
            }
        };
        let mut r = Resolution::new(
            owner,
            format!(
                "Port {port} is held by {what_s} ({}, PID {pid}){started_by}{where_s}, running for {}.",
                p.name,
                human_duration(age)
            ),
        );
        let mut details = details.to_vec();
        details.extend(process_details(p, e));
        if pids.len() > 1 {
            let chain: Vec<String> = pids
                .iter()
                .filter_map(|x| t.get(*x))
                .map(|x| format!("{} ({})", x.name, x.pid))
                .collect();
            details.push(format!("Process tree to stop: {}", chain.join(" → ")));
        }
        if cfg!(target_os = "macos") && p.ppid == Some(1) && root == pid {
            r.warnings.push(format!(
                "{} was started by launchd. If it's a LaunchAgent with KeepAlive it will be respawned; use `launchctl bootout` for it instead.",
                p.name
            ));
        }
        if !e.is_dev {
            r.risk = Risk::Medium;
            r.warnings
                .push(format!("{} doesn't look like a dev server.", p.name));
        }
        r.details = details;
        r.recommendation = if opts.force {
            format!(
                "Force-kill {} process(es) and verify port {port} is free.",
                pids.len()
            )
        } else if pids.len() > 1 {
            format!(
                "Gracefully stop the {} process tree ({} processes): SIGTERM, then SIGKILL after {}, then verify port {port} is free.",
                root_p.name,
                pids.len(),
                fmt_ms(opts.timeout_ms)
            )
        } else {
            format!(
                "Gracefully stop {} (SIGTERM, then SIGKILL after {}) and verify port {port} is free.",
                p.name,
                fmt_ms(opts.timeout_ms)
            )
        };
        r.commands.push(format!("portwise stop {port}"));
        r.commands.push(if cfg!(windows) {
            format!("taskkill /PID {root} /T")
        } else {
            format!("kill -TERM {root}")
        });
        r.steps.push(ctx.signal_step(&pids));
        Some(r)
    }
}

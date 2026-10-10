//! pm2: the God Daemon respawns killed apps, so stop through `pm2 stop`.

use super::{Resolution, ResolveCtx, StopStrategy};
use crate::engine::text::process_details;
use crate::engine::types::{BlockKind, Owner, Step, Supervisor};
use crate::model::{PortEntry, ProcessInfo};
use crate::util::run_with_timeout;
use std::time::Duration;

/// Stops pm2-managed apps with `pm2 stop` so pm2 doesn't respawn them.
pub struct Pm2Strategy;

/// Is `pid` (or an ancestor) a pm2 daemon?
pub(crate) fn under_pm2(ctx: &ResolveCtx, pid: u32) -> bool {
    let t = ctx.table();
    t.ancestors(pid).iter().filter_map(|a| t.get(*a)).any(|a| {
        a.name.starts_with("PM2") || a.cmdline.first().is_some_and(|c| c.starts_with("PM2"))
    })
}

struct Pm2App {
    name: String,
    id: String,
    pid: u32,
}

fn pm2_app(pid: u32, ancestors: &[u32]) -> Option<Pm2App> {
    let out = run_with_timeout("pm2", &["jlist"], Duration::from_secs(5))?;
    identified_app(&out.stdout, pid, ancestors)
}

/// Find the pm2 app (name, id) whose pid is `pid` or one of its ancestors in `pm2 jlist` output.
#[cfg(test)]
fn parse_jlist(json: &str, pid: u32, ancestors: &[u32]) -> Option<(String, String)> {
    identified_app(json, pid, ancestors).map(|app| (app.name, app.id))
}

fn identified_app(json: &str, pid: u32, ancestors: &[u32]) -> Option<Pm2App> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
    let apps = v.as_array()?;
    for candidate in std::iter::once(&pid).chain(ancestors) {
        let mut matching = apps.iter().filter(|app| {
            app["pid"].as_u64().and_then(|pid| u32::try_from(pid).ok()) == Some(*candidate)
        });
        let Some(app) = matching.next() else {
            continue;
        };
        if matching.next().is_some() {
            return None;
        }
        let id = u32::try_from(app["pm_id"].as_u64()?).ok()?;
        if apps
            .iter()
            .filter(|app| app["pm_id"].as_u64() == Some(u64::from(id)))
            .count()
            != 1
        {
            return None;
        }
        return Some(Pm2App {
            name: app["name"].as_str().unwrap_or("app").to_string(),
            id: id.to_string(),
            pid: *candidate,
        });
    }
    None
}

fn unresolved_app(p: &ProcessInfo, port: u16) -> Resolution {
    Resolution::new(Owner::Supervised { supervisor: Supervisor::Pm2 { name: p.name.clone(), id: String::new() }, pid: p.pid, name: p.name.clone() }, format!("Port {port} belongs to a pm2-managed process, but its numeric app ID is unavailable."))
        .block(BlockKind::NothingToStop, "Couldn't verify a unique numeric pm2 app ID; inspect `pm2 list` and stop the specific ID yourself. App names cannot safely identify a stop target.")
}

impl StopStrategy for Pm2Strategy {
    fn name(&self) -> &'static str {
        "pm2"
    }

    fn resolve(&self, ctx: &ResolveCtx, e: &PortEntry, details: &[String]) -> Option<Resolution> {
        let p = e.process.as_ref()?;
        if !under_pm2(ctx, p.pid) {
            return None;
        }
        let ancestors = ctx.table().ancestors(p.pid);
        let Some(app) = pm2_app(p.pid, &ancestors) else {
            return Some(unresolved_app(p, e.port));
        };
        let Some(guard) = ctx.proc_ref(app.pid) else {
            return Some(unresolved_app(p, e.port));
        };
        let Pm2App { name, id, pid } = app;
        let mut r = Resolution::new(
            Owner::Supervised {
                supervisor: Supervisor::Pm2 { name: name.clone(), id: id.clone() },
                pid,
                name: guard.name.clone(),
            },
            format!("Port {} is held by pm2 app \"{name}\" ({}, PID {}). pm2 restarts apps that are killed, so stop it through pm2.", e.port, p.name, p.pid),
        );
        let mut d = details.to_vec();
        d.extend(process_details(p, e));
        r.details = d;
        r.recommendation = format!("Stop it with `pm2 stop {id}`.");
        r.commands.push(format!("pm2 stop {id}"));
        r.steps.push(Step::RunCommand {
            program: "pm2".into(),
            args: vec!["stop".into(), id],
            reason: "stop the pm2-managed app".into(),
            guard: Some(guard),
        });
        Some(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jlist_matches_pid_or_ancestor() {
        let j = r#"[{"pid":10,"name":"api","pm_id":0},{"pid":20,"name":"web","pm_id":1}]"#;
        assert_eq!(parse_jlist(j, 20, &[]), Some(("web".into(), "1".into())));
        assert_eq!(parse_jlist(j, 99, &[10]), Some(("api".into(), "0".into())));
        assert_eq!(parse_jlist(j, 99, &[]), None);
        assert_eq!(parse_jlist("not json", 1, &[]), None);
        assert_eq!(identified_app(j, 99, &[10]).unwrap().pid, 10);
    }

    #[test]
    fn security_pm2_stops_require_unique_numeric_ids_without_name_fallback() {
        for json in [
            r#"[{"pid":20,"name":"all"}]"#,
            r#"[{"pid":20,"name":"duplicate"},{"pid":21,"name":"duplicate"}]"#,
            r#"[{"pid":20,"name":"all","pm_id":"all"}]"#,
            r#"[{"pid":4294967316,"name":"all","pm_id":0}]"#,
            r#"[{"pid":20,"name":"a","pm_id":0},{"pid":21,"name":"b","pm_id":0}]"#,
        ] {
            assert!(parse_jlist(json, 20, &[]).is_none(), "{json}");
        }
        assert_eq!(
            parse_jlist(
                r#"[{"pid":20,"name":"all","pm_id":0},{"pid":21,"name":"all","pm_id":1}]"#,
                20,
                &[]
            ),
            Some(("all".into(), "0".into()))
        );
        let refused = unresolved_app(&crate::process::tests::proc(20, 1, "all", &["all"]), 3000);
        assert!(refused.blocked.is_some());
        assert!(refused.steps.is_empty());
    }
}

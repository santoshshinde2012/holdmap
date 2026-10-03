//! pm2: the God Daemon respawns killed apps, so stop through `pm2 stop`.

use super::{Resolution, ResolveCtx, StopStrategy};
use crate::engine::text::process_details;
use crate::engine::types::{Owner, Step, Supervisor};
use crate::model::PortEntry;
use crate::util::run_with_timeout;
use std::time::Duration;

pub struct Pm2Strategy;

/// Is `pid` (or an ancestor) a pm2 daemon?
pub(crate) fn under_pm2(ctx: &ResolveCtx, pid: u32) -> bool {
    let t = ctx.table();
    t.ancestors(pid).iter().filter_map(|a| t.get(*a)).any(|a| {
        a.name.starts_with("PM2") || a.cmdline.first().is_some_and(|c| c.starts_with("PM2"))
    })
}

fn pm2_app(pid: u32, ancestors: &[u32]) -> Option<(String, String)> {
    let out = run_with_timeout("pm2", &["jlist"], Duration::from_secs(5))?;
    parse_jlist(&out.stdout, pid, ancestors)
}

/// Find the pm2 app (name, id) whose pid is `pid` or one of its ancestors in `pm2 jlist` output.
pub fn parse_jlist(json: &str, pid: u32, ancestors: &[u32]) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
    v.as_array()?.iter().find_map(|app| {
        let apid = app["pid"].as_u64()? as u32;
        let name = app["name"].as_str().unwrap_or("app").to_string();
        (apid == pid || ancestors.contains(&apid)).then(|| {
            let id = app["pm_id"]
                .as_u64()
                .map(|i| i.to_string())
                .unwrap_or_else(|| name.clone());
            (name, id)
        })
    })
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
        let (name, id) =
            pm2_app(p.pid, &ancestors).unwrap_or_else(|| (p.name.clone(), p.name.clone()));
        let mut r = Resolution::new(
            Owner::Supervised {
                supervisor: Supervisor::Pm2 { name: name.clone(), id: id.clone() },
                pid: p.pid,
                name: p.name.clone(),
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
    }
}

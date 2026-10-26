//! IPC adapters for skill trust, risk and scheduled scans. The rules live in
//! `holdmap-analyzers` (`trust`, `risk`, `schedule`); this wires them to the app's store, the
//! power source and OS notifications. Nothing here runs an analyzer: the window runs a due
//! scan through the normal run dock, then reports back with `schedule_finished`.

use crate::state::AppState;
use holdmap_analyzers::schedule::{self, Notice, ScanOutcome, ScheduleError};
use holdmap_analyzers::{
    risk, Due, FileStore, FindingState, PowerSource, RiskEntry, Schedule, SchedulePlan,
    ScheduleRequest, ScheduleStore, Severity, SkillTrust, SkillTrustView, TrustReport,
};
use serde::Serialize;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

fn store(app: &AppHandle) -> Arc<FileStore> {
    Arc::new(FileStore::new(
        app.state::<AppState>().store.dir().join("analyzers"),
    ))
}

fn trust(app: &AppHandle) -> SkillTrust {
    SkillTrust::new(store(app))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The power source, read without privileges.
pub(crate) fn power() -> PowerSource {
    match holdmap_core::power::on_battery() {
        Some(true) => PowerSource::Battery,
        Some(false) => PowerSource::Ac,
        None => PowerSource::Unknown,
    }
}

/// Skill trust from the last scan. Reads no folder.
#[tauri::command]
pub async fn skill_trust(app: AppHandle) -> Result<TrustReport, String> {
    crate::analyzers::blocking(move || Ok(trust(&app).report())).await
}

/// Fingerprint every skill folder in a fresh inventory (hashes, names and sizes only).
#[tauri::command]
pub async fn skill_trust_scan(app: AppHandle) -> Result<TrustReport, String> {
    crate::analyzers::blocking(move || {
        let report = crate::analyzers::inventory(&app)?;
        trust(&app).scan(&report).map_err(|e| e.to_string())
    })
    .await
}

/// Trust the version the last scan saw (with an optional reason), or stop trusting it.
#[tauri::command]
pub async fn skill_trust_set(
    app: AppHandle,
    id: String,
    trusted: bool,
    reason: Option<String>,
) -> Result<SkillTrustView, String> {
    crate::analyzers::blocking(move || {
        let t = trust(&app);
        if trusted {
            t.trust(&id, reason.as_deref())
        } else {
            t.untrust(&id)
        }
        .map_err(|e| e.to_string())
    })
    .await
}

/// Banded risk per agent and folder from stored results and the last trust scan.
#[tauri::command]
pub async fn risks(app: AppHandle) -> Result<Vec<RiskEntry>, String> {
    crate::analyzers::blocking(move || {
        let insights = crate::analyzers::service(&app).insights();
        Ok(risk::risks(&insights.results, Some(&trust(&app).report())))
    })
    .await
}

/// The schedule, what it runs and when.
#[derive(Debug, Serialize)]
pub struct ScheduleStatus {
    schedule: Schedule,
    plan: SchedulePlan,
    next_ms: Option<u64>,
    power: PowerSource,
}

fn status(app: &AppHandle, s: Schedule) -> ScheduleStatus {
    let views = crate::analyzers::service(app).list();
    let plan = SchedulePlan::new(&views, s.every_hours, &s.mcp_opt_in, s.skip_on_battery);
    ScheduleStatus {
        next_ms: s.next_ms(),
        plan,
        schedule: s,
        power: power(),
    }
}

#[tauri::command]
pub async fn schedule_status(app: AppHandle) -> Result<ScheduleStatus, String> {
    crate::analyzers::blocking(move || Ok(status(&app, store(&app).schedule()))).await
}

/// Exactly what a schedule would run, shown before the user confirms.
#[tauri::command]
pub async fn schedule_plan(
    app: AppHandle,
    every_hours: u32,
    mcp_opt_in: Vec<String>,
    skip_on_battery: bool,
) -> Result<SchedulePlan, String> {
    crate::analyzers::blocking(move || {
        let views = crate::analyzers::service(&app).list();
        Ok(SchedulePlan::new(
            &views,
            every_hours,
            &mcp_opt_in,
            skip_on_battery,
        ))
    })
    .await
}

/// Save the schedule. Turning it on or widening it needs `request.confirmed`.
#[tauri::command]
pub async fn schedule_set(
    app: AppHandle,
    request: ScheduleRequest,
) -> Result<ScheduleStatus, String> {
    crate::analyzers::blocking(move || {
        let st = store(&app);
        let views = crate::analyzers::service(&app).list();
        let next = schedule::apply(&st.schedule(), &request, &views, now_ms())
            .map_err(|e: ScheduleError| e.to_string())?;
        st.set_schedule(&next).map_err(|e| e.to_string())?;
        Ok(status(&app, next))
    })
    .await
}

/// Whether a scheduled scan is due now (the window asks while it is open).
#[tauri::command]
pub async fn schedule_due(app: AppHandle) -> Result<Due, String> {
    crate::analyzers::blocking(move || {
        let views = crate::analyzers::service(&app).list();
        Ok(schedule::due(
            &store(&app).schedule(),
            &views,
            now_ms(),
            power(),
        ))
    })
    .await
}

/// What a finished scheduled scan found and whether a notification was shown.
#[derive(Debug, Serialize)]
pub struct ScheduleFinished {
    outcome: ScanOutcome,
    notice: Option<Notice>,
}

/// Record a finished scheduled scan started at `started_ms`: re-fingerprint skills, count new
/// open high and critical findings, and show at most one throttled notification.
#[tauri::command]
pub async fn schedule_finished(
    app: AppHandle,
    started_ms: u64,
) -> Result<ScheduleFinished, String> {
    crate::analyzers::blocking(move || {
        let st = store(&app);
        let mut s = st.schedule();
        let mut outcome = ScanOutcome::default();
        for r in crate::analyzers::service(&app).insights().results {
            if r.run_at_ms < started_ms || r.changes.is_none() {
                continue;
            }
            for f in r
                .findings
                .iter()
                .filter(|f| f.new && f.state == FindingState::Open)
            {
                match f.severity {
                    Severity::Critical => outcome.new_critical += 1,
                    Severity::High => outcome.new_high += 1,
                    _ => {}
                }
            }
        }
        if let Ok(report) = crate::analyzers::inventory(&app) {
            if let Ok(t) = trust(&app).scan(&report) {
                outcome.changed_skills = t
                    .skills
                    .iter()
                    .filter(|v| v.changed_since_last_scan && v.state.alerts())
                    .count();
            }
        }
        let now = now_ms();
        s.last_run_ms = Some(now);
        let notice = schedule::notice(&s, outcome, now, holdmap_core::util::local_hour(now / 1000));
        if let Some(n) = &notice {
            s.last_notified_ms = Some(now);
            let _ = app
                .notification()
                .builder()
                .title(&n.title)
                .body(&n.body)
                .show();
        }
        st.set_schedule(&s).map_err(|e| e.to_string())?;
        Ok(ScheduleFinished { outcome, notice })
    })
    .await
}

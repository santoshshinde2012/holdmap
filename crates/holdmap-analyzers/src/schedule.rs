//! Scheduled rescans, consent first.
//!
//! Off by default. Turning it on records exactly what will run (a [`SchedulePlan`] the user
//! confirmed): which analyzers, how often, and that none of them use the network. Only
//! analyzers that are already approved and offline are eligible; an MCP analyzer only when the
//! user opted that analyzer in (its server is then started for the scheduled run, as for a run
//! you start). holdmap has no background daemon: scheduled scans run only while holdmap is
//! open (in the window or the menu bar/tray), and are skipped on battery when asked.
//!
//! Notifications are only for new high or critical findings, or skills changed since you
//! trusted them; at most one per [`MIN_NOTIFY_GAP_MS`], never in quiet hours.
//! Every decision here is a pure function of its inputs, so tests drive it with a fake clock.

use crate::manifest::RunnerKind;
use crate::service::{AnalyzerState, AnalyzerView};
use serde::{Deserialize, Serialize};
use std::io;

/// Intervals offered, in hours.
pub const INTERVALS_HOURS: [u32; 4] = [6, 12, 24, 168];
/// Notifications are at least this far apart.
pub const MIN_NOTIFY_GAP_MS: u64 = 3_600_000;
const HOUR_MS: u64 = 3_600_000;

/// Hours (local, 0–23) when no notification is shown. `start == end` means never quiet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuietHours {
    /// First quiet hour.
    pub start: u8,
    /// First hour after the quiet period.
    pub end: u8,
}

impl QuietHours {
    /// Whether `hour` (local) is quiet. Wraps midnight (22 → 7).
    pub fn contains(self, hour: u8) -> bool {
        match self.start.cmp(&self.end) {
            std::cmp::Ordering::Equal => false,
            std::cmp::Ordering::Less => (self.start..self.end).contains(&hour),
            std::cmp::Ordering::Greater => hour >= self.start || hour < self.end,
        }
    }
}

impl Default for QuietHours {
    fn default() -> Self {
        Self { start: 22, end: 7 }
    }
}

/// The saved schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    /// On only after the user confirmed a plan.
    #[serde(default)]
    pub enabled: bool,
    /// Hours between scans (one of [`INTERVALS_HOURS`]).
    pub every_hours: u32,
    /// Analyzers the user confirmed.
    #[serde(default)]
    pub analyzers: Vec<String>,
    /// MCP analyzers the user explicitly opted into scheduling.
    #[serde(default)]
    pub mcp_opt_in: Vec<String>,
    /// Skip a scan while on battery.
    pub skip_on_battery: bool,
    /// Show OS notifications.
    pub notify: bool,
    /// No notifications in these hours.
    #[serde(default)]
    pub quiet: QuietHours,
    /// When the plan was confirmed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_ms: Option<u64>,
    /// Last scheduled scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_ms: Option<u64>,
    /// Last notification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_notified_ms: Option<u64>,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            enabled: false,
            every_hours: 24,
            analyzers: Vec::new(),
            mcp_opt_in: Vec::new(),
            skip_on_battery: true,
            notify: true,
            quiet: QuietHours::default(),
            confirmed_ms: None,
            last_run_ms: None,
            last_notified_ms: None,
        }
    }
}

impl Schedule {
    /// When the next scan is due (absent when off).
    pub fn next_ms(&self) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        let from = self.last_run_ms.or(self.confirmed_ms)?;
        Some(from.saturating_add(u64::from(self.every_hours) * HOUR_MS))
    }
}

/// Why an analyzer can't be scheduled, in plain words. `None` means it can.
pub fn ineligible(view: &AnalyzerView, mcp_opted_in: bool) -> Option<&'static str> {
    if view.state != AnalyzerState::Approved {
        return Some("Turn it on first.");
    }
    if !view.permissions.network.is_empty() {
        return Some("It uses the network, so it only runs when you start it.");
    }
    if view.kind == RunnerKind::Mcp && !mcp_opted_in {
        return Some("MCP analyzers run on a schedule only if you opt this one in.");
    }
    None
}

/// One analyzer in a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedAnalyzer {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Starts a local MCP server for each scheduled run.
    pub starts_mcp_server: bool,
    /// Why it can't be scheduled (then it is left out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
}

/// Exactly what turning the schedule on will do; shown before the user confirms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulePlan {
    /// Hours between scans.
    pub every_hours: u32,
    /// Every analyzer considered, with why some are left out.
    pub analyzers: Vec<PlannedAnalyzer>,
    /// Always "none": only offline analyzers are scheduled.
    pub network: String,
    /// When scans can happen.
    pub runs_when: String,
    /// What a notification is for.
    pub notifies: String,
}

impl SchedulePlan {
    /// The plan for `every_hours` and the MCP opt-ins, over every analyzer.
    pub fn new(
        views: &[AnalyzerView],
        every_hours: u32,
        mcp_opt_in: &[String],
        skip_on_battery: bool,
    ) -> Self {
        let analyzers = views
            .iter()
            .map(|v| {
                let opted = mcp_opt_in.contains(&v.id);
                PlannedAnalyzer {
                    id: v.id.clone(),
                    name: v.name.clone(),
                    starts_mcp_server: v.kind == RunnerKind::Mcp && opted,
                    blocked: ineligible(v, opted).map(str::to_string),
                }
            })
            .collect();
        let battery = if skip_on_battery {
            ", not on battery"
        } else {
            ""
        };
        Self {
            every_hours,
            analyzers,
            network: "none".into(),
            runs_when: format!("Only while holdmap is open{battery}. holdmap has no background service; nothing runs after you quit."),
            notifies: "New high or critical findings, and skills changed since you trusted them. At most once an hour, never in quiet hours.".into(),
        }
    }

    /// The analyzers that will run.
    pub fn eligible(&self) -> Vec<String> {
        self.analyzers
            .iter()
            .filter(|a| a.blocked.is_none())
            .map(|a| a.id.clone())
            .collect()
    }
}

/// What the user asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleRequest {
    /// On or off.
    pub enabled: bool,
    /// Hours between scans.
    pub every_hours: u32,
    /// Analyzers (each must be eligible).
    pub analyzers: Vec<String>,
    /// MCP opt-ins.
    #[serde(default)]
    pub mcp_opt_in: Vec<String>,
    /// Skip on battery.
    pub skip_on_battery: bool,
    /// Notifications.
    pub notify: bool,
    /// Quiet hours.
    pub quiet: QuietHours,
    /// The user confirmed the plan shown for exactly these settings.
    #[serde(default)]
    pub confirmed: bool,
}

/// Why a schedule change was refused.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScheduleError {
    /// Turning it on needs the user's confirmation of the plan.
    #[error("confirm what will run before turning on scheduled scans")]
    NotConfirmed,
    /// An interval that isn't offered.
    #[error("choose every 6, 12 or 24 hours, or weekly")]
    Interval,
    /// Quiet hours out of range.
    #[error("quiet hours must be between 0 and 23")]
    Quiet,
    /// Nothing to run.
    #[error("choose at least one analyzer that can run on a schedule")]
    Empty,
    /// An analyzer that can't be scheduled.
    #[error("{0}: {1}")]
    Ineligible(String, String),
}

/// Apply a request to the saved schedule. Turning on (or widening what runs) needs
/// `confirmed`; turning off never does.
pub fn apply(
    saved: &Schedule,
    req: &ScheduleRequest,
    views: &[AnalyzerView],
    now_ms: u64,
) -> Result<Schedule, ScheduleError> {
    if !INTERVALS_HOURS.contains(&req.every_hours) {
        return Err(ScheduleError::Interval);
    }
    if req.quiet.start > 23 || req.quiet.end > 23 {
        return Err(ScheduleError::Quiet);
    }
    let mut next = Schedule {
        enabled: req.enabled,
        every_hours: req.every_hours,
        analyzers: req.analyzers.clone(),
        mcp_opt_in: req
            .mcp_opt_in
            .iter()
            .filter(|id| req.analyzers.contains(id))
            .cloned()
            .collect(),
        skip_on_battery: req.skip_on_battery,
        notify: req.notify,
        quiet: req.quiet,
        ..saved.clone()
    };
    next.analyzers.sort();
    next.analyzers.dedup();
    if !req.enabled {
        return Ok(next);
    }
    if next.analyzers.is_empty() {
        return Err(ScheduleError::Empty);
    }
    for id in &next.analyzers {
        let Some(v) = views.iter().find(|v| &v.id == id) else {
            return Err(ScheduleError::Ineligible(
                id.clone(),
                "not an analyzer holdmap knows".into(),
            ));
        };
        if let Some(why) = ineligible(v, next.mcp_opt_in.contains(id)) {
            return Err(ScheduleError::Ineligible(v.name.clone(), why.into()));
        }
    }
    let widened = !saved.enabled
        || next.every_hours < saved.every_hours
        || next.analyzers.iter().any(|a| !saved.analyzers.contains(a))
        || next
            .mcp_opt_in
            .iter()
            .any(|a| !saved.mcp_opt_in.contains(a))
        || (saved.skip_on_battery && !next.skip_on_battery);
    if widened && !req.confirmed {
        return Err(ScheduleError::NotConfirmed);
    }
    if widened {
        next.confirmed_ms = Some(now_ms);
    }
    Ok(next)
}

/// The machine's power source, as far as holdmap can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerSource {
    /// Plugged in.
    Ac,
    /// On battery.
    Battery,
    /// Couldn't tell (desktops without a battery report this on some systems).
    Unknown,
}

/// What to do now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Due {
    /// Scheduled scans are off.
    Off,
    /// Not yet.
    Wait {
        /// When.
        next_ms: u64,
    },
    /// Due, but skipped for now.
    Skip {
        /// Why, in plain words.
        reason: String,
    },
    /// Run these analyzers now.
    Run {
        /// Ids, each still eligible.
        analyzers: Vec<String>,
    },
}

/// Whether a scan is due at `now_ms`. Analyzers that stopped being eligible (turned off,
/// changed) are dropped; if none are left the scan is skipped.
pub fn due(s: &Schedule, views: &[AnalyzerView], now_ms: u64, power: PowerSource) -> Due {
    let Some(next_ms) = s.next_ms() else {
        return Due::Off;
    };
    if now_ms < next_ms {
        return Due::Wait { next_ms };
    }
    if s.skip_on_battery && power == PowerSource::Battery {
        return Due::Skip {
            reason: "On battery. The scan runs when you plug in.".into(),
        };
    }
    let analyzers: Vec<String> = s
        .analyzers
        .iter()
        .filter(|id| {
            views
                .iter()
                .find(|v| &v.id == *id)
                .is_some_and(|v| ineligible(v, s.mcp_opt_in.contains(id)).is_none())
        })
        .cloned()
        .collect();
    if analyzers.is_empty() {
        return Due::Skip {
            reason: "None of the scheduled analyzers can run. Review them in Settings.".into(),
        };
    }
    Due::Run { analyzers }
}

/// What a finished scheduled scan found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanOutcome {
    /// New open high findings.
    pub new_high: usize,
    /// New open critical findings.
    pub new_critical: usize,
    /// Skills that changed since the last scan and are not trusted as they are.
    pub changed_skills: usize,
}

/// A notification to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    /// Title.
    pub title: String,
    /// Body.
    pub body: String,
}

/// The notification for a finished scan, if any: off, nothing worth it, quiet hours and
/// throttling all mean none.
pub fn notice(s: &Schedule, outcome: ScanOutcome, now_ms: u64, local_hour: u8) -> Option<Notice> {
    let worth = outcome.new_high + outcome.new_critical + outcome.changed_skills > 0;
    if !s.notify || !worth || s.quiet.contains(local_hour) {
        return None;
    }
    if s.last_notified_ms
        .is_some_and(|t| now_ms.saturating_sub(t) < MIN_NOTIFY_GAP_MS)
    {
        return None;
    }
    let mut parts = Vec::new();
    if outcome.new_critical > 0 {
        parts.push(format!("{} new critical", outcome.new_critical));
    }
    if outcome.new_high > 0 {
        parts.push(format!("{} new high", outcome.new_high));
    }
    if outcome.changed_skills > 0 {
        let s = if outcome.changed_skills == 1 { "" } else { "s" };
        parts.push(format!("{} changed skill{s}", outcome.changed_skills));
    }
    Some(Notice {
        title: "holdmap scheduled scan".into(),
        body: format!("{}. Open Security to review.", parts.join(", ")),
    })
}

/// Where the schedule lives.
pub trait ScheduleStore: Send + Sync {
    /// The saved schedule (default: off).
    fn schedule(&self) -> Schedule;
    /// Save it.
    fn set_schedule(&self, schedule: &Schedule) -> io::Result<()>;
}

//! Scheduled rescans: consent first, offline only, battery and quiet hours, throttled notices.

use holdmap_analyzers::manifest::RunnerKind;
use holdmap_analyzers::schedule::{
    apply, due, notice, QuietHours, ScanOutcome, ScheduleError, MIN_NOTIFY_GAP_MS,
};
use holdmap_analyzers::{
    AnalyzerService, AnalyzerState, AnalyzerView, Due, MemoryStore, PowerSource, Schedule,
    SchedulePlan, ScheduleRequest, ScheduleStore, Stores,
};

const H: u64 = 3_600_000;

fn views() -> Vec<AnalyzerView> {
    let all = AnalyzerService::new(Stores::of(MemoryStore::default())).list();
    let mut offline = all.first().expect("a bundled analyzer").clone();
    offline.permissions.network.clear();
    offline.kind = RunnerKind::Cli;
    offline.id = "offline".into();
    offline.name = "Offline".into();
    offline.state = AnalyzerState::Approved;
    let mut off = offline.clone();
    off.id = "not-on".into();
    off.state = AnalyzerState::NotApproved;
    let mut net = offline.clone();
    net.id = "net".into();
    net.permissions.network = vec!["api.osv.dev".into()];
    let mut mcp = offline.clone();
    mcp.id = "mcp".into();
    mcp.kind = RunnerKind::Mcp;
    vec![offline, off, net, mcp]
}

fn req(analyzers: &[&str], confirmed: bool) -> ScheduleRequest {
    ScheduleRequest {
        enabled: true,
        every_hours: 24,
        analyzers: analyzers.iter().map(|s| s.to_string()).collect(),
        mcp_opt_in: vec![],
        skip_on_battery: true,
        notify: true,
        quiet: QuietHours::default(),
        confirmed,
    }
}

#[test]
fn it_is_off_by_default_and_needs_a_confirmed_plan() {
    let store = MemoryStore::default();
    let saved = store.schedule();
    assert!(!saved.enabled);
    assert_eq!(due(&saved, &views(), 0, PowerSource::Ac), Due::Off);
    assert_eq!(
        apply(&saved, &req(&["offline"], false), &views(), 10),
        Err(ScheduleError::NotConfirmed)
    );
    let on = apply(&saved, &req(&["offline"], true), &views(), 10).unwrap();
    assert_eq!((on.enabled, on.confirmed_ms), (true, Some(10)));
    // Turning it off never needs confirming.
    let mut off = req(&["offline"], false);
    off.enabled = false;
    assert!(!apply(&on, &off, &views(), 20).unwrap().enabled);
    // Adding an analyzer or running more often asks again; the same plan doesn't.
    assert!(apply(&on, &req(&["offline"], false), &views(), 30).is_ok());
    let mut more_often = req(&["offline"], false);
    more_often.every_hours = 6;
    assert_eq!(
        apply(&on, &more_often, &views(), 30),
        Err(ScheduleError::NotConfirmed)
    );
}

#[test]
fn only_approved_offline_analyzers_and_opted_in_mcp_ones_can_be_scheduled() {
    let v = views();
    let saved = Schedule::default();
    for id in ["not-on", "net", "mcp"] {
        assert!(
            matches!(
                apply(&saved, &req(&[id], true), &v, 0),
                Err(ScheduleError::Ineligible(..))
            ),
            "{id}"
        );
    }
    let mut opted = req(&["mcp"], true);
    opted.mcp_opt_in = vec!["mcp".into()];
    assert!(apply(&saved, &opted, &v, 0).is_ok());
    let plan = SchedulePlan::new(&v, 24, &[], true);
    assert_eq!(plan.eligible(), ["offline"]);
    assert_eq!(plan.network, "none");
    assert!(plan.runs_when.contains("Only while holdmap is open"));
    let mut bad = req(&["offline"], true);
    bad.every_hours = 1;
    assert_eq!(apply(&saved, &bad, &v, 0), Err(ScheduleError::Interval));
}

#[test]
fn due_waits_skips_on_battery_and_drops_analyzers_that_stopped_being_eligible() {
    let mut v = views();
    let s = apply(&Schedule::default(), &req(&["offline"], true), &v, 0).unwrap();
    assert_eq!(
        due(&s, &v, 23 * H, PowerSource::Ac),
        Due::Wait { next_ms: 24 * H }
    );
    assert!(matches!(
        due(&s, &v, 24 * H, PowerSource::Battery),
        Due::Skip { .. }
    ));
    assert_eq!(
        due(&s, &v, 24 * H, PowerSource::Unknown),
        Due::Run {
            analyzers: vec!["offline".into()]
        }
    );
    let ran = Schedule {
        last_run_ms: Some(24 * H),
        ..s.clone()
    };
    assert_eq!(
        due(&ran, &v, 30 * H, PowerSource::Ac),
        Due::Wait { next_ms: 48 * H }
    );
    v[0].state = AnalyzerState::Changed;
    assert!(matches!(
        due(&s, &v, 24 * H, PowerSource::Ac),
        Due::Skip { .. }
    ));
}

#[test]
fn notices_are_only_for_what_matters_throttled_and_quiet_at_night() {
    let s = Schedule {
        enabled: true,
        ..Schedule::default()
    };
    let high = ScanOutcome {
        new_high: 2,
        ..Default::default()
    };
    assert!(
        notice(&s, ScanOutcome::default(), 0, 12).is_none(),
        "nothing new, no notice"
    );
    assert!(notice(
        &s,
        ScanOutcome {
            new_critical: 0,
            new_high: 0,
            changed_skills: 0
        },
        0,
        12
    )
    .is_none());
    let n = notice(&s, high, 0, 12).unwrap();
    assert_eq!(n.body, "2 new high. Open Security to review.");
    assert!(
        notice(&s, high, 0, 23).is_none(),
        "quiet hours wrap midnight"
    );
    assert!(notice(&s, high, 0, 3).is_none());
    let recent = Schedule {
        last_notified_ms: Some(10),
        ..s.clone()
    };
    assert!(notice(&recent, high, 10 + MIN_NOTIFY_GAP_MS - 1, 12).is_none());
    assert!(notice(&recent, high, 10 + MIN_NOTIFY_GAP_MS, 12).is_some());
    let off = Schedule {
        notify: false,
        ..s.clone()
    };
    assert!(notice(&off, high, 0, 12).is_none());
    let changed = notice(
        &s,
        ScanOutcome {
            changed_skills: 1,
            new_critical: 1,
            ..Default::default()
        },
        0,
        12,
    )
    .unwrap();
    assert_eq!(
        changed.body,
        "1 new critical, 1 changed skill. Open Security to review."
    );
    assert!(!QuietHours { start: 5, end: 5 }.contains(5));
}

#[test]
fn a_schedule_survives_a_file_store_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let store = holdmap_analyzers::FileStore::new(tmp.path());
    assert_eq!(store.schedule(), Schedule::default());
    let s = apply(&Schedule::default(), &req(&["offline"], true), &views(), 5).unwrap();
    store.set_schedule(&s).unwrap();
    assert_eq!(holdmap_analyzers::FileStore::new(tmp.path()).schedule(), s);
}

//! Port events between two snapshots: new listeners, closed listeners and conflicts (two
//! different owners on one port, e.g. one on 127.0.0.1 and one on `::`). Drives `portwise watch`
//! and desktop notifications.

use crate::model::{PortEntry, Protocol, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
/// A change between two snapshots, as streamed by `portwise watch --json`.
pub enum PortEvent {
    /// A new listener appeared.
    Opened {
        /// The new listener.
        entry: Box<PortEntry>,
    },
    /// A listener went away.
    Closed {
        /// The listener that closed.
        entry: Box<PortEntry>,
    },
    /// Several owners listen on the same port/protocol (requests go to whichever bound the
    /// address the client resolves to — a classic `localhost` vs `127.0.0.1` surprise).
    Conflict {
        /// The port with several owners.
        port: u16,
        /// Its protocol.
        protocol: Protocol,
        /// The competing listeners.
        entries: Vec<PortEntry>,
    },
}

impl PortEvent {
    /// The port the event is about.
    pub fn port(&self) -> u16 {
        match self {
            PortEvent::Opened { entry } | PortEvent::Closed { entry } => entry.port,
            PortEvent::Conflict { port, .. } => *port,
        }
    }

    /// One-line human summary.
    pub fn summary(&self) -> String {
        match self {
            PortEvent::Opened { entry } => format!(
                "▲ :{} opened by {}{}",
                entry.port,
                entry.label,
                entry.pid.map(|p| format!(" (PID {p})")).unwrap_or_default()
            ),
            PortEvent::Closed { entry } => format!("▼ :{} closed ({})", entry.port, entry.label),
            PortEvent::Conflict { port, entries, .. } => {
                let who: Vec<String> = entries
                    .iter()
                    .map(|e| format!("{} on {}", e.label, e.addresses.join("/")))
                    .collect();
                format!(
                    "⚠ :{port} has {} owners: {}",
                    entries.len(),
                    who.join(" vs ")
                )
            }
        }
    }

    /// Is this worth a notification under `dev_only` / pinned settings?
    pub fn notable(&self, dev_only: bool, pinned: &[u16]) -> bool {
        if pinned.contains(&self.port()) {
            return true;
        }
        match self {
            PortEvent::Opened { entry } | PortEvent::Closed { entry } => !dev_only || entry.is_dev,
            PortEvent::Conflict { .. } => true,
        }
    }
}

type Key = (Protocol, u16, Option<u32>);

fn listeners(s: &Snapshot) -> BTreeMap<Key, &PortEntry> {
    s.entries
        .iter()
        .filter(|e| e.state.is_listening())
        .map(|e| ((e.protocol, e.port, e.pid), e))
        .collect()
}

fn conflicts(s: &Snapshot) -> BTreeMap<(Protocol, u16), Vec<&PortEntry>> {
    let mut by_port: BTreeMap<(Protocol, u16), Vec<&PortEntry>> = BTreeMap::new();
    for e in s
        .entries
        .iter()
        .filter(|e| e.state.is_listening() && e.protocol == Protocol::Tcp)
    {
        by_port.entry((e.protocol, e.port)).or_default().push(e);
    }
    by_port.retain(|_, v| {
        let owners: BTreeSet<Option<u32>> = v.iter().map(|e| e.pid).collect();
        owners.len() > 1
    });
    by_port
}

/// Events that turn `prev` into `next`.
pub fn diff(prev: &Snapshot, next: &Snapshot) -> Vec<PortEvent> {
    let a = listeners(prev);
    let b = listeners(next);
    let mut out = Vec::new();
    for (k, e) in &b {
        if !a.contains_key(k) {
            out.push(PortEvent::Opened {
                entry: Box::new((*e).clone()),
            });
        }
    }
    for (k, e) in &a {
        if !b.contains_key(k) {
            out.push(PortEvent::Closed {
                entry: Box::new((*e).clone()),
            });
        }
    }
    let before = conflicts(prev);
    for (k, v) in conflicts(next) {
        let owners: BTreeSet<Option<u32>> = v.iter().map(|e| e.pid).collect();
        let was: Option<BTreeSet<Option<u32>>> =
            before.get(&k).map(|v| v.iter().map(|e| e.pid).collect());
        if was.as_ref() != Some(&owners) {
            out.push(PortEvent::Conflict {
                port: k.1,
                protocol: k.0,
                entries: v.into_iter().cloned().collect(),
            });
        }
    }
    out.sort_by_key(|e| e.port());
    out
}

/// Stateful watcher: feed it snapshots, get events.
#[derive(Debug, Default)]
pub struct Watcher {
    prev: Option<Snapshot>,
}

impl Watcher {
    /// A watcher with no previous snapshot (the first diff reports nothing).
    pub fn new() -> Self {
        Self::default()
    }

    /// Events since the previous call (none for the very first snapshot, which is the baseline).
    pub fn observe(&mut self, next: Snapshot) -> Vec<PortEvent> {
        let ev = match &self.prev {
            Some(p) => diff(p, &next),
            None => Vec::new(),
        };
        self.prev = Some(next);
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::{proc, table};
    use crate::scan::build_entries;
    use crate::topology::tests::listen;

    fn snap(raw: Vec<crate::model::RawSocket>) -> Snapshot {
        let t = table(
            vec![
                proc(1, 0, "init", &[]),
                proc(5, 1, "node", &["node", "a.js"]),
                proc(6, 1, "python3", &["python3", "-m", "http.server"]),
                proc(99, 1, "portwise", &[]),
            ],
            99,
        );
        let (entries, hidden) = build_entries(&raw, &t, &[], false);
        Snapshot {
            entries,
            hidden_sockets: hidden,
            platform: "t".into(),
            taken_at_ms: 0,
            scan_ms: 0,
            docker_available: false,
            warnings: vec![],
        }
    }

    #[test]
    fn opened_closed_and_conflicts() {
        let mut w = Watcher::new();
        assert!(
            w.observe(snap(vec![listen(3000, &[5])])).is_empty(),
            "baseline"
        );
        let mut v6 = listen(3000, &[6]);
        v6.local_addr = "::1".parse().unwrap();
        v6.family = crate::model::Family::V6;
        let ev = w.observe(snap(vec![
            listen(3000, &[5]),
            v6.clone(),
            listen(8000, &[6]),
        ]));
        let kinds: Vec<&str> = ev
            .iter()
            .map(|e| match e {
                PortEvent::Opened { .. } => "opened",
                PortEvent::Closed { .. } => "closed",
                PortEvent::Conflict { .. } => "conflict",
            })
            .collect();
        assert_eq!(kinds, ["opened", "conflict", "opened"], "{ev:?}");
        assert!(ev[1].summary().contains("2 owners"), "{}", ev[1].summary());
        // Same conflict again: not re-reported.
        let ev = w.observe(snap(vec![listen(3000, &[5]), v6, listen(8000, &[6])]));
        assert!(ev.is_empty(), "{ev:?}");
        let ev = w.observe(snap(vec![listen(3000, &[5])]));
        assert_eq!(ev.len(), 2);
        assert!(ev.iter().all(|e| matches!(e, PortEvent::Closed { .. })));
        assert!(ev[0].summary().starts_with("▼ :3000"));
    }

    #[test]
    fn notable_respects_dev_only_and_pins() {
        let s = snap(vec![listen(4444, &[5])]);
        let mut e = s.entries[0].clone();
        e.is_dev = false;
        let ev = PortEvent::Opened { entry: Box::new(e) };
        assert!(!ev.notable(true, &[]));
        assert!(ev.notable(false, &[]));
        assert!(ev.notable(true, &[4444]));
    }
}

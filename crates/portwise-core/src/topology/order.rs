//! Dependency ordering for stopping groups of services.
//!
//! An edge `a → b` means *a depends on b* (a is connected to b's port). Dependents are stopped
//! before their dependencies (web → api → db), i.e. a topological order of the dependency graph.

use super::model::Graph;
use std::collections::{BTreeSet, HashMap, HashSet};

/// The result of ordering a set of nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopOrder {
    /// Node ids, dependents first.
    pub order: Vec<String>,
    /// Nodes that were part of a dependency cycle (ordered arbitrarily but deterministically).
    pub cyclic: Vec<String>,
}

/// Order `ids` so that every node comes before the nodes it depends on.
pub fn stop_order(g: &Graph, ids: &[String]) -> StopOrder {
    let set: HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
    // dependents_left[n] = number of not-yet-stopped nodes in the set that depend on n.
    let mut dependents_left: HashMap<&str, usize> = ids.iter().map(|i| (i.as_str(), 0)).collect();
    let mut deps_of: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for e in g.local_edges() {
        if e.from != e.to
            && set.contains(e.from.as_str())
            && set.contains(e.to.as_str())
            && deps_of
                .entry(e.from.as_str())
                .or_default()
                .insert(e.to.as_str())
        {
            *dependents_left.get_mut(e.to.as_str()).expect("in set") += 1;
        }
    }
    // Stable tie-break: the original order of `ids`.
    let rank: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let mut ready: BTreeSet<(usize, &str)> = dependents_left
        .iter()
        .filter(|(_, c)| **c == 0)
        .map(|(n, _)| (rank[n], *n))
        .collect();
    let mut order = Vec::with_capacity(ids.len());
    let mut done: HashSet<&str> = HashSet::new();
    let mut cyclic = Vec::new();
    while order.len() < ids.len() {
        let next = match ready.pop_first() {
            Some((_, n)) => n,
            None => {
                // Cycle: break it at the remaining node that comes first.
                let n = ids
                    .iter()
                    .map(|s| s.as_str())
                    .find(|n| !done.contains(n))
                    .expect("remaining node");
                cyclic.push(n.to_string());
                order.push(n.to_string());
                done.insert(n);
                release(n, &deps_of, &mut dependents_left, &done, &rank, &mut ready);
                continue;
            }
        };
        if !done.insert(next) {
            continue;
        }
        order.push(next.to_string());
        release(
            next,
            &deps_of,
            &mut dependents_left,
            &done,
            &rank,
            &mut ready,
        );
    }
    StopOrder { order, cyclic }
}

fn release<'a>(
    n: &'a str,
    deps_of: &HashMap<&'a str, BTreeSet<&'a str>>,
    dependents_left: &mut HashMap<&'a str, usize>,
    done: &HashSet<&'a str>,
    rank: &HashMap<&'a str, usize>,
    ready: &mut BTreeSet<(usize, &'a str)>,
) {
    for d in deps_of.get(n).into_iter().flatten() {
        let c = dependents_left.get_mut(d).expect("in set");
        *c = c.saturating_sub(1);
        if *c == 0 && !done.contains(d) {
            ready.insert((rank[d], *d));
        }
    }
}

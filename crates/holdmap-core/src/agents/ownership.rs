//! Assigns processes to the nearest recognised agent root.

use super::{catalog, AgentDetector, AgentProduct};
use crate::process::ProcessTable;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum UserIdentity<'a> {
    Uid(u32),
    Name(&'a str),
}

/// Agent roots and, for every process under one, the root it belongs to.
pub(super) fn assign(
    table: &ProcessTable,
    detector: &dyn AgentDetector,
) -> (Vec<(u32, &'static AgentProduct)>, HashMap<u32, u32>) {
    let t = table;
    let matched: HashMap<u32, &'static AgentProduct> = t
        .iter()
        .filter_map(|p| detector.identify(p).map(|x| (p.pid, x)))
        .collect();
    // The topmost process of a product is the agent; the same product below it is a helper.
    let mut roots: Vec<(u32, &'static AgentProduct)> = matched
        .iter()
        .filter(|(pid, prod)| {
            !t.ancestors(**pid)
                .iter()
                .any(|a| matched.get(a).is_some_and(|x| x.id == prod.id))
        })
        .map(|(pid, prod)| (*pid, *prod))
        .collect();
    roots.sort_by_key(|(pid, _)| *pid);
    // An editor, desktop app or agent host is one agent even when a helper was re-parented
    // away from its main process: fold extra roots of the same product and user into one.
    let mut alias: HashMap<u32, u32> = HashMap::new();
    let mut primary: HashMap<(&str, UserIdentity<'_>), u32> = HashMap::new();
    let is_helper = |pid: u32| {
        t.get(pid)
            .is_some_and(|p| catalog::norm(&p.name).contains("helper"))
    };
    let mut ordered = roots.clone();
    ordered.sort_by_key(|(pid, _)| (is_helper(*pid), *pid));
    for (pid, prod) in &ordered {
        if !prod.kind.folds_helpers() {
            continue;
        }
        // Windows reports a user name rather than a numeric UID. Missing identity is not
        // evidence that unrelated roots belong to the same account.
        let Some(user) = t.get(*pid).and_then(|p| {
            p.uid.map(UserIdentity::Uid).or_else(|| {
                p.user
                    .as_deref()
                    .filter(|name| !name.is_empty())
                    .map(UserIdentity::Name)
            })
        }) else {
            continue;
        };
        let key = (prod.id, user);
        match primary.get(&key) {
            Some(main) => {
                alias.insert(*pid, *main);
            }
            None => {
                primary.insert(key, *pid);
            }
        }
    }
    roots.retain(|(pid, _)| !alias.contains_key(pid));
    let root_set: HashSet<u32> = roots.iter().map(|(p, _)| *p).collect();
    let mut owner: HashMap<u32, u32> = HashMap::new();
    for p in t.iter() {
        let chain = std::iter::once(p.pid).chain(t.ancestors(p.pid));
        for a in chain {
            let a = alias.get(&a).copied().unwrap_or(a);
            if root_set.contains(&a) {
                owner.insert(p.pid, a);
                break;
            }
        }
    }
    (roots, owner)
}

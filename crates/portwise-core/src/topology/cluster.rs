//! Cluster detection (Strategy + registry). Each [`ClusterDetector`] proposes a group for a node;
//! the first proposal wins, in priority order compose > Kubernetes > supervisor > workspace > git.
//! Implicit groups (workspace, git) only form when at least two nodes share them.

use super::model::{Cluster, ClusterKind, Node};
use crate::model::{ProcessInfo, ProjectInfo};
use crate::process::ProcessTable;
use crate::safety::ProtectionPolicy;
use crate::tunnel::TunnelKind;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

/// What a detector may look at for one node.
pub struct NodeFacts<'a> {
    /// The node being classified.
    pub node: &'a Node,
    /// Its project, when detected.
    pub project: Option<&'a ProjectInfo>,
    /// Process table, for ancestry checks.
    pub table: &'a ProcessTable,
    /// Protection policy.
    pub policy: &'a dyn ProtectionPolicy,
}

/// A proposed cluster membership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Cluster ID (`compose:<project>`, `k8s:<ns>`, …).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Cluster kind.
    pub kind: ClusterKind,
    /// Extra detail such as the workspace tool.
    pub detail: Option<String>,
    /// Root directory, when there is one.
    pub root: Option<PathBuf>,
    /// Explicit groups (compose, k8s, supervisor) form even with a single member.
    pub explicit: bool,
}

/// Detects one kind of cluster for a node.
pub trait ClusterDetector: Send + Sync {
    /// Return the cluster this node belongs to, if any.
    fn detect(&self, facts: &NodeFacts) -> Option<Candidate>;
}

/// Ordered detectors.
pub struct ClusterRegistry {
    detectors: Vec<Box<dyn ClusterDetector>>,
}

impl Default for ClusterRegistry {
    fn default() -> Self {
        Self::empty()
            .with(ComposeDetector)
            .with(KubernetesDetector)
            .with(SupervisorDetector)
            .with(WorkspaceDetector)
            .with(GitRepoDetector)
    }
}

impl ClusterRegistry {
    /// A registry with no detectors.
    pub fn empty() -> Self {
        Self {
            detectors: Vec::new(),
        }
    }

    /// Add a detector (earlier detectors win).
    pub fn with(mut self, d: impl ClusterDetector + 'static) -> Self {
        self.detectors.push(Box::new(d));
        self
    }

    /// Assign every node to at most one cluster. Returns the clusters (members in node order).
    pub fn assign(&self, facts: &[NodeFacts]) -> (Vec<Option<String>>, Vec<Cluster>) {
        let candidates: Vec<Vec<Candidate>> = facts
            .iter()
            .map(|f| self.detectors.iter().filter_map(|d| d.detect(f)).collect())
            .collect();
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for cs in &candidates {
            for c in cs {
                *counts.entry(c.id.as_str()).or_default() += 1;
            }
        }
        let mut clusters: BTreeMap<String, Cluster> = BTreeMap::new();
        let mut order: Vec<String> = Vec::new();
        let mut assigned = Vec::with_capacity(facts.len());
        for (f, cs) in facts.iter().zip(&candidates) {
            let pick = cs
                .iter()
                .find(|c| c.explicit || counts.get(c.id.as_str()).copied().unwrap_or(0) >= 2);
            assigned.push(pick.map(|c| c.id.clone()));
            if let Some(c) = pick {
                let entry = clusters.entry(c.id.clone()).or_insert_with(|| {
                    order.push(c.id.clone());
                    Cluster {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        kind: c.kind,
                        detail: c.detail.clone(),
                        root: c.root.clone(),
                        nodes: Vec::new(),
                    }
                });
                entry.nodes.push(f.node.id.clone());
            }
        }
        let mut out: Vec<Cluster> = order
            .into_iter()
            .filter_map(|id| clusters.remove(&id))
            .collect();
        out.sort_by(|a, b| (a.kind, &a.name).cmp(&(b.kind, &b.name)));
        (assigned, out)
    }
}

/// docker compose projects, from container labels.
pub struct ComposeDetector;
impl ClusterDetector for ComposeDetector {
    fn detect(&self, f: &NodeFacts) -> Option<Candidate> {
        let c = f.node.container.as_ref()?;
        let p = c.compose_project.as_ref()?;
        Some(Candidate {
            id: format!("compose:{p}"),
            name: p.clone(),
            kind: ClusterKind::Compose,
            detail: Some(c.runtime.clone()),
            root: None,
            explicit: true,
        })
    }
}

/// Kubernetes namespaces, from `kubectl port-forward` command lines.
pub struct KubernetesDetector;
impl ClusterDetector for KubernetesDetector {
    fn detect(&self, f: &NodeFacts) -> Option<Candidate> {
        let t = f
            .node
            .tunnel
            .as_ref()
            .filter(|t| t.kind == TunnelKind::Kubectl)?;
        let ns = t.namespace.clone().unwrap_or_else(|| "default".into());
        let ctx = t.context.clone();
        Some(Candidate {
            id: format!(
                "k8s:{}{ns}",
                ctx.as_ref().map(|c| format!("{c}/")).unwrap_or_default()
            ),
            name: format!("k8s/{ns}"),
            kind: ClusterKind::Kubernetes,
            detail: ctx,
            root: None,
            explicit: true,
        })
    }
}

/// Task runners / process managers that run several services side by side.
pub fn supervisor_tool(p: &ProcessInfo) -> Option<&'static str> {
    if p.name.starts_with("PM2") || p.cmdline.first().is_some_and(|c| c.starts_with("PM2")) {
        return Some("pm2");
    }
    let full = p.name.to_ascii_lowercase();
    let first = full.split_whitespace().next().unwrap_or("");
    let n = first.strip_suffix(".exe").unwrap_or(first);
    const TOOLS: &[&str] = &[
        "turbo",
        "nx",
        "concurrently",
        "foreman",
        "overmind",
        "hivemind",
        "honcho",
        "goreman",
        "forego",
        "mprocs",
        "process-compose",
        "npm-run-all",
        "run-p",
        "lerna",
        "tilt",
        "skaffold",
    ];
    if let Some(t) = TOOLS.iter().find(|t| **t == n) {
        return Some(t);
    }
    if n.starts_with("node") || n == "bun" || n == "deno" {
        let args: Vec<String> = p
            .cmdline
            .iter()
            .skip(1)
            .take(2)
            .map(|a| a.to_ascii_lowercase())
            .collect();
        for (needle, tool) in [
            ("concurrently", "concurrently"),
            ("turbo", "turbo"),
            ("/nx/", "nx"),
            ("/.bin/nx", "nx"),
            ("npm-run-all", "npm-run-all"),
            ("run-p", "npm-run-all"),
            ("lerna", "lerna"),
            ("pm2", "pm2"),
        ] {
            if args.iter().any(|a| a.contains(needle)) {
                return Some(tool);
            }
        }
    }
    None
}

/// The nearest supervisor ancestor of `pid` (not protected, not portwise's own ancestry).
pub fn supervisor_of(
    pid: u32,
    table: &ProcessTable,
    policy: &dyn ProtectionPolicy,
) -> Option<(u32, &'static str)> {
    for anc in table.ancestors(pid) {
        let a = table.get(anc)?;
        if anc <= 1 || table.is_self_or_ancestor(anc) {
            return None;
        }
        if let Some(tool) = supervisor_tool(a) {
            return (!policy.protection(a, table).is_protected()).then_some((anc, tool));
        }
    }
    None
}

/// Supervisors and task runners (pm2, turbo, concurrently, nx) parenting several services.
pub struct SupervisorDetector;
impl ClusterDetector for SupervisorDetector {
    fn detect(&self, f: &NodeFacts) -> Option<Candidate> {
        let pid = f.node.root_pid?;
        let (sup, tool) = supervisor_of(pid, f.table, f.policy)?;
        let p = f.table.get(sup)?;
        let place = p
            .cwd
            .as_ref()
            .and_then(|c| c.file_name())
            .map(|s| s.to_string_lossy().into_owned());
        Some(Candidate {
            id: format!("sup:{sup}"),
            name: match place {
                Some(pl) => format!("{tool} · {pl}"),
                None => format!("{tool} (PID {sup})"),
            },
            kind: ClusterKind::Supervisor,
            detail: Some(tool.to_string()),
            root: p.cwd.clone(),
            explicit: true,
        })
    }
}

/// Monorepo and workspace roots.
pub struct WorkspaceDetector;
impl ClusterDetector for WorkspaceDetector {
    fn detect(&self, f: &NodeFacts) -> Option<Candidate> {
        let w = f.project?.workspace.as_ref()?;
        Some(Candidate {
            id: format!("ws:{}", w.root.display()),
            name: w.name.clone(),
            kind: ClusterKind::Workspace,
            detail: Some(w.kind.clone()),
            root: Some(w.root.clone()),
            explicit: false,
        })
    }
}

/// Git repositories.
pub struct GitRepoDetector;
impl ClusterDetector for GitRepoDetector {
    fn detect(&self, f: &NodeFacts) -> Option<Candidate> {
        let root = f.project?.git_root.as_ref()?;
        Some(Candidate {
            id: format!("git:{}", root.display()),
            name: root
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.display().to_string()),
            kind: ClusterKind::Git,
            detail: crate::project::git_branch(root),
            root: Some(root.clone()),
            explicit: false,
        })
    }
}

//! Orchestrates independent agent discovery and footprint collectors.

use super::*;
use super::{footprint, network, ownership};
use crate::model::{ProcessInfo, ProjectInfo};
use crate::process::ProcessTable;
use crate::project::{ProjectCache, ProjectDetector};
use crate::scan::Scan;
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

/// Builds an [`AgentsReport`] from a scan.
#[derive(Debug)]
pub struct AgentsBuilder<'a> {
    scan: &'a Scan,
    recent: &'a dyn RecentProjects,
    detector: &'a dyn AgentDetector,
    tool_classifier: &'a dyn ToolClassifier,
    project_cache: Arc<ProjectCache>,
    home: Option<PathBuf>,
    detect_projects: bool,
    project_budget: std::time::Duration,
}

impl<'a> AgentsBuilder<'a> {
    /// A builder over `scan` without recent-project sources, for the current user's home.
    pub fn new(scan: &'a Scan) -> Self {
        static NONE: NoRecent = NoRecent;
        static CATALOG: CatalogDetector = CatalogDetector;
        static TOOLS: DefaultToolClassifier = DefaultToolClassifier;
        Self {
            scan,
            recent: &NONE,
            detector: &CATALOG,
            tool_classifier: &TOOLS,
            project_cache: ProjectCache::global(),
            home: std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from),
            detect_projects: true,
            project_budget: crate::project::scan_budget(),
        }
    }

    /// The current account's recent-project history. Attached only to roots whose account
    /// matches the scan's current UID or, when a UID is unavailable, its current user name.
    pub fn with_recent(mut self, recent: &'a dyn RecentProjects) -> Self {
        self.recent = recent;
        self
    }

    /// Identify products with a custom detector.
    pub fn with_detector(mut self, detector: &'a dyn AgentDetector) -> Self {
        self.detector = detector;
        self
    }

    /// Describe child tools with a custom classifier.
    pub fn with_tool_classifier(mut self, classifier: &'a dyn ToolClassifier) -> Self {
        self.tool_classifier = classifier;
        self
    }

    /// Use an isolated or custom project lookup cache; the default is the process-wide cache.
    pub fn with_project_cache(mut self, cache: Arc<ProjectCache>) -> Self {
        self.project_cache = cache;
        self
    }

    /// The home directory used for privacy areas and project detection.
    pub fn with_home(mut self, home: Option<PathBuf>) -> Self {
        self.home = home;
        self
    }

    /// Whether to look for a project (manifest, git) in working directories that no port
    /// entry already explains. Never done inside privacy-protected folders.
    pub fn with_project_detection(mut self, on: bool) -> Self {
        self.detect_projects = on;
        self
    }

    /// How long to wait, in total, for project lookups that aren't cached yet (default: the
    /// scan budget). Lookups still running are left empty and show up on a later build.
    pub fn with_project_budget(mut self, budget: std::time::Duration) -> Self {
        self.project_budget = budget;
        self
    }

    fn table(&self) -> &ProcessTable {
        &self.scan.table
    }

    /// Build the report.
    pub fn build(&self) -> AgentsReport {
        let t = self.table();
        let platform = self.scan.snapshot.platform.as_str();
        let (roots, owner) = ownership::assign(t, self.detector);
        let mut members: HashMap<u32, Vec<u32>> = HashMap::new();
        for (pid, root) in &owner {
            members.entry(*root).or_default().push(*pid);
        }
        let known_projects: Vec<&ProjectInfo> = self
            .scan
            .snapshot
            .entries
            .iter()
            .filter_map(|e| e.project.as_ref())
            .collect();
        let known_paths = footprint::known_paths(self.scan);
        let mut detector = ProjectDetector::with_cache(
            self.project_cache.clone(),
            self.home.clone(),
            self.project_budget,
        );
        if self.detect_projects {
            // Start every lookup at once so one slow folder doesn't use up the budget.
            let home = self.home.as_deref();
            detector.prefetch(
                owner
                    .keys()
                    .filter_map(|p| t.get(*p)?.cwd.as_deref())
                    .filter(|c| c.parent().is_some())
                    .filter(|c| access::privacy_area(c, home, platform).is_none()),
            );
        }
        let local_addrs: HashSet<IpAddr> = self
            .scan
            .raw
            .iter()
            .map(|s| network::norm(s.local_addr))
            .collect();

        let mut agents: Vec<Agent> = roots
            .iter()
            .filter_map(|(pid, product)| {
                let root = t.get(*pid)?;
                let mut pids = members.remove(pid).unwrap_or_default();
                pids.sort_unstable();
                let parent = t
                    .ancestors(*pid)
                    .into_iter()
                    .find_map(|a| owner.get(&a).copied())
                    .map(|r| format!("agent:{r}"));
                Some(self.agent(
                    root,
                    product,
                    &pids,
                    parent,
                    &known_projects,
                    &known_paths,
                    &mut detector,
                    &local_addrs,
                    platform,
                ))
            })
            .collect();
        agents.sort_by(|a, b| a.name.cmp(&b.name).then(a.pid.cmp(&b.pid)));

        let mut limits = vec![
            "Folders are working directories and the agents' recent-project lists; open files aren't collected.".to_string(),
            "Remote hosts are shown by IP address; holdmap doesn't look names up.".to_string(),
            "Chats, settings, tokens and credentials are never read.".to_string(),
            "MCP servers are inferred from running executable or package names; tool calls and transports aren't observed.".to_string(),
            "Recent projects are account-wide history, not proof this agent is working there.".to_string(),
        ];
        if platform == "macos" {
            limits.push("macOS privacy grants (TCC) can't be read without Full Disk Access, and folders it guards aren't inspected.".into());
            limits.push("macOS may omit sockets of inaccessible processes entirely; zero hidden sockets doesn't establish complete visibility.".into());
        }
        if t.is_empty() {
            limits.push("No process metadata was collected; agents can't be identified.".into());
        } else {
            let missing_commands = t.iter().filter(|p| p.cmdline.is_empty()).count();
            let missing_folders = owner
                .keys()
                .filter(|pid| t.get(**pid).is_some_and(|p| p.cwd.is_none()))
                .count();
            if missing_commands > 0 {
                limits.push(format!("{missing_commands} visible processes have no readable command line; some agents and tools may be unidentified."));
            }
            if missing_folders > 0 {
                limits.push(format!(
                    "{missing_folders} agent-owned processes have no readable working directory."
                ));
            }
        }
        limits.extend(
            self.scan
                .snapshot
                .warnings
                .iter()
                .map(|warning| format!("Scan warning: {warning}")),
        );
        if self.scan.snapshot.hidden_sockets > 0 {
            limits.push(format!(
                "{} listening sockets belong to processes holdmap can't see (other users); they can't be attributed.",
                self.scan.snapshot.hidden_sockets
            ));
        }
        AgentsReport {
            agents,
            platform: platform.to_string(),
            taken_at_ms: self.scan.snapshot.taken_at_ms,
            limits,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn agent(
        &self,
        root: &ProcessInfo,
        product: &AgentProduct,
        pids: &[u32],
        parent: Option<String>,
        known_projects: &[&ProjectInfo],
        known_paths: &HashSet<PathBuf>,
        detector: &mut ProjectDetector,
        local_addrs: &HashSet<IpAddr>,
        platform: &str,
    ) -> Agent {
        let t = self.table();
        let set: HashSet<u32> = pids.iter().copied().collect();
        let role = |p: &ProcessInfo| {
            if p.pid == root.pid {
                ProcessRole::Agent
            } else if self
                .detector
                .identify(p)
                .is_some_and(|x| x.id == product.id)
            {
                ProcessRole::Helper
            } else {
                ProcessRole::Child
            }
        };
        let procs: Vec<&ProcessInfo> = pids.iter().filter_map(|p| t.get(*p)).collect();
        let mut processes: Vec<AgentProcess> = procs
            .iter()
            .map(|p| AgentProcess {
                pid: p.pid,
                ppid: p.ppid,
                name: p.name.clone(),
                command: p.command(),
                role: role(p),
                cwd: p.cwd.clone(),
                memory_bytes: p.memory_bytes,
                cpu_percent: p.cpu_percent,
            })
            .collect();
        processes.sort_by_key(|p| {
            (
                match p.role {
                    ProcessRole::Agent => 0,
                    ProcessRole::Child => 1,
                    ProcessRole::Helper => 2,
                },
                p.pid,
            )
        });
        let more_processes = processes.len().saturating_sub(MAX_PROCESSES);
        let memory_bytes = procs.iter().map(|p| p.memory_bytes).sum();
        let cpu_percent = procs.iter().map(|p| p.cpu_percent).sum();
        processes.truncate(MAX_PROCESSES);

        let (mut folders, more_folders, mut listed) = footprint::folders(
            self.home.as_deref(),
            self.detect_projects,
            root,
            &procs,
            known_projects,
            detector,
            platform,
        );
        let current_account = match (root.uid, t.current_uid()) {
            (Some(root), Some(current)) => root == current,
            _ => {
                root.user
                    .as_deref()
                    .is_some_and(|user| !user.trim().is_empty())
                    && t.is_mine(root.pid)
            }
        };
        // The provider describes Holdmap's account, not every account visible in the scan.
        // Skip it entirely when the root belongs to another user or its identity is unknown.
        let recent = if current_account {
            self.recent.recent(product.id, known_paths)
        } else {
            Vec::new()
        };
        for r in recent {
            if !listed.insert(r.path.clone()) {
                continue;
            }
            folders.push(AgentFolder {
                label: footprint::folder_label(&r.path, self.home.as_deref()),
                privacy_area: access::privacy_area(&r.path, self.home.as_deref(), platform)
                    .map(str::to_string),
                project: None,
                source: FolderSource::Recent,
                evidence: r.evidence,
                pids: Vec::new(),
                note: Some(format!(
                    "{}; account-wide history, not attributed to this running instance.",
                    r.note
                )),
                path: r.path,
            });
        }

        let helper_pids: HashSet<u32> = procs
            .iter()
            .filter(|p| role(p) != ProcessRole::Child)
            .map(|p| p.pid)
            .collect();
        let ports = network::ports(self.scan, &set, &helper_pids);
        let (tools, more_tools) = tools::collect(
            procs
                .iter()
                .copied()
                .filter(|p| role(p) == ProcessRole::Child),
            &ports,
            &self.scan.snapshot.entries,
            self.tool_classifier,
        );
        let (links, more_links, remote_hosts) =
            network::links(self.scan, &set, &ports, local_addrs);
        let ancestors: Vec<&ProcessInfo> = t
            .ancestors(root.pid)
            .iter()
            .filter_map(|a| t.get(*a))
            .collect();
        let mine = current_account;
        let facts = vec![
            access::user(root, mine),
            access::sandbox(product.id, root, &ancestors, &procs),
            access::approvals(product.id, root),
            access::network(&ports, remote_hosts),
            access::privacy(platform, &folders),
        ];
        Agent {
            id: format!("agent:{}", root.pid),
            product: product.id.to_string(),
            name: product.name.to_string(),
            vendor: product.vendor.to_string(),
            kind: product.kind,
            pid: root.pid,
            process_name: root.name.clone(),
            command: root.command(),
            started_at: root.start_time,
            parent,
            memory_bytes,
            cpu_percent,
            processes,
            more_processes,
            process_ids: pids.to_vec(),
            tools,
            more_tools,
            folders,
            more_folders,
            ports,
            links,
            more_links,
            access: AgentAccess {
                user: root.user.clone(),
                uid: root.uid,
                root: root.uid == Some(0),
                mine,
                facts,
            },
        }
    }
}

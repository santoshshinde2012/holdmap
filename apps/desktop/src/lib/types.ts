// Mirrors the serde types in crates/holdmap-core/src/{model,engine}.rs.

export type Protocol = "tcp" | "udp";
export type Exposure = "loopback" | "all_interfaces" | "specific";
export type Category =
  | "dev_server"
  | "app_server"
  | "web_server"
  | "database"
  | "cache"
  | "queue"
  | "container"
  | "tool"
  | "app"
  | "system";

export interface ProcessInfo {
  pid: number;
  ppid: number | null;
  name: string;
  exe: string | null;
  cmdline: string[];
  cwd: string | null;
  uid: number | null;
  user: string | null;
  start_time: number;
  start_token: number;
  memory_bytes: number;
  /** CPU usage since the previous scan (0 on the first scan). */
  cpu_percent?: number;
}

export interface ProjectInfo {
  name: string;
  root: string;
  kind: string;
  git_branch: string | null;
  workspace?: { name: string; root: string; kind: string } | null;
  git_root?: string | null;
}

export interface ContainerInfo {
  id: string;
  name: string;
  image: string;
  runtime: string;
  private_port: number;
  compose_project: string | null;
  compose_service: string | null;
}

export interface PortEntry {
  id: string;
  port: number;
  protocol: Protocol;
  state: string;
  addresses: string[];
  families: string[];
  remote: string | null;
  exposure: Exposure;
  pid: number | null;
  pids: number[];
  uid: number | null;
  user: string | null;
  process: ProcessInfo | null;
  project: ProjectInfo | null;
  framework: { name: string; category: Category } | null;
  container: ContainerInfo | null;
  label: string;
  is_dev: boolean;
  is_mine: boolean;
  protected: boolean;
  tunnel?: TunnelInfo | null;
  /** Process + helpers; same figure the graph node shows. */
  app_memory_bytes?: number;
  /** Helpers included in app_memory_bytes (excludes the owner). */
  helper_count?: number;
}

export interface TunnelInfo {
  kind: "kubectl" | "ssh" | "cloudflared" | "ngrok";
  target: string;
  namespace: string | null;
  context: string | null;
  local_port?: number | null;
}

export interface Snapshot {
  entries: PortEntry[];
  hidden_sockets: number;
  platform: string;
  taken_at_ms: number;
  scan_ms: number;
  docker_available: boolean;
  warnings: string[];
}

export type Owner =
  | { kind: "free" }
  | { kind: "process"; pid: number; name: string }
  | { kind: "process_tree"; root_pid: number; root_name: string; pids: number[] }
  | { kind: "container"; container: ContainerInfo; forwarder_pid: number | null }
  | { kind: "supervised"; supervisor: unknown; pid: number; name: string }
  | { kind: "os_service"; service: string; pid: number | null }
  | { kind: "protected"; pid: number; name: string; reason: string }
  | { kind: "hidden"; uid: number | null; user: string | null }
  | { kind: "reserved"; start: number; end: number }
  | { kind: "time_wait"; connections: number };

export interface ProcRef {
  pid: number;
  name: string;
  start_token: number;
  command: string;
}

export type Step =
  | { action: "signal_processes"; processes: ProcRef[]; force: boolean; timeout_ms: number }
  | { action: "stop_container"; id: string; name: string; runtime: string; endpoint: string; timeout_s: number }
  | { action: "run_command"; program: string; args: string[]; reason: string; guard?: ProcRef | null }
  | { action: "verify_free"; port: number; protocol: Protocol; timeout_ms: number };

export type BlockKind = "protected" | "needs_elevation" | "os_service" | "nothing_to_stop";
export type Risk = "low" | "medium" | "high";

export interface ActionPlan {
  /** Native-only, expiring one-use handle for the exact reviewed plan. */
  confirmation_id?: string;
  /** Explicit authorization to stop soft-protected processes (older reports default false). */
  allow_protected?: boolean;
  target: string;
  owners: Owner[];
  summary: string;
  steps: Step[];
  blocked: { kind: BlockKind; message: string; overridable?: boolean } | null;
  warnings: string[];
  risk: Risk;
}

/** Who is connected to a listening port (holdmap_core::details::Connections). */
export interface Peer { address: string; connections: number; local: boolean; process: string | null; pid: number | null }
export interface Connections { total: number; established: number; by_state: Record<string, number>; peers: Peer[]; more_peers: number }
export interface TreeProcess { pid: number; name: string; command: string; depth: number; memory_bytes: number; cpu_percent: number }
export interface ProcessTree { ancestors: TreeProcess[]; process: TreeProcess; children: TreeProcess[]; more_children: number }
export type RiskLevel = "low" | "medium" | "high";
export interface BindRisk { level: RiskLevel; title: string; explanation: string; fix: string | null }
/** The lazily loaded extras for one listener (holdmap_core::details::PortDetails). */
export interface PortDetails { id: string; port: number; started_at: number | null; uptime_secs: number | null; connections: Connections; tree: ProcessTree | null; bind_risk: BindRisk }

/** What an HTTP server on a local port answered (holdmap_core::http::HttpInfo). */
export interface HttpInfo {
  port: number;
  status: number;
  reason: string;
  title: string | null;
  server: string | null;
  location: string | null;
  elapsed_ms: number;
}

export interface Explanation {
  port: number;
  status: "free" | "busy" | "reserved";
  owners: Owner[];
  headline: string;
  details: string[];
  recommendation: string;
  commands: string[];
  entries: PortEntry[];
  plan: ActionPlan | null;
}

export interface StopReport {
  target: string;
  success: boolean;
  freed: boolean;
  ports_still_busy: number[];
  signalled: number[];
  escalated: boolean;
  survivors: number[];
  elapsed_ms: number;
  log: string[];
  error: string | null;
}

export interface AppInfo {
  version: string;
  platform: string;
  tray: boolean;
  shortcut?: string | null;
  config_dir?: string;
}

// ---- topology (crates/holdmap-core/src/topology/model.rs) ----

export type NodeKind = "service" | "container" | "hidden" | "client" | "external";
export type EdgeKind = "local" | "outbound" | "inbound";
export type ClusterKind = "compose" | "kubernetes" | "supervisor" | "workspace" | "git";

export interface NodePort {
  port: number;
  protocol: Protocol;
  exposure: Exposure;
  entry_id: string;
}

export interface GraphNode {
  id: string;
  kind: NodeKind;
  label: string;
  subtitle: string | null;
  root_pid: number | null;
  pids: number[];
  ports: NodePort[];
  framework: { name: string; category: Category } | null;
  project: string | null;
  project_root: string | null;
  container: ContainerInfo | null;
  tunnel: TunnelInfo | null;
  cluster: string | null;
  cpu_percent: number;
  memory_bytes: number;
  is_dev: boolean;
  protected: boolean;
}

export interface GraphEdge {
  id: string;
  from: string;
  to: string;
  kind: EdgeKind;
  port: number;
  connections: number;
  remotes: string[];
}

export interface Cluster {
  id: string;
  name: string;
  kind: ClusterKind;
  detail: string | null;
  root: string | null;
  nodes: string[];
}

export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
  clusters: Cluster[];
  stats: { nodes: number; edges: number; clusters: number; connections: number };
  taken_at_ms: number;
}

// ---- store / history ----

export interface Config {
  pins: { port: number; label: string | null }[];
  notify: boolean;
  notify_dev_only: boolean;
  history_limit: number;
  scan_interval_secs: number;
  /** Global shortcut preset id ("alt-p", "alt-space", "alt-k", "off"). */
  hotkey: string;
  recent_hosts: string[];
}

export interface HistoryEntry {
  at_ms: number;
  port: number;
  protocol: Protocol;
  label: string;
  command: string[];
  cwd: string | null;
  project: string | null;
  framework: string | null;
  pid: number;
}

export type PortEvent =
  | { event: "opened"; entry: PortEntry }
  | { event: "closed"; entry: PortEntry }
  | { event: "conflict"; port: number; protocol: Protocol; entries: PortEntry[] };

// ---- agents (crates/holdmap-core/src/agents/model.rs) ----

export type AgentKind = "cli" | "ide" | "desktop" | "extension" | "host" | "tool";
export type Evidence = "observed" | "inferred" | "unknown";
export type ProcessRole = "agent" | "helper" | "child";
export type FolderSource = "agent" | "child" | "recent";
export type PortRole = "agent" | "dev_server" | "service";
export type LinkKind = "local" | "remote";
export type AccessLevel = "restricted" | "standard" | "elevated" | "unknown";
export type AccessTopic = "user" | "sandbox" | "approvals" | "network" | "privacy";
export type AgentToolKind = "mcp_server" | "dev_server" | "shell" | "command";

export interface AgentProcess { pid: number; ppid: number | null; name: string; command: string; role: ProcessRole; cwd: string | null; memory_bytes: number; cpu_percent: number }
export interface AgentFolder { path: string; label: string; project: ProjectInfo | null; source: FolderSource; evidence: Evidence; pids: number[]; privacy_area: string | null; note: string | null }
export interface AgentPort { entry_id: string; port: number; protocol: Protocol; exposure: Exposure; pid: number | null; process: string | null; label: string; role: PortRole; project: string | null; framework: string | null }
export interface AgentLink { id: string; kind: LinkKind; label: string; address: string; port: number; connections: number; entry_id: string | null; process: string | null; pid: number | null; service: string | null }
export interface AccessFact { topic: AccessTopic; level: AccessLevel; summary: string; evidence: Evidence }
export interface AgentAccess { user: string | null; uid: number | null; root: boolean; mine: boolean; facts: AccessFact[] }
export interface AgentTool { pid: number; ppid: number | null; name: string; kind: AgentToolKind; command: string; cwd: string | null; evidence: Evidence; ports: number[]; memory_bytes: number; cpu_percent: number }

export interface Agent {
  id: string;
  product: string;
  name: string;
  vendor: string;
  kind: AgentKind;
  pid: number;
  process_name: string;
  command: string;
  started_at: number;
  parent: string | null;
  memory_bytes: number;
  cpu_percent: number;
  processes: AgentProcess[];
  /** Every owned process ID, including processes omitted from the display list. */
  process_ids: number[];
  more_processes: number;
  folders: AgentFolder[];
  more_folders: number;
  tools: AgentTool[];
  more_tools: number;
  ports: AgentPort[];
  links: AgentLink[];
  more_links: number;
  access: AgentAccess;
}

export interface AgentsReport { agents: Agent[]; platform: string; taken_at_ms: number; limits: string[] }

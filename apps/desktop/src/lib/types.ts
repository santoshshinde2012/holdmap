// Mirrors the serde types in crates/portwise-core/src/{model,engine}.rs.

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
}

export interface ProjectInfo {
  name: string;
  root: string;
  kind: string;
  git_branch: string | null;
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
  | { action: "run_command"; program: string; args: string[]; reason: string }
  | { action: "verify_free"; port: number; protocol: Protocol; timeout_ms: number };

export type BlockKind = "protected" | "needs_elevation" | "os_service" | "nothing_to_stop";
export type Risk = "low" | "medium" | "high";

export interface ActionPlan {
  target: string;
  owners: Owner[];
  summary: string;
  steps: Step[];
  blocked: { kind: BlockKind; message: string } | null;
  warnings: string[];
  risk: Risk;
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
}

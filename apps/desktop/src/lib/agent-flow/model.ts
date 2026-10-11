import type { AgentToolKind, Evidence, Protocol } from "../types";

/** Native OS observations. These identities grant no action authority to the renderer. */
export interface AgentFlowNode {
  id: string;
  kind: "agent" | "tool" | "process" | "listener" | "remote" | "unknown";
  label: string;
  agent_id: string | null;
  pid: number | null;
  start_token: number | null;
  entry_id: string | null;
  address: string | null;
  port: number | null;
  protocol: Protocol | null;
  tool_kind: AgentToolKind | null;
  evidence: Evidence;
}

export interface AgentFlowEdge {
  id: string;
  from: string;
  to: string;
  kind: "parent" | "owns_listener" | "tcp";
  evidence: Evidence;
  connections: number;
  source_pids: number[];
}

export interface AgentFlowReport {
  taken_at_ms: number;
  nodes: AgentFlowNode[];
  edges: AgentFlowEdge[];
  limits: string[];
  omitted_nodes: number;
  omitted_edges: number;
}

/** Metadata reported by a configured integration, never proof of OS process ownership. */
export interface ActivitySpan {
  id: string;
  trace_id: string;
  parent_id: string | null;
  session_id: string | null;
  agent_id: string | null;
  parent_agent_id: string | null;
  provider: "codex" | "claude" | "open_telemetry" | "holdmap";
  source: "hook" | "otlp" | "mcp";
  kind: "agent" | "tool" | "model" | "handoff" | "step";
  name: string;
  start_at_ms: number | null;
  end_at_ms: number | null;
  duration_ms: number | null;
  status: "requested" | "running" | "ok" | "error" | "cancelled" | "unknown";
  input_bytes: number | null;
  output_bytes: number | null;
  pid: number | null;
  process_started_at: number | null;
}

export interface ActivitySnapshot {
  spans: ActivitySpan[];
  received: number;
  dropped: number;
  limits: string[];
}

export interface RecordingStatus {
  active: boolean;
  endpoint: string | null;
  started_at_ms: number | null;
  received: number;
  dropped: number;
}

export interface RecordingCredentials { endpoint: string; token: string }
export type AgentFlowMode = "overview" | "connections" | "calls";

export interface AgentFlowProps {
  flowReport?: AgentFlowReport | null;
  activity?: ActivitySnapshot | null;
  recording?: RecordingStatus | null;
  flowLoading?: boolean;
  activityLoading?: boolean;
  flowError?: string | null;
  activityError?: string | null;
  demo?: boolean;
  paused?: boolean;
  onpausechange?: (paused: boolean) => void;
  onflowmodechange?: (mode: AgentFlowMode) => void;
  onrefreshflow?: () => void;
  onrefreshactivity?: () => void;
  onstartrecording?: () => Promise<void>;
  onstoprecording?: () => Promise<void>;
  onclearactivity?: () => Promise<void>;
  onactivitysetup?: (provider: "codex" | "claude") => Promise<string>;
  onclearquery?: () => void;
}

export interface VisualNode {
  id: string;
  label: string;
  sub: string;
  kind: AgentFlowNode["kind"] | ActivitySpan["kind"];
  evidence: Evidence;
  status?: ActivitySpan["status"];
  observed?: AgentFlowNode;
  span?: ActivitySpan;
}

export interface VisualEdge {
  id: string;
  from: string;
  to: string;
  label: string;
  kind: AgentFlowEdge["kind"] | "span_parent";
  evidence: Evidence;
  observed?: AgentFlowEdge;
}

export interface VisualGraph { nodes: VisualNode[]; edges: VisualEdge[] }

export const STATUS_LABEL: Record<ActivitySpan["status"], string> = {
  requested: "Requested", running: "Running", ok: "Completed", error: "Failed",
  cancelled: "Cancelled", unknown: "Status unknown",
};

export const PROVIDER_LABEL: Record<ActivitySpan["provider"], string> = {
  codex: "Codex", claude: "Claude Code", open_telemetry: "OpenTelemetry", holdmap: "Holdmap",
};

export const SOURCE_LABEL: Record<ActivitySpan["source"], string> = {
  hook: "Agent hook", otlp: "Agent telemetry", mcp: "MCP integration",
};

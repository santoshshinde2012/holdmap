// Per-agent configuration view model: MCP servers (configured vs running), skills, instruction
// files, permission rules and safety settings. Everything comes from the opt-in inventory; when
// it wasn't inspected the summary is null, never zero.
import type { Agent, AgentCapabilities, AgentMcpServer, AgentsReport, ConfigSetting } from "./types";

export interface ConfigSummary {
  mcp: number;
  running: number;
  skills: number;
  instructions: number;
  rules: number;
  /** Fetching runners (npx/uvx/docker) without an exact version or digest. */
  unpinned: number;
}

export function configSummary(agent: Agent): ConfigSummary | null {
  const c = agent.capabilities;
  if (!c?.inspected) return null;
  return {
    mcp: c.mcp_servers.length + c.more_mcp_servers,
    running: c.mcp_servers.filter((s) => s.runtime?.state === "running").length,
    skills: c.skills.length + c.more_skills,
    instructions: c.instructions?.length ?? 0,
    rules: c.tool_rules?.length ?? 0,
    unpinned: c.mcp_servers.filter(isUnpinned).length,
  };
}

export const RUNNER_LABEL: Record<string, string> = { npx: "npx", uvx: "uvx", docker: "docker", interpreter: "script", binary: "binary" };

/** A server whose runner downloads what it starts, with no exact version or digest named. */
export function isUnpinned(s: AgentMcpServer): boolean {
  return s.enabled !== false && !!s.launch && s.launch.pinned === false && ["npx", "uvx", "docker"].includes(s.launch.runner);
}

export type RunTone = "ok" | "idle" | "unknown" | "off";

/** Configured vs observed: what the run-state chip says and how sure it is. */
export function runState(s: AgentMcpServer): { tone: RunTone; label: string; detail: string } {
  if (s.enabled === false) return { tone: "off", label: "Disabled", detail: "Turned off in its configuration." };
  const pids = s.runtime?.pids ?? [];
  switch (s.runtime?.state) {
    case "running": return { tone: "ok", label: "Running", detail: `Observed: PID ${pids.join(", ")} under this agent.` };
    case "not_seen": return { tone: "idle", label: "Not running", detail: "Configured; no process of this agent runs it right now." };
    default: return {
      tone: "unknown",
      label: "Unknown",
      detail: s.transport === "http" || s.transport === "sse" ? "Remote server: connections can't be tied to it without reading traffic." : "Couldn't tell from the process table.",
    };
  }
}

/** "npx @scope/pkg" with its pinning, or the remote host, or the transport alone. */
export function launchLine(s: AgentMcpServer): string {
  if (s.launch) return [RUNNER_LABEL[s.launch.runner], s.launch.package].filter(Boolean).join(" ");
  if (s.remote_host) return s.remote_host;
  return s.transport === "unknown" ? "transport unknown" : s.transport;
}

const RISKY_SETTINGS: Record<string, Record<string, string>> = {
  "permissions.defaultMode": { bypassPermissions: "skips every permission prompt" },
  approval_policy: { never: "never asks before running commands" },
  sandbox_mode: { "danger-full-access": "runs without a sandbox" },
};

/** Settings that widen what the agent may do without asking. */
export function riskySettings(agent: Agent): { setting: ConfigSetting; why: string }[] {
  return (agent.capabilities?.settings ?? []).flatMap((setting) => {
    const why = RISKY_SETTINGS[setting.key]?.[setting.value];
    return why ? [{ setting, why }] : [];
  });
}

export const DECISION_LABEL = { allow: "Allowed", ask: "Asks first", deny: "Denied" } as const;

/** Whether an agent's configuration was read, can't be read for its product, or is off. */
export type ConfigStatus =
  | { kind: "read"; summary: ConfigSummary }
  | { kind: "unsupported"; reason: string }
  | { kind: "off" };

export function configStatus(agent: Agent): ConfigStatus {
  const summary = configSummary(agent);
  if (summary) return { kind: "read", summary };
  const c = agent.capabilities;
  if (c && !c.inspected) return { kind: "unsupported", reason: c.limits[0] ?? `holdmap doesn't read ${agent.name}'s configuration.` };
  return { kind: "off" };
}

/** Every inventory in a report: running agents first, then configured agents that aren't running. */
export function inventories(report: AgentsReport | null): { product: string; name: string; capabilities?: AgentCapabilities; running: boolean; id: string | null }[] {
  if (!report) return [];
  return [
    ...report.agents.map((a) => ({ product: a.product, name: a.name, capabilities: a.capabilities, running: true, id: a.id })),
    ...(report.configured ?? []).map((c) => ({ product: c.product, name: c.name, capabilities: c.capabilities, running: false, id: null })),
  ];
}

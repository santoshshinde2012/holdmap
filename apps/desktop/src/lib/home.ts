// The overview's view model: headline counts, one dense row per running agent and the
// "needs attention" list. Every figure comes from the latest scan, agents report and opt-in
// configuration inventory; missing data stays null (shown as "scanning"), never zero.
import { accessHeadline } from "./agents";
import { configSummary, isUnpinned, launchLine, riskySettings, type ConfigSummary } from "./agent-config";
import { uptime } from "./format";
import type { AccessLevel, Agent, AgentPort, AgentsReport, PortEntry } from "./types";

export interface Overview {
  agents: number | null;
  devServers: number | null;
  ports: number | null;
  /** Listeners reachable from other machines (bound to all interfaces). */
  exposed: number | null;
  /** Configured MCP servers across inspected agents; null when none were inspected. */
  mcpConfigured: number | null;
  /** Of those, the ones an agent's process runs right now. */
  mcpRunning: number | null;
}

export type AttentionTarget = { view: "agents"; agentId: string } | { view: "list"; entryId: string };

/** A stop to review from the attention list; it always opens the plan first, never stops directly. */
export type AttentionStop = { kind: "entry"; entryId: string } | { kind: "agent"; agentId: string };

export interface AttentionItem {
  id: string;
  /** Present when stopping is the usual fix; the app shows the stop plan before anything happens. */
  stop?: AttentionStop;
  rule: "exposed" | "root" | "elevated" | "settings" | "unpinned" | "exposed-port";
  severity: "high" | "medium";
  title: string;
  detail: string;
  target: AttentionTarget;
}

export function overview(entries: PortEntry[] | null, report: AgentsReport | null): Overview {
  const summaries = (report?.agents ?? []).map(configSummary).filter((s): s is ConfigSummary => !!s);
  return {
    agents: report ? report.agents.length : null,
    devServers: entries ? entries.filter((e) => e.is_dev).length : null,
    ports: entries ? entries.length : null,
    exposed: entries ? entries.filter((e) => e.exposure === "all_interfaces").length : null,
    mcpConfigured: summaries.length ? summaries.reduce((n, s) => n + s.mcp, 0) : null,
    mcpRunning: summaries.length ? summaries.reduce((n, s) => n + s.running, 0) : null,
  };
}

export interface AgentRow {
  agent: Agent;
  /** Observed working folders (not account history), most relevant first. */
  projects: string[];
  moreProjects: number;
  ports: AgentPort[];
  config: ConfigSummary | null;
  posture: { level: AccessLevel; text: string };
}

export function agentRow(agent: Agent): AgentRow {
  const labels = [...new Set(agent.folders.filter((f) => f.source !== "recent").map((f) => f.project?.name ?? f.label))];
  return {
    agent,
    projects: labels.slice(0, 2),
    moreProjects: Math.max(0, labels.length - 2) + (agent.more_folders ?? 0),
    ports: agent.ports,
    config: configSummary(agent),
    posture: accessHeadline(agent),
  };
}

function agentItems(agent: Agent): AttentionItem[] {
  const items: AttentionItem[] = [];
  const target: AttentionTarget = { view: "agents", agentId: agent.id };
  const open = agent.ports.filter((p) => p.exposure === "all_interfaces");
  if (open.length) {
    const ports = open.map((p) => `:${p.port}`).join(", ");
    items.push({ id: `exposed:${agent.id}`, rule: "exposed", severity: "high", stop: { kind: "agent", agentId: agent.id }, title: `${agent.name} is listening on all interfaces`, detail: `${ports} can be reached from other machines on your network.`, target });
  }
  const head = accessHeadline(agent);
  if (agent.access.root) items.push({ id: `root:${agent.id}`, rule: "root", severity: "high", title: `${agent.name} runs as root`, detail: "Everything it starts gets administrator rights.", target });
  else if (head.level === "elevated") items.push({ id: `elevated:${agent.id}`, rule: "elevated", severity: "medium", title: `${agent.name}: ${head.text}`, detail: "Started with flags that skip approvals or the sandbox.", target });
  for (const { setting, why } of riskySettings(agent)) {
    items.push({ id: `settings:${agent.id}:${setting.key}`, rule: "settings", severity: "medium", title: `${agent.name} ${why}`, detail: `${setting.key} = ${setting.value} in ${setting.source_path.split("/").slice(-2).join("/")}`, target });
  }
  const unpinned = (agent.capabilities?.mcp_servers ?? []).filter(isUnpinned);
  if (unpinned.length) {
    const list = unpinned.slice(0, 2).map((s) => `${s.name} (${launchLine(s)})`).join(", ");
    const more = unpinned.length > 2 ? ` and ${unpinned.length - 2} more` : "";
    items.push({ id: `unpinned:${agent.id}`, rule: "unpinned", severity: "medium", title: `${agent.name}: ${unpinned.length} MCP ${unpinned.length === 1 ? "server isn't" : "servers aren't"} pinned`, detail: `${list}${more} can change on the next launch.`, target });
  }
  return items;
}

/** Ports that agents own are reported once, on the agent; the rest are listed per port. */
export function attention(entries: PortEntry[] | null, report: AgentsReport | null): AttentionItem[] {
  const agents = report?.agents ?? [];
  const owned = new Set(agents.flatMap((a) => a.ports.map((p) => p.entry_id)));
  const items = agents.flatMap(agentItems);
  for (const e of entries ?? []) {
    if (e.exposure !== "all_interfaces" || !e.is_dev || owned.has(e.id)) continue;
    items.push({ id: `exposed-port:${e.id}`, rule: "exposed-port", severity: "medium", stop: { kind: "entry", entryId: e.id }, title: `${e.label} on :${e.port} is open to the network`, detail: "Dev servers usually only need localhost.", target: { view: "list", entryId: e.id } });
  }
  return items.sort((a, b) => (a.severity === b.severity ? a.title.localeCompare(b.title) : a.severity === "high" ? -1 : 1));
}

export const ACCESS_LABEL: Record<AccessLevel, string> = { restricted: "Restricted", standard: "Standard", elevated: "Elevated", unknown: "Unknown" };

export interface ServiceRow { id: string; port: number; name: string; detail: string; exposed: boolean }

/** Dev servers for the overview rail, lowest port first. */
export function services(entries: PortEntry[] | null, nowMs = Date.now()): ServiceRow[] {
  return (entries ?? []).filter((e) => e.is_dev).sort((a, b) => a.port - b.port).map((e) => ({
    id: e.id,
    port: e.port,
    name: e.project?.name ?? e.framework?.name ?? e.label,
    detail: [e.framework?.name && e.project ? e.framework.name : null, e.pid ? `PID ${e.pid}` : null, uptime(e, nowMs) ? `up ${uptime(e, nowMs)}` : null].filter(Boolean).join(" · "),
    exposed: e.exposure === "all_interfaces",
  }));
}

// The overview's view model: headline counts and the "needs you" list, with the agent each
// item belongs to. Every figure comes from the latest scan, agents report and opt-in
// configuration inventory; missing data stays null (shown as "scanning"), never zero.
import { accessHeadline } from "./agents";
import { configSummary, isUnpinned, launchLine, riskySettings, type ConfigSummary } from "./agent-config";
import { insightNeeds, type Insights } from "./analyzers";
import type { AccessLevel, Agent, AgentsReport, PortEntry } from "./types";

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
  rule: "exposed" | "root" | "elevated" | "settings" | "unpinned" | "exposed-port" | "analyzer";
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

/** Dev servers no agent started (shared by attention, the cards and the timeline). */
export function unownedDevServers(entries: PortEntry[] | null, report: AgentsReport | null): PortEntry[] {
  const owned = new Set((report?.agents ?? []).flatMap((a) => a.ports.map((p) => p.entry_id)));
  return (entries ?? []).filter((e) => e.is_dev && !owned.has(e.id));
}

/** Ports that agents own are reported once, on the agent; the rest are listed per port.
 *  Open medium-or-worse findings from approved analyzers add one item per agent. */
export function attention(entries: PortEntry[] | null, report: AgentsReport | null, insights: Insights | null = null): AttentionItem[] {
  const items = (report?.agents ?? []).flatMap(agentItems);
  for (const need of insightNeeds(insights, report)) {
    items.push({ id: need.id, rule: "analyzer", severity: need.severity, title: need.title, detail: need.detail, target: { view: "agents", agentId: need.agentId } });
  }
  for (const e of unownedDevServers(entries, report)) {
    if (e.exposure !== "all_interfaces") continue;
    items.push({ id: `exposed-port:${e.id}`, rule: "exposed-port", severity: "medium", stop: { kind: "entry", entryId: e.id }, title: `${e.label} on :${e.port} is open to the network`, detail: "Dev servers usually only need localhost.", target: { view: "list", entryId: e.id } });
  }
  return items.sort((a, b) => (a.severity === b.severity ? a.title.localeCompare(b.title) : a.severity === "high" ? -1 : 1));
}

export const ACCESS_LABEL: Record<AccessLevel, string> = { restricted: "Restricted", standard: "Standard", elevated: "Elevated", unknown: "Unknown" };

/** The agent an attention item is about: its target agent, or the agent holding its port. */
export function attentionAgent(item: AttentionItem, report: AgentsReport | null): string | null {
  if (item.target.view === "agents") return item.target.agentId;
  const entryId = item.target.entryId;
  return report?.agents.find((a) => a.ports.some((p) => p.entry_id === entryId))?.id ?? null;
}

/** Agents whose attention concerns the agent itself; port and MCP issues belong to their node. */
export function hotAgents(items: AttentionItem[]): Set<string> {
  return new Set(items.flatMap((i) => (i.target.view === "agents" && (i.rule === "root" || i.rule === "elevated" || i.rule === "settings" || (i.rule === "analyzer" && i.severity === "high")) ? [i.target.agentId] : [])));
}

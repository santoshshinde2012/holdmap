// The overview screen's view model: headline counts and the short "needs a look" list, derived
// only from the latest port scan and agents report. Missing data stays null (shown as unknown).
import { accessHeadline } from "./agents";
import type { Agent, AgentsReport, PortEntry } from "./types";

export interface Overview {
  agents: number | null;
  devServers: number | null;
  ports: number | null;
  /** Listeners reachable from other machines (bound to all interfaces). */
  exposed: number | null;
}

export type AttentionTarget = { view: "agents"; agentId: string } | { view: "list"; entryId: string };

export interface AttentionItem {
  id: string;
  severity: "high" | "medium";
  title: string;
  detail: string;
  target: AttentionTarget;
}

export function overview(entries: PortEntry[] | null, report: AgentsReport | null): Overview {
  return {
    agents: report ? report.agents.length : null,
    devServers: entries ? entries.filter((e) => e.is_dev).length : null,
    ports: entries ? entries.length : null,
    exposed: entries ? entries.filter((e) => e.exposure === "all_interfaces").length : null,
  };
}

function agentItems(agent: Agent): AttentionItem[] {
  const items: AttentionItem[] = [];
  const target: AttentionTarget = { view: "agents", agentId: agent.id };
  const open = agent.ports.filter((p) => p.exposure === "all_interfaces");
  if (open.length) {
    const ports = open.map((p) => `:${p.port}`).join(", ");
    items.push({ id: `exposed:${agent.id}`, severity: "high", title: `${agent.name} is listening on all interfaces`, detail: `${ports} can be reached from other machines on your network.`, target });
  }
  const head = accessHeadline(agent);
  if (agent.access.root) items.push({ id: `root:${agent.id}`, severity: "high", title: `${agent.name} runs as root`, detail: "Everything it starts gets administrator rights.", target });
  else if (head.level === "elevated") items.push({ id: `elevated:${agent.id}`, severity: "medium", title: `${agent.name}: ${head.text}`, detail: "Check its sandbox and approval settings.", target });
  return items;
}

/** Ports that agents own are reported once, on the agent; the rest are listed per port. */
export function attention(entries: PortEntry[] | null, report: AgentsReport | null): AttentionItem[] {
  const agents = report?.agents ?? [];
  const owned = new Set(agents.flatMap((a) => a.ports.map((p) => p.entry_id)));
  const items = agents.flatMap(agentItems);
  for (const e of entries ?? []) {
    if (e.exposure !== "all_interfaces" || !e.is_dev || owned.has(e.id)) continue;
    items.push({ id: `exposed-port:${e.id}`, severity: "medium", title: `${e.label} on :${e.port} is open to the network`, detail: "Dev servers usually only need localhost.", target: { view: "list", entryId: e.id } });
  }
  return items.sort((a, b) => (a.severity === b.severity ? a.title.localeCompare(b.title) : a.severity === "high" ? -1 : 1));
}

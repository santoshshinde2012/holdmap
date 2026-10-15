import type { Agent, AgentMcpServer, AgentSkill, AgentTool, CapabilityScope } from "./types";

export type InventoryKind = "all" | "mcp" | "skill";
export type InventoryState = "all" | "enabled" | "disabled" | "unknown";
export type CapabilityItem = {
  id: string; kind: "mcp" | "skill"; name: string; scope: CapabilityScope;
  sourcePath: string; projectPath: string | null; owners: Agent[];
  definition: AgentMcpServer | AgentSkill;
};
export type RunningMcpItem = { id: string; kind: "running"; name: string; owners: Agent[]; tool: AgentTool };
export type CapabilitySelection = CapabilityItem | RunningMcpItem;
export const scopeLabel: Record<CapabilityScope, string> = { user: "User", project: "Project", local: "Local project" };
export const pathName = (path: string): string => path.replace(/[/\\]+$/, "").split(/[/\\]/).pop() || path;

/** Share identical declarations across clients; provider-specific IDs alone cannot identify a shared source. */
export function capabilityItems(agents: Agent[]): CapabilityItem[] {
  const rows = new Map<string, CapabilityItem>();
  for (const agent of agents) {
    if (!agent.capabilities?.inspected) continue;
    for (const kind of ["mcp", "skill"] as const) {
      const definitions = kind === "mcp" ? agent.capabilities.mcp_servers : agent.capabilities.skills;
      for (const definition of definitions) {
        const metadata = kind === "mcp"
          ? [(definition as AgentMcpServer).transport, (definition as AgentMcpServer).enabled ?? null]
          : [(definition as AgentSkill).declared_name ?? null, (definition as AgentSkill).description ?? null];
        const id = `${kind}:${JSON.stringify([definition.source_path, definition.scope, definition.project_path, definition.name, definition.evidence, ...metadata])}`;
        const existing = rows.get(id);
        if (existing) { if (!existing.owners.some((owner) => owner.id === agent.id)) existing.owners.push(agent); }
        else rows.set(id, { id, kind, name: kind === "skill" ? (definition as AgentSkill).declared_name || definition.name : definition.name, scope: definition.scope, sourcePath: definition.source_path, projectPath: definition.project_path, definition, owners: [agent] });
      }
    }
  }
  return [...rows.values()].sort((a, b) => a.name.localeCompare(b.name) || a.id.localeCompare(b.id));
}

/** Runtime observations remain separate even when a configured alias has the same name. */
export function runningMcpItems(agents: Agent[]): RunningMcpItem[] {
  const rows = new Map<number, RunningMcpItem>();
  for (const agent of agents) for (const tool of agent.tools ?? []) {
    if (tool.kind !== "mcp_server") continue;
    const existing = rows.get(tool.pid);
    if (existing) { if (!existing.owners.some((owner) => owner.id === agent.id)) existing.owners.push(agent); }
    else rows.set(tool.pid, { id: `running:${tool.pid}`, kind: "running", name: tool.name, owners: [agent], tool });
  }
  return [...rows.values()].sort((a, b) => a.name.localeCompare(b.name) || a.tool.pid - b.tool.pid);
}

export function enabledLabel(item: CapabilityItem): string {
  if (item.kind === "skill") return "SKILL.md present";
  const enabled = (item.definition as AgentMcpServer).enabled;
  return enabled === true ? "Enabled in config" : enabled === false ? "Disabled in config" : "Enablement unknown";
}

export function capabilityMatches(item: CapabilitySelection, query: string): boolean {
  const identity = item.owners.flatMap((owner) => [owner.id, owner.name, owner.product, `pid:${owner.pid}`, String(owner.pid)]);
  const metadata = item.kind === "running"
    ? ["running MCP process", String(item.tool.pid), `pid:${item.tool.pid}`, item.tool.cwd ?? "", ...item.tool.ports.map((port) => `port:${port} :${port}`)]
    : [item.kind === "mcp" ? "MCP server configured" : "skill available", item.scope, scopeLabel[item.scope], item.sourcePath, item.projectPath ?? "", enabledLabel(item), item.kind === "mcp" ? (item.definition as AgentMcpServer).transport : ["SKILL.md", item.definition.name, (item.definition as AgentSkill).description ?? ""].join(" ")];
  const text = [item.name, ...identity, ...metadata].join(" ").toLocaleLowerCase();
  const pids = new Set(item.owners.map((owner) => owner.pid));
  if (item.kind === "running") pids.add(item.tool.pid);
  return query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean).every((term) => {
    const pid = /^(?:pid:)?(\d+)$/.exec(term);
    return pid ? pids.has(Number(pid[1])) : text.includes(term);
  });
}

export function filterCapabilities(items: CapabilityItem[], query: string, kind: InventoryKind, scope: "all" | CapabilityScope, state: InventoryState): CapabilityItem[] {
  return items.filter((item) => capabilityMatches(item, query)
    && (kind === "all" || item.kind === kind)
    && (scope === "all" || item.scope === scope)
    && (state === "all" || item.kind === "mcp" && (state === "enabled" ? (item.definition as AgentMcpServer).enabled === true : state === "disabled" ? (item.definition as AgentMcpServer).enabled === false : (item.definition as AgentMcpServer).enabled == null)));
}

export function capabilityLimits(agents: Agent[]): string[] {
  const lines: string[] = [];
  for (const agent of agents) {
    const prefix = `${agent.name} (PID ${agent.pid})`;
    const inventory = agent.capabilities;
    if (!inventory) lines.push(`${prefix}: capability inventory was not reported by this backend.`);
    else {
      if (!inventory.inspected) lines.push(`${prefix}: configuration and skill sources have not been inspected.`);
      lines.push(...inventory.limits.map((limit) => `${prefix}: ${limit}`));
      if (inventory.more_mcp_servers) lines.push(`${prefix}: ${inventory.more_mcp_servers} more MCP definitions not listed.`);
      if (inventory.more_skills) lines.push(`${prefix}: ${inventory.more_skills} more skill files not listed.`);
    }
  }
  return [...new Set(lines)];
}

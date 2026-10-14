import type { Agent, AgentsReport } from "./types";

export interface FolderTarget {
  agentId: string;
  path: string;
}

/** A shared map folder prefers its selected visible owner, then the first visible owner. */
export function preferredFolderOwner(owners: readonly string[], selectedAgent: string | null): string | null {
  return selectedAgent && owners.includes(selectedAgent) ? selectedAgent : owners[0] ?? null;
}

// Mirrors Agent::known_folders(): only the reported folders grant ownership.
function ownsFolder(agent: Agent, path: string): boolean {
  return agent.folders.some((folder) => folder.path === path);
}

/** Choose an owner once, preferring the selected agent when it knows this exact path. */
export function resolveFolderTarget(
  report: AgentsReport | null,
  path: string,
  preferredAgentId?: string | null,
): FolderTarget | null {
  if (!report) return null;
  const owners = report.agents.filter((agent) => ownsFolder(agent, path));
  const owner = owners.find((agent) => agent.id === preferredAgentId) ?? owners[0];
  return owner ? { agentId: owner.id, path } : null;
}

/** Revalidate the chosen owner without silently switching to another agent. */
export function currentFolderOwner(
  report: AgentsReport | null,
  target: FolderTarget | null,
): Agent | null {
  if (!report || !target) return null;
  const owner = report.agents.find((agent) => agent.id === target.agentId);
  return owner && ownsFolder(owner, target.path) ? owner : null;
}

/** Split at `/` and `\\` so a long path wraps between directories, not mid-token. */
export function pathWrapParts(path: string): string[] {
  const parts: string[] = [];
  let buf = "";
  for (const ch of path) {
    buf += ch;
    if (ch === "/" || ch === "\\") {
      parts.push(buf);
      buf = "";
    }
  }
  if (buf) parts.push(buf);
  return parts.length ? parts : [path];
}

export function fileManagerLabel(platform?: string): string {
  switch (platform?.toLowerCase()) {
    case "macos":
    case "darwin":
      return "Open in Finder";
    case "win":
    case "win32":
    case "windows":
      return "Open in Explorer";
    default:
      return "Open in file manager";
  }
}

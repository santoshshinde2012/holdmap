// Text for agent memory counters and documented locations. Unknown stays unknown:
// a missing counter is never shown as zero.

import { humanBytes } from "./format";
import type { Agent, AgentMemoryReport, AgentsReport, CapabilityPlace, ObservedCount } from "./types";

const ROLE: Record<string, string> = {
  agent: "Agent",
  helper: "Helpers",
  language_server: "Language servers",
  mcp_server: "MCP servers",
  child: "Other processes",
};

/** The observed integer, only when a reason was not also recorded. */
export function observed(figure: ObservedCount | undefined): number | null {
  if (!figure || figure.unknown || figure.value == null) return null;
  return figure.value;
}

export function unknownReason(figure: ObservedCount | undefined): string {
  if (figure?.unknown) return figure.unknown;
  if (figure?.value != null) return "the sample recorded both a value and an unknown reason, so it was not used";
  return "no value and no reason were recorded";
}

export function bytesOrUnknown(figure: ObservedCount | undefined): string {
  const value = observed(figure);
  return value == null ? `unknown (${unknownReason(figure)})` : humanBytes(value);
}

export function shareText(figure: ObservedCount | undefined): string {
  const ppm = observed(figure);
  return ppm == null ? `unknown (${unknownReason(figure)})` : `${(ppm / 10_000).toFixed(1)}% of machine RAM`;
}

export function memoryFor(report: AgentsReport | null | undefined, agentId: string): AgentMemoryReport | null {
  return report?.memory?.find((row) => row.agent_id === agentId) ?? null;
}

const COUNTERS: { key: keyof AgentMemoryReport; label: string }[] = [
  { key: "resident", label: "Resident" },
  { key: "physical_footprint", label: "Physical footprint" },
  { key: "private_bytes", label: "Private" },
  { key: "shared_bytes", label: "Shared" },
  { key: "proportional", label: "Proportional" },
  { key: "compressed", label: "Compressed" },
  { key: "virtual_bytes", label: "Virtual" },
  { key: "swap_bytes", label: "Swap" },
  { key: "commit_bytes", label: "Commit" },
  { key: "peak_resident", label: "Peak resident" },
  { key: "peak_physical", label: "Peak footprint" },
];

export function counterRows(memory: AgentMemoryReport): { label: string; text: string; known: boolean }[] {
  return COUNTERS.map(({ key, label }) => {
    const figure = memory[key] as ObservedCount;
    return { label, text: bytesOrUnknown(figure), known: observed(figure) != null };
  });
}

export function roleLabel(role: string): string {
  return ROLE[role] ?? role;
}

export function memorySummary(agent: Agent, memory: AgentMemoryReport | null): string {
  const lines = [
    `${agent.name} memory`,
    memory
      ? `Headline ${humanBytes(memory.headline_bytes)} (${memory.headline_meaning})`
      : `Headline ${humanBytes(agent.memory_bytes)}. Platform counters were not in this report.`,
  ];
  if (!memory) return lines.join("\n");
  lines.push(`Share ${shareText(memory.share_ppm)}`);
  lines.push(`Trend ${memory.trend.direction}${memory.trend.unknown ? ` (${memory.trend.unknown})` : ""}`);
  for (const row of counterRows(memory)) lines.push(`${row.label}: ${row.text}`);
  for (const group of memory.groups) {
    lines.push(`${roleLabel(group.role)}: ${group.processes} · ${humanBytes(group.scan_bytes)}`);
  }
  for (const row of memory.processes.filter((row) => row.dominant)) {
    lines.push(`Large process: ${row.name} pid ${row.pid} · ${humanBytes(row.scan_bytes)}. ${row.dominant_reason ?? ""}`);
  }
  for (const limit of memory.limits) lines.push(`Note: ${limit}`);
  return lines.join("\n");
}

export function placesSummary(places: CapabilityPlace[]): string {
  const lines = ["Documented locations. A present file is not proof the agent loaded it."];
  for (const place of [...places].sort((a, b) => a.rank - b.rank || a.path.localeCompare(b.path))) {
    const where = place.path || place.kind;
    const size = place.byte_size == null ? "size unknown" : humanBytes(place.byte_size);
    const count = place.file_count == null ? "count unknown" : `${place.file_count} entries`;
    const link = place.symlink_target ? ` → ${place.symlink_target}` : "";
    lines.push(`${place.scope} ${place.kind} (rank ${place.rank}): ${place.status} ${where}${link} · ${count} · ${size}`);
    if (place.status_reason) lines.push(`  ${place.status_reason}`);
    if (place.candidates.length) lines.push(`  ${place.candidates.join(", ")}`);
  }
  return lines.join("\n");
}

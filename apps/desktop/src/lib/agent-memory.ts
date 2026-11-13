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

export function processNoun(count: number): string {
  return `${count} ${count === 1 ? "process" : "processes"}`;
}

const PROMINENT = ["resident", "physical_footprint", "private_bytes", "swap_bytes", "peak_resident", "peak_physical"];

/** Observable counters first. Unsupported counters stay out of the default summary. */
export function splitCounters(memory: AgentMemoryReport): { known: { label: string; text: string }[]; unknown: { label: string; text: string }[] } {
  const rows = counterRows(memory);
  const rank = (label: string) => {
    const key = COUNTERS.find((row) => row.label === label)?.key;
    const at = PROMINENT.indexOf(String(key));
    return at === -1 ? PROMINENT.length : at;
  };
  const known = rows.filter((row) => row.known).sort((a, b) => rank(a.label) - rank(b.label)).map(({ label, text }) => ({ label, text }));
  const unknown = rows.filter((row) => !row.known).map(({ label, text }) => ({ label, text }));
  return { known, unknown };
}

export function sharePercent(figure: ObservedCount | undefined): number | null {
  const ppm = observed(figure);
  return ppm == null ? null : ppm / 10_000;
}

export function trendView(trend: AgentMemoryReport["trend"]): { label: string; delta: string | null; why: string | null } {
  const labels: Record<string, string> = { rising: "Rising", falling: "Falling", flat: "Flat", unknown: "Trend unknown" };
  let delta: string | null = null;
  if (trend.samples.length >= 2) {
    const change = trend.samples[trend.samples.length - 1].bytes - trend.samples[0].bytes;
    const sign = change > 0 ? "+" : change < 0 ? "−" : "";
    delta = change === 0 ? "no change" : `${sign}${humanBytes(Math.abs(change))}`;
  }
  return { label: labels[trend.direction] ?? trend.direction, delta, why: trend.unknown ?? null };
}

const ROLE_ORDER = ["agent", "mcp_server", "language_server", "helper", "child"];

export function processGroups(memory: AgentMemoryReport): { role: string; label: string; processes: import("./types").ProcessMemoryRow[] }[] {
  const grouped = new Map<string, import("./types").ProcessMemoryRow[]>();
  for (const row of memory.processes) {
    const list = grouped.get(row.role) ?? [];
    list.push(row);
    grouped.set(row.role, list);
  }
  return [...grouped.entries()]
    .sort((a, b) => ROLE_ORDER.indexOf(a[0]) - ROLE_ORDER.indexOf(b[0]))
    .map(([role, processes]) => ({
      role,
      label: roleLabel(role),
      processes: processes.sort((a, b) => b.scan_bytes - a.scan_bytes || a.pid - b.pid),
    }));
}

const READING_BITS: [string, string][] = [
  ["resident", "resident"],
  ["physical_footprint", "footprint"],
  ["private_bytes", "private"],
  ["swap_bytes", "swap"],
  ["peak_physical", "peak footprint"],
  ["peak_resident", "peak resident"],
];

/** Observed per-process counters only. Missing ones are not printed as zero. */
export function readingBits(reading: Record<string, ObservedCount> | undefined): string {
  if (!reading) return "";
  return READING_BITS.flatMap(([key, label]) => {
    const value = observed(reading[key]);
    return value == null ? [] : [`${label} ${humanBytes(value)}`];
  }).join(" · ");
}

export interface FleetMemory {
  total: number;
  count: number;
  /** Share of machine RAM when every headline can be compared with one machine size. */
  share: string | null;
  top: { id: string; name: string; bytes: number } | null;
  /** True when the total is the process-table headline rather than platform counters. */
  fromProcessTable: boolean;
}

/** Machine-wide agent memory from the latest report. Unknown machine size leaves the share blank. */
export function fleetMemory(report: AgentsReport | null | undefined): FleetMemory | null {
  if (!report?.agents.length) return null;
  const rows = report.memory ?? [];
  if (!rows.length) {
    const ranked = [...report.agents].sort((a, b) => b.memory_bytes - a.memory_bytes);
    const top = ranked[0];
    return {
      total: ranked.reduce((sum, agent) => sum + agent.memory_bytes, 0),
      count: ranked.length,
      share: null,
      top: top ? { id: top.id, name: top.name, bytes: top.memory_bytes } : null,
      fromProcessTable: true,
    };
  }
  const byId = new Map(report.agents.map((agent) => [agent.id, agent]));
  const ranked = rows
    .map((row) => ({ id: row.agent_id, name: byId.get(row.agent_id)?.name ?? row.agent_id, bytes: row.headline_bytes, machine: observed(row.machine_bytes) }))
    .sort((a, b) => b.bytes - a.bytes);
  const machine = ranked.find((row) => row.machine != null)?.machine ?? null;
  const total = ranked.reduce((sum, row) => sum + row.bytes, 0);
  const top = ranked[0];
  return {
    total,
    count: ranked.length,
    share: machine && machine > 0 ? `${(total / machine * 100).toFixed(1)}% of machine RAM` : null,
    top: top ? { id: top.id, name: top.name, bytes: top.bytes } : null,
    fromProcessTable: false,
  };
}

export const PLACE_KIND: Record<string, string> = {
  skills: "Skills",
  agent_memory: "Context / Memory",
  rules: "Rules",
  mcp_config: "MCP config",
  instructions: "Commands & prompts",
  builtin: "Built-in",
  plugin: "Plugin",
};

export const PLACE_SCOPE: Record<string, string> = {
  local: "Local",
  project: "Project",
  user: "User",
  plugin: "Plugin",
  builtin: "Built-in",
};

export const PLACE_STATUS: Record<string, string> = {
  present: "Present",
  missing: "Missing",
  unreadable: "Unreadable",
  skipped: "Skipped",
};

const SCOPE_ORDER = ["local", "project", "user", "plugin", "builtin"];

export function formatModified(ms: number | null | undefined): string {
  if (ms == null) return "modified time unknown";
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return "modified time unknown";
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

export type PlaceFilter = "all" | "present" | string;

export function filterPlaces(places: CapabilityPlace[], filter: PlaceFilter): CapabilityPlace[] {
  if (filter === "present") return places.filter((place) => place.status === "present" || place.present);
  if (filter !== "all" && PLACE_KIND[filter]) return places.filter((place) => place.kind === filter);
  return places;
}

export function placeLayers(places: CapabilityPlace[]): { scope: string; label: string; rank: number; places: CapabilityPlace[] }[] {
  const grouped = new Map<string, CapabilityPlace[]>();
  for (const place of places) {
    const list = grouped.get(place.scope) ?? [];
    list.push(place);
    grouped.set(place.scope, list);
  }
  return [...grouped.entries()]
    .sort((a, b) => SCOPE_ORDER.indexOf(a[0]) - SCOPE_ORDER.indexOf(b[0]))
    .map(([scope, rows]) => ({
      scope,
      label: PLACE_SCOPE[scope] ?? scope,
      rank: Math.min(...rows.map((place) => place.rank)),
      places: rows.sort((a, b) => a.rank - b.rank || a.kind.localeCompare(b.kind) || a.path.localeCompare(b.path)),
    }));
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
  const trend = trendView(memory.trend);
  lines.push(`Trend ${trend.label}${trend.delta ? ` ${trend.delta}` : ""}${trend.why ? ` (${trend.why})` : ""}`);
  for (const row of counterRows(memory)) lines.push(`${row.label}: ${row.text}`);
  for (const group of processGroups(memory)) {
    lines.push(`${group.label}: ${processNoun(group.processes.length)} · ${humanBytes(group.processes.reduce((sum, row) => sum + row.scan_bytes, 0))}`);
    for (const row of group.processes) {
      const bits = readingBits(row.reading);
      lines.push(`  ${row.name} pid ${row.pid} · ${humanBytes(row.scan_bytes)}${bits ? ` · ${bits}` : ""}${row.dominant ? " · large" : ""}`);
      if (row.dominant && row.dominant_reason) lines.push(`  ${row.dominant_reason}`);
    }
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
    lines.push(`${PLACE_SCOPE[place.scope] ?? place.scope} ${PLACE_KIND[place.kind] ?? place.kind} (rank ${place.rank}): ${PLACE_STATUS[place.status] ?? place.status} ${where}${link} · ${count} · ${size} · ${formatModified(place.modified_unix_ms)}`);
    if (place.status_reason) lines.push(`  ${place.status_reason}`);
    if (place.candidates.length) lines.push(`  ${place.candidates.join(", ")}`);
  }
  return lines.join("\n");
}

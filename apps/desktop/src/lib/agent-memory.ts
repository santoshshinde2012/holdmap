// Text for agent memory counters and documented locations. Unknown stays unknown:
// a missing counter is never shown as zero.

import { humanBytes, humanDuration } from "./format";
import type { Agent, AgentMemoryReport, AgentsReport, CapabilityPlace, ObservedCount } from "./types";
import { observed } from "./memory-text";

// Home's cards need only the headline text, so it lives in a small module of its own.
export { agentMemoryText, memoryFor, observed } from "./memory-text";

const ROLE: Record<string, string> = {
  agent: "Agent",
  helper: "Helpers",
  language_server: "Language servers",
  mcp_server: "MCP servers",
  child: "Other processes",
};


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

const HEADLINE_LABEL: Record<string, string> = {
  physical_footprint: "Physical footprint",
  resident: "Resident (RSS)",
  working_set: "Working set",
};

/** Which counter the headline is, or "Mixed: …" when some processes fell back. */
export function headlineLabel(memory: AgentMemoryReport): string {
  return memory.headline_label || HEADLINE_LABEL[memory.headline_counter] || memory.headline_counter;
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

export function trendView(trend: AgentMemoryReport["trend"]): {
  label: string;
  delta: string | null;
  why: string | null;
  sampleCount: number;
  span: string | null;
  rule: string;
} {
  const labels: Record<string, string> = { rising: "Rising", falling: "Falling", flat: "Flat", unknown: "Trend unknown" };
  let delta: string | null = null;
  let span: string | null = null;
  if (trend.samples.length >= 2) {
    const first = trend.samples[0];
    const last = trend.samples[trend.samples.length - 1];
    const change = last.bytes - first.bytes;
    const sign = change > 0 ? "+" : change < 0 ? "−" : "";
    delta = change === 0 ? "no change" : `${sign}${humanBytes(Math.abs(change))}`;
    if (last.at_ms > first.at_ms) {
      span = humanDuration((last.at_ms - first.at_ms) / 1000);
    }
  }
  return {
    label: labels[trend.direction] ?? trend.direction,
    delta,
    why: trend.unknown ?? null,
    sampleCount: trend.samples.length,
    span,
    rule: trend.rule,
  };
}

/** SVG polyline points for retained memory samples on a 72x18 canvas. */
export function trendSparkline(samples: AgentMemoryReport["trend"]["samples"], width = 72, height = 18): { points: string; areaPoints: string; lastX: number; lastY: number; minBytes: number; maxBytes: number } | null {
  if (samples.length < 2) return null;
  const padX = 2;
  const padY = 3;
  const innerW = Math.max(1, width - padX * 2);
  const innerH = Math.max(1, height - padY * 2);
  const bytes = samples.map((s) => s.bytes);
  const minB = Math.min(...bytes);
  const maxB = Math.max(...bytes);
  const bSpan = maxB - minB;
  const minT = samples[0].at_ms;
  const maxT = samples[samples.length - 1].at_ms;
  const tSpan = maxT - minT;
  const coords = samples.map((sample, index) => {
    const xRatio = tSpan > 0 ? (sample.at_ms - minT) / tSpan : index / (samples.length - 1);
    const yRatio = bSpan > 0 ? (sample.bytes - minB) / bSpan : 0.5;
    const x = padX + xRatio * innerW;
    const y = padY + (1 - yRatio) * innerH;
    return [Number(x.toFixed(1)), Number(y.toFixed(1))] as const;
  });
  const first = coords[0];
  const last = coords[coords.length - 1];
  const line = coords.map(([x, y]) => `${x},${y}`).join(" ");
  return {
    points: line,
    areaPoints: `${first[0]},${height} ${line} ${last[0]},${height}`,
    lastX: last[0],
    lastY: last[1],
    minBytes: minB,
    maxBytes: maxB,
  };
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
  breakdown: { id: string; name: string; bytes: number; pct: number }[];
  /** True when the total is the process-table headline rather than platform counters. */
  fromProcessTable: boolean;
}

function withPct<T extends { id: string; name: string; bytes: number }>(rows: T[], total: number) {
  return rows.map(({ id, name, bytes }) => ({ id, name, bytes, pct: total > 0 ? (bytes / total) * 100 : 0 }));
}

/** Machine-wide agent memory from the latest report. Unknown machine size leaves the share blank. */
export function fleetMemory(report: AgentsReport | null | undefined): FleetMemory | null {
  if (!report?.agents.length) return null;
  const rows = report.memory ?? [];
  if (!rows.length) {
    const ranked = [...report.agents].sort((a, b) => b.memory_bytes - a.memory_bytes).map((a) => ({ id: a.id, name: a.name, bytes: a.memory_bytes }));
    const total = ranked.reduce((sum, a) => sum + a.bytes, 0);
    return { total, count: ranked.length, share: null, top: ranked[0] ?? null, breakdown: withPct(ranked, total), fromProcessTable: true };
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
    breakdown: withPct(ranked, total),
    fromProcessTable: false,
  };
}

export const PLACE_KIND: Record<string, string> = {
  instructions: "Instructions",
  rules: "Rules",
  settings: "Settings",
  mcp_config: "MCP config",
  skills: "Skills",
  commands: "Commands & prompts",
  subagents: "Subagents",
  hooks: "Hooks",
  extensions: "Extensions",
  agent_memory: "Agent memory",
  session_data: "Session data",
  secret_store: "Secrets",
  builtin: "Built-in",
  plugin: "Plugin",
};

export const PLACE_SCOPE: Record<string, string> = {
  managed: "Managed",
  local: "Local",
  project: "Project",
  user: "User",
  system_default: "System default",
  plugin: "Plugin",
  account: "Account",
  builtin: "Built-in",
};

/** How files of one kind combine across layers, in plain words. */
export const PLACE_COMBINE: Record<string, string> = {
  override: "closer layer overrides",
  merge: "layers merge",
  join: "all files load",
  not_config: "not configuration",
};

export const PLACE_STATUS: Record<string, string> = {
  present: "Present",
  missing: "Missing",
  unreadable: "Unreadable",
  skipped: "Skipped",
};

const SCOPE_ORDER = ["managed", "local", "project", "user", "system_default", "plugin", "account", "builtin"];

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
      ? `Headline ${humanBytes(memory.headline_bytes)} · ${headlineLabel(memory)} (${memory.headline_meaning})${memory.headline_mixed ? ` ${memory.headline_mixed}` : ""}`
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

export interface PlaceEntryView {
  name: string;
  kindLabel: string | null;
  bytes: number | null;
  modified: string | null;
}

const ENTRY_KIND_LABEL: Record<string, string> = {
  directory: "dir",
  file: "file",
  symlink: "link",
  unreadable: "unreadable",
};

export function placeEntries(place: CapabilityPlace): PlaceEntryView[] {
  const entries = place.entries ?? [];
  if (entries.length) {
    return entries
      .filter((entry) => Boolean(entry.name))
      .map((entry) => ({
        name: entry.name,
        kindLabel: ENTRY_KIND_LABEL[entry.kind] ?? (entry.kind || null),
        bytes: entry.bytes ?? null,
        modified: entry.modified_unix_ms != null ? formatModified(entry.modified_unix_ms) : null,
      }));
  }
  return place.candidates.map((name) => ({
    name,
    kindLabel: null,
    bytes: null,
    modified: null,
  }));
}

/** Size, count and line facts for one present row; secrets get presence only. */
export function placeFacts(place: CapabilityPlace): string {
  if (place.kind === "secret_store") return "present · not opened";
  const count = place.file_count == null ? "count unknown" : `${place.file_count} ${place.file_count === 1 ? "entry" : "entries"}`;
  const size = place.byte_size == null ? "size unknown" : humanBytes(place.byte_size);
  const bits = [count, size, formatModified(place.modified_unix_ms)];
  if (place.line_count != null && place.load_limit_lines != null) {
    bits.push(place.line_count > place.load_limit_lines
      ? `MEMORY.md ${place.line_count} lines, only the first ${place.load_limit_lines} load`
      : `MEMORY.md ${place.line_count} of ${place.load_limit_lines} lines`);
  }
  if (place.ignored_count) bits.push(`${place.ignored_count} ignored`);
  return bits.join(" · ");
}

export function combineText(place: CapabilityPlace): string {
  return PLACE_COMBINE[place.combine ?? ""] ?? "";
}

export function placesSummary(places: CapabilityPlace[]): string {
  const lines = ["Documented locations. A present file is not proof the agent loaded it."];
  for (const place of [...places].sort((a, b) => a.rank - b.rank || a.path.localeCompare(b.path))) {
    const scope = PLACE_SCOPE[place.scope] ?? place.scope;
    const kind = PLACE_KIND[place.kind] ?? place.kind;
    const status = PLACE_STATUS[place.status] ?? place.status;
    const combine = combineText(place);
    const head = combine ? `${scope} ${kind} (${combine})` : `${scope} ${kind}`;
    if (!place.path) {
      const detail = place.kind === "builtin" && place.candidates.length
        ? ` · ${place.candidates.length} documented ${place.candidates.length === 1 ? "tool" : "tools"}`
        : "";
      lines.push(`${head}: ${status} ${place.label || place.kind}${detail}`);
    } else if (place.status === "missing") {
      lines.push(`${head}: ${status} ${place.path} · not on disk`);
    } else {
      const link = place.symlink_target ? ` → ${place.symlink_target}` : "";
      lines.push(`${head}: ${status} ${place.path}${link} · ${placeFacts(place)}`);
    }
    if (place.status_reason) lines.push(`  ${place.status_reason}`);
    if (place.note) lines.push(`  ${place.note}`);
    if (place.candidates.length && place.kind !== "session_data") lines.push(`  ${place.candidates.join(", ")}`);
  }
  return lines.join("\n");
}

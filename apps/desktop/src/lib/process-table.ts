// One process table for an agent: memory (from the platform counters when reported, else the
// process table), CPU and role, sortable, or shown as the parent/child tree.
import { processRows } from "./agents";
import { readingBits, roleLabel } from "./agent-memory";
import type { Agent, AgentMemoryReport } from "./types";

export type ProcessSortKey = "memory" | "cpu" | "name";
export type SortDirection = "asc" | "desc";

export interface ProcessTableRow {
  pid: number;
  name: string;
  role: string;
  roleLabel: string;
  depth: number;
  bytes: number;
  /** null when this process is not in the agent's process list (CPU unknown). */
  cpu: number | null;
  command: string;
  dominant: boolean;
  dominantReason: string | null;
  bits: string;
}

/** Rows in tree order (parents before children, depth set), one per pid. */
export function processTableRows(agent: Agent, memory: AgentMemoryReport | null): ProcessTableRow[] {
  const counters = new Map((memory?.processes ?? []).map((row) => [row.pid, row]));
  const rows: ProcessTableRow[] = [];
  const seen = new Set<number>();
  for (const { process: p, depth } of processRows(agent)) {
    const row = counters.get(p.pid);
    seen.add(p.pid);
    const role = row?.role ?? (p.role === "agent" ? "agent" : p.role === "child" ? "child" : "helper");
    rows.push({
      pid: p.pid, name: p.name, role, roleLabel: roleLabel(role), depth,
      bytes: row?.scan_bytes ?? p.memory_bytes, cpu: p.cpu_unknown ? null : p.cpu_percent, command: p.command,
      dominant: !!row?.dominant, dominantReason: row?.dominant_reason ?? null, bits: readingBits(row?.reading),
    });
  }
  for (const row of memory?.processes ?? []) {
    if (seen.has(row.pid)) continue;
    rows.push({
      pid: row.pid, name: row.name, role: row.role, roleLabel: roleLabel(row.role), depth: 0,
      bytes: row.scan_bytes, cpu: null, command: "", dominant: row.dominant, dominantReason: row.dominant_reason ?? null, bits: readingBits(row.reading),
    });
  }
  return rows;
}

/** A sorted copy. Unknown CPU sorts last in either direction; ties fall back to pid. */
export function sortProcessRows(rows: ProcessTableRow[], key: ProcessSortKey, direction: SortDirection): ProcessTableRow[] {
  const sign = direction === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => {
    if (key === "cpu" && (a.cpu == null) !== (b.cpu == null)) return a.cpu == null ? 1 : -1;
    const diff = key === "name" ? a.name.localeCompare(b.name) : key === "cpu" ? (a.cpu ?? 0) - (b.cpu ?? 0) : a.bytes - b.bytes;
    return diff * sign || a.pid - b.pid;
  });
}

/** The next sort after a header click: same column flips, a new column starts at its natural order. */
export function nextSort(current: { key: ProcessSortKey; direction: SortDirection }, key: ProcessSortKey): { key: ProcessSortKey; direction: SortDirection } {
  if (current.key === key) return { key, direction: current.direction === "asc" ? "desc" : "asc" };
  return { key, direction: key === "name" ? "asc" : "desc" };
}

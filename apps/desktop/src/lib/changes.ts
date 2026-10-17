// Recent changes for the overview, observed by this window: listeners that opened or closed,
// agents that started or exited between scans, and stops from the saved history. The first
// scan is a baseline, not a change, so nothing is invented on launch.
import type { Agent, HistoryEntry, PortEntry } from "./types";

export type ChangeKind = "port_opened" | "port_closed" | "agent_started" | "agent_exited" | "stopped";
export type ChangeTarget = { view: "list"; entryId: string } | { view: "agents"; agentId: string } | null;

export interface ChangeEvent {
  id: string;
  at: number;
  kind: ChangeKind;
  title: string;
  detail: string;
  target: ChangeTarget;
}

export const MAX_CHANGES = 30;

const name = (e: PortEntry) => e.project?.name ?? e.framework?.name ?? e.label;

export function diffPorts(prev: PortEntry[] | null, next: PortEntry[], at: number): ChangeEvent[] {
  if (!prev) return [];
  const before = new Map(prev.map((e) => [e.id, e]));
  const after = new Map(next.map((e) => [e.id, e]));
  const out: ChangeEvent[] = [];
  for (const [id, e] of after) {
    if (!before.has(id)) out.push({ id: `open:${id}:${at}`, at, kind: "port_opened", title: `:${e.port} opened`, detail: `${name(e)}${e.pid ? ` · PID ${e.pid}` : ""}`, target: { view: "list", entryId: id } });
  }
  for (const [id, e] of before) {
    if (!after.has(id)) out.push({ id: `close:${id}:${at}`, at, kind: "port_closed", title: `:${e.port} closed`, detail: name(e), target: null });
  }
  return out;
}

export function diffAgents(prev: Agent[] | null, next: Agent[], at: number): ChangeEvent[] {
  if (!prev) return [];
  const before = new Map(prev.map((a) => [a.id, a]));
  const after = new Map(next.map((a) => [a.id, a]));
  const out: ChangeEvent[] = [];
  for (const [id, a] of after) {
    if (!before.has(id)) out.push({ id: `start:${id}:${at}`, at, kind: "agent_started", title: `${a.name} started`, detail: `PID ${a.pid}`, target: { view: "agents", agentId: id } });
  }
  for (const [id, a] of before) {
    if (!after.has(id)) out.push({ id: `exit:${id}:${at}`, at, kind: "agent_exited", title: `${a.name} exited`, detail: `PID ${a.pid}`, target: null });
  }
  return out;
}

export function stopEvents(history: HistoryEntry[]): ChangeEvent[] {
  return history.map((h) => ({
    id: `stop:${h.pid}:${h.port}:${h.at_ms}`,
    at: h.at_ms,
    kind: "stopped" as const,
    title: `Stopped :${h.port}`,
    detail: h.project ?? h.framework ?? h.label,
    target: null,
  }));
}

/** Newest first, one entry per id, bounded. */
export function mergeChanges(existing: ChangeEvent[], incoming: ChangeEvent[], max = MAX_CHANGES): ChangeEvent[] {
  const seen = new Set<string>();
  return [...incoming, ...existing]
    .sort((a, b) => b.at - a.at)
    .filter((e) => (seen.has(e.id) ? false : (seen.add(e.id), true)))
    .slice(0, max);
}

/** "just now", "4m", "2h", "3d": compact relative time for dense lists. */
export function ago(at: number, now: number): string {
  const s = Math.max(0, Math.round((now - at) / 1000));
  if (s < 45) return "just now";
  if (s < 3600) return `${Math.max(1, Math.round(s / 60))}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

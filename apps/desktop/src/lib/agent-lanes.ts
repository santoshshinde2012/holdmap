// Agent lanes view model: one lane per agent from its start to now, with a bar per listening
// port from its process start time and the agent's MCP servers at the live edge. Time uses a
// log scale so a 2-day-old app and a 5-minute-old dev server are both readable.
import { humanDuration } from "./format";
import { isUnpinned } from "./agent-config";
import { unownedDevServers } from "./home";
import type { Agent, AgentsReport, PortEntry } from "./types";

export interface LaneBar { id: string; entryId: string; start: number; port: number; label: string; warn: boolean }
export interface LaneMcp { id: string; name: string; running: boolean; warn: boolean }
export interface Lane {
  id: string;
  /** null for the lane of dev servers no agent started. */
  agent: Agent | null;
  start: number | null;
  uptime: string | null;
  bars: LaneBar[];
  mcp: LaneMcp[];
}

const TAU = 5 * 60_000;
const TICKS = [5, 15, 60, 180, 720, 1440, 2880, 4320, 10080].map((m) => m * 60_000);

/** Maps a time to 0 (oldest) … 1 (now) on a log scale, with duration ticks. */
export function timeScale(now: number, oldest: number) {
  const max = Math.max(now - oldest, 60 * 60_000) * 1.04;
  const denom = Math.log1p(max / TAU);
  const x = (t: number) => Math.min(1, Math.max(0, 1 - Math.log1p(Math.max(0, now - t) / TAU) / denom));
  const ticks = TICKS.filter((age) => age < max * 0.97).map((age) => ({ at: now - age, x: x(now - age), label: shortAge(age) }));
  return { x, ticks };
}

export function shortAge(ms: number): string {
  const m = Math.round(ms / 60_000);
  if (m < 60) return `${m}m`;
  if (m < 1440) return `${Math.round(m / 60)}h`;
  return `${Math.round(m / 1440)}d`;
}

function bar(e: PortEntry | undefined, id: string, port: number, label: string, warn: boolean): LaneBar[] {
  const t = e?.process?.start_time;
  return t ? [{ id, entryId: id, start: t * 1000, port, label, warn }] : [];
}

export function lanes(report: AgentsReport | null, entries: PortEntry[] | null, now: number): Lane[] {
  const byId = new Map((entries ?? []).map((e) => [e.id, e]));
  const out: Lane[] = (report?.agents ?? []).map((agent) => {
    const start = agent.started_at * 1000;
    return {
      id: agent.id,
      agent,
      start,
      uptime: humanDuration(Math.max(0, now - start) / 1000),
      bars: agent.ports.flatMap((p) => bar(byId.get(p.entry_id), p.entry_id, p.port, p.framework ?? p.label, p.exposure === "all_interfaces")).sort((a, b) => a.start - b.start),
      mcp: (agent.capabilities?.mcp_servers ?? []).filter((s) => s.enabled !== false).map((s) => ({
        id: s.id, name: s.name, running: s.runtime?.state === "running",
        warn: isUnpinned(s),
      })),
    };
  }).sort((a, b) => (a.start ?? 0) - (b.start ?? 0));
  const loose = unownedDevServers(entries, report);
  if (loose.length) {
    out.push({ id: "unowned", agent: null, start: null, uptime: null, mcp: [],
      bars: loose.flatMap((e) => bar(e, e.id, e.port, e.project?.name ?? e.framework?.name ?? e.label, e.exposure === "all_interfaces")).sort((a, b) => a.start - b.start) });
  }
  return out;
}

/** Oldest start on screen, for the time scale. */
export function oldest(ls: Lane[], now: number): number {
  return Math.min(now, ...ls.flatMap((l) => [l.start ?? now, ...l.bars.map((b) => b.start)]));
}

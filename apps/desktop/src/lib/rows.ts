// Pure view-model helpers for the port list rows: what each column shows, in which order,
// and how much of it fits. Kept free of Svelte so the rules are unit-tested directly.
import type { PortEntry } from "./types";
import { humanBytes, title, uptime } from "./format";

export type Density = "comfortable" | "compact";
/** Row heights in px; mirrored by `--row-h` in the list CSS. */
export const ROW_HEIGHT: Record<Density, number> = { comfortable: 44, compact: 36 };

export function parseDensity(v: string | null | undefined): Density {
  return v === "compact" ? "compact" : "comfortable";
}

export type Status = "listening" | "bound" | "other" | "hidden" | "busy";

/** The leading status dot: one glance tells listening / UDP / other TCP state / unknown owner. */
export function rowStatus(e: PortEntry, busy = false): { status: Status; label: string } {
  if (busy) return { status: "busy", label: "Stopping…" };
  const proto = e.protocol.toUpperCase();
  if (!e.process && !e.container && !e.pid) return { status: "hidden", label: `${proto} · owner hidden` };
  if (e.protocol === "udp") return { status: "bound", label: `${proto} · bound` };
  if (e.state === "listen") return { status: "listening", label: `${proto} · listening` };
  return { status: "other", label: `${proto} · ${e.state.replace(/_/g, " ")}` };
}

export type BadgeTone = "amber" | "blue" | "violet" | "neutral" | "quiet";
export interface RowBadge { id: string; label: string; tone: BadgeTone; icon?: string; tip: string }

/**
 * Badges in priority order: what changes a decision (network exposure, protection) beats
 * context (container runtime, owner), which beats trivia (graph link count).
 */
export function rowBadges(e: PortEntry, links = 0): RowBadge[] {
  const out: RowBadge[] = [];
  if (e.exposure === "all_interfaces")
    out.push({ id: "exposed", label: "Exposed", tone: "amber", icon: "globe", tip: `Bound to ${e.addresses.join(", ")}: reachable from your network` });
  if (e.protected) out.push({ id: "protected", label: "Protected", tone: "neutral", icon: "lock", tip: "Protected: holdmap won't stop this without an explicit override" });
  if (e.container) out.push({ id: "container", label: cap(e.container.runtime), tone: "blue", icon: "box", tip: `Container ${e.container.name} (${e.container.image})` });
  if (!e.is_mine && e.user) out.push({ id: "user", label: e.user, tone: "neutral", icon: "user", tip: `Owned by ${e.user}` });
  if (links > 0) out.push({ id: "links", label: String(links), tone: "quiet", icon: "graph", tip: `${links} connected service${links === 1 ? "" : "s"} — see the graph view` });
  // Only badge really heavy apps so the column stays quiet for ordinary tools.
  if (memoryHeavy(e)) {
    const hint = memoryHint(e) ?? memLabel(entryMemory(e))!;
    out.push({ id: "memory", label: memLabel(entryMemory(e)) ?? hint, tone: "violet", icon: "activity", tip: `Uses ${hint}` });
  }
  return out;
}

/** Show at most `max` badges; the rest collapse into a "+N" chip whose tooltip lists them. */
export function splitBadges(badges: RowBadge[], max = 2): { shown: RowBadge[]; rest: RowBadge[]; overflow: string | null } {
  if (badges.length <= max) return { shown: badges, rest: [], overflow: null };
  // Keep a slot for the "+N" chip so the column never grows past `max` chips.
  const keep = Math.max(0, max - 1);
  const rest = badges.slice(keep);
  return { shown: badges.slice(0, keep), rest, overflow: rest.map((b) => (b.id === "links" ? `${b.label} connected` : b.label)).join(" · ") };
}

export interface RowMeta {
  /** Process name, container image, or a hint that the owner is hidden. */
  owner: string;
  pid: number | null;
  branch: string | null;
  uptime: string | null;
}

export function rowMeta(e: PortEntry, nowMs = Date.now()): RowMeta {
  const owner = e.container ? e.container.image : e.process ? e.process.name : e.pid ? "process" : "owner hidden";
  return { owner, pid: e.process?.pid ?? e.pid ?? null, branch: e.project?.git_branch ?? null, uptime: uptime(e, nowMs) };
}

/** Framework shown next to the name only when it adds something ("shop-web Next.js", not "Redis Redis"). */
export function rowFramework(e: PortEntry): string | null {
  return e.framework && e.framework.name !== title(e) ? e.framework.name : null;
}

export function rowLabel(e: PortEntry, opts: { pinned?: boolean; badges?: RowBadge[] } = {}): string {
  const fw = rowFramework(e);
  const bits = [`Port ${e.port} ${e.protocol}`, title(e)];
  if (fw) bits.push(fw);
  if (opts.pinned) bits.push("pinned");
  for (const b of opts.badges ?? []) if (b.id === "exposed") bits.push("exposed to network");
  return bits.join(", ");
}

export function memLabel(bytes: number | undefined): string | null {
  return bytes && bytes > 0 ? humanBytes(bytes) : null;
}

/** Preferred memory for a row: the app total (with helpers) when known, else the process alone. */
export function entryMemory(e: Pick<PortEntry, "app_memory_bytes" | "process" | "helper_count">): number {
  if (e.app_memory_bytes && e.app_memory_bytes > 0) return e.app_memory_bytes;
  return e.process?.memory_bytes ?? 0;
}

/** "1.2 GB · 4 helpers" when the app is heavy enough to call out; otherwise null. */
export function memoryHint(e: Pick<PortEntry, "app_memory_bytes" | "helper_count" | "process">): string | null {
  const bytes = entryMemory(e);
  const helpers = e.helper_count ?? 0;
  const notable = bytes >= 256 * 1024 * 1024 || (helpers >= 2 && bytes >= 64 * 1024 * 1024);
  if (!notable) return null;
  const size = humanBytes(bytes);
  if (helpers <= 0) return size;
  return helpers === 1 ? `${size} · 1 helper` : `${size} · ${helpers} helpers`;
}

/** True when the row should show a memory badge (≥ 256 MB app total). */
export function memoryHeavy(e: Pick<PortEntry, "app_memory_bytes" | "process">): boolean {
  return entryMemory(e) >= 256 * 1024 * 1024;
}

/**
 * Rolling CPU samples per entry, one per scan, for the row sparkline. Entries that vanish are
 * dropped so the map never grows without bound.
 */
export class UsageHistory {
  private samples = new Map<string, number[]>();
  constructor(
    private readonly size = 24,
    /** What to sample: CPU percent by default; memory for the details pane's trend. */
    private readonly pick: (e: Pick<PortEntry, "id" | "process" | "app_memory_bytes">) => number | undefined = (e) => e.process?.cpu_percent,
  ) {}

  push(entries: Pick<PortEntry, "id" | "process" | "app_memory_bytes">[]): void {
    const seen = new Set<string>();
    for (const e of entries) {
      seen.add(e.id);
      const cpu = this.pick(e);
      if (cpu === undefined || !Number.isFinite(cpu)) continue;
      const s = this.samples.get(e.id) ?? [];
      s.push(Math.max(0, cpu));
      if (s.length > this.size) s.splice(0, s.length - this.size);
      this.samples.set(e.id, s);
    }
    for (const id of [...this.samples.keys()]) if (!seen.has(id)) this.samples.delete(id);
  }

  get(id: string): number[] {
    return this.samples.get(id) ?? [];
  }
}

/** SVG polyline points for a sparkline of `w`×`h`; a flat mid line when there's no signal. */
export function sparkPoints(values: number[], w: number, h: number, floor = 5): string {
  if (values.length < 2) return "";
  const max = Math.max(floor, ...values);
  const step = w / (values.length - 1);
  const pad = 1;
  return values
    .map((v, i) => `${round(i * step)},${round(h - pad - (v / max) * (h - pad * 2))}`)
    .join(" ");
}

/** Like `sparkPoints`, but scaled between the samples' own min and max (memory trends, where
 * the interesting part is the change, not the distance from zero). Flat when nothing moved. */
export function rangePoints(values: number[], w: number, h: number): string {
  if (values.length < 2) return "";
  const lo = Math.min(...values), hi = Math.max(...values);
  const step = w / (values.length - 1);
  const pad = 1;
  const span = hi - lo;
  // Changes under 1% of the value are noise: draw them flat.
  const flat = span <= Math.abs(hi) * 0.01;
  return values
    .map((v, i) => `${round(i * step)},${round(flat ? h / 2 : h - pad - ((v - lo) / span) * (h - pad * 2))}`)
    .join(" ");
}

/** Toggle membership in a set without mutating it (for collapsed groups). */
export function toggled<T>(set: ReadonlySet<T>, v: T): Set<T> {
  const next = new Set(set);
  if (next.has(v)) next.delete(v);
  else next.add(v);
  return next;
}

export function parseCollapsed(raw: string | null): Set<string> {
  try {
    const v = JSON.parse(raw ?? "[]");
    return new Set(Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);
  } catch {
    return new Set();
  }
}

const round = (n: number) => Math.round(n * 10) / 10;
const cap = (s: string) => (s ? s[0].toUpperCase() + s.slice(1) : s);

/** Plain-object view of a history, for reactive state in the app. */
export function usageRecord(h: UsageHistory, ids: string[]): Record<string, number[]> {
  return Object.fromEntries(ids.map((id) => [id, h.get(id)]));
}

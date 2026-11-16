// Pure view model for the port list: filtering, ordering, sections and the summary counts.
// App.svelte only wires state to these functions, so the rules are unit-tested here.
import { GROUPS, groupOf, matches, type Filters } from "./format";
import { sectionsByCluster } from "./graph";
import type { Graph, PortEntry } from "./types";

export type Sort = "group" | "cluster" | "port" | "newest" | "memory";
export const SORTS: readonly Sort[] = ["group", "cluster", "port", "newest", "memory"];

export interface PortSection { id: string; title: string; hint: string; items: PortEntry[] }
export interface PortStats { total: number; dev: number; mine: number; exposed: number }

const memory = (e: PortEntry) => e.app_memory_bytes || e.process?.memory_bytes || 0;
const ORDER: Record<Sort, (a: PortEntry, b: PortEntry) => number> = {
  group: (a, b) => a.port - b.port,
  cluster: (a, b) => a.port - b.port,
  port: (a, b) => a.port - b.port,
  newest: (a, b) => (b.process?.start_time ?? 0) - (a.process?.start_time ?? 0),
  memory: (a, b) => memory(b) - memory(a),
};

/** Entries that pass the filters, in the chosen order. Never mutates `entries`. */
export function visiblePorts(entries: readonly PortEntry[], filters: Filters, sort: Sort): PortEntry[] {
  return entries.filter((e) => matches(e, filters)).sort(ORDER[sort]);
}

/** Pinned first, then grouped by kind, cluster, or one flat section. Empty sections are dropped. */
export function portSections(visible: readonly PortEntry[], pins: ReadonlySet<number>, sort: Sort, graph: Graph | null): PortSection[] {
  const pinned = visible.filter((e) => pins.has(e.port));
  const rest = visible.filter((e) => !pins.has(e.port));
  const head: PortSection[] = pinned.length ? [{ id: "pinned", title: "Pinned", hint: "Your favourites", items: pinned }] : [];
  if (sort === "cluster") return [...head, ...sectionsByCluster(rest, graph)];
  if (sort !== "group") return [...head, { id: "all", title: head.length ? "Everything else" : "", hint: "", items: rest }].filter((g) => g.items.length);
  return [...head, ...GROUPS.map((g) => ({ ...g, id: g.id as string, items: rest.filter((e) => groupOf(e) === g.id) }))].filter((g) => g.items.length);
}

/** Rows the keyboard can reach: collapsed titled sections are skipped (untitled ones never collapse).
 *  `hidden` drops sockets folded into another row of the same process. */
export function reachablePorts(sections: readonly PortSection[], collapsed: ReadonlySet<string>, hidden: ReadonlySet<string> = new Set()): PortEntry[] {
  return sections.flatMap((s) => (s.title && collapsed.has(s.id) ? [] : s.items.filter((e) => !hidden.has(e.id))));
}

/** Fold key for repeated non-dev sockets of one process. Dev servers, containers and databases stay one row each. */
export function socketBundleKey(e: PortEntry): string | null {
  if (e.is_dev || e.container) return null;
  const group = groupOf(e);
  if (group === "dev" || group === "data" || group === "containers") return null;
  const pid = e.process?.pid ?? e.pid;
  return pid ? String(pid) : null;
}

export interface SocketBundle { key: string; count: number; open: boolean }
export interface BundleView { hidden: Set<string>; more: Map<string, SocketBundle> }

/**
 * Three or more non-dev sockets of one process collapse onto the first row.
 * A search shows every match. `expanded` keys are process ids the user opened.
 */
export function bundleView(sections: readonly PortSection[], expanded: ReadonlySet<string>, searching: boolean): BundleView {
  const hidden = new Set<string>();
  const more = new Map<string, SocketBundle>();
  if (searching) return { hidden, more };
  for (const section of sections) {
    const groups = new Map<string, PortEntry[]>();
    for (const entry of section.items) {
      const key = socketBundleKey(entry);
      if (!key) continue;
      groups.set(key, [...(groups.get(key) ?? []), entry]);
    }
    for (const [key, group] of groups) {
      if (group.length < 3) continue;
      const open = expanded.has(key);
      more.set(group[0].id, { key, count: group.length - 1, open });
      if (!open) for (const entry of group.slice(1)) hidden.add(entry.id);
    }
  }
  return { hidden, more };
}

export function portStats(entries: readonly PortEntry[]): PortStats {
  return {
    total: entries.length,
    dev: entries.filter((e) => e.is_dev).length,
    mine: entries.filter((e) => e.is_mine).length,
    exposed: entries.filter((e) => e.exposure === "all_interfaces").length,
  };
}

/** `3000` or `:3000` in the search box means "check this port"; anything else is a text search. */
export function queryPort(query: string): number | null {
  const m = query.trim().match(/^:?(\d{1,5})$/);
  const n = m ? Number(m[1]) : NaN;
  return n > 0 && n < 65536 ? n : null;
}

export function filtersActive(f: Filters): boolean {
  return f.query !== "" || f.dev || f.mine || f.exposed || f.proto !== "any";
}

export function clearedFilters(f: Filters): Filters {
  return { ...f, query: "", dev: false, mine: false, exposed: false, proto: "any" };
}

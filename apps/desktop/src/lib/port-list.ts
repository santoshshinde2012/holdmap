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

/** Rows the keyboard can reach: collapsed titled sections are skipped (untitled ones never collapse). */
export function reachablePorts(sections: readonly PortSection[], collapsed: ReadonlySet<string>): PortEntry[] {
  return sections.flatMap((s) => (s.title && collapsed.has(s.id) ? [] : s.items));
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

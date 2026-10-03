// Pure view logic for the details pane: which tabs exist, what the footer may do, and the
// CLI equivalents. Kept out of the components so it is unit-tested.
import type { ActionPlan, Explanation, PortEntry } from "./types";
import type { TabItem } from "../components/ui/Tabs.svelte";
import { canOverride } from "./format";

export type DetailTab = "overview" | "connections" | "process" | "network" | "commands";
export interface Connections { deps: number; users: number; cluster: boolean }

/** Tabs for an entry; Connections only when it is part of the graph, Process only when there is one. */
export function detailTabs(entry: PortEntry, explanation: Explanation | null, conn: Connections): TabItem<DetailTab>[] {
  const tabs: TabItem<DetailTab>[] = [{ id: "overview", label: "Overview", alert: !!explanation?.plan?.blocked }];
  if (conn.cluster || conn.deps || conn.users) tabs.push({ id: "connections", label: "Connections", count: conn.deps + conn.users || null });
  if (entry.process || entry.container) tabs.push({ id: "process", label: entry.container ? "Container" : "Process" });
  tabs.push({ id: "network", label: "Network", alert: entry.exposure === "all_interfaces" });
  tabs.push({ id: "commands", label: "Commands", count: cliCommands(entry, explanation).length });
  return tabs;
}

/** The tab to show: the requested one when it exists for this entry, else Overview. */
export function resolveTab(want: DetailTab, tabs: TabItem<DetailTab>[]): DetailTab {
  return tabs.some((t) => t.id === want) ? want : "overview";
}

export interface StopState {
  /** Something we can signal or a container to stop. */
  stoppable: boolean;
  /** Blocked, but the user may explicitly override (soft-protected). */
  overridable: boolean;
  /** Why Stop is unavailable, for the tooltip / footer note. */
  reason: string | null;
}

export function stopState(entry: PortEntry, plan: ActionPlan | null): StopState {
  if (!entry.process && !entry.container) return { stoppable: false, overridable: false, reason: entry.pid === null ? "The owner is hidden — run portwise with admin rights" : "Nothing to stop" };
  const b = plan?.blocked;
  if (!b) return { stoppable: true, overridable: false, reason: null };
  const reason = b.kind === "needs_elevation" ? "Needs administrator rights" : b.kind === "os_service" ? "Operating-system feature — turn it off in Settings" : b.kind === "protected" ? "Protected process" : "Nothing to stop";
  return { stoppable: false, overridable: canOverride(plan!), reason };
}

export interface CliCommand { label: string; cmd: string }

/** Terminal equivalents: the explanation's own commands first, then portwise's. */
export function cliCommands(entry: PortEntry, explanation: Explanation | null): CliCommand[] {
  const out: CliCommand[] = (explanation?.commands ?? []).map((cmd) => ({ label: "Suggested", cmd }));
  const p = entry.port;
  const add = (label: string, cmd: string) => { if (!out.some((c) => c.cmd === cmd)) out.push({ label, cmd }); };
  add("Explain", `portwise explain ${p}`);
  if (entry.process || entry.container) add("Preview the stop", `portwise stop ${p} --dry-run`);
  add("Watch this port", `portwise wait ${p} --free`);
  return out;
}

export interface Glance { total: number; dev: number; exposed: PortEntry[]; exposedCount: number }

/** The empty details pane's summary: counts, and the ports other devices can reach (owned ones first). */
export function glance(entries: PortEntry[], max = 5): Glance {
  const exposed = entries
    .filter((e) => e.exposure === "all_interfaces")
    .sort((a, b) => Number(!!b.process || !!b.container) - Number(!!a.process || !!a.container) || a.port - b.port);
  return { total: entries.length, dev: entries.filter((e) => e.is_dev).length, exposed: exposed.slice(0, max), exposedCount: exposed.length };
}

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
  if (!entry.process && !entry.container) return { stoppable: false, overridable: false, reason: entry.pid === null ? "The owner is hidden — run holdmap with admin rights" : "Nothing to stop" };
  const b = plan?.blocked;
  if (!b) return { stoppable: true, overridable: false, reason: null };
  const reason = b.kind === "needs_elevation" ? "Needs administrator rights" : b.kind === "os_service" ? "Operating-system feature — turn it off in Settings" : b.kind === "protected" ? "Protected process" : "Nothing to stop";
  return { stoppable: false, overridable: canOverride(plan!), reason };
}

export interface CliCommand { label: string; cmd: string }

/** Terminal equivalents: the explanation's own commands first, then holdmap's. */
export function cliCommands(entry: PortEntry, explanation: Explanation | null): CliCommand[] {
  const out: CliCommand[] = (explanation?.commands ?? []).map((cmd) => ({ label: "Suggested", cmd }));
  const p = entry.port;
  const add = (label: string, cmd: string) => { if (!out.some((c) => c.cmd === cmd)) out.push({ label, cmd }); };
  add("Explain", `holdmap explain ${p}`);
  if (entry.process || entry.container) add("Preview the stop", `holdmap stop ${p} --dry-run`);
  add("Watch this port", `holdmap wait ${p} --free`);
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

/** `curl` for a quick look at what the port answers (headers included). */
export function curlCommand(entry: PortEntry): string {
  return `curl -i http://localhost:${entry.port}/`;
}

/** The terminal command that stops the owner, for people who'd rather run it themselves. */
export function killCommand(entry: PortEntry, platform: string): string | null {
  if (entry.container) return `docker stop ${entry.container.name}`;
  const pid = entry.process?.pid;
  if (!pid) return null;
  return platform === "windows" ? `taskkill /PID ${pid}` : `kill -TERM ${pid}`;
}

/**
 * Copy-paste command that stops helper children but leaves the listener alone.
 * Only offered when the app has helpers and isn't protected — never run automatically.
 */
export function killHelpersCommand(entry: PortEntry, platform: string): string | null {
  if (entry.protected || entry.container || !entry.process) return null;
  if ((entry.helper_count ?? 0) < 1) return null;
  const pid = entry.process.pid;
  // pkill by parent: children of the listener. Safe to copy; user still runs it.
  if (platform === "windows") return null; // no safe one-liner without enumerating PIDs
  return `pkill -TERM -P ${pid}`;
}

/** The folder quick actions work on: the project root, else the process's directory. */
export function projectFolder(entry: PortEntry): string | null {
  return entry.project?.root ?? entry.process?.cwd ?? null;
}

/** Restart (stop, then run the same command in the same folder) only makes sense for a
 * process of yours that holdmap can stop and start again: not a container or a supervisor. */
export function canRestart(entry: PortEntry, plan: ActionPlan | null): boolean {
  if (!entry.process || entry.container || !entry.is_mine || entry.protected) return false;
  if (!plan || plan.blocked) return false;
  return plan.steps.some((s) => s.action === "signal_processes");
}

/** "Google Chrome ×3", "192.168.1.24". */
export function peerLabel(p: { address: string; connections: number; process: string | null }): string {
  const who = p.process ?? p.address;
  return p.connections > 1 ? `${who} ×${p.connections}` : who;
}

/** Local date and time for a Unix timestamp in seconds: "Oct 4, 14:02" (with the year when it isn't this year). */
export function startedAt(secs: number, now = new Date()): string {
  const d = new Date(secs * 1000);
  const sameYear = d.getFullYear() === now.getFullYear();
  const date = d.toLocaleDateString(undefined, { month: "short", day: "numeric", ...(sameYear ? {} : { year: "numeric" }) });
  const time = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  return `${date}, ${time}`;
}

/** Compact start time for a tile: "14:02" today, "Oct 4" otherwise. */
export function startedShort(secs: number, now = new Date()): string {
  const d = new Date(secs * 1000);
  return d.toDateString() === now.toDateString()
    ? d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

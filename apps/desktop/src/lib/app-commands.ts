// The command palette's catalogue. A pure function of what the app shows right now; every
// command runs through an action the app supplies, so stops still open the reviewed plan.
import type { View } from "../components/shell/ViewToolbar.svelte";
import { canOpen, title, type Filters } from "./format";
import type { Command } from "./palette";
import type { Density } from "./rows";
import { formatClock } from "./screen-record";
import { THEME_ICON, type SettingsSection, type Theme } from "./settings";
import type { Sort } from "./port-list";
import type { Cluster, GraphNode, PortEntry } from "./types";

export interface CommandState {
  view: View;
  graphDetailsOpen: boolean;
  /** The selected listener, if one is in context. */
  selected: PortEntry | null;
  queryPort: number | null;
  clusters: Cluster[];
  entries: PortEntry[];
  update: string | null;
  pinned: (port: number) => boolean;
  powerAvailable: boolean;
  native: boolean;
  activeSchedules: number;
  mod: string;
  filters: Filters;
  sideOpen: boolean;
  selectedGraphNode: GraphNode | null;
  version: string;
  platform: string;
  recording: { active: boolean; elapsedMs: number; hasResult: boolean; checking: boolean; supported: boolean };
  snapshotHasResult: boolean;
  notify: boolean;
  density: Density;
  autostart: boolean;
  graphQuery: string;
  singleKeys: boolean;
  /** Port-list groups the user collapsed (their rows are out of keyboard reach). */
  collapsedGroups?: number;
}

export interface CommandActions {
  stopCluster: (name: string) => void;
  stopAllDev: () => void;
  installUpdate: () => void;
  togglePin: (e: PortEntry) => void;
  goToPort: (e: PortEntry) => void;
  /** Opens the stop plan for review; never stops directly. */
  stop: (e: PortEntry) => void;
  openInBrowser: (e: PortEntry) => void;
  scheduleShutdown: () => void;
  checkPort: (port: number) => void;
  refresh: () => void;
  schedules: () => void;
  findFree: (near: number) => void;
  setFilters: (patch: Partial<Filters>) => void;
  toggleAllSockets: () => void;
  clearFilters: () => void;
  setTheme: (theme: Theme) => void;
  setSort: (sort: Sort) => void;
  help: () => void;
  toggleSide: () => void;
  toggleGraph: () => void;
  toggleAgents: () => void;
  settings: (section?: SettingsSection) => void;
  downloads: () => void;
  pinWithLabel: (e: PortEntry | null) => void;
  remote: () => void;
  history: () => void;
  stopRecording: () => void;
  openRecording: () => void;
  snapshot: () => void;
  snapshotRecording: () => void;
  toggleNotify: () => void;
  toggleDensity: () => void;
  toggleAutostart: () => void;
  clearGraphQuery: () => void;
  expandGroups?: () => void;
}

/** Ports beyond this many are reachable by search, not listed one by one. */
const PORT_COMMANDS_MAX = 60;

export function appCommands(st: CommandState, act: CommandActions): Command[] {
  const cmds: Command[] = [];
  const s = st.view === "list" || st.view === "graph" && st.graphDetailsOpen ? st.selected : null;
  const q = st.view === "list" ? st.queryPort : null;
  const f = st.filters;
  for (const c of st.clusters) {
    cmds.push({ id: `stopc-${c.id}`, group: "Actions", icon: "stop", danger: true, title: `Stop cluster ${c.name}`, subtitle: `${c.nodes.length} services · dependency order · asks first`, keywords: `cluster ${c.kind} ${c.name}`, run: () => act.stopCluster(c.name) });
  }
  const dev = st.entries.filter((e) => e.is_dev && e.is_mine && !e.protected && !e.container && e.process);
  if (dev.length > 1) cmds.push({ id: "stop-dev", group: "Actions", icon: "stop", danger: true, title: "Stop all dev servers", subtitle: `${dev.length} running · asks first`, keywords: "kill every dev server all", run: act.stopAllDev });
  if (st.update) cmds.push({ id: "update", group: "Actions", icon: "sparkles", title: `Install holdmap ${st.update} and restart`, keywords: "update upgrade new version", run: act.installUpdate });
  if (s) cmds.push({ id: "pin", group: "Actions", icon: "star", title: st.pinned(s.port) ? `Unpin :${s.port}` : `Pin :${s.port}`, shortcut: ["P"], run: () => act.togglePin(s) });
  for (const e of st.entries.slice(0, PORT_COMMANDS_MAX)) {
    const boost = (s?.id === e.id ? 12 : 0) + (e.is_dev ? 3 : 0);
    const sub = `${e.framework?.name ?? e.process?.name ?? ""}${e.project ? ` · ${e.project.name}` : ""}`;
    cmds.push({ id: `go-${e.id}`, group: "Ports", icon: "hash", title: `:${e.port}  ${title(e)}`, subtitle: sub, boost, keywords: `${e.port} ${e.label} ${e.process?.name ?? ""}`, run: () => act.goToPort(e) });
    if ((e.process || e.container) && !e.protected && e.is_mine)
      cmds.push({ id: `stop-${e.id}`, group: "Actions", icon: "stop", danger: true, title: `Stop :${e.port}  ${title(e)}`, subtitle: s?.id === e.id ? "selected · asks first" : "asks first", boost, keywords: `kill ${e.port} ${e.label}`, run: () => act.stop(e) });
    if (canOpen(e))
      cmds.push({ id: `open-${e.id}`, group: "Actions", icon: "external", title: `Open :${e.port} in browser`, subtitle: title(e), boost, keywords: `browser ${e.port}`, run: () => act.openInBrowser(e) });
  }
  if (st.powerAvailable) cmds.push({ id: "schedule-shutdown", group: "Actions", icon: "clock", title: st.native ? "Schedule computer shutdown…" : "Schedule shutdown simulation…", subtitle: "This computer · review first", keywords: "power shutdown schedule timer hour", run: act.scheduleShutdown });
  if (q) cmds.unshift({ id: "check", group: "Ports", icon: "search", title: `Check port ${q}`, subtitle: "who's using it?", keywords: `${q} why explain`, run: () => act.checkPort(q) });
  const rec = st.recording;
  cmds.push(
    { id: "refresh", group: "Actions", icon: "refresh", title: "Refresh now", shortcut: [st.mod, "R"], run: act.refresh },
    { id: "schedules", group: "Actions", icon: "clock", title: "Scheduled actions", subtitle: `${st.activeSchedules} active · review or cancel`, keywords: "schedule timer countdown shutdown stop port agent", run: act.schedules },
    { id: "free", group: "Actions", icon: "sparkles", title: "Find a free port near 3000", keywords: "available unused", run: () => act.findFree(3000) },
    { id: "free8", group: "Actions", icon: "sparkles", title: "Find a free port near 8000", keywords: "available unused", run: () => act.findFree(8000) },
    { id: "dev", group: "Filters", icon: "filter", title: f.dev ? "Show all processes" : "Only dev servers", shortcut: ["D"], run: () => act.setFilters({ dev: !f.dev }) },
    { id: "mine", group: "Filters", icon: "filter", title: f.mine ? "Include other users" : "Only my processes", shortcut: ["M"], run: () => act.setFilters({ mine: !f.mine }) },
    { id: "exposed", group: "Filters", icon: "globe", title: f.exposed ? "Show local ports too" : "Only network-exposed ports", shortcut: ["E"], run: () => act.setFilters({ exposed: !f.exposed }) },
    { id: "all", group: "Filters", icon: "radar", title: f.all ? "Listening sockets only" : "Show all sockets (established, TIME_WAIT…)", shortcut: ["A"], run: act.toggleAllSockets },
    { id: "proto", group: "Filters", icon: "filter", title: "Cycle protocol (any → TCP → UDP)", shortcut: ["T"], run: () => act.setFilters({ proto: nextProto(f.proto) }) },
    { id: "clear", group: "Filters", icon: "x", title: "Clear search and filters", shortcut: ["Esc"], run: act.clearFilters },
    { id: "t-light", group: "View", icon: "sun", title: "Theme: light", run: () => act.setTheme("light") },
    { id: "t-dark", group: "View", icon: "moon", title: "Theme: dark", run: () => act.setTheme("dark") },
    { id: "t-sys", group: "View", icon: THEME_ICON.system, title: "Theme: match system", run: () => act.setTheme("system") },
    { id: "sort-g", group: "View", icon: "filter", title: "Sort: grouped by kind", run: () => act.setSort("group") },
    { id: "sort-p", group: "View", icon: "hash", title: "Sort: by port", run: () => act.setSort("port") },
    { id: "sort-n", group: "View", icon: "clock", title: "Sort: newest first", run: () => act.setSort("newest") },
    { id: "help", group: "View", icon: "keyboard", title: "Keyboard shortcuts", shortcut: ["?"], run: act.help },
    { id: "side", group: "View", icon: "panel", title: st.view === "graph" ? st.graphDetailsOpen ? "Close service details" : "Inspect selected service" : st.sideOpen ? "Hide the details pane" : "Show the details pane", shortcut: ["B"], keywords: "sidebar panel details larger graph canvas hide", run: act.toggleSide },
    { id: "view", group: "View", icon: st.view === "graph" ? "list" : "graph", title: st.view === "graph" ? "Show the list" : "Show the service graph", shortcut: ["G"], keywords: "mesh topology network dependencies", run: act.toggleGraph },
    { id: "agents", group: "View", icon: st.view === "agents" ? "list" : "bot", title: st.view === "agents" ? "Show the list" : "Show agents, tools and apps", shortcut: ["⇧", "A"], keywords: "ai coding agents tools apps docker claude codex cursor copilot gemini windsurf aider access folders stop", run: act.toggleAgents },
    ...(st.view === "list" && st.collapsedGroups && act.expandGroups ? [{ id: "expand-groups", group: "View" as const, icon: "layers", title: "Expand collapsed port groups", keywords: "collapse expand section group list show rows", run: act.expandGroups }] : []),
    { id: "sort-c", group: "View", icon: "layers", title: "Group by cluster (compose, workspace, supervisor…)", run: () => act.setSort("cluster") },
    { id: "settings", group: "Settings", icon: "sliders", title: "Open settings", shortcut: [st.mod, ","], keywords: "preferences options config interval hotkey", run: () => act.settings() },
    { id: "about", group: "Settings", icon: "info", title: "About holdmap", subtitle: st.version ? `${st.platform === "browser" ? "Preview" : "Running"} version ${st.version}` : undefined, keywords: "version app location architecture updates upgrade installation", run: () => act.settings("about") },
    { id: "desktop-downloads", group: "Settings", icon: "download", title: "Download desktop app", subtitle: "Open official downloads in your browser", keywords: "update upgrade latest version installer manual", run: act.downloads },
    { id: "pin-new", group: "Actions", icon: "star", title: s ? `Pin :${s.port} with a label…` : "Pin a port…", shortcut: s ? ["⇧", "P"] : undefined, keywords: "favourite label watch", run: () => act.pinWithLabel(s) },
    { id: "remote", group: "Actions", icon: "server", title: "Inspect a remote host over SSH…", keywords: "ssh remote server machine", run: act.remote },
    { id: "history", group: "Actions", icon: "history", title: "Recently stopped — restart", shortcut: ["H"], keywords: "history restart undo", run: act.history },
    rec.active
      ? { id: "screen-stop", group: "Actions", icon: "stop", title: "Stop screen recording", subtitle: formatClock(rec.elapsedMs), keywords: "capture video gif mp4 stop", shortcut: ["⇧", "V"], run: act.stopRecording }
      : rec.hasResult
        ? { id: "screen-review", group: "Actions", icon: "video", title: "Review recording", subtitle: "Preview and save the latest recording from this session", keywords: "capture video gif mp4 download record", shortcut: ["⇧", "V"], run: act.openRecording }
        : { id: "screen-record", group: "Actions", icon: "video", title: "Record screen…", subtitle: rec.checking ? "Checking screen recording support…" : rec.supported ? "MP4 or GIF · clip or full session · never uploaded" : "Screen capture is unavailable in this window", keywords: "capture video gif mp4 download", shortcut: ["⇧", "V"], run: act.openRecording },
    { id: "snapshot", group: "Actions", icon: "camera", title: st.snapshotHasResult ? "Review snapshot" : "Take snapshot…", subtitle: "Holdmap view or a selected screen/window · local PNG", keywords: "screenshot capture image png snapshot", shortcut: ["⇧", "S"], run: act.snapshot },
    ...(rec.active ? [{ id: "snapshot-recording", group: "Actions" as const, icon: "camera", title: "Snapshot recording source", subtitle: "Capture a PNG without stopping the video", keywords: "screenshot current recording still png", run: act.snapshotRecording }] : []),
    { id: "notify", group: "Settings", icon: "bell", title: st.notify ? "Turn notifications off" : "Turn notifications on", keywords: "alert conflict new listener", run: act.toggleNotify },
    { id: "density", group: "Settings", icon: "list", title: st.density === "compact" ? "Comfortable rows" : "Compact rows", keywords: "density dense rows height compact comfortable", run: act.toggleDensity },
    { id: "autostart", group: "Settings", icon: "sparkles", title: st.autostart ? "Don't launch at login" : "Launch at login", keywords: "startup boot login", run: act.toggleAutostart },
  );
  if (st.view === "graph" && st.graphQuery) cmds.push({ id: "graph-clear", group: "View", icon: "x", title: "Clear service search", shortcut: ["Esc"], run: act.clearGraphQuery });
  return cmds.filter((command) => (st.view === "list" || (command.group !== "Filters" && !command.id.startsWith("sort-") && command.id !== "density")) && (st.view !== "graph" || command.id !== "side" || !!st.selectedGraphNode?.ports.length))
    // Without single-key shortcuts, only modifier and Esc hints stay truthful.
    .map((command) => !st.singleKeys && command.shortcut && !command.shortcut.includes(st.mod) && !command.shortcut.includes("Esc") ? { ...command, shortcut: undefined } : command);
}

export const nextProto = (p: Filters["proto"]): Filters["proto"] => (p === "any" ? "tcp" : p === "tcp" ? "udp" : "any");

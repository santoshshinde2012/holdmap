<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { dataAge, freshness, pruned, share, shouldPoll, withKey } from "./lib/live";
  import Icon from "./components/Icon.svelte";
  import PortRow from "./components/list/PortRow.svelte";
  import GroupHeader from "./components/list/GroupHeader.svelte";
  import { UsageHistory, parseCollapsed, parseDensity, toggled, usageRecord, type Density } from "./lib/rows";
  import { DETAILS_CACHE_MAX, LruMap, STAMP_CACHE_MAX } from "./lib/cache";
  import DetailPane from "./components/detail/DetailPane.svelte";
  import SettingsDialog from "./components/SettingsDialog.svelte";
  import ShutdownDialog from "./components/ShutdownDialog.svelte";
  import SchedulesDialog from "./components/SchedulesDialog.svelte";
  import { scheduleCountdown, scheduleDeadline, type ScheduledAction, type ScheduledStopRequest } from "./lib/schedules";
  import type { ShutdownPreview } from "./lib/power";
  import PinDialog from "./components/PinDialog.svelte";
  import RemoteDialog from "./components/RemoteDialog.svelte";
  import { ActionDock, Button, Dialog, FilterChip, IconButton, Kbd, Select, Splitter, TextField } from "./components/ui";
  import { restartRootPid, type DetailTab } from "./lib/detail";
  import { THEME_ICON, THEME_LABEL, type SettingsActions, type SettingsModel, type SettingsSection, type HotkeyPreset, type Theme } from "./lib/settings";
  import ConfirmDialog, { type Phase } from "./components/ConfirmDialog.svelte";
  import ShortcutsDialog from "./components/ShortcutsDialog.svelte";
  import ScreenRecordDialog from "./components/ScreenRecordDialog.svelte";
  import CommandPalette from "./components/CommandPalette.svelte";
  import Onboarding from "./components/Onboarding.svelte";
  import EmptyState from "./components/EmptyState.svelte";
  import Toasts, { type Toast } from "./components/Toasts.svelte";
  import HistoryPanel from "./components/HistoryPanel.svelte";
  import * as api from "./lib/api";
  import { agentMatches, filterAgents, step, stoppableEntries, type Picture } from "./lib/agents";
  import { createAgentReportLoader } from "./lib/agents-live";
  import { createFlowLoader } from "./lib/agent-flow/live";
  import type { AgentFlowMode, AgentFlowReport, ActivitySnapshot, RecordingStatus } from "./lib/agent-flow/model";
  import type { AgentsReport, ActionPlan, Cluster, Config, DesktopHelpDestination, Explanation, Graph, GraphNode, HistoryEntry, HttpInfo, PortDetails, PortEntry, PortEvent, Snapshot, StopReport } from "./lib/types";
  import { linksLabel, localLinkCounts, nodeForEntry, sectionsByCluster } from "./lib/graph";
  import type { Command } from "./lib/palette";
  import { GROUPS, groupOf, matches, seconds, stopTarget, title, url, canOpen, type Filters, type Group } from "./lib/format";
  import { prefGet, prefGetBoolean, prefGetChoice, prefSet } from "./lib/prefs";
  import { commandForShell } from "./lib/shell";
  import { portHolderKeys } from "./lib/port-live";
  import { shortcutContext } from "./lib/keyboard";
  import { displayCaptureError, formatClock, MAX_DURATION_MS, type CaptureResult } from "./lib/screen-record";
  import { initialRecordingSupport, resolveRecordingSupport, startRecording as startScreenRecording, type RecordingSession } from "./lib/screen-record-backend";

  type Sort = "group" | "cluster" | "port" | "newest" | "memory";
  type View = "list" | "graph" | "agents";
  interface Confirm { entry: PortEntry | null; /** Reviewed plans for more ports after `entry` (Agents "Stop all"). */ queue?: { entry: PortEntry; plan: ActionPlan }[]; agentName?: string; agentId?: string; cluster: Cluster | null; plan: ActionPlan; force: boolean; allowProtected: boolean; phase: Phase; log: string[]; report: StopReport | null; completedPortCount?: number; attemptedPortCount?: number; restart?: boolean; timingIntent?: "combined" | "now" | "schedule"; scheduled?: ScheduledAction; schedulingBusy?: boolean; schedulingError?: string }

  // Live data is replaced wholesale on every poll, so it is raw state run through `share`:
  // unchanged entries keep their identity and nothing downstream re-renders or re-fetches.
  let snapshot = $state.raw<Snapshot | null>(null);
  let error = $state<string | null>(null);
  let refreshing = $state(false);
  /** Only a refresh the user asked for spins the button; background scans stay quiet. */
  let manualRefresh = $state(false);
  let filters = $state<Filters>({ query: "", all: false, proto: "any", dev: false, mine: false, exposed: false });
  let sort = $state<Sort>(prefGetChoice("sort", ["group", "cluster", "port", "newest", "memory"], "group"));
  let selectedId = $state<string | null>(null);
  // Details are stale-while-revalidate: the last value stays on screen while each scan
  // refreshes it in the background, and the skeleton only shows on a port's first load.
  let explanations = $state.raw<Record<number, Explanation>>({});
  let explainingPort = $state<number | null>(null);
  let httpInfo = $state.raw<Record<number, HttpInfo | null>>({});
  /** Bumped by every scan; the selected port's details are revalidated once per scan. */
  let scanGen = $state(0);
  /** Per port: the complete holder identity, the scan it's from, the newest request. */
  const explainMeta = new LruMap<number, { holder: string; gen: number; seq: number }>(DETAILS_CACHE_MAX);
  type ProbeStamp = { at: number; seq: number };
  const httpAt = new LruMap<number, ProbeStamp>(STAMP_CACHE_MAX);
  const HTTP_TTL_MS = 15_000;
  let explainSeq = 0;
  /** Ports whose explanation failed: retried quietly each scan, without a skeleton or toast. */
  const explainFailed = new Set<number>();
  let update = $state<string | null>(null);
  const installUpdate = () => api.installUpdate().catch((e) => toast("error", "Couldn't install the update", String(e)));
  const openDesktopHelp = (destination: DesktopHelpDestination) => api.openDesktopHelp(destination).catch((e) => toast("error", "Couldn't open desktop help", String(e)));
  let freePort = $state<Explanation | null>(null);
  let checkingPort = $state<number | null>(null);
  let portCheckError = $state<string | null>(null);
  let portCheckRetry = $state(0);
  let portCheckPending = false;
  let queuedPortCheck: (() => Promise<void>) | null = null;
  let confirm = $state<Confirm | null>(null);
  let pendingReview = $state<string | null>(null);
  let reviewSequence = 0;
  let busy = $state<Record<string, boolean>>({});
  let toasts = $state<Toast[]>([]);
  let showHelp = $state(false);
  let showPalette = $state(false);
  let onboarded = $state(prefGet("onboarded") === "1");
  let theme = $state<Theme>(prefGetChoice("theme", ["system", "light", "dark"], "system"));
  let density = $state<Density>(parseDensity(prefGet("density")));
  let singleKeyShortcuts = $state(prefGetBoolean("singleKeyShortcuts", true));
  let collapsed = $state<Set<string>>(parseCollapsed(prefGet("collapsed")));
  const usageHist = new UsageHistory(24);
  /** Memory samples for the details pane's trend line (one per scan, like CPU). */
  const memHist = new UsageHistory(24, (e) => e.app_memory_bytes || e.process?.memory_bytes);
  let memUsage = $state.raw<Record<string, number[]>>({});
  /** Connections, process tree and bind risk: loaded lazily for the selected port only. */
  let portDetails = $state.raw<Record<number, PortDetails[]>>({});
  const detailsAt = new LruMap<number, ProbeStamp>(STAMP_CACHE_MAX);
  const DETAILS_TTL_MS = 3_000;
  let usage = $state.raw<Record<string, number[]>>({});
  let systemDark = $state(matchMedia("(prefers-color-scheme: dark)").matches);
  let narrow = $state(matchMedia("(max-width: 900px)").matches);
  let drawerOpen = $state(false);
  let isMac = $state(/Mac/.test(navigator.platform));
  let search = $state<HTMLInputElement | undefined>();
  let listEl = $state<HTMLDivElement | undefined>();
  let now = $state(Date.now());
  let toastSeq = 0;
  let view = $state<View>(prefGetChoice("view", ["list", "graph", "agents"], "list"));
  /** GraphView (and @xyflow) loads only when the Graph tab is opened — keeps the first paint light. */
  let GraphView = $state<typeof import("./components/GraphView.svelte").default | null>(null);
  let graph = $state.raw<Graph | null>(null);
  /** AgentsView loads, and agents are looked up, only while the Agents tab is open. */
  let AgentsView = $state<typeof import("./components/AgentsView.svelte").default | null>(null);
  let agentsReport = $state.raw<AgentsReport | null>(null);
  let selectedAgent = $state<string | null>(null);
  let agentQuery = $state("");
  let agentsRefreshing = $state(false);
  let agentsStarted = $state(0);
  let agentFlowMode = $state<AgentFlowMode>("overview");
  let agentSection = $state<"overview" | "flow">(prefGetChoice("agents.section", ["overview", "flow"], "overview"));
  $effect(() => prefSet("agents.section", agentSection));
  let flowSource = $state<"connections" | "calls">(prefGetChoice("flow.source", ["connections", "calls"], "connections"));
  $effect(() => prefSet("flow.source", flowSource));
  const savedPicture = prefGetChoice("agents.picture", ["context", "footprint", "project"] as const, "context");
  let picture = $state<Picture>(savedPicture === "project" ? "context" : savedPicture);
  $effect(() => prefSet("agents.picture", picture));
  let projectReady = $state(false);
  let projectCaption = $state("");
  let flowPaused = $state(false);
  let flowReport = $state.raw<AgentFlowReport | null>(null);
  let activity = $state.raw<ActivitySnapshot | null>(null);
  let recording = $state.raw<RecordingStatus | null>(null);
  let flowLoading = $state(false), activityLoading = $state(false);
  let flowError = $state<string | null>(null), activityError = $state<string | null>(null);

  const visibleAgentIds = $derived(filterAgents(agentsReport?.agents ?? [], agentQuery).map((a) => a.id));
  let graphAll = $state(false);
  const topologyWanted = $derived(view === "graph" || (view === "list" && sort === "cluster"));
  const topologyAll = $derived(view === "graph" ? graphAll : filters.all);
  let selectedNode = $state<string | null>(null);
  let config = $state<Config | null>(null);
  let configError = $state<string | null>(null);
  let configLoading = $state(false);
  let showHistory = $state(false);
  let historyItems = $state<HistoryEntry[]>([]);
  let reduced = $state(matchMedia("(prefers-reduced-motion: reduce)").matches);
  let autostartOn = $state(false);
  let shortcut = $state<string | null>(null);
  let showSettings = $state(false);
  let showShutdown = $state(false);
  let shutdownIntent = $state<"now" | "schedule">("now");
  let shutdownSource = $state<"settings" | "schedules">("settings");
  let shutdownPreview = $state.raw<ShutdownPreview | null>(null);
  let shutdownLoading = $state(false);
  let shutdownBusy = $state(false);
  let shutdownRequested = $state(false);
  let shutdownError = $state<string | null>(null);
  let shutdownSequence = 0;
  let shutdownScheduled = $state.raw<ScheduledAction | null>(null);
  let schedules = $state.raw<ScheduledAction[]>([]);
  let showSchedules = $state(false);
  let schedulesLoading = $state(false);
  let schedulesError = $state<string | null>(null);
  let schedulesSequence = 0;
  let schedulesTrigger = $state<HTMLButtonElement | undefined>();
  const activeSchedules = $derived(schedules.filter((s) => ["pending", "checking", "running"].includes(s.status)));
  const nextSchedule = $derived([...activeSchedules].sort((a, b) => a.due_at_ms - b.due_at_ms)[0] ?? null);
  let settingsSection = $state<SettingsSection>("general");
  let pinDialog = $state<{ port: number | null; label: string; pinned: boolean } | null>(null);
  let showRemote = $state(false);
  let screenCapture = $state<RecordingSession | null>(null);
  let screenCaptureSupport = $state(initialRecordingSupport());
  let screenCaptureSupportSequence = 0;
  let screenCaptureElapsed = $state(0);
  let screenCaptureDialog = $state<"confirm" | "encoding" | "ready" | null>(null);
  let screenCaptureResult = $state<CaptureResult | null>(null);
  let screenCaptureError = $state<string | null>(null);
  let screenCaptureErrorTitle = $state("Couldn't start");
  let screenCaptureBusy = $state(false);
  let screenCaptureStarting = $state(false);
  let screenCaptureFinishing = false;
  let screenCaptureSequence = 0;
  let screenCaptureController: AbortController | null = null;
  let screenCaptureEncoding: RecordingSession | null = null;
  let screenCaptureTrigger: HTMLElement | null = null;
  let hotkeyList = $state<HotkeyPreset[]>([]);
  let info = $state<{ version: string; platform: string; tray: boolean; configDir: string | null; updater: boolean | null; appPath: string | null; architecture: string | null }>({ version: "", platform: "", tray: false, configDir: null, updater: null, appPath: null, architecture: null });
  let detailTab = $state<DetailTab>("overview");
  const PANE_DEFAULT = 460;
  let paneWidth = $state(Math.min(680, Math.max(340, Number(prefGet("pane")) || PANE_DEFAULT)));
  let sideOpen = $state(prefGetBoolean("side", true));
  $effect(() => { prefSet("view", view); });
  $effect(() => {
    void view;
    // A view change must not reopen the drawer for a listener selected in another view.
    drawerOpen = false;
  });
  $effect(() => {
    const visible = topologyWanted, all = topologyAll, graphVisible = view === "graph";
    untrack(() => topologyLoader.invalidate());
    if (!visible) return;
    // untrack: loadTopology reads `graph` before its await; tracking it re-ran this effect on
    // every new graph (CPU moves each scan), so Graph view rebuilt the topology in a tight loop.
    untrack(() => {
      void all;
      void loadTopology();
    });
    if (graphVisible && untrack(() => !GraphView)) {
      import("./components/GraphView.svelte").then((m) => { GraphView = m.default; }).catch((e) => console.error("Couldn't load the graph", e));
    }
    return () => topologyLoader.invalidate();
  });
  $effect(() => {
    if (view !== "agents") return;
    untrack(loadAgents); // same reason as above: loadAgents reads agentsReport
    if (!AgentsView) {
      import("./components/AgentsView.svelte").then((m) => { AgentsView = m.default; }).catch((e) => console.error("Couldn't load the agents map", e));
    }
  });
  const pins = $derived(new Set((config?.pins ?? []).map((p) => p.port)));
  const linkCount = $derived(localLinkCounts(graph));
  const mod = $derived(isMac ? "⌘" : "Ctrl");
  const modK = $derived(isMac ? "⌘K" : "Ctrl K");

  const resolvedTheme = $derived(theme === "system" ? (systemDark ? "dark" : "light") : theme);
  $effect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
    prefSet("theme", theme);
  });
  $effect(() => { prefSet("sort", sort); });
  $effect(() => { prefSet("density", density); });
  $effect(() => { prefSet("singleKeyShortcuts", singleKeyShortcuts ? "1" : "0"); });
  $effect(() => { prefSet("collapsed", JSON.stringify([...collapsed])); });

  const visible = $derived.by(() => {
    const list = (snapshot?.entries ?? []).filter((e) => matches(e, filters));
    const cmp: Record<Sort, (a: PortEntry, b: PortEntry) => number> = {
      group: (a, b) => a.port - b.port,
      cluster: (a, b) => a.port - b.port,
      port: (a, b) => a.port - b.port,
      newest: (a, b) => (b.process?.start_time ?? 0) - (a.process?.start_time ?? 0),
      memory: (a, b) => ((b.app_memory_bytes || b.process?.memory_bytes || 0) - (a.app_memory_bytes || a.process?.memory_bytes || 0)),
    };
    return list.sort(cmp[sort]);
  });
  const sections = $derived.by(() => {
    type Section = { id: string; title: string; hint: string; items: PortEntry[] };
    const pinned = visible.filter((e) => pins.has(e.port));
    const rest = visible.filter((e) => !pins.has(e.port));
    const head: Section[] = pinned.length ? [{ id: "pinned", title: "Pinned", hint: "Your favourites", items: pinned }] : [];
    if (sort === "cluster") return [...head, ...sectionsByCluster(rest, graph)];
    if (sort !== "group") return [...head, { id: "all", title: head.length ? "Everything else" : "", hint: "", items: rest }].filter((g) => g.items.length);
    return [...head, ...GROUPS.map((g) => ({ ...g, id: g.id as Group | string, items: rest.filter((e) => groupOf(e) === g.id) }))].filter((g) => g.items.length);
  });
  /** Rows the keyboard can reach: everything except collapsed sections (untitled sections never collapse). */
  const ordered = $derived(sections.flatMap((s) => (s.title && collapsed.has(s.id) ? [] : s.items)));
  const sectionOf = (id: string) => sections.find((s) => s.items.some((e) => e.id === id));
  function setCollapsed(id: string, on: boolean) { if (collapsed.has(id) !== on) collapsed = toggled(collapsed, id); }
  const selected = $derived(ordered.find((e) => e.id === selectedId) ?? null);
  const stats = $derived.by(() => {
    const es = snapshot?.entries ?? [];
    return { total: es.length, dev: es.filter((e) => e.is_dev).length, mine: es.filter((e) => e.is_mine).length, exposed: es.filter((e) => e.exposure === "all_interfaces").length };
  });
  const queryPort = $derived.by(() => {
    const m = filters.query.trim().match(/^:?(\d{1,5})$/);
    const n = m ? +m[1] : NaN;
    return n > 0 && n < 65536 ? n : null;
  });
  const filtersActive = $derived(filters.query !== "" || filters.dev || filters.mine || filters.exposed || filters.proto !== "any");
  // The agents map has no side pane: a port picked there opens in the drawer, as on narrow windows.
  const showDrawer = $derived((narrow || view === "agents") && drawerOpen && !!selected);
  /** Any overlay that owns the keyboard (they stop key events themselves; this is a backstop). */
  const modalOpen = $derived(!!confirm || !!pendingReview || showHelp || showHistory || showPalette || showSettings || showShutdown || showSchedules || showRemote || !!pinDialog || showDrawer || !!screenCaptureDialog);
  const scanMs = $derived(Math.min(60, Math.max(1, config?.scan_interval_secs ?? 3)) * 1000);

  /** When the scan in flight started: the header stays "Live" while one is merely running. */
  let scanStarted = $state(0);
  let disposed = false;

  async function refresh(manual = false) {
    if (refreshing) return;
    const all = filters.all;
    refreshing = true;
    manualRefresh = manual;
    scanStarted = Date.now();
    try {
      const next = await api.scan(all, manual ? 0 : undefined);
      // A socket-state change during this request needs its own scan. A reply for
      // the previous mode must not appear under the newly selected control.
      if (filters.all === all) {
        apply(next);
        if (manual && !disposed) toast("info", "Refreshed", `${next.entries.length} port${next.entries.length === 1 ? "" : "s"} · scanned in ${next.scan_ms} ms`);
      }
    } catch (e) {
      if (!disposed && filters.all === all) error = String(e);
    } finally {
      refreshing = false;
      manualRefresh = false;
    }
    if (!disposed && filters.all !== all) await refresh(manual);
  }

  /** Newest snapshot waiting for the next animation frame (coalesces bursty watcher + poll). */
  let applyQueued: Snapshot | null = null;
  let applyRaf = 0;

  /** Take in a snapshot, from our own scan or pushed by the app's watcher; never go back in time. */
  function apply(next: Snapshot) {
    if (disposed) return;
    if (snapshot && next.taken_at_ms < snapshot.taken_at_ms) return;
    if (applyQueued && next.taken_at_ms < applyQueued.taken_at_ms) return;
    applyQueued = next;
    if (applyRaf) return;
    applyRaf = requestAnimationFrame(() => {
      applyRaf = 0;
      const n = applyQueued;
      applyQueued = null;
      if (n) applyNow(n);
    });
  }

  function applyNow(next: Snapshot) {
    if (snapshot && next.taken_at_ms < snapshot.taken_at_ms) return;
    {
      const s = share(snapshot, next);
      snapshot = s;
      usageHist.push(s.entries);
      usage = share(usage, usageRecord(usageHist, s.entries.map((e) => e.id)));
      memHist.push(s.entries);
      memUsage = share(memUsage, usageRecord(memHist, s.entries.map((e) => e.id)));
      error = null;
      // Forget details for ports that are gone or now held by a different process.
      const holder = portHolderKeys(s.entries);
      const selectedPort = s.entries.find((e) => e.id === selectedId)?.port ?? null;
      const keep = (port: number) => holder.has(port) && explainMeta.peek(port)?.holder === holder.get(port);
      explanations = capRecord(pruned(explanations, keep), DETAILS_CACHE_MAX, selectedPort);
      httpInfo = capRecord(pruned(httpInfo, keep), DETAILS_CACHE_MAX, selectedPort);
      portDetails = capRecord(pruned(portDetails, keep), DETAILS_CACHE_MAX, selectedPort);
      httpAt.prune(keep);
      detailsAt.prune(keep);
      explainMeta.prune(keep);
      for (const port of [...explainFailed]) if (!keep(port)) explainFailed.delete(port);
      scanGen++;
      if (selectedId && !s.entries.some((e) => e.id === selectedId)) selectedId = null;
      // Build topology for the graph or the list's cluster grouping, only while needed.
      if (topologyWanted) loadTopology();
      else graph = null;
      if (view === "agents") loadAgents();
      else if (!agentsReport) loadAgents(); // badge count without opening the tab
    }
  }

  /** Keep at most `max` keys, preferring `prefer` (the selected port). */
  function capRecord<V>(rec: Record<number, V>, max: number, prefer: number | null): Record<number, V> {
    const keys = Object.keys(rec).map(Number);
    if (keys.length <= max) return rec;
    const drop = keys.filter((k) => k !== prefer).slice(0, keys.length - max);
    if (!drop.length) return rec;
    const out = { ...rec };
    for (const k of drop) delete out[k];
    return out;
  }

  const pullAgents = createAgentReportLoader(api.agents, () => agentsReport, (report) => {
    agentsReport = share(agentsReport, report);
    if (selectedAgent && !agentsReport.agents.some((a) => a.id === selectedAgent)) selectedAgent = null;
  });
  async function loadAgents() {
    if (agentsRefreshing) return;
    agentsRefreshing = true;
    agentsStarted = Date.now();
    try {
      await pullAgents();
    } catch (e) {
      if (view === "agents") toast("error", "Couldn't look up the agents", String(e));
    } finally { agentsRefreshing = false; }
  }
  const flowLoader = createFlowLoader(api.agentFlow,
    (value) => { flowReport = share(flowReport, value); },
    (message) => { flowError = message; },
    (busy) => { flowLoading = busy; });
  const activityLoader = createFlowLoader(async () => {
    const [snapshot, status] = await Promise.all([api.activitySnapshot(), api.activityStatus()]);
    return { snapshot, status };
  }, (value) => { activity = share(activity, value.snapshot); recording = share(recording, value.status); },
  (message) => { activityError = message; }, (busy) => { activityLoading = busy; });

  $effect(() => {
    const mode = agentFlowMode;
    const visible = view === "agents" && mode !== "overview" && !flowPaused;
    const interval = Math.max(1_000, scanMs);
    untrack(() => { flowLoader.invalidate(); activityLoader.invalidate(); });
    if (!visible) return;
    const loader = mode === "connections" ? flowLoader : activityLoader;
    untrack(() => { void loader.request(); });
    const timer = setInterval(() => { if (!document.hidden) void loader.request(); }, interval);
    return () => { clearInterval(timer); loader.invalidate(); };
  });

  async function mutateRecording(action: () => Promise<unknown>) {
    await action();
    // An explicit action refreshes the status even when following the view is paused.
    activityLoader.invalidate();
    if (view === "agents" && agentFlowMode === "calls") await activityLoader.request();
  }
  const startRecording = () => mutateRecording(api.startActivity);
  const stopRecording = () => mutateRecording(api.stopActivity);
  const clearRecordedActivity = () => mutateRecording(api.clearActivity);

  /** Open a port from the agents map in the details drawer, making sure the list can show it. */
  function openEntry(e: PortEntry) {
    if (!ordered.some((x) => x.id === e.id)) {
      clearFilters();
      const sec = sections.find((x) => x.items.some((i) => i.id === e.id));
      if (sec?.title && collapsed.has(sec.id)) setCollapsed(sec.id, false);
    }
    selectedId = e.id;
    drawerOpen = true;
  }
  function selectAgent(delta: number) {
    selectedAgent = step(visibleAgentIds, selectedAgent, delta);
  }

  const topologyLoader = createFlowLoader(() => api.topology(topologyAll),
    (value) => {
      graph = share(graph, value);
      if (selectedNode && !graph.nodes.some((n) => n.id === selectedNode)) selectedNode = null;
    },
    (message) => { if (message && topologyWanted) toast("error", "Couldn't build the service graph", message); },
    () => {});
  function loadTopology() {
    return topologyLoader.request();
  }

  // Keep the graph selection in step with the list selection.
  $effect(() => {
    const e = selected;
    if (!e) return;
    const n = nodeForEntry(graph, e.id);
    if (n && n.id !== selectedNode) selectedNode = n.id;
  });

  function selectNode(n: GraphNode | null) {
    selectedNode = n?.id ?? null;
    const id = n?.ports[0]?.entry_id;
    if (!id) { if (!n) selectedId = null; return; }
    if (!ordered.some((e) => e.id === id)) clearFilters();
    selectedId = id;
    if (view === "list") requestAnimationFrame(() => document.getElementById("row-" + id)?.scrollIntoView({ block: "nearest" }));
  }

  /** Every action review shares one generation, including overrides and bulk actions. */
  async function reviewAction(subject: string, build: () => Promise<Confirm>) {
    if (disposed || confirm?.phase === "running" || confirm?.schedulingBusy) return;
    const sequence = ++reviewSequence;
    pendingReview = subject;
    try {
      const reviewed = await build();
      if (!disposed && sequence === reviewSequence) confirm = reviewed;
    } catch (e) {
      if (!disposed && sequence === reviewSequence) toast("error", `Couldn't review ${subject}`, String(e));
    } finally {
      if (sequence === reviewSequence) pendingReview = null;
    }
  }

  function cancelReview() {
    reviewSequence++;
    pendingReview = null;
    confirm = null;
  }

  async function requestClusterStop(name: string) {
    const c = graph?.clusters.find((x) => x.name === name) ?? null;
    return reviewAction(`stop for cluster ${name}`, async () => {
      const plan = await api.plan(`cluster:${name}`, false, false);
      return { entry: null, cluster: c ?? { id: name, name, kind: "workspace", detail: null, root: null, nodes: [] }, plan, force: false, allowProtected: false, phase: "confirm", log: [], report: null };
    });
  }

  /** Every running dev server of yours at once (tray, palette); the dialog shows the full plan. */
  async function requestStopAllDev() {
    return reviewAction("stop for all dev servers", async () => {
      const plan = await api.plan("dev:all", false, false);
      return { entry: null, cluster: null, plan, force: false, allowProtected: false, phase: "confirm", log: [], report: null };
    });
  }

  /** Stop every unprotected port an agent started (dev servers and services), asking once. */
  async function requestStopAgentPorts(agentId: string) {
    const a = agentsReport?.agents.find((x) => x.id === agentId);
    if (!a) return;
    const list = stoppableEntries(a, snapshot?.entries ?? []);
    if (!list.length) {
      toast("info", `${a.name} has no stoppable ports`, "Its own IDE / agent listeners stay protected.");
      return;
    }
    if (list.length > 32) {
      toast("info", "Too many ports for one stop", `${a.name} has ${list.length} ports. Stop selected services individually from its card; a batch supports up to 32 reviewed ports.`);
      return;
    }
    if (list.length === 1) return requestStop(list[0], false, false, { agentId });
    return reviewAction(`stop for ${a.name}'s ports`, async () => {
      const plans = await Promise.all(list.map((entry) => api.plan(stopTarget(entry), false, false)));
      const plan = plans[0];
      return {
        entry: list[0],
        agentName: a.name,
        agentId,
        queue: list.slice(1).map((entry, index) => ({ entry, plan: plans[index + 1] })),
        cluster: null,
        plan: {
          ...plan,
          summary: `Stop ${list.length} ports ${a.name} started: ${list.map((e) => `:${e.port}`).join(", ")}`,
          owners: plans.flatMap((p) => p.owners),
          steps: plans.flatMap((p) => p.steps),
          warnings: [...new Set(plans.flatMap((p) => p.warnings))],
          blocked: plans.find((p) => p.blocked)?.blocked ?? null,
          risk: plans.some((p) => p.risk === "high") ? "high" : plans.some((p) => p.risk === "medium") ? "medium" : "low",
        },
        force: false,
        allowProtected: false,
        phase: "confirm",
        log: [],
        report: null,
      };
    });
  }

  async function revealAgentFolder(agentId: string, path: string): Promise<boolean> {
    if (!api.isTauri) {
      toast("info", "File manager preview", "The desktop app opens this folder in your file manager.");
      return true;
    }
    try {
      await api.revealAgentFolder(agentId, path);
      return true;
    } catch (e) {
      toast("error", "Couldn't open the folder", String(e));
      return false;
    }
  }

  async function openAgentFolder(agentId: string, path: string): Promise<boolean> {
    if (!api.isTauri) {
      toast("info", "Editor preview", "The desktop app opens this folder in your preferred editor.");
      return true;
    }
    try {
      const editor = await api.openAgentFolder(agentId, path);
      toast("ok", `Opened in ${editor}`, path);
      return true;
    } catch (e) {
      toast("error", "Couldn't open the folder in an editor", String(e));
      return false;
    }
  }

  async function togglePin(e: PortEntry) {
    try {
      config = await api.togglePin(e.port, title(e));
      toast("ok", pins.has(e.port) ? `Pinned :${e.port}` : `Unpinned :${e.port}`, pins.has(e.port) ? "Shown first; you'll be notified when it starts or stops." : undefined);
    } catch (err) { toast("error", "Couldn't save the pin", String(err)); }
  }

  function openPin(e: PortEntry | null) {
    const pin = e ? config?.pins.find((p) => p.port === e.port) : undefined;
    pinDialog = { port: e?.port ?? null, label: pin?.label ?? (e ? title(e) : ""), pinned: !!pin };
  }
  async function savePin(port: number, label: string) {
    config = await api.setPin(port, label || null);
    toast("ok", `Pinned :${port}`, label ? `“${label}” — shown first; you'll be notified when it starts or stops.` : "Shown first; you'll be notified when it starts or stops.");
  }
  async function unpinPort(port: number) {
    config = await api.unpin(port);
    toast("info", `Unpinned :${port}`);
  }
  const portHolder = (port: number) => { const e = snapshot?.entries.find((x) => x.port === port); return e ? `${title(e)}${e.process ? ` (PID ${e.process.pid})` : ""}` : null; };

  const settingsModel = $derived<SettingsModel | null>(config ? { theme, density, singleKeyShortcuts, config, autostart: autostartOn, shortcut, hotkeys: hotkeyList, version: info.version, platform: info.platform, configDir: info.configDir, updater: info.updater, appPath: info.appPath, architecture: info.architecture, powerAvailable: !api.isTauri || ["macos", "linux", "windows"].includes(info.platform), powerDemo: !api.isTauri, scheduledCount: activeSchedules.length } : null);
  const settingsActions: SettingsActions = {
    setTheme: (t) => { theme = t; },
    setDensity: (d) => { density = d; },
    setSingleKeyShortcuts: (on) => { singleKeyShortcuts = on; },
    setAutostart: async (on) => { autostartOn = await api.autostart(on); },
    setHotkey: async (id) => { shortcut = await api.setHotkey(id); config = await api.getConfig(); },
    setNotify: async (on, devOnly) => { config = await api.setNotify(on, devOnly); },
    setScanInterval: async (secs) => { config = await api.setPreferences({ scanIntervalSecs: secs }); },
    setHistoryLimit: async (n) => { config = await api.setPreferences({ historyLimit: n }); },
    clearHistory: async () => { await api.clearHistory(); historyItems = []; },
    copy: (t, w) => copy(t, w),
    openDesktopHelp: api.openDesktopHelp,
    shutdown: () => openShutdown("now"),
    scheduleShutdown: () => openShutdown("schedule"),
    viewSchedules: openSchedules,
  };
  async function loadConfig() {
    if (configLoading) return;
    configLoading = true;
    try {
      const value = await api.getConfig();
      if (!disposed) { config = value; configError = null; }
    } catch (e) {
      if (!disposed) {
        configError = String(e);
        if (!showSettings) toast("error", "Couldn't load settings", configError, { label: "Open settings", run: () => openSettings() });
      }
    } finally { configLoading = false; }
  }
  function openSettings(section: SettingsSection = "general") {
    if (pendingReview) cancelReview();
    settingsSection = section;
    showSettings = true;
  }

  async function openShutdown(intent: "now" | "schedule" = "now", source: "settings" | "schedules" = "settings") {
    if (shutdownBusy || shutdownLoading) return;
    if (pendingReview) cancelReview();
    shutdownIntent = intent;
    shutdownSource = source;
    showSettings = false;
    showSchedules = false;
    showShutdown = true;
    shutdownPreview = null;
    shutdownError = null;
    shutdownRequested = false;
    shutdownScheduled = null;
    shutdownLoading = true;
    const sequence = ++shutdownSequence;
    try {
      const preview = await api.shutdownPreview();
      if (showShutdown && sequence === shutdownSequence) shutdownPreview = preview;
    } catch (e) {
      if (showShutdown && sequence === shutdownSequence) shutdownError = String(e);
    } finally {
      if (sequence === shutdownSequence) shutdownLoading = false;
    }
  }

  function closeShutdown() {
    if (shutdownBusy) return;
    showShutdown = false;
    shutdownSequence++;
    shutdownLoading = false;
    shutdownPreview = null;
    const back = shutdownSource;
    shutdownSource = "settings";
    if (back === "schedules") openSchedules();
    else openSettings("power");
  }

  async function confirmShutdown() {
    if (shutdownIntent !== "now" || !shutdownPreview || shutdownBusy || shutdownLoading || shutdownRequested || shutdownScheduled) return;
    shutdownBusy = true;
    shutdownError = null;
    try {
      const result = await api.shutdownMachine(shutdownPreview.confirmation_id);
      if (!result.requested) throw new Error("The operating system did not accept the shutdown request.");
      shutdownRequested = true;
    } catch (e) {
      shutdownError = String(e);
      shutdownPreview = null; // Native handles are consumed on every attempt; re-review to retry.
    } finally {
      shutdownBusy = false;
    }
  }

  async function scheduleShutdown(delaySecs: number) {
    if (shutdownIntent !== "schedule" || !shutdownPreview || shutdownBusy || shutdownLoading || shutdownRequested || shutdownScheduled) return;
    shutdownBusy = true;
    shutdownError = null;
    try {
      shutdownScheduled = await api.scheduleShutdown(shutdownPreview.confirmation_id, delaySecs);
      await loadSchedules();
    } catch (e) {
      shutdownError = String(e);
      shutdownPreview = null;
    } finally {
      shutdownBusy = false;
    }
  }

  function acceptSchedules(items: ScheduledAction[], notify = false) {
    if (notify) {
      for (const item of items) {
        const previous = schedules.find((s) => s.id === item.id);
        if (!previous || previous.status === item.status) continue;
        if (item.status === "needs_review" || item.status === "failed") {
          toast("error", `${item.label}: ${item.status === "needs_review" ? "review required" : "action failed"}`, item.message ?? undefined, { label: "View schedules", run: openSchedules });
        } else if (item.status === "completed") {
          toast("ok", api.isTauri ? `${item.label}: complete` : "Scheduled simulation complete", item.message ?? undefined);
          refresh();
          if (view === "agents") loadAgents();
        }
      }
    }
    schedules = items;
  }

  async function loadSchedules(visible = false) {
    const sequence = ++schedulesSequence;
    if (visible) schedulesLoading = true;
    try {
      const items = await api.listSchedules();
      if (sequence === schedulesSequence) {
        acceptSchedules(items, true);
        schedulesError = null;
      }
    } catch (e) {
      if (sequence === schedulesSequence) schedulesError = String(e);
    } finally {
      if (sequence === schedulesSequence) schedulesLoading = false;
    }
  }

  function openSchedules() {
    if (shutdownBusy || confirm?.schedulingBusy || confirm?.phase === "running") return;
    showSettings = false;
    showShutdown = false;
    showPalette = false;
    showRemote = false;
    showHistory = false;
    showHelp = false;
    pinDialog = null;
    drawerOpen = false;
    shutdownSequence++;
    shutdownLoading = false;
    shutdownPreview = null;
    cancelReview();
    confirm = null;
    showSchedules = true;
    loadSchedules(true);
  }

  async function cancelSchedule(id: string) {
    const cancelled = await api.cancelSchedule(id);
    acceptSchedules(schedules.map((s) => s.id === id ? cancelled : s));
    await loadSchedules();
  }

  function closeSchedules() {
    showSchedules = false;
    requestAnimationFrame(() => schedulesTrigger?.focus());
  }

  async function openHistory() {
    try { historyItems = await api.history(50); showHistory = true; } catch (e) { toast("error", "Couldn't read history", String(e)); }
  }

  async function restartEntry(h: HistoryEntry) {
    try {
      const r = await api.restart(h);
      showHistory = false;
      toast("ok", `Restarted :${h.port}`, `${r.command} (PID ${r.pid}) · log ${r.log}`);
      setTimeout(() => refresh(), 1200);
    } catch (e) { toast("error", `Couldn't restart :${h.port}`, String(e)); }
  }

  async function toggleNotify() {
    if (!config) return;
    try { config = await api.setNotify(!config.notify, config.notify_dev_only); toast("info", config.notify ? "Notifications on" : "Notifications off", config.notify ? "New and conflicting listeners pop a desktop notification." : undefined); }
    catch (e) { toast("error", "Couldn't save settings", String(e)); }
  }

  async function toggleAutostart() {
    try { autostartOn = await api.autostart(!autostartOn); toast("info", autostartOn ? "holdmap starts at login" : "Launch at login off"); }
    catch (e) { toast("error", "Couldn't change launch at login", String(e)); }
  }

  /** Graph-view keyboard navigation: services in cluster order. */
  const graphOrder = $derived.by(() => {
    if (!graph) return [] as GraphNode[];
    const byCluster = (n: GraphNode) => (n.cluster ? graph!.clusters.findIndex((c) => c.id === n.cluster) : 999);
    return graph.nodes.filter((n) => n.kind !== "external").slice().sort((a, b) => byCluster(a) - byCluster(b) || a.label.localeCompare(b.label));
  });
  function selectGraph(delta: number) {
    if (!graphOrder.length) return;
    const i = graphOrder.findIndex((n) => n.id === selectedNode);
    const next = i < 0 ? (delta > 0 ? 0 : graphOrder.length - 1) : Math.max(0, Math.min(graphOrder.length - 1, i + delta));
    selectNode(graphOrder[next]);
  }
  const selectedGraphNode = $derived(graph?.nodes.find((n) => n.id === selectedNode) ?? null);

  // Primitives, so the effect below re-runs on a new selection or a new scan, not on every
  // poll that hands the selected entry a fresh memory or CPU figure.
  const selPort = $derived(selected?.port ?? null);
  const holderKeys = $derived(portHolderKeys(snapshot?.entries ?? []));
  const selHolder = $derived(selPort === null ? null : holderKeys.get(selPort) ?? null);
  const selTcp = $derived(selected?.protocol === "tcp");
  $effect(() => {
    const port = selPort, holder = selHolder, tcp = selTcp, gen = scanGen;
    if (port === null || holder === null) return;
    const meta = explainMeta.get(port);
    const first = !untrack(() => explanations[port]) && !explainFailed.has(port);
    if (!first && meta?.gen === gen) return;
    if (first) explainingPort = port;
    // Debounced so arrowing through the list doesn't fire a request per row.
    const t = setTimeout(async () => {
      const seq = ++explainSeq;
      explainMeta.set(port, { holder, gen, seq });
      if (Date.now() - (detailsAt.get(port)?.at ?? -Infinity) > DETAILS_TTL_MS) {
        const stamp = { at: Date.now(), seq };
        detailsAt.set(port, stamp);
        api.portDetails(port).then((d) => {
          if (explainMeta.peek(port)?.holder === holder && detailsAt.peek(port) === stamp) portDetails = withKey(portDetails, port, share(portDetails[port], d));
        }, () => { if (detailsAt.peek(port) === stamp) detailsAt.delete(port); });
      }
      if (tcp && Date.now() - (httpAt.get(port)?.at ?? -Infinity) > HTTP_TTL_MS) {
        const stamp = { at: Date.now(), seq };
        httpAt.set(port, stamp);
        api.http(port).then((h) => h, () => null).then((h) => {
          if (explainMeta.peek(port)?.holder === holder && httpAt.peek(port) === stamp) httpInfo = withKey(httpInfo, port, share(httpInfo[port], h));
        });
      }
      try {
        const x = await api.explain(port);
        // Only the newest request for this port may land, and only while it's still wanted.
        if (explainMeta.peek(port)?.seq === seq) {
          explanations = withKey(explanations, port, share(explanations[port], x));
          explainFailed.delete(port);
        }
      } catch (err) {
        if (explainMeta.peek(port)?.seq === seq) {
          if (first) toast("error", "Couldn't explain this port", String(err));
          explainFailed.add(port);
        }
      } finally {
        if (explainMeta.peek(port)?.seq === seq && explainingPort === port) explainingPort = null;
      }
    }, first ? 60 : 0);
    return () => clearTimeout(t);
  });
  const explanation = $derived(selected ? explanations[selected.port] ?? null : null);
  const selectedHttp = $derived(selected ? httpInfo[selected.port] ?? null : null);
  const selectedDetails = $derived(selected ? portDetails[selected.port]?.find((d) => d.id === selected.id) ?? null : null);
  const cpuTrend = $derived(selected ? usage[selected.id] ?? [] : []);
  const memTrend = $derived(selected ? memUsage[selected.id] ?? [] : []);
  const explaining = $derived(selPort !== null && explainingPort === selPort);

  /** A slow native check owns one request; only the latest follow-up is retained. */
  async function runPortCheck(check: () => Promise<void>) {
    if (portCheckPending) { queuedPortCheck = check; return; }
    portCheckPending = true;
    try { await check(); }
    finally {
      portCheckPending = false;
      const next = queuedPortCheck;
      queuedPortCheck = null;
      if (next && !disposed) void runPortCheck(next);
    }
  }

  $effect(() => {
    const p = queryPort, current = snapshot;
    // Availability belongs to a scan, not just a search string. Check again when new
    // data arrives, and never offer a stale "free" result after a failed scan.
    void scanGen;
    void portCheckRetry;
    const failed = error;
    portCheckError = null;
    checkingPort = null;
    if (p === null || !current || failed || current.entries.some((e) => e.port === p)) { freePort = null; return; }
    // Keep the same result visible during revalidation to avoid blinking on every
    // scan; changing the query or a failed check removes it immediately.
    if (untrack(() => freePort?.port) !== p) freePort = null;
    checkingPort = p;
    let active = true;
    const t = setTimeout(() => void runPortCheck(async () => {
      if (!active || disposed) return;
      try {
        const result = await api.explain(p);
        if (active) freePort = result;
      } catch (err) {
        if (active) { freePort = null; portCheckError = String(err); }
      } finally {
        if (active) checkingPort = null;
      }
    }), 120);
    return () => { active = false; clearTimeout(t); };
  });

  function toast(kind: Toast["kind"], text: string, detail?: string, action?: Toast["action"]) {
    const id = ++toastSeq;
    toasts = [...toasts, { id, kind, text, detail, action }].slice(-4);
    setTimeout(() => (toasts = toasts.filter((t) => t.id !== id)), kind === "error" ? 9000 : action ? 8000 : 3200);
  }

  function openScreenRecord() {
    if (screenCapture) { void finishScreenCapture(); return; }
    if (screenCaptureBusy || screenCaptureFinishing || screenCaptureDialog) return;
    screenCaptureTrigger = document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null;
    screenCaptureError = null;
    screenCaptureErrorTitle = "Couldn't start";
    screenCaptureDialog = screenCaptureResult ? "ready" : "confirm";
    if (!screenCaptureResult) refreshScreenCaptureSupport();
  }

  function refreshScreenCaptureSupport() {
    const sequence = ++screenCaptureSupportSequence;
    screenCaptureSupport = initialRecordingSupport();
    if (screenCaptureSupport.pending) void resolveRecordingSupport().then((support) => {
      if (!disposed && sequence === screenCaptureSupportSequence) screenCaptureSupport = support;
    });
  }

  async function beginScreenCapture() {
    if (disposed || screenCaptureBusy || screenCaptureDialog !== "confirm" || !screenCaptureSupport.backend) return;
    const sequence = ++screenCaptureSequence;
    screenCaptureError = null;
    screenCaptureErrorTitle = "Couldn't start";
    screenCaptureElapsed = 0;
    screenCaptureBusy = true;
    screenCaptureStarting = false;
    const controller = new AbortController();
    screenCaptureController = controller;
    try {
      let ended = false;
      const session = await startScreenRecording(screenCaptureSupport.backend, {
        onTick: (ms) => { if (!disposed && sequence === screenCaptureSequence) screenCaptureElapsed = Math.min(MAX_DURATION_MS, Math.max(0, ms)); },
        onEnded: () => {
          ended = true;
          if (!disposed && sequence === screenCaptureSequence) void finishScreenCapture();
        },
      }, controller.signal, () => {
        if (!disposed && sequence === screenCaptureSequence) screenCaptureStarting = true;
      });
      if (disposed || sequence !== screenCaptureSequence) { session.discard(); return; }
      screenCapture = session;
      screenCaptureDialog = null;
      screenCaptureBusy = false;
      // Sharing can end while the preview or encoder is still starting.
      if (ended) void finishScreenCapture();
    } catch (cause) {
      if (disposed || sequence !== screenCaptureSequence) return;
      screenCaptureBusy = false;
      screenCaptureErrorTitle = screenCaptureStarting ? "Couldn't prepare recording" : "Couldn't start";
      screenCaptureError = screenCaptureStarting ? (cause instanceof Error ? cause.message : String(cause)) : displayCaptureError(cause).message;
      screenCaptureDialog = "confirm";
    } finally {
      if (sequence === screenCaptureSequence) { screenCaptureBusy = false; screenCaptureStarting = false; }
    }
  }

  async function finishScreenCapture() {
    if (screenCaptureFinishing) return;
    const session = screenCapture;
    if (!session) return;
    const sequence = screenCaptureSequence;
    screenCaptureFinishing = true;
    screenCaptureEncoding = session;
    screenCapture = null;
    screenCaptureDialog = "encoding";
    try {
      const result = await session.stop();
      if (disposed || sequence !== screenCaptureSequence) return;
      screenCaptureResult = result;
      screenCaptureDialog = "ready";
    } catch (cause) {
      if (disposed || sequence !== screenCaptureSequence) return;
      screenCaptureDialog = "confirm";
      screenCaptureErrorTitle = "Couldn't finish";
      screenCaptureError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (sequence === screenCaptureSequence) {
        screenCaptureEncoding = null;
        screenCaptureFinishing = false;
        screenCaptureBusy = false;
      }
    }
  }

  function closeScreenCaptureDialog() {
    // Closing a preview must not destroy the only copy after a cancelled or failed save.
    // Keep one bounded result for this app session; an explicit discard clears it.
    if (screenCaptureDialog === "ready" && screenCaptureResult) screenCaptureDialog = null;
    else discardScreenCapture();
    restoreScreenCaptureFocus();
  }

  function discardScreenCaptureDialog() {
    discardScreenCapture();
    restoreScreenCaptureFocus();
  }

  function restoreScreenCaptureFocus() {
    // Let the outgoing dialog restore its own focus first, including a palette
    // that opened this preview, then choose the still-mounted recording trigger.
    void tick().then(() => {
      if (disposed || modalOpen) return;
      const trigger = screenCaptureTrigger?.isConnected ? screenCaptureTrigger : document.querySelector<HTMLElement>('[data-screen-record]');
      trigger?.focus();
    });
  }

  function discardScreenCapture() {
    ++screenCaptureSequence;
    screenCaptureController?.abort();
    screenCaptureController = null;
    screenCapture?.discard();
    screenCaptureEncoding?.discard();
    screenCapture = null;
    screenCaptureEncoding = null;
    screenCaptureFinishing = false;
    screenCaptureDialog = null;
    screenCaptureResult = null;
    screenCaptureError = null;
    screenCaptureBusy = false;
    screenCaptureStarting = false;
    screenCaptureElapsed = 0;
  }

  async function requestStop(entry: PortEntry, force: boolean, allowProtected = false, options: { agentId?: string; intent?: "combined" | "now" | "schedule" } = {}) {
    const { agentId, intent: timingIntent = agentId ? "combined" : "now" } = options;
    const holder = holderKeys.get(entry.port);
    return reviewAction(`stop for :${entry.port}`, async () => {
      const plan = await api.plan(stopTarget(entry), force, allowProtected);
      if (!holder || holderKeys.get(entry.port) !== holder) throw new Error("The port's owner changed while the stop was being reviewed. Review its current owner again.");
      return { entry, cluster: null, plan, force, allowProtected, timingIntent, phase: "confirm", log: [], report: null, agentId, agentName: agentId ? agentsReport?.agents.find((a) => a.id === agentId)?.name : undefined };
    });
  }

  function requestScheduledStop(entry: PortEntry) {
    return requestStop(entry, false, false, { intent: "schedule" });
  }

  function restartCommand(c: Confirm): string | null {
    const s = c.plan.steps.find((x) => x.action === "signal_processes");
    if (!s || s.action !== "signal_processes" || !s.processes.length) return null;
    const dir = c.entry?.project?.root ?? c.entry?.process?.cwd;
    const args = c.entry?.process?.cmdline;
    return args?.length ? commandForShell(args, dir, info.platform) : null;
  }

  async function runStop() {
    if (!confirm || confirm.timingIntent === "schedule" || confirm.phase !== "confirm" || confirm.plan.blocked || confirm.schedulingBusy || confirm.scheduled || confirm.schedulingError) return;
    const c = confirm;
    c.phase = "running";
    c.attemptedPortCount = 0;
    c.completedPortCount = 0;
    const queue = c.queue ?? [];
    const target = c.entry ? stopTarget(c.entry) : c.cluster ? `cluster:${c.cluster.name}` : "dev:all";
    const key = c.entry?.id ?? target;
    const what = queue.length && c.entry
      ? `${queue.length + 1} ports`
      : c.entry ? title(c.entry) : c.cluster ? `cluster ${c.cluster.name}` : "All dev servers";
    busy[key] = true;
    // Keep keyboard flow: after a successful stop, the selection moves to the neighbouring row.
    const idx = c.entry ? ordered.findIndex((e) => e.id === c.entry!.id) : -1;
    const neighbour = idx >= 0 ? (ordered[idx + 1] ?? ordered[idx - 1])?.id ?? null : null;
    try {
      let reports: StopReport[];
      if (queue.length) {
        if (!c.agentId || !c.entry) throw new Error("Review the agent's complete stop plan again.");
        const reviewed = [{ entry: c.entry, plan: c.plan }, ...queue];
        const requests = reviewed.map(({ entry, plan }) => ({ target: stopTarget(entry), confirmation_id: plan.confirmation_id ?? "", force: false, allow_protected: false }));
        reports = await api.stopMany(c.agentId, requests);
        c.log = reports.flatMap((report, index) => [`→ :${reviewed[index]?.entry.port ?? report.target}`, ...report.log]);
      } else {
        reports = [await api.stop(target, c.force, c.allowProtected, c.plan.confirmation_id)];
        if (!c.log.length) c.log = reports[0].log;
      }
      if (!reports.length) throw new Error("No reviewed stop was acknowledged. Review the current services again.");
      c.attemptedPortCount = reports.length;
      c.completedPortCount = reports.filter((report) => report.freed).length;
      const incomplete = reports.find((report) => !report.success || !report.freed || report.history_error);
      const r = incomplete ?? reports[reports.length - 1];
      c.report = { ...r, elapsed_ms: reports.reduce((elapsed, report) => elapsed + report.elapsed_ms, 0) };
      // A partial native result ends the batch. Never dispatch additional work from the UI.
      if (incomplete) { c.phase = r.history_error && r.success && r.freed ? "done" : "failed"; return; }
      if (reports.length !== queue.length + 1) {
        c.report = { ...c.report, success: false, error: `${reports.length} of ${queue.length + 1} reviewed targets were acknowledged. Review the remaining services again.` };
        c.phase = "failed";
        return;
      }
      if (c.report?.success && c.report.freed) {
        c.phase = "done";
        if (c.entry && selectedId === c.entry.id) selectedId = neighbour;
        if (!c.entry) { selectedId = null; selectedNode = null; }
        if (view === "agents") loadAgents();
        const restart = restartCommand(c);
        if (c.restart && c.entry && !queue.length) {
          const port = c.entry.port;
          try {
            const pid = restartRootPid(c.entry, c.plan);
            if (pid === null) throw new Error("Couldn't identify one recorded process from the reviewed stop plan. Choose a recording in Recently stopped or use the service's normal launcher.");
            const s = await api.restartStopped(port, c.entry.protocol, pid);
            setTimeout(() => { if (confirm === c) confirm = null; }, 600);
            toast("ok", `Restarted :${port}`, `${s.command} (PID ${s.pid}) · log ${s.log}`);
          } catch (err) {
            setTimeout(() => { if (confirm === c) confirm = null; }, 600);
            toast("error", `Stopped :${port}, but couldn't start it again`, String(err), { label: "Restart…", run: openHistory });
          }
          return;
        }
        setTimeout(() => {
          if (confirm === c) confirm = null;
          toast("ok",
            queue.length ? `${what} are free` : c.entry ? `Port ${c.entry.port} is free` : c.cluster ? `Cluster ${c.cluster.name} stopped` : "Dev servers stopped",
            `${what} stopped in ${seconds(c.report?.elapsed_ms ?? r.elapsed_ms)}${reports.some((report) => report.escalated) ? " (needed SIGKILL)" : ""}`,
            restart && c.entry && !queue.length ? { label: "Copy restart command", run: () => copy(restart, "Restart command") } : { label: "Restart…", run: openHistory });
        }, 900);
      } else {
        c.phase = "failed";
        if (r.success) c.report = { ...r, error: `Stopped ${what}, but :${r.ports_still_busy.join(", :")} is still busy — something else grabbed it or a supervisor restarted it.` };
      }
    } catch (e) {
      c.phase = "failed";
      c.report = { target: c.plan.target, success: false, freed: false, ports_still_busy: [], signalled: [], escalated: false, survivors: [], elapsed_ms: 0, log: [], error: String(e) };
    } finally {
      delete busy[key];
      refresh();
    }
  }

  function closeConfirm() {
    if (confirm && confirm.phase !== "running" && !confirm.schedulingBusy) cancelReview();
  }

  async function scheduleStop(delaySecs: number) {
    const c = confirm;
    if (!c?.entry || c.timingIntent === "now" || c.phase !== "confirm" || c.plan.blocked || c.force || c.allowProtected || c.restart || c.scheduled || c.schedulingBusy || c.schedulingError) return;
    c.schedulingBusy = true;
    try {
      const reviewed = [{ entry: c.entry, plan: c.plan }, ...(c.queue ?? [])];
      const requests: ScheduledStopRequest[] = reviewed.map(({ entry, plan }) => ({ target: stopTarget(entry), confirmation_id: plan.confirmation_id ?? "", force: false, allow_protected: false }));
      c.scheduled = await api.scheduleStops(requests, delaySecs, c.agentId);
      await loadSchedules();
    } catch (e) {
      c.schedulingError = String(e);
    } finally {
      c.schedulingBusy = false;
    }
  }

  function reviewStop() {
    const c = confirm;
    if (!c || c.schedulingBusy || c.phase === "running") return;
    if (c.agentId && c.queue?.length) requestStopAgentPorts(c.agentId);
    else if (c.restart && c.entry) requestRestart(c.entry);
    else if (c.entry) requestStop(c.entry, false, false, { agentId: c.agentId, intent: c.timingIntent });
    else if (c.cluster) requestClusterStop(c.cluster.name);
    else requestStopAllDev();
  }

  function selectEntry(e: PortEntry, openDrawer = true) {
    selectedId = e.id;
    if (openDrawer) drawerOpen = true;
    requestAnimationFrame(() => document.getElementById("row-" + e.id)?.scrollIntoView({ block: "nearest" }));
  }

  function select(delta: number) {
    if (!ordered.length) return;
    const i = ordered.findIndex((e) => e.id === selectedId);
    if (i < 0 && selectedId) {
      // The selection sits in a collapsed section: continue from where it would be.
      const all = sections.flatMap((x) => x.items);
      const at = all.findIndex((e) => e.id === selectedId);
      const visible = new Set(ordered.map((e) => e.id));
      const after = all.slice(at + 1).find((e) => visible.has(e.id));
      const before = all.slice(0, Math.max(0, at)).reverse().find((e) => visible.has(e.id));
      const target = delta > 0 ? after ?? before : before ?? after;
      if (target) { selectEntry(target, false); return; }
    }
    const next = i < 0 ? (delta > 0 ? 0 : ordered.length - 1) : Math.max(0, Math.min(ordered.length - 1, i + delta));
    selectEntry(ordered[next], false);
  }

  async function open(e: PortEntry) {
    try { await api.openPort(e.port); } catch (err) { toast("error", "Couldn't open the browser", String(err)); }
  }

  async function reveal(e: PortEntry) {
    try { await api.revealProject(e.port); } catch (err) { toast("error", "Couldn't show the folder", String(err)); }
  }

  async function openEditor(e: PortEntry) {
    try { const name = await api.openInEditor(e.port); toast("ok", `Opened in ${name}`, e.project?.root ?? e.process?.cwd ?? undefined); }
    catch (err) { toast("error", "Couldn't open an editor", String(err)); }
  }

  /** Restart = the normal confirmed stop, then the same command again in the same folder. */
  async function requestRestart(entry: PortEntry) {
    const holder = holderKeys.get(entry.port);
    return reviewAction(`restart for :${entry.port}`, async () => {
      const plan = await api.plan(stopTarget(entry), false, false);
      if (!holder || holderKeys.get(entry.port) !== holder) throw new Error("The port's owner changed while the restart was being reviewed. Review its current owner again.");
      return { entry, cluster: null, plan, force: false, allowProtected: false, phase: "confirm", log: [], report: null, restart: true };
    });
  }

  async function copy(text: string, what: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast("ok", `${what} copied`, text.length > 90 ? text.slice(0, 90) + "…" : text);
      return true;
    } catch {
      toast("error", "Clipboard unavailable", "Your system blocked clipboard access.");
      return false;
    }
  }

  async function findFree(near = 3000) {
    try {
      const p = await api.freePort(near);
      if (p) toast("ok", `Port ${p} is free`, `First free port at or after ${near}.`, { label: `Copy ${p}`, run: () => copy(String(p), "Port") });
      else toast("error", "No free port found", `Nothing free at or after ${near}.`);
    } catch (e) { toast("error", "Couldn't search for a free port", String(e)); }
  }

  function clearFilters() {
    filters.query = ""; filters.dev = filters.mine = filters.exposed = false; filters.proto = "any";
  }
  function cycleTheme() { theme = theme === "system" ? "light" : theme === "light" ? "dark" : "system"; }
  function dismissOnboarding() {
    onboarded = true;
    prefSet("onboarded", "1")
    listEl?.focus(); // the banner's button disappears; keep keyboard focus in the list, not on <body>
  }

  const commands = $derived.by((): Command[] => {
    const cmds: Command[] = [];
    const portView = view !== "agents";
    const s = portView ? selected : null;
    const q = portView ? queryPort : null;
    for (const c of graph?.clusters ?? []) {
      cmds.push({ id: `stopc-${c.id}`, group: "Actions", icon: "stop", danger: true, title: `Stop cluster ${c.name}`, subtitle: `${c.nodes.length} services · dependency order · asks first`, keywords: `cluster ${c.kind} ${c.name}`, run: () => requestClusterStop(c.name) });
    }
    const dev = (snapshot?.entries ?? []).filter((e) => e.is_dev && e.is_mine && !e.protected && !e.container && e.process);
    if (dev.length > 1) cmds.push({ id: "stop-dev", group: "Actions", icon: "stop", danger: true, title: "Stop all dev servers", subtitle: `${dev.length} running · asks first`, keywords: "kill every dev server all", run: requestStopAllDev });
    if (update) cmds.push({ id: "update", group: "Actions", icon: "sparkles", title: `Install holdmap ${update} and restart`, keywords: "update upgrade new version", run: installUpdate });
    if (s) cmds.push({ id: "pin", group: "Actions", icon: "star", title: pins.has(s.port) ? `Unpin :${s.port}` : `Pin :${s.port}`, shortcut: ["P"], run: () => togglePin(s) });
    for (const e of (snapshot?.entries ?? []).slice(0, 60)) {
      const boost = (s?.id === e.id ? 12 : 0) + (e.is_dev ? 3 : 0);
      const sub = `${e.framework?.name ?? e.process?.name ?? ""}${e.project ? ` · ${e.project.name}` : ""}`;
      cmds.push({ id: `go-${e.id}`, group: "Ports", icon: "hash", title: `:${e.port}  ${title(e)}`, subtitle: sub, boost, keywords: `${e.port} ${e.label} ${e.process?.name ?? ""}`, run: () => { view = "list"; clearFilters(); selectEntry(e); } });
      if ((e.process || e.container) && !e.protected && e.is_mine)
        cmds.push({ id: `stop-${e.id}`, group: "Actions", icon: "stop", danger: true, title: `Stop :${e.port}  ${title(e)}`, subtitle: s?.id === e.id ? "selected · asks first" : "asks first", boost, keywords: `kill ${e.port} ${e.label}`, run: () => requestStop(e, false) });
      if (canOpen(e))
        cmds.push({ id: `open-${e.id}`, group: "Actions", icon: "external", title: `Open :${e.port} in browser`, subtitle: title(e), boost, keywords: `browser ${e.port}`, run: () => open(e) });
    }
    if (settingsModel?.powerAvailable) cmds.push({ id: "schedule-shutdown", group: "Actions", icon: "clock", title: api.isTauri ? "Schedule computer shutdown…" : "Schedule shutdown simulation…", subtitle: "This computer · review first", keywords: "power shutdown schedule timer hour", run: () => openShutdown("schedule") });
    if (q) cmds.unshift({ id: "check", group: "Ports", icon: "search", title: `Check port ${q}`, subtitle: "who's using it?", keywords: `${q} why explain`, run: () => { filters.query = String(q); } });
    cmds.push(
      { id: "refresh", group: "Actions", icon: "refresh", title: "Refresh now", shortcut: [mod, "R"], run: () => refresh(true) },
      { id: "schedules", group: "Actions", icon: "clock", title: "Scheduled actions", subtitle: `${activeSchedules.length} active · review or cancel`, keywords: "schedule timer countdown shutdown stop port agent", run: openSchedules },
      { id: "free", group: "Actions", icon: "sparkles", title: "Find a free port near 3000", keywords: "available unused", run: () => findFree(3000) },
      { id: "free8", group: "Actions", icon: "sparkles", title: "Find a free port near 8000", keywords: "available unused", run: () => findFree(8000) },
      { id: "dev", group: "Filters", icon: "filter", title: filters.dev ? "Show all processes" : "Only dev servers", shortcut: ["D"], run: () => (filters.dev = !filters.dev) },
      { id: "mine", group: "Filters", icon: "filter", title: filters.mine ? "Include other users" : "Only my processes", shortcut: ["M"], run: () => (filters.mine = !filters.mine) },
      { id: "exposed", group: "Filters", icon: "globe", title: filters.exposed ? "Show local ports too" : "Only network-exposed ports", shortcut: ["E"], run: () => (filters.exposed = !filters.exposed) },
      { id: "all", group: "Filters", icon: "radar", title: filters.all ? "Listening sockets only" : "Show all sockets (established, TIME_WAIT…)", shortcut: ["A"], run: () => { filters.all = !filters.all; refresh(); } },
      { id: "proto", group: "Filters", icon: "filter", title: "Cycle protocol (any → TCP → UDP)", shortcut: ["T"], run: () => (filters.proto = filters.proto === "any" ? "tcp" : filters.proto === "tcp" ? "udp" : "any") },
      { id: "clear", group: "Filters", icon: "x", title: "Clear search and filters", shortcut: ["Esc"], run: clearFilters },
      { id: "t-light", group: "View", icon: "sun", title: "Theme: light", run: () => (theme = "light") },
      { id: "t-dark", group: "View", icon: "moon", title: "Theme: dark", run: () => (theme = "dark") },
      { id: "t-sys", group: "View", icon: THEME_ICON.system, title: "Theme: match system", run: () => (theme = "system") },
      { id: "sort-g", group: "View", icon: "filter", title: "Sort: grouped by kind", run: () => (sort = "group") },
      { id: "sort-p", group: "View", icon: "hash", title: "Sort: by port", run: () => (sort = "port") },
      { id: "sort-n", group: "View", icon: "clock", title: "Sort: newest first", run: () => (sort = "newest") },
      { id: "help", group: "View", icon: "keyboard", title: "Keyboard shortcuts", shortcut: ["?"], run: () => (showHelp = true) },
      { id: "side", group: "View", icon: "panel", title: sideOpen ? "Hide the details pane" : "Show the details pane", shortcut: ["B"], keywords: "sidebar panel details larger graph canvas hide", run: () => (sideOpen = !sideOpen) },
      { id: "view", group: "View", icon: view === "graph" ? "list" : "graph", title: view === "graph" ? "Show the list" : "Show the service graph", shortcut: ["G"], keywords: "mesh topology network dependencies", run: () => (view = view === "graph" ? "list" : "graph") },
      { id: "agents", group: "View", icon: view === "agents" ? "list" : "bot", title: view === "agents" ? "Show the list" : "Show agents, tools and apps", shortcut: ["⇧", "A"], keywords: "ai coding agents tools apps docker claude codex cursor copilot gemini windsurf aider access folders stop", run: () => (view = view === "agents" ? "list" : "agents") },
      { id: "sort-c", group: "View", icon: "layers", title: "Group by cluster (compose, workspace, supervisor…)", run: () => (sort = "cluster") },
      { id: "settings", group: "Settings", icon: "sliders", title: "Open settings", shortcut: [mod, ","], keywords: "preferences options config interval hotkey", run: () => openSettings() },
      { id: "about", group: "Settings", icon: "info", title: "About holdmap", subtitle: info.version ? `${info.platform === "browser" ? "Preview" : "Running"} version ${info.version}` : undefined, keywords: "version app location architecture updates upgrade installation", run: () => openSettings("about") },
      { id: "desktop-downloads", group: "Settings", icon: "download", title: "Download desktop app", subtitle: "Open official downloads in your browser", keywords: "update upgrade latest version installer manual", run: () => void openDesktopHelp("downloads") },
      { id: "pin-new", group: "Actions", icon: "star", title: s ? `Pin :${s.port} with a label…` : "Pin a port…", shortcut: s ? ["⇧", "P"] : undefined, keywords: "favourite label watch", run: () => openPin(s) },
      { id: "remote", group: "Actions", icon: "server", title: "Inspect a remote host over SSH…", keywords: "ssh remote server machine", run: () => (showRemote = true) },
      { id: "history", group: "Actions", icon: "history", title: "Recently stopped — restart", shortcut: ["H"], keywords: "history restart undo", run: openHistory },
      screenCapture
        ? { id: "screen-stop", group: "Actions", icon: "stop", title: "Stop screen recording", subtitle: formatClock(screenCaptureElapsed), keywords: "capture video gif mp4 screenshot stop", shortcut: ["⇧", "V"], run: () => void finishScreenCapture() }
        : screenCaptureResult
          ? { id: "screen-review", group: "Actions", icon: "video", title: "Review recording", subtitle: "Preview and save the latest recording from this session", keywords: "capture video gif mp4 screenshot download record", shortcut: ["⇧", "V"], run: openScreenRecord }
          : { id: "screen-record", group: "Actions", icon: "video", title: "Record screen…", subtitle: screenCaptureSupport.pending ? "Checking screen recording support…" : screenCaptureSupport.backend ? `Local MP4 and GIF · up to ${Math.round(MAX_DURATION_MS / 60_000)} min · never uploaded` : "Screen capture is unavailable in this window", keywords: "capture video gif mp4 screenshot download", shortcut: ["⇧", "V"], run: openScreenRecord },
      { id: "notify", group: "Settings", icon: "bell", title: config?.notify ? "Turn notifications off" : "Turn notifications on", keywords: "alert conflict new listener", run: toggleNotify },
      { id: "density", group: "Settings", icon: "list", title: density === "compact" ? "Comfortable rows" : "Compact rows", keywords: "density dense rows height compact comfortable", run: () => { density = density === "compact" ? "comfortable" : "compact"; } },
      { id: "autostart", group: "Settings", icon: "sparkles", title: autostartOn ? "Don't launch at login" : "Launch at login", keywords: "startup boot login", run: toggleAutostart },
    );
    return cmds.filter((command) => portView || (command.group !== "Filters" && !command.id.startsWith("sort-")))
      .map((command) => !singleKeyShortcuts && command.shortcut && !command.shortcut.includes(mod) && !command.shortcut.includes("Esc") ? { ...command, shortcut: undefined } : command);
  });

  function onKey(e: KeyboardEvent) {
    const context = shortcutContext(e, document.activeElement);
    if (context.blocked || modalOpen) return; // overlays handle their own keys
    const typing = context.editing;
    const control = context.control;
    const flowView = view === "agents" && agentFlowMode !== "overview";
    if (context.command && e.key.toLowerCase() === "k") { showPalette = true; e.preventDefault(); return; }
    if (context.command && e.key === ",") { openSettings(); e.preventDefault(); return; }
    if ((context.command && e.key.toLowerCase() === "f") || (!typing && !control && context.character && singleKeyShortcuts && e.key === "/")) { search?.focus(); search?.select(); e.preventDefault(); return; }
    if (context.command && e.key.toLowerCase() === "r") { refresh(true); e.preventDefault(); return; }
    if (!typing && context.character && singleKeyShortcuts && e.key === "V" && document.activeElement?.matches("[data-screen-record]")) {
      openScreenRecord(); e.preventDefault(); return;
    }
    if (control) return; // native/select/button controls retain their own keyboard behavior
    const inSearch = document.activeElement === search || e.target === search;
    if (typing && !inSearch) return;
    const move = view === "graph" && !typing ? selectGraph : view === "agents" ? selectAgent : select;
    if (view === "list" && !typing && context.navigation && (e.key === "ArrowLeft" || e.key === "ArrowRight") && selectedId) {
      // Tree-style: ← collapses the selected row's section, → expands it.
      const sec = sectionOf(selectedId);
      if (sec?.title) setCollapsed(sec.id, e.key === "ArrowLeft");
      e.preventDefault(); return;
    }
    if (view === "list" && !typing && context.navigation && (e.key === "Home" || e.key === "End") && ordered.length) { selectEntry(ordered[e.key === "Home" ? 0 : ordered.length - 1], false); e.preventDefault(); return; }
    if (view === "list" && !typing && context.navigation && (e.key === "PageDown" || e.key === "PageUp")) { select(e.key === "PageDown" ? 10 : -10); e.preventDefault(); return; }
    if (!typing && !flowView && ((context.navigation && (e.key === "ArrowDown" || e.key === "ArrowRight" && view === "graph")) || (context.character && singleKeyShortcuts && e.key === "j"))) { move(1); e.preventDefault(); return; }
    if (!typing && !flowView && ((context.navigation && (e.key === "ArrowUp" || e.key === "ArrowLeft" && view === "graph")) || (context.character && singleKeyShortcuts && e.key === "k"))) { move(-1); e.preventDefault(); return; }
    if (context.navigation && e.key === "Escape") {
      if (typing) search?.blur();
      else if (view === "agents" && agentQuery) agentQuery = "";
      else if (view === "agents") selectedAgent = null;
      else if (filtersActive) clearFilters();
      else selectedId = null;
      e.preventDefault();
      return;
    }
    if (context.navigation && e.key === "Enter" && view === "agents") {
      if (typing) { if (!selectedAgent || !visibleAgentIds.includes(selectedAgent)) selectAgent(1); search?.blur(); }
      return; // buttons and map nodes handle it outside search
    }
    if (context.navigation && e.key === "Enter") { if (typing) { if (!selected) select(1); search?.blur(); } else if (selected) drawerOpen = true; return; }
    if (typing || !context.character) return;
    // Only plain keys below; the list (or the page) has focus.
    // Agent metadata must never reuse a listener selection from another view.
    const s = view === "list" || view === "graph" ? selected : null;
    if (e.key === "Backspace" || e.key === "Delete") {
      if (s && (s.process || s.container)) { requestStop(s, e.shiftKey); e.preventDefault(); }
      return;
    }
    if (!singleKeyShortcuts) return;
    switch (e.key) {
      case "o": if (s && canOpen(s)) open(s); else return; break;
      case "c": if (s) copy(url(s), "URL"); else return; break;
      case "r": refresh(true); break;
      case "a": if (view === "agents") return; filters.all = !filters.all; refresh(); break;
      case "t": if (view === "agents") return; filters.proto = filters.proto === "any" ? "tcp" : filters.proto === "tcp" ? "udp" : "any"; break;
      case "d": if (view === "agents") return; filters.dev = !filters.dev; break;
      case "m": if (view === "agents") return; filters.mine = !filters.mine; break;
      case "e": if (view === "agents") return; filters.exposed = !filters.exposed; break;
      case "L": cycleTheme(); break;
      case "b": sideOpen = !sideOpen; break;
      case "g": view = view === "graph" ? "list" : "graph"; break;
      case "A": view = view === "agents" ? "list" : "agents"; break;
      case "h": openHistory(); break;
      case "V": if (screenCapture) void finishScreenCapture(); else openScreenRecord(); break;
      case "p": if (s) togglePin(s); else return; break;
      case "P": if (s) openPin(s); else return; break;
      case "s": { const n = view === "graph" ? selectedGraphNode : null; const c = n?.cluster ? graph?.clusters.find((x) => x.id === n.cluster) : null; if (c) requestClusterStop(c.name); else return; break; }
      case "?": showHelp = true; break;
      default: return;
    }
    e.preventDefault();
  }

  onMount(() => {
    refreshScreenCaptureSupport();
    if (typeof window !== "undefined" && window.__holdmapRecovered) {
      const why = window.__holdmapRecovered;
      delete window.__holdmapRecovered;
      toast("info", "holdmap recovered from a UI glitch", why);
    }
    refresh();
    loadSchedules();
    api.appInfo().then((i) => { shortcut = i.shortcut ?? null; info = { version: i.version, platform: i.platform, tray: i.tray, configDir: i.config_dir ?? null, updater: typeof i.updater === "boolean" ? i.updater : null, appPath: i.app_path ?? null, architecture: i.architecture ?? null }; if (i.platform === "macos") isMac = true; else if (i.platform !== "browser") isMac = false; }).catch((e) => toast("error", "Couldn't load app information", String(e)));
    api.hotkeys().then((h) => (hotkeyList = h)).catch(() => {});
    void loadConfig();
    api.autostart().then((v) => (autostartOn = v)).catch(() => {});
    const rq = matchMedia("(prefers-reduced-motion: reduce)");
    const onRq = () => (reduced = rq.matches);
    rq.addEventListener("change", onRq);
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const nq = matchMedia("(max-width: 900px)");
    const onMq = () => (systemDark = mq.matches);
    const onNq = () => (narrow = nq.matches);
    mq.addEventListener("change", onMq);
    nq.addEventListener("change", onNq);
    const clock = setInterval(() => (now = Date.now()), 1000);
    const schedulePoll = setInterval(() => { if (showSchedules || activeSchedules.length) loadSchedules(); }, 5000);
    const unlisten: Promise<() => void>[] = [
      api.onEvent<{ target: string; line: string }>("stop-progress", (p) => { if (confirm) confirm.log = [...confirm.log, p.line]; }),
      api.onEvent<ScheduledAction[]>("schedules-changed", (items) => { schedulesSequence++; schedulesLoading = false; schedulesError = null; acceptSchedules(items, true); }),
      api.onEvent<number>("focus-port", async (port) => {
        await refresh();
        clearFilters();
        const e = snapshot?.entries.find((x) => x.port === port);
        if (e) selectEntry(e);
      }),
      api.onEvent("refresh", () => refresh()),
      api.onEvent("stop-all-dev", () => { if (!confirm) requestStopAllDev(); }),
      api.onEvent<{ version: string; notes: string | null }>("update-available", (u) => {
        update = u.version;
        toast("info", `holdmap ${u.version} is available`, "Install it from here or the command palette; the app restarts.", { label: "Install and restart", run: installUpdate });
      }),
      api.onEvent<PortEvent[]>("port-events", (evs) => {
        for (const ev of evs) {
          if (ev.event === "conflict") toast("error", `Port conflict on :${ev.port}`, ev.entries.map((x) => `${title(x)} on ${x.addresses.join("/")}`).join(" vs "));
        }
        // The snapshot these events came from follows as a `snapshot` push.
      }),
      api.onEvent<Snapshot>("snapshot", (s) => {
        pushed = true;
        if (!filters.all && !refreshing && !confirm) apply(s);
      }),
    ].map((registration) => registration.catch((e) => {
      if (!disposed) toast("error", "Live updates unavailable", String(e));
      return () => {};
    }));
    // Back from another app, Space or a covered window: catch up at once, not on the next tick.
    const onVisible = () => {
      if (document.hidden) {
        // Drop sparkline history while hidden so a long overnight run can't grow the heap.
        // The next visible scan re-seeds from the live snapshot.
        usageHist.push([]);
        memHist.push([]);
        usage = {};
        memUsage = {};
        return;
      }
      if (!confirm && Date.now() - (snapshot?.taken_at_ms ?? 0) > 1000) refresh();
      loadSchedules();
    };
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", onVisible);
    return () => {
      disposed = true;
      reviewSequence++;
      queuedPortCheck = null;
      cancelAnimationFrame(applyRaf);
      applyRaf = 0;
      applyQueued = null;
      topologyLoader.invalidate();
      flowLoader.invalidate();
      activityLoader.invalidate();
      clearInterval(clock);
      clearInterval(schedulePoll);
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", onVisible);
      mq.removeEventListener("change", onMq);
      nq.removeEventListener("change", onNq);
      rq.removeEventListener("change", onRq);
      unlisten.forEach((u) => u.then((f) => f()).catch((e) => console.warn("Couldn't remove a live listener", e)));
      discardScreenCapture();
    };
  });

  // Foreground re-scan on the user's cadence (Settings → Scanning); paused while an action runs.
  // In the app the watcher pushes each scan, so this only fills in when pushes stop.
  /** Whether the app's watcher has pushed a snapshot (the browser build has no watcher). */
  let pushed = false;
  $effect(() => {
    const ms = scanMs;
    const t = setInterval(() => {
      if (document.hidden || confirm || refreshing || showPalette) return;
      if (shouldPoll({ pushed, all: filters.all, nowMs: Date.now(), takenAtMs: snapshot?.taken_at_ms ?? null, intervalMs: ms })) refresh();
    }, ms);
    return () => clearInterval(t);
  });
  $effect(() => { prefSet("pane", String(paneWidth)); });
  $effect(() => { prefSet("side", sideOpen ? "1" : "0"); });
  const showSide = $derived(!narrow && view !== "agents" && sideOpen);

  const ago = $derived(dataAge(now, view === "agents" ? agentsReport?.taken_at_ms ?? null : snapshot?.taken_at_ms ?? null, view === "agents" ? agentsRefreshing ? agentsStarted : null : refreshing ? scanStarted : null));
  const fresh = $derived(freshness(ago, scanMs / 1000));
  const themeIcon = $derived(THEME_ICON[theme]);
</script>

<svelte:window onkeydown={onKey} />

<div class="app" class:mac={isMac}>
  <header class="titlebar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <img src={`${import.meta.env.BASE_URL}icon.svg`} alt="" width="22" height="22" />
      <span class="name">holdmap</span>
    </div>

    <div class="search">
      {#if view === "agents"}
      <TextField
        bind:input={search}
        bind:value={agentQuery}
        type="search"
        variant="filled"
        icon="search"
        label="Search agents"
        labelHidden
        placeholder="Search agents, tools, folders, PIDs…"
        clearable
        kbdHint={singleKeyShortcuts ? "/" : undefined}
        spellcheck="false"
        autocomplete="off"
        aria-controls="agent-list"
      />
      {:else}
      <TextField
        bind:input={search}
        bind:value={filters.query}
        type="search"
        variant="filled"
        icon="search"
        label="Search ports"
        labelHidden
        placeholder="Search ports, projects, processes…"
        clearable
        kbdHint={singleKeyShortcuts ? "/" : undefined}
        spellcheck="false"
        autocomplete="off"
        aria-controls="port-list"
      />
      {/if}
    </div>

    <div class="tools">
      <span class="live" class:stale={fresh.stale || (view !== "agents" && !!error)} aria-live="off">
        <span class="pulse" class:on={view === "agents" ? agentsRefreshing : refreshing}></span><span class="live-text">{view !== "agents" && error ? refreshing ? "Retrying…" : "Scan failed" : fresh.text}</span>
      </span>
      <div class="cluster">
        <IconButton icon="command" label="Commands" kbd={modK} onclick={() => (showPalette = true)} aria-keyshortcuts="Meta+K Control+K" />
        <IconButton icon="refresh" label="Refresh" kbd={singleKeyShortcuts ? "R" : undefined} onclick={() => refresh(true)} disabled={refreshing} aria-busy={manualRefresh || undefined} class={manualRefresh ? "spinning" : ""} />
      </div>
      {#if screenCapture}
        <div class="cluster rec" role="status">
          <span class="rec-dot" aria-hidden="true"></span>
          <span class="rec-time">{formatClock(screenCaptureElapsed)}</span>
          <IconButton icon="stop" label="Stop screen recording" data-screen-record kbd={singleKeyShortcuts ? ["⇧", "V"] : undefined} onclick={() => void finishScreenCapture()} />
        </div>
      {/if}
      <div class="cluster">
        <IconButton icon="video" label={screenCapture ? "Recording screen" : screenCaptureResult ? "Review recording" : "Record screen"} data-screen-record kbd={singleKeyShortcuts ? ["⇧", "V"] : undefined} pressed={!!screenCapture || !!screenCaptureResult} onclick={openScreenRecord} />
        <IconButton icon="history" label="Recently stopped" kbd={singleKeyShortcuts ? "H" : undefined} onclick={openHistory} />
        <IconButton icon="clock" label={activeSchedules.length ? `Scheduled actions (${activeSchedules.length} active)` : "Scheduled actions"} badge={activeSchedules.length || undefined} onclick={openSchedules} bind:el={schedulesTrigger} />
        <IconButton icon="server" label="Remote host…" onclick={() => (showRemote = true)} />
      </div>
      <div class="cluster">
        <IconButton icon={themeIcon} label="Theme: {THEME_LABEL[theme]}" kbd={singleKeyShortcuts ? ["⇧", "L"] : undefined} onclick={cycleTheme} />
        <IconButton icon="sliders" label="Settings" kbd={[mod, ","]} onclick={() => openSettings()} />
      </div>
    </div>
  </header>

  <div class="toolbar" role="toolbar" aria-label="View and filters">
    <ActionDock hinge={false} animate={false} label="View" bind:value={view} options={[{ value: "list", label: "List", icon: "list", title: singleKeyShortcuts ? "Port list (G)" : "Port list" }, { value: "graph", label: "Graph", icon: "graph", count: graph?.stats.edges || null, countLabel: linksLabel(graph?.stats.edges ?? 0), title: singleKeyShortcuts ? "Service graph (G)" : "Service graph" }, { value: "agents", label: "Agents", icon: "bot", count: agentsReport?.agents.length || null, countLabel: agentsReport ? `${agentsReport.agents.length} agents and tools` : undefined, title: singleKeyShortcuts ? "Agents, tools and apps (⇧A)" : "Agents, tools and apps" }]} />
    {#if view === "agents"}
      <ActionDock label="Agents workspace" bind:value={agentSection} options={[{ value: "overview", label: "Overview", icon: "bot", title: "Agents, resources and access" }, { value: "flow", label: "Flow", icon: "branch", title: "Connections and recorded calls" }]} />
      {#if agentSection === "overview"}
        <ActionDock label="Map picture" bind:value={picture} options={[
          { value: "context", label: "Computer", icon: "monitor", title: "Context: this computer, the agents on it, and systems they talk to" },
          { value: "footprint", label: "Footprint", icon: "layers", title: "Containers: folders, tools, ports and hosts across every agent" },
          { value: "project", label: "Project", icon: "folder", title: projectReady ? `Components in ${projectCaption || "the selected source tree"}` : "Select a folder on Footprint to see that source tree", disabled: !projectReady },
        ]} />
      {/if}
      {#if agentSection === "flow"}
        <ActionDock label="Flow data source" bind:value={flowSource} options={[{ value: "connections", label: "Connections", icon: "link", title: "Observed process trees, listeners and live sockets" }, { value: "calls", label: "Calls", icon: "activity", title: "Recorded tool calls from configured agents" }]} />
      {/if}
    {/if}
    {#if view !== "agents"}
      <ActionDock label="Socket states" value={filters.all ? "all" : "listen"} options={[{ value: "listen", label: "Listening" }, { value: "all", label: "All sockets", title: `Include established, TIME_WAIT…${singleKeyShortcuts ? " (A)" : ""}` }]} onchange={(v) => { filters.all = v === "all"; refresh(); }} />
      <ActionDock hinge={false} animate={false} label="Protocol" bind:value={filters.proto} options={[{ value: "any", label: "Any" }, { value: "tcp", label: "TCP" }, { value: "udp", label: "UDP" }]} />
      <span class="divider" aria-hidden="true"></span>
      <FilterChip label="Dev servers" dot="var(--tone-green)" count={stats.dev} bind:pressed={filters.dev} title="Only dev servers" kbd={singleKeyShortcuts ? "D" : undefined} />
      <FilterChip label="Mine" icon="lock" count={stats.mine} bind:pressed={filters.mine} title="Only my processes" kbd={singleKeyShortcuts ? "M" : undefined} />
      <FilterChip label="Exposed" icon="globe" count={stats.exposed} tone={stats.exposed > 0 ? "warn" : "accent"} bind:pressed={filters.exposed} title="Reachable from the network" kbd={singleKeyShortcuts ? "E" : undefined} />
      {#if filtersActive}<Button size="sm" variant="ghost" icon="x" onclick={clearFilters}>Clear</Button>{/if}
      <div class="spacer"></div>
      {#if !narrow}<IconButton icon="panel" size="sm" pressed={sideOpen} label={sideOpen ? "Hide details pane" : "Show details pane"} kbd={singleKeyShortcuts ? "B" : undefined} onclick={() => (sideOpen = !sideOpen)} />{/if}
      <Select size="sm" prefix="Sort" label="Sort by" labelHidden bind:value={sort} width="168px" options={[{ value: "group", label: "Grouped", description: "By kind: dev, containers, databases…" }, { value: "cluster", label: "Cluster", description: "Compose project, workspace, supervisor" }, { value: "port", label: "Port", description: "Lowest port number first" }, { value: "newest", label: "Newest", description: "Most recently seen first" }, { value: "memory", label: "Memory", description: "Highest memory use first" }]} />
    {:else}
      <div class="spacer"></div>
      {#if agentQuery}<Button size="sm" variant="ghost" icon="x" onclick={() => (agentQuery = "")}>Clear search</Button>{/if}
    {/if}
  </div>

  <div class="notices">
  {#if error && (snapshot || view !== "list")}
    <div class="scan-banner" role="region" aria-label="Scan status">
      <Icon name="alert" size={16} />
      <div class="scan-summary">
        <div role="alert">
          <strong>Couldn't update port status</strong>
          <p class="selectable">{error}</p>
        </div>
        <span>{snapshot ? "Showing the last successful scan. " : ""}Retrying automatically.</span>
      </div>
      <Button size="sm" icon="refresh" loading={refreshing} loadingText="Retrying…" onclick={() => refresh(true)}>Try again</Button>
    </div>
  {/if}
  {#if nextSchedule}
    <div class="schedule-banner" role="region" aria-label="Active scheduled actions">
      <Icon name="clock" size={16} />
      <div class="schedule-summary">
        <strong>{activeSchedules.length} scheduled {activeSchedules.length === 1 ? "action" : "actions"}{!api.isTauri ? " · demo" : ""}</strong>
        <span>{nextSchedule.label} · {nextSchedule.status === "running" ? "Running" : nextSchedule.status === "checking" ? "Checking the reviewed target" : scheduleCountdown(nextSchedule.due_at_ms, now)} · <time datetime={new Date(nextSchedule.due_at_ms).toISOString()}>{scheduleDeadline(nextSchedule.due_at_ms)}</time></span>
      </div>
      <Button size="sm" variant="secondary" onclick={openSchedules}>View / cancel</Button>
    </div>
  {/if}
  </div>

  <main class="content" class:narrow class:wide={view === "agents" || !showSide} style="--pane-w: {paneWidth}px">
    {#if view === "graph"}
      <div class="graph-wrap">
        {#if GraphView}
          <GraphView {graph} {selectedNode} dark={resolvedTheme === "dark"} {reduced} all={graphAll} ontoggleall={() => { graphAll = !graphAll; }} onselect={selectNode} onstopcluster={requestClusterStop} />
        {:else}
          <div class="graph-loading" aria-busy="true">Loading graph…</div>
        {/if}
      </div>
    {:else if view === "agents"}
      <div class="graph-wrap agents-wrap">
        {#if AgentsView}
          <AgentsView bind:section={agentSection} bind:flowSource bind:picture bind:projectReady bind:projectCaption pictureChrome={false} report={agentsReport} query={agentQuery} entries={snapshot?.entries ?? []} {selectedAgent} selectedEntry={drawerOpen ? selectedId : null} dark={resolvedTheme === "dark"} {reduced} platform={info.platform}
            onselectagent={(id) => { const a = agentsReport?.agents.find((a) => a.id === id); if (a && !agentMatches(a, agentQuery)) agentQuery = ""; selectedAgent = id; }} onentry={openEntry}
            onreveal={revealAgentFolder} oneditor={openAgentFolder}
            onstop={(e, agentId) => requestStop(e, false, false, { agentId })} onstopall={requestStopAgentPorts}
            {flowReport} {activity} {recording} {flowLoading} {activityLoading} {flowError} {activityError} demo={!api.isTauri}
            paused={flowPaused} onpausechange={(paused) => { flowPaused = paused; }} onflowmodechange={(mode) => { agentFlowMode = mode; }}
            onrefreshflow={() => { void flowLoader.request(); }} onrefreshactivity={() => { void activityLoader.request(); }}
            onstartrecording={startRecording} onstoprecording={stopRecording} onclearactivity={clearRecordedActivity}
            onactivitysetup={api.activitySetup} onclearquery={() => { agentQuery = ""; }} />
        {:else}
          <div class="graph-loading" aria-busy="true">Loading agents…</div>
        {/if}
      </div>
    {:else}
    <div class="list {density}" id="port-list" role="listbox" aria-label="Ports in use" aria-activedescendant={selectedId ? "row-" + selectedId : undefined} tabindex="0" bind:this={listEl}>
      {#if snapshot && !onboarded}<Onboarding mod={mod} tray={api.isTauri && info.tray} platform={info.platform} {shortcut} ondismiss={dismissOnboarding} />{/if}

      {#if error && !snapshot}
        <EmptyState tone="err" title="Couldn't read the socket table">
          <p class="selectable">{error}</p>
          <Button variant="primary" icon="refresh" loading={refreshing} loadingText="Retrying…" onclick={() => refresh(true)}>Try again</Button>
        </EmptyState>
      {:else if !snapshot}
        <div class="skeletons" aria-busy="true" aria-label="Loading ports">
          <div class="sk-gh shimmer"></div>
          {#each Array(9) as _, i}
            <div class="sk-row" style="animation-delay: {i * 70}ms; opacity: {1 - i * 0.08}">
              <div class="shimmer" style="height:7px;width:7px;border-radius:50%"></div>
              <div class="shimmer" style="height:12px;width:38px"></div>
              <div style="display:flex;gap:8px;align-items:center"><div class="shimmer" style="height:22px;width:22px;border-radius:6px"></div><div class="shimmer" style="height:11px;width:{40 + ((i * 37) % 35)}%"></div></div>
              <div class="shimmer" style="height:9px;width:{45 + ((i * 23) % 30)}%"></div>
            </div>
          {/each}
        </div>
      {:else if ordered.length === 0}
        {#if error}
          <EmptyState tone="warn" title="Port status unavailable">
            <p>The last scan couldn't be updated. Check again before starting your server.</p>
          </EmptyState>
        {:else if checkingPort !== null && !freePort}
          <EmptyState icon="radar" title="Checking port {checkingPort}…">
            <p aria-busy="true">Checking its current availability.</p>
          </EmptyState>
        {:else if portCheckError && queryPort !== null}
          <EmptyState tone="err" title="Couldn't check port {queryPort}">
            <p class="selectable">{portCheckError}</p>
            <Button icon="refresh" onclick={() => portCheckRetry++}>Check again</Button>
          </EmptyState>
        {:else if freePort && freePort.status === "free"}
          <EmptyState tone="ok" icon="check" title="Port {freePort.port} is free">
            <p aria-busy={checkingPort !== null || undefined}>{checkingPort !== null ? "Checking availability…" : "Nothing is listening on it — start your server there:"}</p>
            <Button icon="copy" onclick={() => copy(`holdmap run -p ${freePort?.port} -- npm run dev`, "Command")}><code>holdmap run -p {freePort.port} -- npm run dev</code></Button>
            <Button variant="ghost" icon="star" onclick={() => openPin(null)}>Pin :{freePort.port} and watch it</Button>
          </EmptyState>
        {:else if freePort}
          <EmptyState tone="warn" title="Port {freePort.port}">
            <p class="selectable">{freePort.headline}</p>
            <p class="selectable">{freePort.recommendation}</p>
          </EmptyState>
        {:else if filtersActive}
          <EmptyState title="No ports match">
            <p>Nothing matches {filters.query ? `“${filters.query}”` : "these filters"}{stats.total ? ` — ${stats.total} port${stats.total === 1 ? " is" : "s are"} hidden by your search and filters` : ""}.</p>
            <Button kbd="Esc" onclick={clearFilters}>Clear search & filters</Button>
          </EmptyState>
        {:else}
          <EmptyState tone="ok" title="All quiet">
            <p>Nothing is listening right now. Start a dev server and it shows up here within a few seconds.</p>
            <Button icon="sparkles" onclick={() => findFree(3000)}>Find a free port</Button>
          </EmptyState>
        {/if}
      {:else}
        {#each sections as s (s.id)}
          {@const isCollapsed = !!s.title && collapsed.has(s.id)}
          <div class="lgroup" role="group" aria-labelledby={s.title ? `gl-${s.id}` : undefined} aria-label={s.title ? undefined : "Ports"}>
            {#if s.title}
              <GroupHeader id={s.id} title={s.title} count={s.items.length} hint={s.hint} showHint={sort === "cluster" && s.id !== "pinned"} collapsed={isCollapsed} ontoggle={() => setCollapsed(s.id, !isCollapsed)} />
            {/if}
            {#if !isCollapsed}
              <div class="lrows" id="grp-{s.id}" role="presentation">
                {#each s.items as e (e.id)}
                  <PortRow entry={e} {density} pinned={pins.has(e.port)} links={linkCount.get(e.id) ?? 0} usage={usage[e.id] ?? []} selected={e.id === selectedId} busy={!!busy[e.id]}
                    posinset={ordered.indexOf(e) + 1} setsize={ordered.length}
                    onselect={() => { selectEntry(e); listEl?.focus({ preventScroll: true }); }} onstop={() => requestStop(e, false)} onopen={() => open(e)} onpin={() => togglePin(e)} />
                {/each}
              </div>
            {/if}
          </div>
        {/each}
        {#if snapshot.hidden_sockets > 0 || !snapshot.docker_available}
          <div class="foot-note">
            {#if snapshot.hidden_sockets > 0}<span><Icon name="lock" size={12} />{snapshot.hidden_sockets} socket{snapshot.hidden_sockets === 1 ? "" : "s"} owned by other users — run with admin rights to see them</span>{/if}
            {#if !snapshot.docker_available}<span><Icon name="box" size={12} />Docker isn't running — container names unavailable</span>{/if}
          </div>
        {/if}
      {/if}
    </div>
    {/if}

    {#if showSide}
      <div class="pane-wrap">
        <Splitter bind:width={paneWidth} min={340} max={680} initial={PANE_DEFAULT} />
        <DetailPane entry={selected} {explanation} http={selectedHttp} loading={explaining} busy={selected ? !!busy[selected.id] : false} bind:tab={detailTab} {mod}
          onstop={() => selected && requestStop(selected, false)} onschedule={() => selected && requestScheduledStop(selected)} onkill={() => selected && requestStop(selected, true)} onopen={() => selected && open(selected)} oncopy={copy} onhide={() => (sideOpen = false)}
          {graph} pinned={!!selected && pins.has(selected.port)} onpin={() => selected && togglePin(selected)} onstopcluster={requestClusterStop} onselectnode={(n) => selectNode(n)}
          entries={snapshot?.entries ?? []} onselectentry={(e) => selectEntry(e, false)} details={selectedDetails} {cpuTrend} {memTrend} platform={info.platform} onrestart={() => selected && requestRestart(selected)} onreveal={() => selected && reveal(selected)} oneditor={() => selected && openEditor(selected)} />
      </div>
    {:else if !narrow && view !== "agents"}
      <IconButton class="reveal-side" icon="panel" label="Show details pane" kbd={singleKeyShortcuts ? "B" : undefined} onclick={() => (sideOpen = true)} />
    {/if}
  </main>

  <footer class="status" aria-label="Summary">
    {#if snapshot}
      <span><b>{stats.total}</b> ports</span><span class="g"><b>{stats.dev}</b> dev</span>{#if stats.exposed}<span class="w"><b>{stats.exposed}</b> exposed</span>{/if}
      <span class="sp"></span>
      <span class="hints">{#if view === "agents" && agentFlowMode !== "overview"}<span><Kbd keys="Tab" size="sm" />flow controls</span>{:else}<span><Kbd keys={["↑", "↓"]} size="sm" />move</span>{#if view !== "agents"}<span><Kbd keys="⌫" size="sm" />stop</span>{/if}{/if}{#if singleKeyShortcuts}<span><Kbd keys="G" size="sm" />graph</span><span><Kbd keys={["⇧", "A"]} size="sm" />agents</span>{/if}<span><Kbd keys={[mod, "K"]} size="sm" />commands</span>{#if singleKeyShortcuts}<span><Kbd keys="?" size="sm" />shortcuts</span>{/if}{#if shortcut}<span class="gs"><Kbd keys={shortcut} size="sm" />from anywhere</span>{/if}</span>
    {/if}
  </footer>
</div>

{#if showDrawer && selected}
  <Dialog placement="right" bare label="Details for port {selected.port}" onclose={() => (drawerOpen = false)} initialFocus="self">
    <DetailPane drawer entry={selected} {explanation} http={selectedHttp} loading={explaining} busy={!!busy[selected.id]} bind:tab={detailTab} {mod}
      onstop={() => selected && requestStop(selected, false)} onschedule={() => selected && requestScheduledStop(selected)} onkill={() => selected && requestStop(selected, true)} onopen={() => selected && open(selected)} oncopy={copy} onclose={() => (drawerOpen = false)}
      {graph} pinned={pins.has(selected.port)} onpin={() => selected && togglePin(selected)} onstopcluster={requestClusterStop} onselectnode={(n) => selectNode(n)} details={selectedDetails} {cpuTrend} {memTrend} platform={info.platform} onrestart={() => selected && requestRestart(selected)} onreveal={() => selected && reveal(selected)} oneditor={() => selected && openEditor(selected)} />
  </Dialog>
{/if}
{#if pendingReview}
  <Dialog title="Reviewing {pendingReview}…" size="sm" onclose={cancelReview} initialFocus="[data-primary]">
    <p role="status" aria-busy="true">Checking the current owners and the steps this action would take.</p>
    {#snippet footer()}<Button onclick={cancelReview} data-primary>Cancel</Button>{/snippet}
  </Dialog>
{:else if confirm}
  <ConfirmDialog intent={confirm.timingIntent ?? "combined"} entry={confirm.entry} cluster={confirm.cluster} plan={confirm.plan} portCount={(confirm.queue?.length ?? 0) + 1} completedPortCount={confirm.completedPortCount} attemptedPortCount={confirm.attemptedPortCount} agentName={confirm.agentName} restart={!!confirm.restart} phase={confirm.phase} log={confirm.log} report={confirm.report}
    onschedule={confirm.entry && !confirm.force && !confirm.allowProtected && !confirm.restart ? scheduleStop : undefined} schedulingBusy={!!confirm.schedulingBusy} schedulingError={confirm.schedulingError ?? null} scheduled={confirm.scheduled ?? null} onviewschedules={openSchedules} onreview={reviewStop} demo={!api.isTauri}
    oncopyrestart={confirm.entry && !confirm.queue?.length && restartCommand(confirm) ? () => { const command = confirm && restartCommand(confirm); if (command) void copy(command, "Restart command"); } : undefined}
    onconfirm={runStop} oncancel={closeConfirm} onoverride={() => confirm?.entry && requestStop(confirm.entry, confirm.force, true, { agentId: confirm.agentId, intent: "now" })} />
{/if}
{#if showPalette}<CommandPalette {commands} onclose={() => { showPalette = false; }} />{/if}
{#if showHistory}<HistoryPanel items={historyItems} platform={info.platform} onrestart={restartEntry} oncopy={copy} onclose={() => (showHistory = false)} onclear={async () => { await api.clearHistory(); historyItems = []; }} />{/if}
{#if showHelp}<ShortcutsDialog {mod} {singleKeyShortcuts} onclose={() => (showHelp = false)} />{/if}
{#if showSettings && settingsModel}<SettingsDialog model={settingsModel} actions={settingsActions} bind:section={settingsSection} onclose={() => (showSettings = false)} />{/if}
{#if showSettings && !settingsModel}
  <Dialog title="Settings unavailable" size="sm" onclose={() => (showSettings = false)} initialFocus="[data-primary]">
    <p role={configError ? "alert" : "status"}>{configError ?? "Loading your settings…"}</p>
    {#snippet footer()}
      <Button onclick={() => (showSettings = false)}>Close</Button>
      <Button variant="primary" icon="refresh" loading={configLoading} loadingText="Loading…" onclick={() => loadConfig()} data-primary>Try again</Button>
    {/snippet}
  </Dialog>
{/if}
{#if showShutdown}<ShutdownDialog intent={shutdownIntent} preview={shutdownPreview} loading={shutdownLoading} busy={shutdownBusy} requested={shutdownRequested} error={shutdownError} demo={!api.isTauri} onconfirm={confirmShutdown} onclose={closeShutdown} onretry={() => openShutdown(shutdownIntent)} onschedule={scheduleShutdown} scheduled={shutdownScheduled} onviewschedules={openSchedules} />{/if}
{#if showSchedules && !confirm && !showShutdown}<SchedulesDialog {schedules} loading={schedulesLoading} error={schedulesError} demo={!api.isTauri} powerAvailable={!api.isTauri || ["macos", "linux", "windows"].includes(info.platform)} entries={snapshot?.entries ?? []} onclose={closeSchedules} onrefresh={() => loadSchedules(true)} oncancel={cancelSchedule} onschedulepower={() => openShutdown("schedule", "schedules")} onscheduleport={(entry) => { closeSchedules(); requestScheduledStop(entry); }} />{/if}
{#if pinDialog}<PinDialog port={pinDialog.port} label={pinDialog.label} pinned={pinDialog.pinned} inUse={portHolder} onsave={savePin} onunpin={unpinPort} onclose={() => (pinDialog = null)} />{/if}
{#if showRemote}<RemoteDialog recent={config?.recent_hosts ?? []} onscan={async (h) => { const r = await api.remoteScan(h); config = await api.getConfig(); return r; }} oncopy={copy} onclose={() => (showRemote = false)} />{/if}
{#if screenCaptureDialog}
  <ScreenRecordDialog phase={screenCaptureDialog} error={screenCaptureError} errorTitle={screenCaptureErrorTitle} result={screenCaptureResult} busy={screenCaptureBusy} starting={screenCaptureStarting} supported={!!screenCaptureSupport.backend} checkingSupport={screenCaptureSupport.pending} unsupportedReason={screenCaptureSupport.reason} onstart={() => void beginScreenCapture()} onclose={closeScreenCaptureDialog} ondiscard={discardScreenCaptureDialog} />
{/if}
<Toasts {toasts} ondismiss={(id) => (toasts = toasts.filter((t) => t.id !== id))} />

<style>
  .app { display: grid; grid-template-columns: minmax(0, 1fr); grid-template-rows: auto auto auto minmax(0, 1fr) auto; height: 100vh; }
  .notices { min-width: 0; }
  .scan-banner { display: flex; align-items: center; gap: var(--sp-3); padding: var(--sp-3) var(--sp-5); background: var(--warn-soft); border-bottom: 1px solid var(--border); color: var(--warn); }
  .scan-summary { display: grid; gap: 3px; flex: 1; min-width: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .scan-summary strong { font-weight: var(--fw-semibold); }
  .scan-summary p { margin: 3px 0 0; color: var(--text-2); overflow-wrap: anywhere; }
  .scan-summary > span { color: var(--text-2); }
  .schedule-banner { display: flex; align-items: center; gap: var(--sp-3); padding: var(--sp-3) var(--sp-5); background: var(--accent-soft); border-bottom: 1px solid var(--border); color: var(--accent); }
  .schedule-summary { display: grid; gap: 3px; flex: 1; min-width: 0; font-size: var(--fs-body-sm); }
  .schedule-summary span { color: var(--text-2); overflow-wrap: anywhere; }
  @media (max-width: 560px) { .schedule-banner, .scan-banner { padding: var(--sp-2) var(--sp-3); gap: var(--sp-2); } }
  .titlebar { display: flex; align-items: center; gap: var(--sp-4); height: 52px; padding: 0 var(--sp-3) 0 var(--sp-4); background: var(--surface); border-bottom: 1px solid var(--border); }
  .app.mac .titlebar { padding-left: 84px; }
  .brand { display: flex; align-items: center; gap: var(--sp-2); flex: none; }
  .brand img { border-radius: 6px; box-shadow: var(--shadow-sm); }
  .name { font-weight: var(--fw-semibold); font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); }
  .search { flex: 1; max-width: 520px; margin: 0 auto; min-width: 140px; }
  .tools { display: flex; align-items: center; gap: 8px; flex: none; }
  .cluster { display: flex; align-items: center; gap: 1px; padding: 2px; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-md); }
  .cluster.rec { gap: 6px; padding: 2px 4px 2px 10px; color: var(--danger); border-color: color-mix(in srgb, var(--danger) 28%, var(--border)); }
  .rec-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--danger); box-shadow: 0 0 0 3px var(--danger-soft); }
  .rec-time { font-size: var(--fs-caption); font-variant-numeric: tabular-nums; font-weight: var(--fw-semibold); min-width: 2.4em; }
  @media (prefers-reduced-motion: no-preference) { .rec-dot { animation: beat 1.1s ease-in-out infinite; } }
  .tools :global(.spinning svg) { animation: spin 0.9s linear infinite; }
  .live { display: inline-flex; align-items: center; gap: 7px; height: var(--h-md); padding: 0 10px 0 8px; border-radius: 999px; background: var(--surface-2); border: 1px solid var(--border); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .pulse { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 3px var(--ok-soft); }
  .pulse.on { animation: beat 0.9s ease-in-out infinite; }
  .live.stale .pulse { background: var(--warn); box-shadow: 0 0 0 3px var(--warn-soft); }
  @keyframes beat { 50% { transform: scale(0.6); opacity: 0.6; } }

  .toolbar { display: flex; align-items: center; gap: var(--sp-2); padding: 8px var(--sp-4); background: var(--surface); border-bottom: 1px solid var(--border); overflow-x: auto; scrollbar-width: none; white-space: nowrap; min-height: 46px; }
  .toolbar::-webkit-scrollbar { display: none; }
  .divider { width: 1px; height: 18px; background: var(--border-strong); margin: 0 var(--sp-1); flex: none; }
  .spacer { flex: 1; min-width: var(--sp-2); }

  .content { position: relative; display: grid; grid-template-columns: minmax(0, 1fr) var(--pane-w, 440px); min-height: 0; }
  .content.narrow, .content.wide { grid-template-columns: 1fr; }
  .pane-wrap { position: relative; min-height: 0; min-width: 0; }
  .content :global(.reveal-side) { position: absolute; top: 12px; right: 12px; z-index: 6; background: var(--surface); border: 1px solid var(--border); box-shadow: var(--shadow-sm); }
  .list { overflow-y: auto; padding: 0 0 var(--sp-6); outline: none; container: portlist / inline-size; }
  .lgroup + .lgroup { margin-top: var(--sp-2); }
  .list:focus-visible:not([aria-activedescendant]) { box-shadow: inset 0 0 0 2px var(--ring); }
  .foot-note { display: flex; flex-direction: column; gap: 6px; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); padding: var(--sp-5) var(--sp-5) 0; }
  .foot-note span { display: inline-flex; align-items: center; gap: 6px; }

  .skeletons { padding: var(--sp-3) var(--sp-2); }
  .sk-gh { height: 9px; width: 110px; margin: var(--sp-4) var(--sp-5) var(--sp-3); }
  .sk-row { display: grid; grid-template-columns: 8px 48px minmax(0, 1.4fr) minmax(0, 1fr); gap: var(--sp-3); align-items: center; height: 44px; padding: 0 var(--sp-3); }

  .status { display: flex; align-items: center; gap: var(--sp-4); height: 30px; padding: 0 var(--sp-4); border-top: 1px solid var(--border); background: var(--surface); color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-variant-numeric: tabular-nums; white-space: nowrap; overflow: hidden; }
  .status b { color: var(--text); font-weight: var(--fw-medium); }
  .status .g b { color: var(--tone-green); }
  .status .w b { color: var(--warn); }
  .status .sp { flex: 1; }
  .hints { display: inline-flex; align-items: center; gap: 14px; }
  .hints > span { display: inline-flex; align-items: center; gap: 6px; }
  .graph-loading { display: grid; place-items: center; height: 100%; color: var(--muted); font-size: var(--fs-body-sm); }
  .graph-wrap { position: relative; min-height: 0; min-width: 0; }
  @keyframes spin { to { transform: rotate(360deg); } }

  @media (max-width: 760px) { .brand .name, .live-text, .hints { display: none; } .live { padding: 0; width: var(--h-md); justify-content: center; border: 0; background: transparent; } .titlebar { gap: var(--sp-2); } }
  @media (max-width: 560px) {
    .titlebar { display: grid; grid-template-columns: auto minmax(0, 1fr); height: auto; padding-top: var(--sp-2); padding-bottom: var(--sp-2); }
    .app.mac .titlebar { padding-left: var(--sp-4); }
    .search { width: 100%; min-width: 0; }
    .tools { grid-column: 1 / -1; justify-self: end; }
  }
</style>

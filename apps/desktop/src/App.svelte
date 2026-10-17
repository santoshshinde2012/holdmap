<script lang="ts">
  import { insightsStore } from "./lib/insights-store.svelte";
  import { onMount, tick, untrack } from "svelte";
  import { dataAge, freshness, pruned, share, shouldPoll, withKey } from "./lib/live";
  import PortList from "./components/list/PortList.svelte";
  import TitleBar from "./components/shell/TitleBar.svelte";
  import ViewToolbar from "./components/shell/ViewToolbar.svelte";
  import Notices from "./components/shell/Notices.svelte";
  import StatusBar from "./components/shell/StatusBar.svelte";
  import HomeView from "./components/home/HomeView.svelte";
  import { nextLayout } from "./lib/home-layout";
  import FirstRun from "./components/FirstRun.svelte";
  import { firstRunPlan, type FirstRunChoices } from "./lib/onboarding";
  import { UsageHistory, toggled, usageRecord } from "./lib/rows";
  import { DETAILS_CACHE_MAX, LruMap, STAMP_CACHE_MAX } from "./lib/cache";
  import DetailPane from "./components/detail/DetailPane.svelte";
  import SettingsDialog from "./components/SettingsDialog.svelte";
  import ShutdownDialog from "./components/ShutdownDialog.svelte";
  import SchedulesDialog from "./components/SchedulesDialog.svelte";
  import { type ScheduledAction, type ScheduledStopRequest } from "./lib/schedules";
  import type { ShutdownPreview } from "./lib/power";
  import PinDialog from "./components/PinDialog.svelte";
  import RemoteDialog from "./components/RemoteDialog.svelte";
  import { Dialog, IconButton, Splitter } from "./components/ui";
  import { restartRootPid, type DetailTab } from "./lib/detail";
  import { THEME_ICON, THEME_LABEL, type SettingsActions, type SettingsModel, type SettingsSection, type HotkeyPreset } from "./lib/settings";
  import ConfirmDialog, { type Phase } from "./components/ConfirmDialog.svelte";
  import ShortcutsDialog from "./components/ShortcutsDialog.svelte";
  import ScreenRecordDialog from "./components/ScreenRecordDialog.svelte";
  import SnapshotCapture from "./components/SnapshotCapture.svelte";
  import type { SnapshotCaptureHandle } from "./lib/snapshot-controller";
  import CommandPalette from "./components/CommandPalette.svelte";
  import Toasts, { type Toast } from "./components/Toasts.svelte";
  import HistoryPanel from "./components/HistoryPanel.svelte";
  import SearchBox from "./components/shell/SearchBox.svelte";
  import ReviewPendingDialog from "./components/ReviewPendingDialog.svelte";
  import SettingsUnavailableDialog from "./components/SettingsUnavailableDialog.svelte";
  import * as api from "./lib/api";
  import { agentMatches, filterAgents, step, stoppableEntries } from "./lib/agents";
  import { createAgentReportLoader } from "./lib/agents-live";
  import { createFlowLoader } from "./lib/agent-flow/live";
  import type { AgentFlowMode, AgentFlowReport, ActivitySnapshot, RecordingStatus } from "./lib/agent-flow/model";
  import type { AgentsReport, ActionPlan, Cluster, Config, DesktopHelpDestination, Explanation, Graph, GraphNode, HistoryEntry, HttpInfo, PortDetails, PortEntry, PortEvent, Snapshot, StopReport } from "./lib/types";
  import { localLinkCounts, nodeForEntry } from "./lib/graph";
  import { appCommands, nextProto } from "./lib/app-commands";
  import { seconds, stopTarget, title, url, canOpen, type Filters } from "./lib/format";
  import { clearedFilters, filtersActive as isFiltering, portSections, portStats, queryPort as parseQueryPort, reachablePorts, visiblePorts } from "./lib/port-list";
  import { prefGet, prefSet } from "./lib/prefs";
  import { AppPrefs, PANE_DEFAULT, PANE_MAX, PANE_MIN } from "./lib/app-prefs.svelte";
  import { commandForShell } from "./lib/shell";
  import { portHolderKeys } from "./lib/port-live";
  import { shortcutContext } from "./lib/keyboard";
  import { decideKey, type KeyIntent } from "./lib/app-shortcuts";
  import { searchControls } from "./lib/search-target";
  import { ScreenCaptureController } from "./lib/screen-capture.svelte";

  interface Confirm { entry: PortEntry | null; /** Reviewed plans for more ports after `entry` (Agents "Stop all"). */ queue?: { entry: PortEntry; plan: ActionPlan }[]; agentName?: string; agentId?: string; cluster: Cluster | null; plan: ActionPlan; force: boolean; allowProtected: boolean; phase: Phase; log: string[]; report: StopReport | null; completedPortCount?: number; attemptedPortCount?: number; restart?: boolean; timingIntent?: "combined" | "now" | "schedule"; scheduled?: ScheduledAction; schedulingBusy?: boolean; schedulingError?: string }

  // Live data is replaced wholesale on every poll, so it is raw state run through `share`:
  // unchanged entries keep their identity and nothing downstream re-renders or re-fetches.
  const prefs = new AppPrefs();
  let snapshot = $state.raw<Snapshot | null>(null);
  let error = $state<string | null>(null);
  let refreshing = $state(false);
  /** Only a refresh the user asked for spins the button; background scans stay quiet. */
  let manualRefresh = $state(false);
  let filters = $state<Filters>({ query: "", all: false, proto: "any", dev: false, mine: false, exposed: false });
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
  let viewportWidth = $state(window.innerWidth);
  let drawerOpen = $state(false);
  let graphDetailsOpen = $state(false);
  let isMac = $state(/Mac/.test(navigator.platform));
  let search = $state<HTMLInputElement | undefined>();
  let listEl = $state<HTMLDivElement | undefined>();
  let now = $state(Date.now());
  let toastSeq = 0;
  /** GraphView (and @xyflow) loads only when the Graph tab is opened — keeps the first paint light. */
  let GraphView = $state<typeof import("./components/GraphView.svelte").default | null>(null);
  let graph = $state.raw<Graph | null>(null);
  let topologyLoading = $state(false);
  /** AgentsView loads, and agents are looked up, only while the Agents tab is open. */
  let AgentsView = $state<typeof import("./components/AgentsView.svelte").default | null>(null);
  let agentsReport = $state.raw<AgentsReport | null>(null);
  let selectedAgent = $state<string | null>(null);
  let agentQuery = $state("");
  let agentsRefreshing = $state(false);
  let agentsError = $state<string | null>(null);
  let agentsStarted = $state(0);
  let agentFlowMode = $state<AgentFlowMode>("overview");
  /** Opt-in: read agents' MCP, skills and permission files on every agents scan. */
  function enableInventory() { enableInventoryChoice(true); }
  function enableInventoryChoice(on: boolean) { prefs.inventoryOn = on; if (on) void loadAgents(); }
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
  let graphQuery = $state("");
  let graphVisibleIds = $state<string[]>([]);
  const topologyWanted = $derived(prefs.view === "graph" || (prefs.view === "list" && prefs.sort === "cluster"));
  const topologyAll = $derived(prefs.view === "graph" ? graphAll : filters.all);
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
  const capture: ScreenCaptureController = new ScreenCaptureController({ blocked: (): boolean => snapshotCaptureState.open || snapshotCaptureState.busy, modalOpen: (): boolean => modalOpen });
  let snapshotCaptureUi = $state<SnapshotCaptureHandle | undefined>();
  let snapshotCaptureState = $state({ open: false, busy: false, hasResult: false });
  let hotkeyList = $state<HotkeyPreset[]>([]);
  let info = $state<{ version: string; platform: string; tray: boolean; configDir: string | null; updater: boolean | null; appPath: string | null; architecture: string | null }>({ version: "", platform: "", tray: false, configDir: null, updater: null, appPath: null, architecture: null });
  let detailTab = $state<DetailTab>("overview");
  $effect(() => {
    void prefs.view;
    // A view change must not reopen the drawer for a listener selected in another view.
    drawerOpen = false;
    graphDetailsOpen = false;
  });
  $effect(() => {
    const visible = topologyWanted, all = topologyAll, graphVisible = prefs.view === "graph";
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
    if (prefs.view !== "agents") return;
    // Opening MCP & skills explicitly includes bounded local configuration metadata.
    prefs.agentSection;
    untrack(loadAgents); // same reason as above: loadAgents reads agentsReport
    if (!AgentsView) {
      import("./components/AgentsView.svelte").then((m) => { AgentsView = m.default; }).catch((e) => console.error("Couldn't load the agents map", e));
    }
  });
  const pins = $derived(new Set((config?.pins ?? []).map((p) => p.port)));
  const linkCount = $derived(localLinkCounts(graph));
  const mod = $derived(isMac ? "⌘" : "Ctrl");
  const modK = $derived(isMac ? "⌘K" : "Ctrl K");

  const resolvedTheme = $derived(prefs.theme === "system" ? (systemDark ? "dark" : "light") : prefs.theme);
  $effect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
  });

  const visible = $derived(visiblePorts(snapshot?.entries ?? [], filters, prefs.sort));
  const sections = $derived(portSections(visible, pins, prefs.sort, graph));
  /** Rows the keyboard can reach: everything except collapsed sections (untitled sections never collapse). */
  const ordered = $derived(reachablePorts(sections, prefs.collapsed));
  const sectionOf = (id: string) => sections.find((s) => s.items.some((e) => e.id === id));
  function setCollapsed(id: string, on: boolean) { if (prefs.collapsed.has(id) !== on) prefs.collapsed = toggled(prefs.collapsed, id); }
  // A graph selection must not depend on hidden List filters or collapsed groups.
  const selected = $derived((prefs.view === "graph" ? snapshot?.entries : ordered)?.find((e) => e.id === selectedId) ?? null);
  const stats = $derived(portStats(snapshot?.entries ?? []));
  const queryPort = $derived(prefs.view === "list" ? parseQueryPort(filters.query) : null);
  const filtersActive = $derived(isFiltering(filters));
  // Keep at least 600px for the graph even when a wide details pane was saved in List.
  const graphDetailsDrawer = $derived(narrow || viewportWidth - prefs.paneWidth < 600);
  // Agents and small graph canvases use the existing modal details drawer.
  const showDrawer = $derived(!!selected && (prefs.view === "graph" ? graphDetailsDrawer && graphDetailsOpen : (narrow || prefs.view === "agents") && drawerOpen));
  /** Any overlay that owns the keyboard (they stop key events themselves; this is a backstop). */
  const otherModalOpen: boolean = $derived(!!confirm || !!pendingReview || showHelp || showHistory || showPalette || showSettings || showShutdown || showSchedules || showRemote || !!pinDialog || showDrawer || !!capture.dialog);
  const modalOpen: boolean = $derived(otherModalOpen || snapshotCaptureState.open || snapshotCaptureState.busy);
  const scanMs = $derived(Math.min(60, Math.max(1, config?.scan_interval_secs ?? 4)) * 1000);

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
      if (selectedId && !s.entries.some((e) => e.id === selectedId)) {
        selectedId = null;
        if (prefs.view === "graph") graphDetailsOpen = false;
      }
      // Build topology for the graph or the list's cluster grouping, only while needed.
      if (topologyWanted) loadTopology();
      else graph = null;
      if (prefs.view === "agents" || prefs.view === "home") loadAgents();
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
    if (disposed) return;
    agentsReport = share(agentsReport, report);
    void insightsStore.afterAgents();
    if (selectedAgent && !agentsReport.agents.some((a) => a.id === selectedAgent)) selectedAgent = null;
  }, () => !disposed && (prefs.inventoryOn || prefs.view === "agents" && prefs.agentSection === "capabilities"));
  async function loadAgents() {
    if (agentsRefreshing) return;
    agentsRefreshing = true;
    agentsStarted = Date.now();
    try {
      await pullAgents();
      agentsError = null;
    } catch (e) {
      agentsError = String(e);
      if (prefs.view === "agents" && prefs.agentSection !== "capabilities") toast("error", "Couldn't look up the agents", String(e));
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
    const visible = prefs.view === "agents" && mode !== "overview" && !flowPaused;
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
    if (prefs.view === "agents" && agentFlowMode === "calls") await activityLoader.request();
  }
  const startRecording = () => mutateRecording(api.startActivity);
  const stopRecording = () => mutateRecording(api.stopActivity);
  const clearRecordedActivity = () => mutateRecording(api.clearActivity);

  /** Open a port from the agents map in the details drawer, making sure the list can show it. */
  function openEntry(e: PortEntry) {
    if (!ordered.some((x) => x.id === e.id)) {
      clearFilters();
      const sec = sections.find((x) => x.items.some((i) => i.id === e.id));
      if (sec?.title && prefs.collapsed.has(sec.id)) setCollapsed(sec.id, false);
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
      if (selectedNode && !graph.nodes.some((n) => n.id === selectedNode)) {
        selectedNode = null;
        if (prefs.view === "graph") { selectedId = null; graphDetailsOpen = false; }
      }
    },
    (message) => { if (message && topologyWanted) toast("error", "Couldn't build the service graph", message); },
    (loading) => (topologyLoading = loading));
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
    if (prefs.view === "graph") {
      selectedId = id ?? null;
      graphDetailsOpen = !!id;
      return;
    }
    if (!id) { selectedId = null; return; }
    if (!ordered.some((e) => e.id === id)) clearFilters();
    selectedId = id;
    if (prefs.view === "list") requestAnimationFrame(() => document.getElementById("row-" + id)?.scrollIntoView({ block: "nearest" }));
  }

  function closeGraphDetails() {
    const id = selectedNode;
    graphDetailsOpen = false;
    selectedId = null;
    selectedNode = null;
    focusGraphService(id);
  }

  function focusGraphService(id: string | null) {
    const expectedSelection = selectedNode;
    void tick().then(() => {
      if (prefs.view !== "graph" || modalOpen || selectedNode !== expectedSelection) return;
      const region = document.getElementById("service-graph");
      const card = [...(region?.querySelectorAll<HTMLButtonElement>("button[data-service-id]") ?? [])].find((button) => button.dataset.serviceId === id);
      (card ?? region)?.focus({ preventScroll: true });
    });
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

  const settingsModel = $derived<SettingsModel | null>(config ? { theme: prefs.theme, density: prefs.density, singleKeyShortcuts: prefs.singleKeyShortcuts, config, autostart: autostartOn, shortcut, hotkeys: hotkeyList, version: info.version, platform: info.platform, configDir: info.configDir, updater: info.updater, appPath: info.appPath, architecture: info.architecture, powerAvailable: !api.isTauri || ["macos", "linux", "windows"].includes(info.platform), powerDemo: !api.isTauri, scheduledCount: activeSchedules.length } : null);
  const settingsActions: SettingsActions = {
    setTheme: (t) => { prefs.theme = t; },
    setDensity: (d) => { prefs.density = d; },
    setSingleKeyShortcuts: (on) => { prefs.singleKeyShortcuts = on; },
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
          if (prefs.view === "agents") loadAgents();
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
    graphDetailsOpen = false;
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
    const ids = new Set(graphVisibleIds);
    const byCluster = (n: GraphNode) => (n.cluster ? graph!.clusters.findIndex((c) => c.id === n.cluster) : 999);
    return graph.nodes.filter((n) => ids.has(n.id)).slice().sort((a, b) => byCluster(a) - byCluster(b) || a.label.localeCompare(b.label));
  });
  function selectGraph(delta: number) {
    if (!graphOrder.length) return;
    const i = graphOrder.findIndex((n) => n.id === selectedNode);
    const next = i < 0 ? (delta > 0 ? 0 : graphOrder.length - 1) : Math.max(0, Math.min(graphOrder.length - 1, i + delta));
    selectNode(graphOrder[next]);
    focusGraphService(graphOrder[next].id);
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
  /** What the details pane and the details drawer share; each adds its own close and list wiring. */
  const detailProps = $derived({
    entry: selected, explanation, http: selectedHttp, loading: explaining, busy: selected ? !!busy[selected.id] : false, mod, graph,
    pinned: !!selected && pins.has(selected.port), details: selectedDetails, cpuTrend, memTrend, platform: info.platform,
    onstop: () => selected && requestStop(selected, false), onschedule: () => selected && requestScheduledStop(selected), onkill: () => selected && requestStop(selected, true),
    onopen: () => selected && open(selected), oncopy: copy, onpin: () => selected && togglePin(selected), onstopcluster: requestClusterStop, onselectnode: (n: GraphNode) => selectNode(n),
    onrestart: () => selected && requestRestart(selected), onreveal: () => selected && reveal(selected), oneditor: () => selected && openEditor(selected),
  });

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
    // List keeps its row flow; Graph must not inspect a hidden List neighbour after a stop.
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
        if (c.entry && selectedId === c.entry.id) {
          selectedId = prefs.view === "graph" ? null : neighbour;
          if (prefs.view === "graph") { selectedNode = null; graphDetailsOpen = false; }
        }
        if (!c.entry) { selectedId = null; selectedNode = null; graphDetailsOpen = false; }
        if (prefs.view === "agents") loadAgents();
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

  function clearFilters() { filters = clearedFilters(filters); }
  function cycleTheme() { prefs.theme = prefs.theme === "system" ? "light" : prefs.theme === "light" ? "dark" : "system"; }
  function finishOnboarding() {
    onboarded = true;
    prefSet("onboarded", "1");
  }
  /** Applies only what the user chose in the welcome dialog; each integration stays opt-in. */
  async function applyFirstRun(choices: FirstRunChoices) {
    finishOnboarding();
    const current = { notify: config?.notify ?? false, autostart: autostartOn, singleKeys: prefs.singleKeyShortcuts, inventory: prefs.inventoryOn };
    for (const change of firstRunPlan(choices, current, api.isTauri)) {
      try {
        if (change.kind === "view") prefs.view = change.view;
        else if (change.kind === "singleKeys") prefs.singleKeyShortcuts = change.on;
        else if (change.kind === "inventory") enableInventoryChoice(change.on);
        else if (change.kind === "notify") config = await api.setNotify(change.on, config?.notify_dev_only ?? false);
        else autostartOn = await api.autostart(change.on);
      } catch (e) { toast("error", "Couldn't apply a setting", String(e)); }
    }
  }
  // Home has no list of its own: typing in its search box continues on the port list.
  $effect(() => { if (prefs.view === "home" && filters.query) prefs.view = "list"; });
  /** Home → one agent, in a single click: the Agents view with that agent selected. */
  function openAgent(id: string, section: "overview" | "capabilities" = "overview") {
    const a = agentsReport?.agents.find((x) => x.id === id);
    if (a && !agentMatches(a, agentQuery)) agentQuery = "";
    prefs.agentSection = section;
    prefs.view = "agents";
    selectedAgent = id;
  }
  function openEntryById(id: string) {
    const e = snapshot?.entries.find((x) => x.id === id);
    prefs.view = "list";
    if (e) openEntry(e);
  }

  /** B: the graph's service details, or the list's details pane. False when there is nothing to show. */
  function toggleSide(): boolean {
    if (prefs.view !== "graph") { prefs.sideOpen = !prefs.sideOpen; return true; }
    if (graphDetailsOpen) closeGraphDetails();
    else if (selectedGraphNode) selectNode(selectedGraphNode);
    else return false;
    return true;
  }
  const toggleView = (target: "graph" | "agents") => { prefs.view = prefs.view === target ? "list" : target; };

  const commands = $derived(appCommands({
    view: prefs.view, graphDetailsOpen, selected, queryPort, clusters: graph?.clusters ?? [], entries: snapshot?.entries ?? [], update,
    pinned: (port) => pins.has(port), powerAvailable: !!settingsModel?.powerAvailable, native: api.isTauri, activeSchedules: activeSchedules.length, mod,
    filters, sideOpen: prefs.sideOpen, selectedGraphNode, version: info.version, platform: info.platform,
    recording: { active: !!capture.session, elapsedMs: capture.elapsed, hasResult: !!capture.result, checking: capture.support.pending, supported: !!capture.support.backend },
    snapshotHasResult: snapshotCaptureState.hasResult, notify: !!config?.notify, density: prefs.density, autostart: autostartOn, graphQuery, singleKeys: prefs.singleKeyShortcuts,
  }, {
    stopCluster: (name) => void requestClusterStop(name), stopAllDev: () => void requestStopAllDev(), installUpdate: () => void installUpdate(), togglePin: (e) => void togglePin(e),
    goToPort: (e) => { prefs.view = "list"; clearFilters(); selectEntry(e); }, stop: (e) => void requestStop(e, false), openInBrowser: (e) => void open(e),
    scheduleShutdown: () => void openShutdown("schedule"), checkPort: (port) => { filters.query = String(port); }, refresh: () => void refresh(true), schedules: openSchedules,
    findFree: (near) => void findFree(near), setFilters: (patch) => Object.assign(filters, patch), toggleAllSockets: () => { filters.all = !filters.all; void refresh(); }, clearFilters,
    setTheme: (t) => (prefs.theme = t), setSort: (s) => (prefs.sort = s), help: () => (showHelp = true), toggleSide: () => void toggleSide(),
    toggleGraph: () => toggleView("graph"), toggleAgents: () => toggleView("agents"), settings: (section) => openSettings(section), downloads: () => void openDesktopHelp("downloads"),
    pinWithLabel: openPin, remote: () => (showRemote = true), history: () => void openHistory(), stopRecording: () => void capture.finish(), openRecording: capture.open,
    snapshot: () => snapshotCaptureUi?.show(), snapshotRecording: () => void snapshotCaptureUi?.captureRecording(), toggleNotify: () => void toggleNotify(),
    toggleDensity: () => { prefs.density = prefs.density === "compact" ? "comfortable" : "compact"; }, toggleAutostart: () => void toggleAutostart(), clearGraphQuery: () => (graphQuery = ""),
  }));

  function onKey(e: KeyboardEvent) {
    const active = document.activeElement;
    const s = prefs.view === "list" || prefs.view === "graph" && graphDetailsOpen ? selected : null;
    const n = prefs.view === "graph" ? selectedGraphNode : null;
    const decision = decideKey(e.key, e.shiftKey, shortcutContext(e, active), {
      view: prefs.view, singleKeys: prefs.singleKeyShortcuts, modalOpen, snapshotBusy: snapshotCaptureState.busy,
      flowView: prefs.view === "agents" && agentFlowMode !== "overview",
      focus: { inSearch: active === search || e.target === search, recordButton: !!active?.matches("[data-screen-record]"), snapshotButton: !!active?.matches("[data-screen-snapshot]"), graphService: !!active?.matches("button[data-service-id]") },
      graph: { detailsOrSelection: graphDetailsOpen || !!selectedNode, query: !!graphQuery },
      agents: { query: !!agentQuery },
      list: { selected: !!selectedId, filtersActive, hasRows: ordered.length > 0 },
      selection: { present: !!s, stoppable: !!s && !!(s.process || s.container), openable: !!s && canOpen(s) },
      graphCluster: !!n?.cluster && !!graph?.clusters.some((x) => x.id === n.cluster),
    });
    if (!decision) return;
    const done = decision.intent ? runKey(decision.intent, s) : true;
    if (decision.prevent && done) e.preventDefault();
  }

  /** Performs a keyboard intent; false when there turned out to be nothing to do. */
  function runKey(intent: KeyIntent, s: PortEntry | null): boolean {
    switch (intent.do) {
      case "cancel-snapshot": snapshotCaptureUi?.cancel(); break;
      case "palette": showPalette = true; break;
      case "settings": openSettings(); break;
      case "search": search?.focus(); search?.select(); break;
      case "refresh": void refresh(true); break;
      case "record": capture.open(); break;
      case "toggle-record": if (capture.session) void capture.finish(); else capture.open(); break;
      case "snapshot": snapshotCaptureUi?.show(); break;
      case "graph-close-details": closeGraphDetails(); break;
      case "graph-clear-query": graphQuery = ""; break;
      case "graph-step": selectGraph(intent.delta); break;
      case "graph-open-selected": {
        const node = graphOrder.find((x) => x.id === selectedNode) ?? graphOrder[0];
        if (node) { selectNode(node); focusGraphService(node.id); }
        if (intent.blur) search?.blur();
        break;
      }
      case "list-collapse": { const sec = selectedId ? sectionOf(selectedId) : null; if (sec?.title) setCollapsed(sec.id, intent.collapse); break; }
      case "list-edge": selectEntry(ordered[intent.end ? ordered.length - 1 : 0], false); break;
      case "move": (prefs.view === "graph" ? selectGraph : prefs.view === "agents" ? selectAgent : select)(intent.delta); break;
      case "blur-search": search?.blur(); break;
      case "agents-clear-query": agentQuery = ""; break;
      case "agents-deselect": selectedAgent = null; break;
      case "clear-filters": clearFilters(); break;
      case "list-deselect": selectedId = null; break;
      case "agents-pick-from-search": if (!selectedAgent || !visibleAgentIds.includes(selectedAgent)) selectAgent(1); search?.blur(); break;
      case "list-pick-from-search": if (!selected) select(1); search?.blur(); break;
      case "open-drawer": if (selected) drawerOpen = true; break;
      case "stop": if (s) void requestStop(s, intent.force); break;
      case "open": if (s) void open(s); break;
      case "copy-url": if (s) void copy(url(s), "URL"); break;
      case "toggle-all-sockets": filters.all = !filters.all; void refresh(); break;
      case "cycle-proto": filters.proto = nextProto(filters.proto); break;
      case "toggle-dev": filters.dev = !filters.dev; break;
      case "toggle-mine": filters.mine = !filters.mine; break;
      case "toggle-exposed": filters.exposed = !filters.exposed; break;
      case "cycle-theme": cycleTheme(); break;
      case "toggle-side": return toggleSide();
      case "toggle-graph": toggleView("graph"); break;
      case "toggle-home-layout": prefs.homeLayout = nextLayout(prefs.homeLayout); break;
      case "toggle-agents": toggleView("agents"); break;
      case "history": void openHistory(); break;
      case "pin": if (s) void togglePin(s); break;
      case "pin-label": openPin(s); break;
      case "stop-cluster": { const n = selectedGraphNode; const c = n?.cluster ? graph?.clusters.find((x) => x.id === n.cluster) : null; if (c) void requestClusterStop(c.name); break; }
      case "help": showHelp = true; break;
    }
    return true;
  }

  onMount(() => {
    capture.refreshSupport();
    if (typeof window !== "undefined" && window.__holdmapRecovered) {
      const why = window.__holdmapRecovered;
      delete window.__holdmapRecovered;
      toast("info", "holdmap recovered from a UI glitch", why);
    }
    refresh();
    loadSchedules();
    void insightsStore.refresh();
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
      capture.dispose();
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
  const showSide = $derived(prefs.view === "graph" ? graphDetailsOpen && !!selected && !graphDetailsDrawer : !narrow && prefs.view === "list" && prefs.sideOpen);

  const ago = $derived(dataAge(now, prefs.view === "agents" ? agentsReport?.taken_at_ms ?? null : snapshot?.taken_at_ms ?? null, prefs.view === "agents" ? agentsRefreshing ? agentsStarted : null : refreshing ? scanStarted : null));
  /** What the title-bar search filters, only while it is rendered. */
  const searchTarget = $derived(searchControls(prefs.view, prefs.agentSection));
  const fresh = $derived(freshness(ago, scanMs / 1000));
  const themeIcon = $derived(THEME_ICON[prefs.theme]);
</script>

<svelte:window onkeydown={onKey} bind:innerWidth={viewportWidth} />

<div id="holdmap-capture-root" class="capture-viewport">
<div class="app" class:mac={isMac}>
  <TitleBar
    liveText={prefs.view !== "agents" && prefs.view !== "home" && error ? refreshing ? "Retrying…" : "Scan failed" : fresh.text}
    liveStale={fresh.stale || (prefs.view !== "agents" && prefs.view !== "home" && !!error)}
    livePulse={prefs.view === "agents" ? agentsRefreshing : refreshing}
    {modK} {mod} singleKeys={prefs.singleKeyShortcuts} {refreshing} {manualRefresh}
    recordingMs={capture.session ? capture.elapsed : null}
    scheduleCount={activeSchedules.length} {themeIcon} themeLabel={THEME_LABEL[prefs.theme]}
    bind:schedulesTrigger
    onpalette={() => (showPalette = true)} onrefresh={() => refresh(true)} onstoprecording={() => void capture.finish()}
    onhistory={openHistory} onschedules={openSchedules} onremote={() => (showRemote = true)} ontheme={cycleTheme} onsettings={() => openSettings()}>
    {#snippet searchbox()}
      <SearchBox view={prefs.view} capabilities={prefs.agentSection === "capabilities"} controls={searchTarget} singleKeys={prefs.singleKeyShortcuts} bind:input={search} bind:agentQuery bind:graphQuery bind:portQuery={filters.query} />
    {/snippet}
  </TitleBar>

  <ViewToolbar bind:view={prefs.view} bind:agentSection={prefs.agentSection} bind:picture={prefs.picture} bind:flowSource={prefs.flowSource} bind:filters bind:sort={prefs.sort} bind:sideOpen={prefs.sideOpen} bind:graphQuery bind:agentQuery
    graphEdges={graph?.stats.edges ?? 0} agentCount={agentsReport ? agentsReport.agents.length : null} {stats} {filtersActive} {narrow} singleKeys={prefs.singleKeyShortcuts}
    {projectReady} {projectCaption} onsocketschange={() => refresh()} onclearfilters={clearFilters} />

  <Notices {error} retrying={refreshing} showError={!!error && (!!snapshot || prefs.view !== "list")} hasSnapshot={!!snapshot}
    next={nextSchedule} activeCount={activeSchedules.length} {now} demo={!api.isTauri} onretry={() => refresh(true)} onschedules={openSchedules} />

  <main class="content" class:narrow class:wide={prefs.view === "agents" || prefs.view === "home" || !showSide} style="--pane-w: {prefs.paneWidth}px">
    {#if prefs.view === "graph"}
      <div class="graph-wrap">
        {#if GraphView}
          <GraphView {graph} {selectedNode} loading={topologyLoading} query={graphQuery} onclearquery={() => (graphQuery = "")} onvisiblechange={(ids) => (graphVisibleIds = ids)} dark={resolvedTheme === "dark"} {reduced} all={graphAll} ontoggleall={() => { graphAll = !graphAll; }} onselect={selectNode} onstopcluster={requestClusterStop} />
        {:else}
          <div class="graph-loading" aria-busy="true">Loading graph…</div>
        {/if}
      </div>
    {:else if prefs.view === "home"}
      <HomeView entries={snapshot?.entries ?? null} report={agentsReport} inventory={prefs.inventoryOn} layout={prefs.homeLayout} onlayout={(l) => (prefs.homeLayout = l)} timeline={prefs.homeTimeline} ontimeline={(open) => (prefs.homeTimeline = open)} now={snapshot?.taken_at_ms ?? Date.now()} onopenagent={openAgent}
        onstopentry={(id) => { const e = snapshot?.entries.find((x) => x.id === id); if (e) void requestStop(e, false); }} onstopagent={(id) => void requestStopAgentPorts(id)} onopenentry={openEntryById} onview={(v) => (prefs.view = v)} oninventory={enableInventory} />
    {:else if prefs.view === "agents"}
      <div class="graph-wrap agents-wrap">
        {#if AgentsView}
          <AgentsView bind:section={prefs.agentSection} bind:flowSource={prefs.flowSource} bind:picture={prefs.picture} bind:projectReady bind:projectCaption pictureChrome={false} report={agentsReport} query={agentQuery} entries={snapshot?.entries ?? []} {selectedAgent} selectedEntry={drawerOpen ? selectedId : null} dark={resolvedTheme === "dark"} {reduced} platform={info.platform}
            capabilitiesLoading={agentsRefreshing} capabilitiesError={agentsError} onrefreshcapabilities={() => { void loadAgents(); }}
            onselectagent={(id) => { const a = agentsReport?.agents.find((a) => a.id === id); if (a && !agentMatches(a, agentQuery)) agentQuery = ""; selectedAgent = id; }} onentry={openEntry}
            onreveal={revealAgentFolder} oneditor={openAgentFolder}
            onstop={(e, agentId) => requestStop(e, false, false, { agentId })} onstopall={requestStopAgentPorts}
            {flowReport} {activity} {recording} {flowLoading} {activityLoading} {flowError} {activityError} demo={api.usesSampleData()}
            paused={flowPaused} onpausechange={(paused) => { flowPaused = paused; }} onflowmodechange={(mode) => { agentFlowMode = mode; }}
            onrefreshflow={() => { void flowLoader.request(); }} onrefreshactivity={() => { void activityLoader.request(); }}
            onstartrecording={startRecording} onstoprecording={stopRecording} onclearactivity={clearRecordedActivity}
            onactivitysetup={api.activitySetup} onclearquery={() => { agentQuery = ""; }} oninventory={enableInventory} />
        {:else}
          <div class="graph-loading" aria-busy="true">Loading agents…</div>
        {/if}
      </div>
    {:else}
    <PortList bind:listEl {snapshot} {error} {refreshing} {sections} {ordered} collapsed={prefs.collapsed} sort={prefs.sort} density={prefs.density} {pins} links={linkCount} {usage} {selectedId} {busy}
      query={filters.query} {queryPort} {filtersActive} {stats} {checkingPort} {freePort} {portCheckError}
      onretry={() => refresh(true)} oncheckagain={() => portCheckRetry++} onclearfilters={clearFilters} onfindfree={findFree} oncopy={copy} onpinfree={() => openPin(null)}
      ontogglesection={setCollapsed} onselect={(e) => selectEntry(e)} onstop={(e) => requestStop(e, false)} onopen={open} onpin={togglePin} />
    {/if}

    {#if showSide}
      <div class="pane-wrap">
        <Splitter bind:width={prefs.paneWidth} min={PANE_MIN} max={prefs.view === "graph" ? Math.min(PANE_MAX, viewportWidth - 600) : PANE_MAX} initial={PANE_DEFAULT} />
        <DetailPane {...detailProps} bind:tab={detailTab} entries={snapshot?.entries ?? []} onselectentry={(e) => selectEntry(e, false)}
          onhide={() => { if (prefs.view === "graph") closeGraphDetails(); else prefs.sideOpen = false; }} />
      </div>
    {:else if !narrow && prefs.view === "list"}
      <IconButton class="reveal-side" icon="panel" label="Show details pane" kbd={prefs.singleKeyShortcuts ? "B" : undefined} onclick={() => (prefs.sideOpen = true)} />
    {/if}
  </main>

  <StatusBar stats={snapshot ? stats : null} hints={prefs.view === "agents" ? agentFlowMode !== "overview" ? "flow" : "agents" : prefs.view === "home" ? "home" : "list"} {mod} singleKeys={prefs.singleKeyShortcuts} {shortcut}
    recording={!!capture.session} recordingResult={!!capture.result} snapshot={snapshotCaptureState} onrecord={capture.open}
    onsnapshot={() => snapshotCaptureState.busy ? snapshotCaptureUi?.cancel() : snapshotCaptureUi?.show()} />
</div>

{#if showDrawer && selected}
  <Dialog placement="right" bare label="Details for port {selected.port}" onclose={() => { if (prefs.view === "graph") closeGraphDetails(); else drawerOpen = false; }} initialFocus="self">
    <DetailPane drawer {...detailProps} bind:tab={detailTab} onclose={() => { if (prefs.view === "graph") closeGraphDetails(); else drawerOpen = false; }} />
  </Dialog>
{/if}
{#if pendingReview}
  <ReviewPendingDialog subject={pendingReview} oncancel={cancelReview} />
{:else if confirm}
  <ConfirmDialog intent={confirm.timingIntent ?? "combined"} entry={confirm.entry} cluster={confirm.cluster} plan={confirm.plan} portCount={(confirm.queue?.length ?? 0) + 1} completedPortCount={confirm.completedPortCount} attemptedPortCount={confirm.attemptedPortCount} agentName={confirm.agentName} restart={!!confirm.restart} phase={confirm.phase} log={confirm.log} report={confirm.report}
    onschedule={confirm.entry && !confirm.force && !confirm.allowProtected && !confirm.restart ? scheduleStop : undefined} schedulingBusy={!!confirm.schedulingBusy} schedulingError={confirm.schedulingError ?? null} scheduled={confirm.scheduled ?? null} onviewschedules={openSchedules} onreview={reviewStop} demo={!api.isTauri}
    oncopyrestart={confirm.entry && !confirm.queue?.length && restartCommand(confirm) ? () => { const command = confirm && restartCommand(confirm); if (command) void copy(command, "Restart command"); } : undefined}
    onconfirm={runStop} oncancel={closeConfirm} onoverride={() => confirm?.entry && requestStop(confirm.entry, confirm.force, true, { agentId: confirm.agentId, intent: "now" })} />
{/if}
{#if snapshot && !onboarded && !modalOpen}<FirstRun current={{ notify: config?.notify ?? false, autostart: autostartOn, singleKeys: prefs.singleKeyShortcuts, inventory: prefs.inventoryOn }} canAutostart={api.isTauri} tray={api.isTauri && info.tray} platform={info.platform} {shortcut} {mod} onapply={(c) => void applyFirstRun(c)} onskip={finishOnboarding} />{/if}
{#if showPalette}<CommandPalette {commands} onclose={() => { showPalette = false; }} />{/if}
{#if showHistory}<HistoryPanel items={historyItems} platform={info.platform} onrestart={restartEntry} oncopy={copy} onclose={() => (showHistory = false)} onclear={async () => { await api.clearHistory(); historyItems = []; }} />{/if}
{#if showHelp}<ShortcutsDialog {mod} singleKeyShortcuts={prefs.singleKeyShortcuts} onclose={() => (showHelp = false)} />{/if}
{#if showSettings && settingsModel}<SettingsDialog model={settingsModel} actions={settingsActions} bind:section={settingsSection} onclose={() => (showSettings = false)} />{/if}
{#if showSettings && !settingsModel}<SettingsUnavailableDialog error={configError} loading={configLoading} onretry={() => loadConfig()} onclose={() => (showSettings = false)} />{/if}
{#if showShutdown}<ShutdownDialog intent={shutdownIntent} preview={shutdownPreview} loading={shutdownLoading} busy={shutdownBusy} requested={shutdownRequested} error={shutdownError} demo={!api.isTauri} onconfirm={confirmShutdown} onclose={closeShutdown} onretry={() => openShutdown(shutdownIntent)} onschedule={scheduleShutdown} scheduled={shutdownScheduled} onviewschedules={openSchedules} />{/if}
{#if showSchedules && !confirm && !showShutdown}<SchedulesDialog {schedules} loading={schedulesLoading} error={schedulesError} demo={!api.isTauri} powerAvailable={!api.isTauri || ["macos", "linux", "windows"].includes(info.platform)} entries={snapshot?.entries ?? []} onclose={closeSchedules} onrefresh={() => loadSchedules(true)} oncancel={cancelSchedule} onschedulepower={() => openShutdown("schedule", "schedules")} onscheduleport={(entry) => { closeSchedules(); requestScheduledStop(entry); }} />{/if}
{#if pinDialog}<PinDialog port={pinDialog.port} label={pinDialog.label} pinned={pinDialog.pinned} inUse={portHolder} onsave={savePin} onunpin={unpinPort} onclose={() => (pinDialog = null)} />{/if}
{#if showRemote}<RemoteDialog recent={config?.recent_hosts ?? []} onscan={async (h) => { const r = await api.remoteScan(h); config = await api.getConfig(); return r; }} oncopy={copy} onclose={() => (showRemote = false)} />{/if}
{#if capture.dialog}
  <ScreenRecordDialog phase={capture.dialog} error={capture.error} errorTitle={capture.errorTitle} result={capture.result} busy={capture.busy} starting={capture.starting} supported={!!capture.support.backend} checkingSupport={capture.support.pending} unsupportedReason={capture.support.reason} onstart={() => void capture.begin()} onclose={capture.close} ondiscard={capture.discardAndClose} />
{/if}
<SnapshotCapture bind:this={snapshotCaptureUi} session={capture.session} blocked={otherModalOpen} onstatechange={(state) => (snapshotCaptureState = state)} />
{#if snapshotCaptureState.busy}<span class="sr-only" role="status">Capturing snapshot. Press Escape or use Cancel snapshot capture to cancel.</span>{/if}
<Toasts {toasts} ondismiss={(id) => (toasts = toasts.filter((t) => t.id !== id))} />
</div>

<style>
  .capture-viewport { position: relative; width: 100%; height: 100vh; overflow: hidden; }
  .app { display: grid; grid-template-columns: minmax(0, 1fr); grid-template-rows: auto auto auto minmax(0, 1fr) auto; height: 100vh; }

  .content { position: relative; display: grid; grid-template-columns: minmax(0, 1fr) var(--pane-w, 440px); min-height: 0; }
  .content.narrow, .content.wide { grid-template-columns: 1fr; }
  .pane-wrap { position: relative; min-height: 0; min-width: 0; }
  .content :global(.reveal-side) { position: absolute; top: 12px; right: 12px; z-index: 6; background: var(--surface); border: 1px solid var(--border); box-shadow: var(--shadow-sm); }

  .graph-loading { display: grid; place-items: center; height: 100%; color: var(--muted); font-size: var(--fs-body-sm); }
  .graph-wrap { position: relative; min-height: 0; min-width: 0; }

</style>

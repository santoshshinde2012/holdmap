<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { dataAge, freshness, pruned, share, shouldPoll, withKey } from "./lib/live";
  import Icon from "./components/Icon.svelte";
  import PortRow from "./components/list/PortRow.svelte";
  import GroupHeader from "./components/list/GroupHeader.svelte";
  import { UsageHistory, parseCollapsed, parseDensity, toggled, usageRecord, type Density } from "./lib/rows";
  import DetailPane from "./components/detail/DetailPane.svelte";
  import SettingsDialog from "./components/SettingsDialog.svelte";
  import PinDialog from "./components/PinDialog.svelte";
  import RemoteDialog from "./components/RemoteDialog.svelte";
  import { Button, Dialog, FilterChip, IconButton, Kbd, SegmentedControl, Select, Splitter, TextField } from "./components/ui";
  import type { DetailTab } from "./lib/detail";
  import { THEME_ICON, THEME_LABEL, type SettingsActions, type SettingsModel, type SettingsSection, type HotkeyPreset, type Theme } from "./lib/settings";
  import ConfirmDialog, { type Phase } from "./components/ConfirmDialog.svelte";
  import ShortcutsDialog from "./components/ShortcutsDialog.svelte";
  import CommandPalette from "./components/CommandPalette.svelte";
  import Onboarding from "./components/Onboarding.svelte";
  import EmptyState from "./components/EmptyState.svelte";
  import Toasts, { type Toast } from "./components/Toasts.svelte";
  import GraphView from "./components/GraphView.svelte";
  import HistoryPanel from "./components/HistoryPanel.svelte";
  import * as api from "./lib/api";
  import type { ActionPlan, Cluster, Config, Explanation, Graph, GraphNode, HistoryEntry, HttpInfo, PortEntry, PortEvent, Snapshot, StopReport } from "./lib/types";
  import { linksLabel, nodeForEntry, sectionsByCluster } from "./lib/graph";
  import type { Command } from "./lib/palette";
  import { GROUPS, groupOf, matches, seconds, stopTarget, title, url, canOpen, type Filters, type Group } from "./lib/format";

  type Sort = "group" | "cluster" | "port" | "newest" | "memory";
  type View = "list" | "graph";
  interface Confirm { entry: PortEntry | null; cluster: Cluster | null; plan: ActionPlan; force: boolean; allowProtected: boolean; phase: Phase; log: string[]; report: StopReport | null }

  const store = (k: string) => { try { return localStorage.getItem(k); } catch { return null; } };
  // Live data is replaced wholesale on every poll, so it is raw state run through `share`:
  // unchanged entries keep their identity and nothing downstream re-renders or re-fetches.
  let snapshot = $state.raw<Snapshot | null>(null);
  let error = $state<string | null>(null);
  let refreshing = $state(false);
  /** Only a refresh the user asked for spins the button; background scans stay quiet. */
  let manualRefresh = $state(false);
  let filters = $state<Filters>({ query: "", all: false, proto: "any", dev: false, mine: false, exposed: false });
  let sort = $state<Sort>((store("pw.sort") as Sort) ?? "group");
  let selectedId = $state<string | null>(null);
  // Details are stale-while-revalidate: the last value stays on screen while each scan
  // refreshes it in the background, and the skeleton only shows on a port's first load.
  let explanations = $state.raw<Record<number, Explanation>>({});
  let explainingPort = $state<number | null>(null);
  let httpInfo = $state.raw<Record<number, HttpInfo | null>>({});
  /** Bumped by every scan; the selected port's details are revalidated once per scan. */
  let scanGen = $state(0);
  /** Per port: the holder's PID the explanation is about, the scan it's from, the newest request. */
  const explainMeta = new Map<number, { pid: number | null; gen: number; seq: number }>();
  const httpAt = new Map<number, number>();
  const HTTP_TTL_MS = 15_000;
  let explainSeq = 0;
  /** Ports whose explanation failed: retried quietly each scan, without a skeleton or toast. */
  const explainFailed = new Set<number>();
  let update = $state<string | null>(null);
  const installUpdate = () => api.installUpdate().catch((e) => toast("error", "Couldn't install the update", String(e)));
  let freePort = $state<Explanation | null>(null);
  let confirm = $state<Confirm | null>(null);
  let busy = $state<Record<string, boolean>>({});
  let toasts = $state<Toast[]>([]);
  let showHelp = $state(false);
  let showPalette = $state(false);
  let onboarded = $state(store("pw.onboarded") === "1");
  let theme = $state<Theme>((store("pw.theme") as Theme) ?? "system");
  let density = $state<Density>(parseDensity(store("pw.density")));
  let collapsed = $state<Set<string>>(parseCollapsed(store("pw.collapsed")));
  const usageHist = new UsageHistory(24);
  let usage = $state.raw<Record<string, number[]>>({});
  let systemDark = $state(matchMedia("(prefers-color-scheme: dark)").matches);
  let narrow = $state(matchMedia("(max-width: 900px)").matches);
  let drawerOpen = $state(false);
  let isMac = $state(/Mac/.test(navigator.platform));
  let search = $state<HTMLInputElement | undefined>();
  let listEl = $state<HTMLDivElement | undefined>();
  let now = $state(Date.now());
  let toastSeq = 0;
  let view = $state<View>((store("pw.view") as View) ?? "list");
  let graph = $state.raw<Graph | null>(null);
  let graphAll = $state(false);
  let selectedNode = $state<string | null>(null);
  let config = $state<Config | null>(null);
  let showHistory = $state(false);
  let historyItems = $state<HistoryEntry[]>([]);
  let reduced = $state(matchMedia("(prefers-reduced-motion: reduce)").matches);
  let autostartOn = $state(false);
  let shortcut = $state<string | null>(null);
  let showSettings = $state(false);
  let settingsSection = $state<SettingsSection>("general");
  let pinDialog = $state<{ port: number | null; label: string; pinned: boolean } | null>(null);
  let showRemote = $state(false);
  let hotkeyList = $state<HotkeyPreset[]>([]);
  let info = $state<{ version: string; platform: string; configDir: string | null }>({ version: "", platform: "", configDir: null });
  let detailTab = $state<DetailTab>("overview");
  const PANE_DEFAULT = 460;
  let paneWidth = $state(Math.min(680, Math.max(340, Number(store("pw.pane")) || PANE_DEFAULT)));
  $effect(() => { try { localStorage.setItem("pw.view", view); } catch { /* ignore */ } });
  const pins = $derived(new Set((config?.pins ?? []).map((p) => p.port)));
  const linkCount = $derived.by(() => {
    const m = new Map<string, number>();
    for (const n of graph?.nodes ?? []) {
      const c = new Set(graph!.edges.filter((e) => e.kind === "local" && (e.from === n.id || e.to === n.id)).map((e) => (e.from === n.id ? e.to : e.from))).size;
      for (const p of n.ports) m.set(p.entry_id, c);
    }
    return m;
  });
  const mod = $derived(isMac ? "⌘" : "Ctrl");
  const modK = $derived(isMac ? "⌘K" : "Ctrl K");

  const resolvedTheme = $derived(theme === "system" ? (systemDark ? "dark" : "light") : theme);
  $effect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
    try { localStorage.setItem("pw.theme", theme); } catch { /* private mode */ }
  });
  $effect(() => { try { localStorage.setItem("pw.sort", sort); } catch { /* ignore */ } });
  $effect(() => { try { localStorage.setItem("pw.density", density); } catch { /* ignore */ } });
  $effect(() => { try { localStorage.setItem("pw.collapsed", JSON.stringify([...collapsed])); } catch { /* ignore */ } });

  const visible = $derived.by(() => {
    const list = (snapshot?.entries ?? []).filter((e) => matches(e, filters));
    const cmp: Record<Sort, (a: PortEntry, b: PortEntry) => number> = {
      group: (a, b) => a.port - b.port,
      cluster: (a, b) => a.port - b.port,
      port: (a, b) => a.port - b.port,
      newest: (a, b) => (b.process?.start_time ?? 0) - (a.process?.start_time ?? 0),
      memory: (a, b) => (b.process?.memory_bytes ?? 0) - (a.process?.memory_bytes ?? 0),
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
  const showDrawer = $derived(narrow && drawerOpen && !!selected);
  /** Any overlay that owns the keyboard (they stop key events themselves; this is a backstop). */
  const modalOpen = $derived(!!confirm || showHelp || showHistory || showPalette || showSettings || showRemote || !!pinDialog || showDrawer);
  const scanMs = $derived(Math.min(60, Math.max(1, config?.scan_interval_secs ?? 3)) * 1000);

  /** When the scan in flight started: the header stays "Live" while one is merely running. */
  let scanStarted = $state(0);

  async function refresh(manual = false) {
    if (refreshing) return;
    refreshing = true;
    manualRefresh = manual;
    scanStarted = Date.now();
    try {
      apply(await api.scan(filters.all, manual ? 0 : undefined));
      if (manual && snapshot) toast("info", "Refreshed", `${snapshot.entries.length} port${snapshot.entries.length === 1 ? "" : "s"} · scanned in ${snapshot.scan_ms} ms`);
    } catch (e) {
      error = String(e);
    } finally {
      refreshing = false;
      manualRefresh = false;
    }
  }

  /** Take in a snapshot, from our own scan or pushed by the app's watcher; never go back in time. */
  function apply(next: Snapshot) {
    if (snapshot && next.taken_at_ms < snapshot.taken_at_ms) return;
    {
      const s = share(snapshot, next);
      snapshot = s;
      usageHist.push(s.entries);
      usage = share(usage, usageRecord(usageHist, s.entries.map((e) => e.id)));
      error = null;
      // Forget details for ports that are gone or now held by a different process.
      const holder = new Map(s.entries.map((e) => [e.port, e.pid]));
      const keep = (port: number) => holder.has(port) && (explainMeta.get(port)?.pid ?? null) === holder.get(port);
      explanations = pruned(explanations, keep);
      httpInfo = pruned(httpInfo, (port) => holder.has(port));
      for (const port of [...explainMeta.keys()]) if (!keep(port)) { explainMeta.delete(port); explainFailed.delete(port); }
      scanGen++;
      if (selectedId && !s.entries.some((e) => e.id === selectedId)) selectedId = null;
      loadTopology();
    }
  }

  async function loadTopology() {
    try {
      graph = share(graph, await api.topology(graphAll));
      if (selectedNode && !graph.nodes.some((n) => n.id === selectedNode)) selectedNode = null;
    } catch (e) {
      if (view === "graph") toast("error", "Couldn't build the service graph", String(e));
    }
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

  async function requestClusterStop(name: string) {
    const c = graph?.clusters.find((x) => x.name === name) ?? null;
    try {
      const plan = await api.plan(`cluster:${name}`, false, false);
      confirm = { entry: null, cluster: c ?? { id: name, name, kind: "workspace", detail: null, root: null, nodes: [] }, plan, force: false, allowProtected: false, phase: "confirm", log: [], report: null };
    } catch (e) {
      toast("error", `Couldn't plan stopping ${name}`, String(e));
    }
  }

  /** Every running dev server of yours at once (tray, palette); the dialog shows the full plan. */
  async function requestStopAllDev() {
    try {
      const plan = await api.plan("dev:all", false, false);
      confirm = { entry: null, cluster: null, plan, force: false, allowProtected: false, phase: "confirm", log: [], report: null };
    } catch (e) {
      toast("error", "Couldn't plan stopping the dev servers", String(e));
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

  const settingsModel = $derived<SettingsModel | null>(config ? { theme, density, config, autostart: autostartOn, shortcut, hotkeys: hotkeyList, version: info.version, platform: info.platform, configDir: info.configDir } : null);
  const settingsActions: SettingsActions = {
    setTheme: (t) => { theme = t; },
    setDensity: (d) => { density = d; },
    setAutostart: async (on) => { autostartOn = await api.autostart(on); },
    setHotkey: async (id) => { shortcut = await api.setHotkey(id); config = await api.getConfig(); },
    setNotify: async (on, devOnly) => { config = await api.setNotify(on, devOnly); },
    setScanInterval: async (secs) => { config = await api.setPreferences({ scanIntervalSecs: secs }); },
    setHistoryLimit: async (n) => { config = await api.setPreferences({ historyLimit: n }); },
    clearHistory: async () => { await api.clearHistory(); historyItems = []; },
    copy: (t, w) => copy(t, w),
  };
  function openSettings(section: SettingsSection = "general") { settingsSection = section; showSettings = true; }

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
    try { autostartOn = await api.autostart(!autostartOn); toast("info", autostartOn ? "portwise starts at login" : "Launch at login off"); }
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
  const selPid = $derived(selected?.pid ?? null);
  const selTcp = $derived(selected?.protocol === "tcp");
  $effect(() => {
    const port = selPort, pid = selPid, tcp = selTcp, gen = scanGen;
    if (port === null) return;
    const meta = explainMeta.get(port);
    const first = !untrack(() => explanations[port]) && !explainFailed.has(port);
    if (!first && meta?.gen === gen) return;
    if (first) explainingPort = port;
    // Debounced so arrowing through the list doesn't fire a request per row.
    const t = setTimeout(async () => {
      const seq = ++explainSeq;
      explainMeta.set(port, { pid, gen, seq });
      if (tcp && Date.now() - (httpAt.get(port) ?? -Infinity) > HTTP_TTL_MS) {
        httpAt.set(port, Date.now());
        api.http(port).then((h) => h, () => null).then((h) => { if (explainMeta.has(port)) httpInfo = withKey(httpInfo, port, share(httpInfo[port], h)); });
      }
      try {
        const x = await api.explain(port);
        // Only the newest request for this port may land, and only while it's still wanted.
        if (explainMeta.get(port)?.seq === seq) explanations = withKey(explanations, port, share(explanations[port], x));
        explainFailed.delete(port);
      } catch (err) {
        if (first) toast("error", "Couldn't explain this port", String(err));
        explainFailed.add(port);
      } finally {
        if (explainingPort === port) explainingPort = null;
      }
    }, first ? 60 : 0);
    return () => clearTimeout(t);
  });
  const explanation = $derived(selected ? explanations[selected.port] ?? null : null);
  const selectedHttp = $derived(selected ? httpInfo[selected.port] ?? null : null);
  const explaining = $derived(selPort !== null && explainingPort === selPort);

  $effect(() => {
    const p = queryPort;
    if (p === null || visible.some((e) => e.port === p)) { freePort = null; return; }
    if (freePort?.port === p) return;
    const t = setTimeout(async () => {
      try { freePort = await api.explain(p); } catch (err) { toast("error", `Couldn't check port ${p}`, String(err)); }
    }, 120);
    return () => clearTimeout(t);
  });

  function toast(kind: Toast["kind"], text: string, detail?: string, action?: Toast["action"]) {
    const id = ++toastSeq;
    toasts = [...toasts, { id, kind, text, detail, action }].slice(-4);
    setTimeout(() => (toasts = toasts.filter((t) => t.id !== id)), kind === "error" ? 9000 : action ? 8000 : 3200);
  }

  async function requestStop(entry: PortEntry, force: boolean, allowProtected = false) {
    try {
      const plan = await api.plan(stopTarget(entry), force, allowProtected);
      confirm = { entry, cluster: null, plan, force, allowProtected, phase: "confirm", log: [], report: null };
    } catch (e) {
      toast("error", `Couldn't plan stopping :${entry.port}`, String(e));
    }
  }

  function restartCommand(c: Confirm): string | null {
    const s = c.plan.steps.find((x) => x.action === "signal_processes");
    if (!s || s.action !== "signal_processes" || !s.processes.length) return null;
    const dir = c.entry?.project?.root ?? c.entry?.process?.cwd;
    return `${dir ? `cd ${JSON.stringify(dir)} && ` : ""}${s.processes[0].command}`;
  }

  async function runStop() {
    if (!confirm || confirm.phase !== "confirm" || confirm.plan.blocked) return;
    const c = confirm;
    c.phase = "running";
    const target = c.entry ? stopTarget(c.entry) : c.cluster ? `cluster:${c.cluster.name}` : "dev:all";
    const key = c.entry?.id ?? target;
    const what = c.entry ? title(c.entry) : c.cluster ? `cluster ${c.cluster.name}` : "All dev servers";
    busy[key] = true;
    // Keep keyboard flow: after a successful stop, the selection moves to the neighbouring row.
    const idx = c.entry ? ordered.findIndex((e) => e.id === c.entry!.id) : -1;
    const neighbour = idx >= 0 ? (ordered[idx + 1] ?? ordered[idx - 1])?.id ?? null : null;
    try {
      const r = await api.stop(target, c.force, c.allowProtected);
      c.report = r;
      if (!c.log.length) c.log = r.log;
      if (r.freed) {
        c.phase = "done";
        if (c.entry && selectedId === c.entry.id) selectedId = neighbour;
        if (!c.entry) { selectedId = null; selectedNode = null; }
        const restart = restartCommand(c);
        setTimeout(() => {
          if (confirm === c) confirm = null;
          toast("ok", c.entry ? `Port ${c.entry.port} is free` : c.cluster ? `Cluster ${c.cluster.name} stopped` : "Dev servers stopped", `${what} stopped in ${seconds(r.elapsed_ms)}${r.escalated ? " (needed SIGKILL)" : ""}`,
            restart && c.entry ? { label: "Copy restart command", run: () => copy(restart, "Restart command") } : { label: "Restart…", run: openHistory });
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
    if (confirm && confirm.phase !== "running") confirm = null;
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
    try { await api.openUrl(url(e)); } catch (err) { toast("error", "Couldn't open the browser", String(err)); }
  }

  async function copy(text: string, what: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast("ok", `${what} copied`, text.length > 90 ? text.slice(0, 90) + "…" : text);
    } catch {
      toast("error", "Clipboard unavailable", "Your system blocked clipboard access.");
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
    try { localStorage.setItem("pw.onboarded", "1"); } catch { /* ignore */ }
    listEl?.focus(); // the banner's button disappears; keep keyboard focus in the list, not on <body>
  }

  const commands = $derived.by((): Command[] => {
    const cmds: Command[] = [];
    const q = queryPort;
    for (const c of graph?.clusters ?? []) {
      cmds.push({ id: `stopc-${c.id}`, group: "Actions", icon: "stop", danger: true, title: `Stop cluster ${c.name}`, subtitle: `${c.nodes.length} services · dependency order · asks first`, keywords: `cluster ${c.kind} ${c.name}`, run: () => requestClusterStop(c.name) });
    }
    const dev = (snapshot?.entries ?? []).filter((e) => e.is_dev && e.is_mine && !e.protected && !e.container && e.process);
    if (dev.length > 1) cmds.push({ id: "stop-dev", group: "Actions", icon: "stop", danger: true, title: "Stop all dev servers", subtitle: `${dev.length} running · asks first`, keywords: "kill every dev server all", run: requestStopAllDev });
    if (update) cmds.push({ id: "update", group: "Actions", icon: "sparkles", title: `Install portwise ${update} and restart`, keywords: "update upgrade new version", run: installUpdate });
    if (selected) cmds.push({ id: "pin", group: "Actions", icon: "star", title: pins.has(selected.port) ? `Unpin :${selected.port}` : `Pin :${selected.port}`, shortcut: ["P"], run: () => selected && togglePin(selected) });
    for (const e of (snapshot?.entries ?? []).slice(0, 60)) {
      const boost = (selected?.id === e.id ? 12 : 0) + (e.is_dev ? 3 : 0);
      const sub = `${e.framework?.name ?? e.process?.name ?? ""}${e.project ? ` · ${e.project.name}` : ""}`;
      cmds.push({ id: `go-${e.id}`, group: "Ports", icon: "hash", title: `:${e.port}  ${title(e)}`, subtitle: sub, boost, keywords: `${e.port} ${e.label} ${e.process?.name ?? ""}`, run: () => { clearFilters(); selectEntry(e); } });
      if ((e.process || e.container) && !e.protected && e.is_mine)
        cmds.push({ id: `stop-${e.id}`, group: "Actions", icon: "stop", danger: true, title: `Stop :${e.port}  ${title(e)}`, subtitle: selected?.id === e.id ? "selected · asks first" : "asks first", boost, keywords: `kill ${e.port} ${e.label}`, run: () => requestStop(e, false) });
      if (canOpen(e))
        cmds.push({ id: `open-${e.id}`, group: "Actions", icon: "external", title: `Open :${e.port} in browser`, subtitle: title(e), boost, keywords: `browser ${e.port}`, run: () => open(e) });
    }
    if (q) cmds.unshift({ id: "check", group: "Ports", icon: "search", title: `Check port ${q}`, subtitle: "who's using it?", keywords: `${q} why explain`, run: () => { filters.query = String(q); } });
    cmds.push(
      { id: "refresh", group: "Actions", icon: "refresh", title: "Refresh now", shortcut: ["R"], run: () => refresh(true) },
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
      { id: "view", group: "View", icon: view === "graph" ? "list" : "graph", title: view === "graph" ? "Show the list" : "Show the service graph", shortcut: ["G"], keywords: "mesh topology network dependencies", run: () => (view = view === "graph" ? "list" : "graph") },
      { id: "sort-c", group: "View", icon: "layers", title: "Group by cluster (compose, workspace, supervisor…)", run: () => (sort = "cluster") },
      { id: "settings", group: "Settings", icon: "sliders", title: "Open settings", shortcut: [mod, ","], keywords: "preferences options config interval hotkey", run: () => openSettings() },
      { id: "pin-new", group: "Actions", icon: "star", title: selected ? `Pin :${selected.port} with a label…` : "Pin a port…", shortcut: ["⇧", "P"], keywords: "favourite label watch", run: () => openPin(selected) },
      { id: "remote", group: "Actions", icon: "server", title: "Inspect a remote host over SSH…", keywords: "ssh remote server machine", run: () => (showRemote = true) },
      { id: "history", group: "Actions", icon: "history", title: "Recently stopped — restart", shortcut: ["H"], keywords: "history restart undo", run: openHistory },
      { id: "notify", group: "Settings", icon: "bell", title: config?.notify ? "Turn notifications off" : "Turn notifications on", keywords: "alert conflict new listener", run: toggleNotify },
      { id: "density", group: "Settings", icon: "list", title: density === "compact" ? "Comfortable rows" : "Compact rows", keywords: "density dense rows height compact comfortable", run: () => { density = density === "compact" ? "comfortable" : "compact"; } },
      { id: "autostart", group: "Settings", icon: "sparkles", title: autostartOn ? "Don't launch at login" : "Launch at login", keywords: "startup boot login", run: toggleAutostart },
    );
    return cmds;
  });

  function onKey(e: KeyboardEvent) {
    const typing = document.activeElement === search;
    const modKey = e.metaKey || e.ctrlKey;
    if (modalOpen) return; // overlays handle (and stop) their own keys
    if (modKey && e.key.toLowerCase() === "k") { showPalette = true; e.preventDefault(); return; }
    if (modKey && e.key === ",") { openSettings(); e.preventDefault(); return; }
    if (!typing && (e.key === "/" || (modKey && e.key.toLowerCase() === "f"))) { search?.focus(); search?.select(); e.preventDefault(); return; }
    if (modKey && e.key.toLowerCase() === "r") { refresh(true); e.preventDefault(); return; }
    const move = view === "graph" && !typing ? selectGraph : select;
    if (view === "list" && !typing && !modKey && (e.key === "ArrowLeft" || e.key === "ArrowRight") && selectedId) {
      // Tree-style: ← collapses the selected row's section, → expands it.
      const sec = sectionOf(selectedId);
      if (sec?.title) setCollapsed(sec.id, e.key === "ArrowLeft");
      e.preventDefault(); return;
    }
    if (view === "list" && !typing && (e.key === "Home" || e.key === "End") && ordered.length) { selectEntry(ordered[e.key === "Home" ? 0 : ordered.length - 1], false); e.preventDefault(); return; }
    if (view === "list" && !typing && (e.key === "PageDown" || e.key === "PageUp")) { select(e.key === "PageDown" ? 10 : -10); e.preventDefault(); return; }
    if (e.key === "ArrowDown" || e.key === "ArrowRight" && view === "graph" && !typing || (!typing && e.key === "j")) { move(1); e.preventDefault(); return; }
    if (e.key === "ArrowUp" || e.key === "ArrowLeft" && view === "graph" && !typing || (!typing && e.key === "k")) { move(-1); e.preventDefault(); return; }
    if (e.key === "Escape") {
      if (typing) search?.blur();
      else if (filtersActive) clearFilters();
      else selectedId = null;
      e.preventDefault();
      return;
    }
    if (e.key === "Enter") { if (typing) { if (!selected) select(1); search?.blur(); } else if (selected) drawerOpen = true; return; }
    if (typing || modKey || e.altKey) return;
    // Only plain keys below; the list (or the page) has focus.
    if (e.target instanceof HTMLElement && e.target.closest("input, textarea, [role=combobox], [role=tablist], [role=separator]")) return;
    const s = selected;
    switch (e.key) {
      case "Backspace": case "Delete": if (s && (s.process || s.container)) requestStop(s, e.shiftKey); break;
      case "o": if (s && canOpen(s)) open(s); break;
      case "c": if (s) copy(url(s), "URL"); break;
      case "r": refresh(true); break;
      case "a": filters.all = !filters.all; refresh(); break;
      case "t": filters.proto = filters.proto === "any" ? "tcp" : filters.proto === "tcp" ? "udp" : "any"; break;
      case "d": filters.dev = !filters.dev; break;
      case "m": filters.mine = !filters.mine; break;
      case "e": filters.exposed = !filters.exposed; break;
      case "L": cycleTheme(); break;
      case "g": view = view === "graph" ? "list" : "graph"; break;
      case "h": openHistory(); break;
      case "p": if (s) togglePin(s); break;
      case "P": openPin(s); break;
      case "s": { const n = selectedGraphNode; const c = n?.cluster ? graph?.clusters.find((x) => x.id === n.cluster) : null; if (c) requestClusterStop(c.name); else return; break; }
      case "?": showHelp = true; break;
      default: return;
    }
    e.preventDefault();
  }

  onMount(() => {
    refresh();
    api.appInfo().then((i) => { shortcut = i.shortcut ?? null; info = { version: i.version, platform: i.platform, configDir: i.config_dir ?? null }; if (i.platform === "macos") isMac = true; else if (i.platform !== "browser") isMac = false; });
    api.hotkeys().then((h) => (hotkeyList = h)).catch(() => {});
    api.getConfig().then((c) => (config = c)).catch(() => {});
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
    const unlisten: Promise<() => void>[] = [
      api.onEvent<{ target: string; line: string }>("stop-progress", (p) => { if (confirm) confirm.log = [...confirm.log, p.line]; }),
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
        toast("info", `portwise ${u.version} is available`, "Install it from here or the command palette; the app restarts.", { label: "Install and restart", run: installUpdate });
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
    ];
    // Back from another app, Space or a covered window: catch up at once, not on the next tick.
    const onVisible = () => { if (!document.hidden && !confirm && Date.now() - (snapshot?.taken_at_ms ?? 0) > 1000) refresh(); };
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", onVisible);
    return () => {
      clearInterval(clock);
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", onVisible);
      mq.removeEventListener("change", onMq);
      nq.removeEventListener("change", onNq);
      rq.removeEventListener("change", onRq);
      unlisten.forEach((u) => u.then((f) => f()));
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
  $effect(() => { try { localStorage.setItem("pw.pane", String(paneWidth)); } catch { /* ignore */ } });

  const ago = $derived(dataAge(now, snapshot?.taken_at_ms ?? null, refreshing ? scanStarted : null));
  const fresh = $derived(freshness(ago, scanMs / 1000));
  const themeIcon = $derived(THEME_ICON[theme]);
</script>

<svelte:window onkeydown={onKey} />

<div class="app" class:mac={isMac}>
  <header class="titlebar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <img src="/icon.svg" alt="" width="22" height="22" />
      <span class="name">portwise</span>
    </div>

    <div class="search">
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
        kbdHint="/"
        spellcheck="false"
        autocomplete="off"
        aria-controls="port-list"
      />
    </div>

    <div class="tools">
      <span class="live" class:stale={fresh.stale} aria-live="off">
        <span class="pulse" class:on={refreshing}></span>{fresh.text}
      </span>
      <Button size="sm" icon="command" kbd={modK} class="cmdk" onclick={() => (showPalette = true)} aria-keyshortcuts="Meta+K Control+K" tip={{ text: "Command palette", kbd: modK }}>Commands</Button>
      <span class="tsep" aria-hidden="true"></span>
      <IconButton icon="refresh" label="Refresh" kbd="R" onclick={() => refresh(true)} class={manualRefresh ? "spinning" : ""} />
      <IconButton icon="history" label="Recently stopped" kbd="H" onclick={openHistory} />
      <IconButton icon="server" label="Remote host…" onclick={() => (showRemote = true)} />
      <IconButton icon={themeIcon} label="Theme: {THEME_LABEL[theme]}" kbd={["⇧", "L"]} onclick={cycleTheme} />
      <IconButton icon="sliders" label="Settings" kbd={[mod, ","]} onclick={() => openSettings()} />
    </div>
  </header>

  <div class="toolbar" role="toolbar" aria-label="View and filters">
    <SegmentedControl label="View" bind:value={view} options={[{ value: "list", label: "List", icon: "list", title: "List (G)" }, { value: "graph", label: "Graph", icon: "graph", count: graph?.stats.edges || null, countLabel: linksLabel(graph?.stats.edges ?? 0), title: graph?.stats.edges ? `Service graph · ${linksLabel(graph.stats.edges)} between services (G)` : "Service graph (G)" }]} />
    <span class="divider" aria-hidden="true"></span>
    <SegmentedControl label="Socket states" value={filters.all ? "all" : "listen"} options={[{ value: "listen", label: "Listening" }, { value: "all", label: "All sockets", title: "Include established, TIME_WAIT… (A)" }]} onchange={(v) => { filters.all = v === "all"; refresh(); }} />
    <SegmentedControl label="Protocol" bind:value={filters.proto} options={[{ value: "any", label: "Any" }, { value: "tcp", label: "TCP" }, { value: "udp", label: "UDP" }]} />
    <span class="divider" aria-hidden="true"></span>
    <FilterChip label="Dev servers" dot="var(--tone-green)" count={stats.dev} bind:pressed={filters.dev} title="Only dev servers" kbd="D" />
    <FilterChip label="Mine" icon="lock" count={stats.mine} bind:pressed={filters.mine} title="Only my processes" kbd="M" />
    <FilterChip label="Exposed" icon="globe" count={stats.exposed} tone={stats.exposed > 0 ? "warn" : "accent"} bind:pressed={filters.exposed} title="Reachable from the network" kbd="E" />
    {#if filtersActive}<Button size="sm" variant="ghost" icon="x" onclick={clearFilters}>Clear</Button>{/if}

    <div class="spacer"></div>
    <Select size="sm" prefix="Sort" label="Sort by" labelHidden bind:value={sort} width="150px" options={[{ value: "group", label: "Grouped", description: "By kind: dev, containers, databases…" }, { value: "cluster", label: "Cluster", description: "Compose project, workspace, supervisor" }, { value: "port", label: "Port" }, { value: "newest", label: "Newest" }, { value: "memory", label: "Memory" }]} />
  </div>

  <main class="content" class:narrow style="--pane-w: {paneWidth}px">
    {#if view === "graph"}
      <div class="graph-wrap">
        <GraphView {graph} {selectedNode} dark={resolvedTheme === "dark"} {reduced} all={graphAll} ontoggleall={() => { graphAll = !graphAll; loadTopology(); }} onselect={selectNode} onstopcluster={requestClusterStop} />
      </div>
    {:else}
    <div class="list {density}" id="port-list" role="listbox" aria-label="Ports in use" aria-activedescendant={selectedId ? "row-" + selectedId : undefined} tabindex="0" bind:this={listEl}>
      {#if snapshot && !onboarded}<Onboarding mod={mod} ondismiss={dismissOnboarding} />{/if}

      {#if error && !snapshot}
        <EmptyState tone="err" title="Couldn't read the socket table">
          <p class="selectable">{error}</p>
          <Button variant="primary" icon="refresh" onclick={() => refresh(true)}>Try again</Button>
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
        {#if freePort && freePort.status === "free"}
          <EmptyState tone="ok" icon="check" title="Port {freePort.port} is free">
            <p>Nothing is listening on it — start your server there:</p>
            <Button icon="copy" onclick={() => copy(`portwise run -p ${freePort?.port} -- npm run dev`, "Command")}><code>portwise run -p {freePort.port} -- npm run dev</code></Button>
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

    {#if !narrow}
      <div class="pane-wrap">
        <Splitter bind:width={paneWidth} min={340} max={680} initial={PANE_DEFAULT} />
        <DetailPane entry={selected} {explanation} http={selectedHttp} loading={explaining} busy={selected ? !!busy[selected.id] : false} bind:tab={detailTab} {mod}
          onstop={() => selected && requestStop(selected, false)} onkill={() => selected && requestStop(selected, true)} onopen={() => selected && open(selected)} oncopy={copy}
          {graph} pinned={!!selected && pins.has(selected.port)} onpin={() => selected && togglePin(selected)} onstopcluster={requestClusterStop} onselectnode={(n) => selectNode(n)}
          entries={snapshot?.entries ?? []} onselectentry={(e) => selectEntry(e, false)} />
      </div>
    {/if}
  </main>

  <footer class="status" aria-label="Summary">
    {#if snapshot}
      <span><b>{stats.total}</b> ports</span><span class="g"><b>{stats.dev}</b> dev</span>{#if stats.exposed}<span class="w"><b>{stats.exposed}</b> exposed</span>{/if}
      <span class="sp"></span>
      <span class="hints"><span><Kbd keys={["↑", "↓"]} size="sm" />move</span><span><Kbd keys="⌫" size="sm" />stop</span><span><Kbd keys="G" size="sm" />graph</span><span><Kbd keys={[mod, "K"]} size="sm" />commands</span><span><Kbd keys="?" size="sm" />shortcuts</span>{#if shortcut}<span class="gs"><Kbd keys={shortcut} size="sm" />from anywhere</span>{/if}</span>
    {/if}
  </footer>
</div>

{#if showDrawer && selected}
  <Dialog placement="right" bare label="Details for port {selected.port}" onclose={() => (drawerOpen = false)} initialFocus="self">
    <DetailPane drawer entry={selected} {explanation} http={selectedHttp} loading={explaining} busy={!!busy[selected.id]} bind:tab={detailTab} {mod}
      onstop={() => selected && requestStop(selected, false)} onkill={() => selected && requestStop(selected, true)} onopen={() => selected && open(selected)} oncopy={copy} onclose={() => (drawerOpen = false)}
      {graph} pinned={pins.has(selected.port)} onpin={() => selected && togglePin(selected)} onstopcluster={requestClusterStop} onselectnode={(n) => selectNode(n)} />
  </Dialog>
{/if}
{#if confirm}
  <ConfirmDialog entry={confirm.entry} cluster={confirm.cluster} plan={confirm.plan} phase={confirm.phase} log={confirm.log} report={confirm.report}
    onconfirm={runStop} oncancel={closeConfirm} onoverride={() => confirm?.entry && requestStop(confirm.entry, confirm.force, true)} />
{/if}
{#if showPalette}<CommandPalette {commands} onclose={() => { showPalette = false; }} />{/if}
{#if showHistory}<HistoryPanel items={historyItems} onrestart={restartEntry} oncopy={copy} onclose={() => (showHistory = false)} onclear={async () => { await api.clearHistory(); historyItems = []; }} />{/if}
{#if showHelp}<ShortcutsDialog {mod} onclose={() => (showHelp = false)} />{/if}
{#if showSettings && settingsModel}<SettingsDialog model={settingsModel} actions={settingsActions} bind:section={settingsSection} onclose={() => (showSettings = false)} />{/if}
{#if pinDialog}<PinDialog port={pinDialog.port} label={pinDialog.label} pinned={pinDialog.pinned} inUse={portHolder} onsave={savePin} onunpin={unpinPort} onclose={() => (pinDialog = null)} />{/if}
{#if showRemote}<RemoteDialog recent={config?.recent_hosts ?? []} onscan={async (h) => { const r = await api.remoteScan(h); config = await api.getConfig(); return r; }} oncopy={copy} onclose={() => (showRemote = false)} />{/if}
<Toasts {toasts} ondismiss={(id) => (toasts = toasts.filter((t) => t.id !== id))} />

<style>
  .app { display: grid; grid-template-rows: auto auto minmax(0, 1fr) auto; height: 100vh; }
  .titlebar { display: flex; align-items: center; gap: var(--sp-4); height: 52px; padding: 0 var(--sp-3) 0 var(--sp-4); background: var(--surface); border-bottom: 1px solid var(--border); }
  .app.mac .titlebar { padding-left: 84px; }
  .brand { display: flex; align-items: center; gap: var(--sp-2); flex: none; }
  .brand img { border-radius: 6px; box-shadow: var(--shadow-sm); }
  .name { font-weight: var(--fw-semibold); font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); }
  .search { flex: 1; max-width: 520px; margin: 0 auto; min-width: 140px; }
  .tools { display: flex; align-items: center; gap: 2px; flex: none; }
  .tools :global(.cmdk) { margin-right: 2px; color: var(--text-2); }
  .tools :global(.spinning svg) { animation: spin 0.9s linear infinite; }
  .tsep { width: 1px; height: 18px; background: var(--border-strong); margin: 0 6px; }
  .live { display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); margin-right: var(--sp-3); font-variant-numeric: tabular-nums; white-space: nowrap; min-width: 76px; justify-content: flex-end; }
  .pulse { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 3px var(--ok-soft); }
  .pulse.on { animation: beat 0.9s ease-in-out infinite; }
  .live.stale .pulse { background: var(--warn); box-shadow: 0 0 0 3px var(--warn-soft); }
  @keyframes beat { 50% { transform: scale(0.6); opacity: 0.6; } }

  .toolbar { display: flex; align-items: center; gap: var(--sp-2); padding: 8px var(--sp-4); background: var(--surface); border-bottom: 1px solid var(--border); overflow-x: auto; scrollbar-width: none; white-space: nowrap; min-height: 46px; }
  .toolbar::-webkit-scrollbar { display: none; }
  .divider { width: 1px; height: 18px; background: var(--border-strong); margin: 0 var(--sp-1); flex: none; }
  .spacer { flex: 1; min-width: var(--sp-2); }

  .content { display: grid; grid-template-columns: minmax(0, 1fr) var(--pane-w, 440px); min-height: 0; }
  .content.narrow { grid-template-columns: 1fr; }
  .pane-wrap { position: relative; min-height: 0; min-width: 0; }
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
  .graph-wrap { position: relative; min-height: 0; min-width: 0; }
  @keyframes spin { to { transform: rotate(360deg); } }

  @media (max-width: 1080px) { .tools :global(.cmdk .lbl), .tools :global(.cmdk .kbds) { display: none; } .tools :global(.cmdk) { padding: 0 8px; } }
  @media (max-width: 760px) { .brand .name, .live, .hints, .tsep { display: none; } .titlebar { gap: var(--sp-2); } }
</style>

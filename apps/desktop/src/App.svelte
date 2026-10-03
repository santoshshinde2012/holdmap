<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./components/Icon.svelte";
  import PortRow from "./components/PortRow.svelte";
  import DetailPane from "./components/DetailPane.svelte";
  import ConfirmDialog, { type Phase } from "./components/ConfirmDialog.svelte";
  import ShortcutsDialog from "./components/ShortcutsDialog.svelte";
  import CommandPalette from "./components/CommandPalette.svelte";
  import Onboarding from "./components/Onboarding.svelte";
  import EmptyState from "./components/EmptyState.svelte";
  import Toasts, { type Toast } from "./components/Toasts.svelte";
  import * as api from "./lib/api";
  import type { ActionPlan, Explanation, PortEntry, Snapshot, StopReport } from "./lib/types";
  import type { Command } from "./lib/palette";
  import { GROUPS, groupOf, matches, seconds, stopTarget, title, url, canOpen, type Filters, type Group } from "./lib/format";

  type Theme = "system" | "light" | "dark";
  type Sort = "group" | "port" | "newest" | "memory";
  interface Confirm { entry: PortEntry; plan: ActionPlan; force: boolean; allowProtected: boolean; phase: Phase; log: string[]; report: StopReport | null }

  const store = (k: string) => { try { return localStorage.getItem(k); } catch { return null; } };
  let snapshot = $state<Snapshot | null>(null);
  let error = $state<string | null>(null);
  let refreshing = $state(false);
  let filters = $state<Filters>({ query: "", all: false, proto: "any", dev: false, mine: false, exposed: false });
  let sort = $state<Sort>((store("pw.sort") as Sort) ?? "group");
  let selectedId = $state<string | null>(null);
  let explanations = $state<Record<number, Explanation>>({});
  let explaining = $state(false);
  let freePort = $state<Explanation | null>(null);
  let confirm = $state<Confirm | null>(null);
  let busy = $state<Record<string, boolean>>({});
  let toasts = $state<Toast[]>([]);
  let showHelp = $state(false);
  let showPalette = $state(false);
  let onboarded = $state(store("pw.onboarded") === "1");
  let theme = $state<Theme>((store("pw.theme") as Theme) ?? "system");
  let systemDark = $state(matchMedia("(prefers-color-scheme: dark)").matches);
  let narrow = $state(matchMedia("(max-width: 900px)").matches);
  let drawerOpen = $state(false);
  let isMac = $state(/Mac/.test(navigator.platform));
  let search = $state<HTMLInputElement | undefined>();
  let listEl = $state<HTMLDivElement | undefined>();
  let now = $state(Date.now());
  let toastSeq = 0;
  const mod = $derived(isMac ? "⌘" : "Ctrl");

  const resolvedTheme = $derived(theme === "system" ? (systemDark ? "dark" : "light") : theme);
  $effect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
    try { localStorage.setItem("pw.theme", theme); } catch { /* private mode */ }
  });
  $effect(() => { try { localStorage.setItem("pw.sort", sort); } catch { /* ignore */ } });

  const visible = $derived.by(() => {
    const list = (snapshot?.entries ?? []).filter((e) => matches(e, filters));
    const cmp: Record<Sort, (a: PortEntry, b: PortEntry) => number> = {
      group: (a, b) => a.port - b.port,
      port: (a, b) => a.port - b.port,
      newest: (a, b) => (b.process?.start_time ?? 0) - (a.process?.start_time ?? 0),
      memory: (a, b) => (b.process?.memory_bytes ?? 0) - (a.process?.memory_bytes ?? 0),
    };
    return list.sort(cmp[sort]);
  });
  const sections = $derived.by(() => {
    if (sort !== "group") return [{ id: "all" as Group | "all", title: "", hint: "", items: visible }];
    return GROUPS.map((g) => ({ ...g, id: g.id as Group | "all", items: visible.filter((e) => groupOf(e) === g.id) })).filter((g) => g.items.length);
  });
  const ordered = $derived(sections.flatMap((s) => s.items));
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

  async function refresh(manual = false) {
    if (refreshing) return;
    refreshing = true;
    try {
      const s = await api.scan(filters.all);
      snapshot = s;
      error = null;
      explanations = {};
      if (selectedId && !s.entries.some((e) => e.id === selectedId)) selectedId = null;
      if (manual) toast("info", "Refreshed", `${s.entries.length} ports · scanned in ${s.scan_ms} ms`);
    } catch (e) {
      error = String(e);
    } finally {
      refreshing = false;
    }
  }

  $effect(() => {
    const e = selected;
    if (!e || explanations[e.port]) return;
    explaining = true;
    const port = e.port;
    const t = setTimeout(async () => {
      try { explanations[port] = await api.explain(port); }
      catch (err) { toast("error", "Couldn't explain this port", String(err)); }
      finally { explaining = false; }
    }, 60);
    return () => clearTimeout(t);
  });

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
      confirm = { entry, plan, force, allowProtected, phase: "confirm", log: [], report: null };
    } catch (e) {
      toast("error", `Couldn't plan stopping :${entry.port}`, String(e));
    }
  }

  function restartCommand(c: Confirm): string | null {
    const s = c.plan.steps.find((x) => x.action === "signal_processes");
    if (!s || s.action !== "signal_processes" || !s.processes.length) return null;
    const dir = c.entry.project?.root ?? c.entry.process?.cwd;
    return `${dir ? `cd ${JSON.stringify(dir)} && ` : ""}${s.processes[0].command}`;
  }

  async function runStop() {
    if (!confirm || confirm.phase !== "confirm" || confirm.plan.blocked) return;
    const c = confirm;
    c.phase = "running";
    busy[c.entry.id] = true;
    try {
      const r = await api.stop(stopTarget(c.entry), c.force, c.allowProtected);
      c.report = r;
      if (!c.log.length) c.log = r.log;
      if (r.freed) {
        c.phase = "done";
        const restart = restartCommand(c);
        setTimeout(() => {
          if (confirm === c) confirm = null;
          toast("ok", `Port ${c.entry.port} is free`, `${title(c.entry)} stopped in ${seconds(r.elapsed_ms)}${r.escalated ? " (needed SIGKILL)" : ""}`,
            restart ? { label: "Copy restart command", run: () => copy(restart, "Restart command") } : undefined);
        }, 900);
      } else {
        c.phase = "failed";
        if (r.success) c.report = { ...r, error: `Stopped ${title(c.entry)}, but :${r.ports_still_busy.join(", :")} is still busy — something else grabbed it or a supervisor restarted it.` };
      }
    } catch (e) {
      c.phase = "failed";
      c.report = { target: c.plan.target, success: false, freed: false, ports_still_busy: [], signalled: [], escalated: false, survivors: [], elapsed_ms: 0, log: [], error: String(e) };
    } finally {
      delete busy[c.entry.id];
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
  function dismissOnboarding() { onboarded = true; try { localStorage.setItem("pw.onboarded", "1"); } catch { /* ignore */ } }

  const commands = $derived.by((): Command[] => {
    const cmds: Command[] = [];
    const q = queryPort;
    for (const e of (snapshot?.entries ?? []).slice(0, 60)) {
      const sub = `${e.framework?.name ?? e.process?.name ?? ""}${e.project ? ` · ${e.project.name}` : ""}`;
      cmds.push({ id: `go-${e.id}`, group: "Ports", icon: "hash", title: `:${e.port}  ${title(e)}`, subtitle: sub, keywords: `${e.port} ${e.label} ${e.process?.name ?? ""}`, run: () => { clearFilters(); selectEntry(e); } });
      if ((e.process || e.container) && !e.protected && e.is_mine)
        cmds.push({ id: `stop-${e.id}`, group: "Actions", icon: "stop", danger: true, title: `Stop :${e.port}  ${title(e)}`, subtitle: "asks first", keywords: `kill ${e.port} ${e.label}`, run: () => requestStop(e, false) });
      if (canOpen(e))
        cmds.push({ id: `open-${e.id}`, group: "Actions", icon: "external", title: `Open :${e.port} in browser`, subtitle: title(e), keywords: `browser ${e.port}`, run: () => open(e) });
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
      { id: "t-sys", group: "View", icon: "monitor", title: "Theme: match system", run: () => (theme = "system") },
      { id: "sort-g", group: "View", icon: "filter", title: "Sort: grouped by kind", run: () => (sort = "group") },
      { id: "sort-p", group: "View", icon: "hash", title: "Sort: by port", run: () => (sort = "port") },
      { id: "sort-n", group: "View", icon: "clock", title: "Sort: newest first", run: () => (sort = "newest") },
      { id: "help", group: "View", icon: "keyboard", title: "Keyboard shortcuts", shortcut: ["?"], run: () => (showHelp = true) },
    );
    return cmds;
  });

  function onKey(e: KeyboardEvent) {
    const typing = document.activeElement === search;
    const modKey = e.metaKey || e.ctrlKey;
    if (showPalette) return; // the palette handles its own keys
    if (modKey && e.key.toLowerCase() === "k") { showPalette = true; e.preventDefault(); return; }
    if (confirm) {
      if (e.key === "Escape") { closeConfirm(); e.preventDefault(); }
      else if (e.key === "Enter" && confirm.phase === "confirm" && !confirm.plan.blocked && !(document.activeElement instanceof HTMLButtonElement && document.activeElement.textContent?.includes("Cancel"))) { runStop(); e.preventDefault(); }
      return;
    }
    if (showHelp) { if (e.key === "Escape" || e.key === "?") { showHelp = false; e.preventDefault(); } return; }
    if (!typing && (e.key === "/" || (modKey && e.key.toLowerCase() === "f"))) { search?.focus(); search?.select(); e.preventDefault(); return; }
    if (modKey && e.key.toLowerCase() === "r") { refresh(true); e.preventDefault(); return; }
    if (e.key === "ArrowDown" || (!typing && e.key === "j")) { select(1); e.preventDefault(); return; }
    if (e.key === "ArrowUp" || (!typing && e.key === "k")) { select(-1); e.preventDefault(); return; }
    if (e.key === "Escape") {
      if (typing && filters.query) filters.query = "";
      else if (typing) search?.blur();
      else if (showDrawer) drawerOpen = false;
      else if (filtersActive) clearFilters();
      else selectedId = null;
      e.preventDefault();
      return;
    }
    if (e.key === "Enter") { if (typing) { if (!selected) select(1); search?.blur(); } else if (selected) drawerOpen = true; return; }
    if (typing || modKey || e.altKey) return;
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
      case "?": showHelp = true; break;
      default: return;
    }
    e.preventDefault();
  }

  onMount(() => {
    refresh();
    api.appInfo().then((i) => { if (i.platform === "macos") isMac = true; else if (i.platform !== "browser") isMac = false; });
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const nq = matchMedia("(max-width: 900px)");
    const onMq = () => (systemDark = mq.matches);
    const onNq = () => (narrow = nq.matches);
    mq.addEventListener("change", onMq);
    nq.addEventListener("change", onNq);
    const timer = setInterval(() => {
      now = Date.now();
      if (!document.hidden && !confirm && !refreshing && !showPalette) refresh();
    }, 3000);
    const unlisten: Promise<() => void>[] = [
      api.onEvent<{ target: string; line: string }>("stop-progress", (p) => { if (confirm) confirm.log = [...confirm.log, p.line]; }),
      api.onEvent<number>("focus-port", async (port) => {
        await refresh();
        clearFilters();
        const e = snapshot?.entries.find((x) => x.port === port);
        if (e) selectEntry(e);
      }),
      api.onEvent("refresh", () => refresh()),
    ];
    return () => {
      clearInterval(timer);
      mq.removeEventListener("change", onMq);
      nq.removeEventListener("change", onNq);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });

  const ago = $derived(snapshot ? Math.max(0, Math.round((now - snapshot.taken_at_ms) / 1000)) : null);
  const themeIcon = $derived(theme === "system" ? "monitor" : theme === "light" ? "sun" : "moon");
</script>

<svelte:window onkeydown={onKey} />

<div class="app" class:mac={isMac}>
  <header class="titlebar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <img src="/icon.svg" alt="" width="22" height="22" />
      <span class="name">portwise</span>
    </div>

    <label class="search" class:active={filters.query}>
      <Icon name="search" size={15} />
      <span class="sr-only">Search ports</span>
      <input
        bind:this={search}
        bind:value={filters.query}
        type="search"
        placeholder="Search ports, projects, processes…"
        spellcheck="false"
        autocomplete="off"
        aria-controls="port-list"
        aria-describedby="search-hint"
      />
      <span id="search-hint" class="sr-only">Supports :3000, 3000-3999, proto:udp and pid:123</span>
      {#if filters.query}
        <button class="clear" aria-label="Clear search" onclick={() => (filters.query = "")}><Icon name="x" size={12} /></button>
      {:else}
        <kbd>/</kbd>
      {/if}
    </label>

    <div class="tools">
      <span class="live" class:stale={ago !== null && ago > 6} aria-live="off" title={snapshot ? `Last scan ${ago}s ago (${snapshot.scan_ms} ms)` : ""}>
        <span class="pulse" class:on={refreshing}></span>{#if ago === null}starting…{:else if ago < 4}Live{:else}{ago}s ago{/if}
      </span>
      <button class="cmdk" onclick={() => (showPalette = true)} aria-label="Open command palette ({mod}K)" aria-keyshortcuts="Meta+K Control+K">
        <Icon name="command" size={13} />Commands<kbd>{mod}K</kbd>
      </button>
      <button class="icon-btn" onclick={() => refresh(true)} aria-label="Refresh (R)" title="Refresh (R)"><span class:spin={refreshing} style="display:inline-flex"><Icon name="refresh" size={16} /></span></button>
      <button class="icon-btn" onclick={cycleTheme} aria-label="Theme: {theme}. Click to change (Shift+L)" title="Theme: {theme} (⇧L)"><Icon name={themeIcon} size={16} /></button>
      <button class="icon-btn" onclick={() => (showHelp = true)} aria-label="Keyboard shortcuts (?)" title="Keyboard shortcuts (?)"><Icon name="keyboard" size={16} /></button>
    </div>
  </header>

  <div class="toolbar" role="toolbar" aria-label="Filters">
    <div class="seg" role="radiogroup" aria-label="Socket states">
      <button role="radio" aria-checked={!filters.all} class:on={!filters.all} onclick={() => { filters.all = false; refresh(); }}>Listening</button>
      <button role="radio" aria-checked={filters.all} class:on={filters.all} onclick={() => { filters.all = true; refresh(); }}>All sockets</button>
    </div>
    <div class="seg" role="radiogroup" aria-label="Protocol">
      {#each [["any", "Any"], ["tcp", "TCP"], ["udp", "UDP"]] as [v, l]}
        <button role="radio" aria-checked={filters.proto === v} class:on={filters.proto === v} onclick={() => (filters.proto = v as Filters["proto"])}>{l}</button>
      {/each}
    </div>
    <span class="divider" aria-hidden="true"></span>
    <button class="chip" aria-pressed={filters.dev} class:on={filters.dev} onclick={() => (filters.dev = !filters.dev)}><span class="cdot green"></span>Dev servers<span class="count">{stats.dev}</span></button>
    <button class="chip" aria-pressed={filters.mine} class:on={filters.mine} onclick={() => (filters.mine = !filters.mine)}><Icon name="lock" size={12} />Mine<span class="count">{stats.mine}</span></button>
    <button class="chip" aria-pressed={filters.exposed} class:on={filters.exposed} class:warnchip={stats.exposed > 0} onclick={() => (filters.exposed = !filters.exposed)}><Icon name="globe" size={12} />Exposed<span class="count">{stats.exposed}</span></button>
    {#if filtersActive}<button class="btn ghost sm" onclick={clearFilters}>Clear</button>{/if}

    <div class="spacer"></div>
    <label class="sort">
      <span>Sort</span>
      <select bind:value={sort} aria-label="Sort by">
        <option value="group">Grouped</option>
        <option value="port">Port</option>
        <option value="newest">Newest</option>
        <option value="memory">Memory</option>
      </select>
    </label>
  </div>

  <main class="content" class:narrow>
    <div class="list" id="port-list" role="listbox" aria-label="Ports in use" aria-activedescendant={selectedId ? "row-" + selectedId : undefined} tabindex="0" bind:this={listEl}>
      {#if snapshot && !onboarded}<Onboarding mod={mod} ondismiss={dismissOnboarding} />{/if}

      {#if error && !snapshot}
        <EmptyState tone="err" title="Couldn't read the socket table">
          <p class="selectable">{error}</p>
          <button class="btn primary" onclick={() => refresh(true)}><Icon name="refresh" size={14} />Try again</button>
        </EmptyState>
      {:else if !snapshot}
        <div class="skeletons" aria-busy="true" aria-label="Loading ports">
          <div class="sk-gh shimmer"></div>
          {#each Array(9) as _, i}
            <div class="sk-row" style="animation-delay: {i * 70}ms; opacity: {1 - i * 0.08}">
              <div><div class="shimmer" style="height:16px;width:52px"></div><div class="shimmer" style="height:8px;width:64px;margin-top:6px"></div></div>
              <div class="shimmer" style="height:32px;width:32px;border-radius:9px"></div>
              <div><div class="shimmer" style="height:12px;width:{40 + ((i * 37) % 35)}%"></div><div class="shimmer" style="height:9px;width:{55 + ((i * 23) % 30)}%;margin-top:8px"></div></div>
            </div>
          {/each}
        </div>
      {:else if ordered.length === 0}
        {#if freePort && freePort.status === "free"}
          <EmptyState tone="ok" title="Port {freePort.port} is free">
            <p>Nothing is listening on it — start your server there:</p>
            <button class="btn" onclick={() => copy(`portwise run -p ${freePort?.port} -- npm run dev`, "Command")}><Icon name="copy" size={13} /><code>portwise run -p {freePort.port} -- npm run dev</code></button>
          </EmptyState>
        {:else if freePort}
          <EmptyState tone="warn" title="Port {freePort.port}">
            <p class="selectable">{freePort.headline}</p>
            <p class="selectable">{freePort.recommendation}</p>
          </EmptyState>
        {:else if filtersActive}
          <EmptyState title="No ports match">
            <p>Nothing matches {filters.query ? `“${filters.query}”` : "these filters"}.</p>
            <button class="btn" onclick={clearFilters}>Clear search & filters <kbd>Esc</kbd></button>
          </EmptyState>
        {:else}
          <EmptyState tone="ok" title="All quiet">
            <p>Nothing is listening right now. Start a dev server and it shows up here within a few seconds.</p>
            <button class="btn" onclick={() => findFree(3000)}><Icon name="sparkles" size={13} />Find a free port</button>
          </EmptyState>
        {/if}
      {:else}
        {#each sections as s (s.id)}
          {#if s.title}
            <div class="group" role="presentation">
              <span class="gt">{s.title}</span><span class="gc">{s.items.length}</span><span class="gh">{s.hint}</span>
            </div>
          {/if}
          {#each s.items as e (e.id)}
            <PortRow entry={e} selected={e.id === selectedId} busy={!!busy[e.id]} onselect={() => { selectEntry(e); listEl?.focus(); }} onstop={() => requestStop(e, false)} onopen={() => open(e)} />
          {/each}
        {/each}
        {#if snapshot.hidden_sockets > 0 || !snapshot.docker_available}
          <div class="foot-note">
            {#if snapshot.hidden_sockets > 0}<span><Icon name="lock" size={12} />{snapshot.hidden_sockets} socket{snapshot.hidden_sockets === 1 ? "" : "s"} owned by other users — run with admin rights to see them</span>{/if}
            {#if !snapshot.docker_available}<span><Icon name="box" size={12} />Docker isn't running — container names unavailable</span>{/if}
          </div>
        {/if}
      {/if}
    </div>

    {#if !narrow}
      <DetailPane entry={selected} explanation={selected ? explanations[selected.port] ?? null : null} loading={explaining} busy={selected ? !!busy[selected.id] : false}
        onstop={() => selected && requestStop(selected, false)} onkill={() => selected && requestStop(selected, true)} onopen={() => selected && open(selected)} oncopy={copy} />
    {/if}
  </main>

  <footer class="status" aria-label="Summary">
    {#if snapshot}
      <span><b>{stats.total}</b> ports</span><span class="g"><b>{stats.dev}</b> dev</span>{#if stats.exposed}<span class="w"><b>{stats.exposed}</b> exposed</span>{/if}
      <span class="sp"></span>
      <span class="hints"><kbd>↑↓</kbd> move <kbd>⌫</kbd> stop <kbd>O</kbd> open <kbd>{mod}K</kbd> commands <kbd>?</kbd> help</span>
    {/if}
  </footer>
</div>

{#if showDrawer}
  <div class="drawer-wrap" role="presentation" onclick={() => (drawerOpen = false)}>
    <div class="drawer" role="dialog" aria-modal="true" aria-label="Port details" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
      <DetailPane drawer entry={selected} explanation={selected ? explanations[selected.port] ?? null : null} loading={explaining} busy={selected ? !!busy[selected.id] : false}
        onstop={() => selected && requestStop(selected, false)} onkill={() => selected && requestStop(selected, true)} onopen={() => selected && open(selected)} oncopy={copy} onclose={() => (drawerOpen = false)} />
    </div>
  </div>
{/if}
{#if confirm}
  <ConfirmDialog entry={confirm.entry} plan={confirm.plan} phase={confirm.phase} log={confirm.log} report={confirm.report}
    onconfirm={runStop} oncancel={closeConfirm} onoverride={() => confirm && requestStop(confirm.entry, confirm.force, true)} />
{/if}
{#if showPalette}<CommandPalette {commands} onclose={() => { showPalette = false; listEl?.focus(); }} />{/if}
{#if showHelp}<ShortcutsDialog onclose={() => (showHelp = false)} />{/if}
<Toasts {toasts} ondismiss={(id) => (toasts = toasts.filter((t) => t.id !== id))} />

<style>
  .app { display: grid; grid-template-rows: auto auto minmax(0, 1fr) auto; height: 100vh; }
  .titlebar { display: flex; align-items: center; gap: var(--sp-4); height: 52px; padding: 0 var(--sp-3) 0 var(--sp-4); background: var(--surface); border-bottom: 1px solid var(--border); }
  .app.mac .titlebar { padding-left: 84px; }
  .brand { display: flex; align-items: center; gap: var(--sp-2); }
  .brand img { border-radius: 6px; box-shadow: var(--shadow-sm); }
  .name { font-weight: 700; font-size: var(--fs-md); letter-spacing: -0.02em; }
  .search { flex: 1; max-width: 560px; margin: 0 auto; display: flex; align-items: center; gap: var(--sp-2); height: 34px; padding: 0 var(--sp-2) 0 11px; border-radius: var(--r-md); background: var(--surface-2); border: 1px solid var(--border); color: var(--muted); transition: border-color var(--dur-2), background var(--dur-2), box-shadow var(--dur-2); }
  .search:hover { border-color: var(--border-strong); }
  .search:focus-within { background: var(--surface); border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft); }
  .search input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; color: var(--text); font-size: var(--fs-base); }
  .search input::placeholder { color: var(--faint); }
  .search input::-webkit-search-cancel-button { display: none; }
  .clear { border: 0; background: var(--surface-3); color: var(--muted); width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; }
  .tools { display: flex; align-items: center; gap: 2px; }
  .live { display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-xs); color: var(--muted); margin-right: var(--sp-2); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .pulse { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 3px var(--ok-soft); }
  .pulse.on { animation: beat 0.9s ease-in-out infinite; }
  .live.stale .pulse { background: var(--warn); box-shadow: 0 0 0 3px var(--warn-soft); }
  @keyframes beat { 50% { transform: scale(0.6); opacity: 0.6; } }
  .cmdk { display: inline-flex; align-items: center; gap: 6px; height: 28px; padding: 0 6px 0 9px; margin-right: var(--sp-1); border-radius: var(--r-sm); border: 1px solid var(--border-strong); background: var(--surface); color: var(--text-2); font-size: var(--fs-xs); font-weight: 550; box-shadow: var(--shadow-sm); }
  .cmdk:hover { background: var(--surface-2); color: var(--text); }

  .toolbar { display: flex; align-items: center; gap: var(--sp-2); padding: var(--sp-2) var(--sp-4); background: var(--surface); border-bottom: 1px solid var(--border); overflow-x: auto; scrollbar-width: none; white-space: nowrap; }
  .toolbar::-webkit-scrollbar { display: none; }
  .seg { display: inline-flex; padding: 2px; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-sm); flex: none; }
  .seg button { border: 0; background: transparent; height: 24px; padding: 0 10px; border-radius: 5px; color: var(--muted); font-weight: 550; font-size: var(--fs-xs); transition: background var(--dur-2), color var(--dur-2); }
  .seg button:hover { color: var(--text); }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: var(--shadow-sm), 0 0 0 1px var(--border); }
  .divider { width: 1px; height: 18px; background: var(--border-strong); margin: 0 var(--sp-1); flex: none; }
  .chip { flex: none; display: inline-flex; align-items: center; gap: 6px; height: 28px; padding: 0 6px 0 10px; border-radius: var(--r-full); border: 1px solid var(--border-strong); background: var(--surface); color: var(--text-2); font-size: var(--fs-xs); font-weight: 550; transition: all var(--dur-2) var(--ease); }
  .chip:hover { background: var(--surface-2); color: var(--text); }
  .chip.on { border-color: var(--accent); background: var(--accent-soft); color: var(--accent); }
  .count { min-width: 20px; height: 18px; padding: 0 5px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-size: 10.5px; display: inline-grid; place-items: center; font-variant-numeric: tabular-nums; font-weight: 650; }
  .chip.on .count { background: var(--accent); color: var(--accent-fg); }
  .warnchip:not(.on) :global(svg) { color: var(--warn); }
  .cdot { width: 7px; height: 7px; border-radius: 50%; }
  .cdot.green { background: var(--tone-green); }
  .spacer { flex: 1; }
  .sort { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); font-size: var(--fs-xs); flex: none; }
  .sort select { height: 28px; border-radius: var(--r-sm); border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); padding: 0 6px; font-size: var(--fs-xs); }

  .content { display: grid; grid-template-columns: minmax(0, 1fr) clamp(360px, 36vw, 480px); min-height: 0; }
  .content.narrow { grid-template-columns: 1fr; }
  .list { overflow-y: auto; padding: var(--sp-1) 0 var(--sp-6); outline: none; }
  .list:focus-visible { box-shadow: inset 0 0 0 2px var(--ring); }
  .group { position: sticky; top: -4px; z-index: 2; display: flex; align-items: baseline; gap: var(--sp-2); padding: var(--sp-4) var(--sp-5) var(--sp-2); background: color-mix(in srgb, var(--bg) 90%, transparent); backdrop-filter: blur(10px); }
  .gt { font-size: var(--fs-2xs); font-weight: 700; text-transform: uppercase; letter-spacing: 0.09em; color: var(--text-2); }
  .gc { font-size: 10.5px; color: var(--muted); background: var(--surface-3); padding: 1px 6px; border-radius: var(--r-full); font-variant-numeric: tabular-nums; font-weight: 650; }
  .gh { font-size: var(--fs-xs); color: var(--muted); }
  .foot-note { display: flex; flex-direction: column; gap: 6px; color: var(--muted); font-size: var(--fs-xs); padding: var(--sp-5) var(--sp-5) 0; }
  .foot-note span { display: inline-flex; align-items: center; gap: 6px; }

  .skeletons { padding: var(--sp-3) var(--sp-2); }
  .sk-gh { height: 10px; width: 120px; margin: var(--sp-3) var(--sp-3) var(--sp-3); }
  .sk-row { display: grid; grid-template-columns: 92px 32px 1fr; gap: var(--sp-3); align-items: center; padding: 12px var(--sp-4); }

  .status { display: flex; align-items: center; gap: var(--sp-4); height: 28px; padding: 0 var(--sp-4); border-top: 1px solid var(--border); background: var(--surface); color: var(--muted); font-size: var(--fs-xs); font-variant-numeric: tabular-nums; white-space: nowrap; overflow: hidden; }
  .status b { color: var(--text); font-weight: 650; }
  .status .g b { color: var(--tone-green); }
  .status .w b { color: var(--warn); }
  .status .sp { flex: 1; }
  .hints { display: inline-flex; align-items: center; gap: 4px; }
  .hints kbd { margin-left: 8px; height: 16px; font-size: 10px; }

  .drawer-wrap { position: fixed; inset: 0; z-index: 40; background: var(--backdrop); animation: fadein var(--dur-2) var(--ease); }
  .drawer { position: absolute; top: 0; right: 0; bottom: 0; width: min(440px, 94vw); animation: slidein var(--dur-3) var(--ease); outline: none; }
  @keyframes fadein { from { opacity: 0; } }
  @keyframes slidein { from { transform: translateX(24px); opacity: 0.4; } }

  @media (max-width: 1080px) { .cmdk { font-size: 0; gap: 0; padding: 0 6px; } .cmdk kbd { display: none; } }
  @media (max-width: 760px) { .brand .name, .live, .sort span, .hints { display: none; } .titlebar { gap: var(--sp-2); } }
</style>

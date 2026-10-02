<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./components/Icon.svelte";
  import PortRow from "./components/PortRow.svelte";
  import DetailPane from "./components/DetailPane.svelte";
  import ConfirmDialog from "./components/ConfirmDialog.svelte";
  import ShortcutsDialog from "./components/ShortcutsDialog.svelte";
  import Toasts, { type Toast } from "./components/Toasts.svelte";
  import * as api from "./lib/api";
  import type { ActionPlan, Explanation, PortEntry, Snapshot } from "./lib/types";
  import { GROUPS, groupOf, matches, stopTarget, title, url, type Filters, type Group } from "./lib/format";

  type Theme = "system" | "light" | "dark";
  type Sort = "group" | "port" | "newest" | "memory";

  let snapshot = $state<Snapshot | null>(null);
  let error = $state<string | null>(null);
  let refreshing = $state(false);
  let filters = $state<Filters>({ query: "", all: false, proto: "any", dev: false, mine: false, exposed: false });
  let sort = $state<Sort>((localStorage.getItem("pw.sort") as Sort) ?? "group");
  let selectedId = $state<string | null>(null);
  let explanations = $state<Record<number, Explanation>>({});
  let explaining = $state(false);
  let freePort = $state<Explanation | null>(null);
  let confirm = $state<{ entry: PortEntry; plan: ActionPlan; force: boolean; allowProtected: boolean; running: boolean; log: string[] } | null>(null);
  let busy = $state<Record<string, boolean>>({});
  let toasts = $state<Toast[]>([]);
  let showHelp = $state(false);
  let theme = $state<Theme>((localStorage.getItem("pw.theme") as Theme) ?? "system");
  let systemDark = $state(matchMedia("(prefers-color-scheme: dark)").matches);
  let isMac = $state(/Mac/.test(navigator.platform));
  let search = $state<HTMLInputElement | undefined>();
  let listEl = $state<HTMLDivElement | undefined>();
  let now = $state(Date.now());
  let toastSeq = 0;

  const resolvedTheme = $derived(theme === "system" ? (systemDark ? "dark" : "light") : theme);
  $effect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
    localStorage.setItem("pw.theme", theme);
  });
  $effect(() => localStorage.setItem("pw.sort", sort));

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
    if (sort !== "group") return [{ id: "all" as const, title: "", hint: "", items: visible }];
    return GROUPS.map((g) => ({ ...g, items: visible.filter((e) => groupOf(e) === g.id) })).filter((g) => g.items.length);
  });
  const ordered = $derived(sections.flatMap((s) => s.items));
  const selected = $derived(ordered.find((e) => e.id === selectedId) ?? null);
  const stats = $derived.by(() => {
    const es = snapshot?.entries ?? [];
    return {
      total: es.length,
      dev: es.filter((e) => e.is_dev).length,
      exposed: es.filter((e) => e.exposure === "all_interfaces").length,
    };
  });
  const queryPort = $derived.by(() => {
    const m = filters.query.trim().match(/^:?(\d{1,5})$/);
    const n = m ? +m[1] : NaN;
    return n > 0 && n < 65536 ? n : null;
  });
  const filtersActive = $derived(filters.query !== "" || filters.dev || filters.mine || filters.exposed || filters.proto !== "any");

  async function refresh(manual = false) {
    if (refreshing) return;
    refreshing = true;
    try {
      const s = await api.scan(filters.all);
      snapshot = s;
      error = null;
      explanations = {};
      if (selectedId && !s.entries.some((e) => e.id === selectedId)) selectedId = null;
      if (!selectedId && s.entries.length) selectedId = null;
      if (manual) toast("info", `Scanned ${s.entries.length} ports in ${s.scan_ms} ms`);
    } catch (e) {
      error = String(e);
    } finally {
      refreshing = false;
    }
  }

  // Load the explanation for the selected port (cached per scan).
  $effect(() => {
    const e = selected;
    if (!e || explanations[e.port]) return;
    explaining = true;
    const port = e.port;
    const t = setTimeout(async () => {
      try {
        explanations[port] = await api.explain(port);
      } catch (err) {
        toast("error", "Couldn't explain this port", String(err));
      } finally {
        explaining = false;
      }
    }, 60);
    return () => clearTimeout(t);
  });

  // "Is 4321 free?" — explain a port typed in the search box that has no rows.
  $effect(() => {
    const p = queryPort;
    freePort = null;
    if (p === null || visible.some((e) => e.port === p)) return;
    const t = setTimeout(async () => {
      try { freePort = await api.explain(p); } catch { /* ignore */ }
    }, 150);
    return () => clearTimeout(t);
  });

  function toast(kind: Toast["kind"], text: string, detail?: string) {
    const id = ++toastSeq;
    toasts = [...toasts, { id, kind, text, detail }].slice(-4);
    setTimeout(() => (toasts = toasts.filter((t) => t.id !== id)), kind === "error" ? 8000 : 3500);
  }

  async function requestStop(entry: PortEntry, force: boolean, allowProtected = false) {
    try {
      const plan = await api.plan(stopTarget(entry), force, allowProtected);
      confirm = { entry, plan, force, allowProtected, running: false, log: [] };
    } catch (e) {
      toast("error", `Couldn't plan stopping :${entry.port}`, String(e));
    }
  }

  async function runStop() {
    if (!confirm || confirm.running) return;
    const c = confirm;
    c.running = true;
    busy[c.entry.id] = true;
    try {
      const r = await api.stop(stopTarget(c.entry), c.force, c.allowProtected);
      if (r.freed) {
        toast("ok", `Port ${c.entry.port} is free`, `${title(c.entry)} stopped in ${(r.elapsed_ms / 1000).toFixed(1)}s${r.escalated ? " (needed SIGKILL)" : ""}`);
      } else if (r.success) {
        toast("info", `${title(c.entry)} stopped, but :${r.ports_still_busy.join(", :")} is still busy`, "Something else may have grabbed it, or a supervisor restarted it. Check Explain.");
      } else {
        toast("error", `Couldn't stop :${c.entry.port}`, r.error ?? r.log.at(-1));
      }
      confirm = null;
    } catch (e) {
      toast("error", `Couldn't stop :${c.entry.port}`, String(e));
      confirm = null;
    } finally {
      delete busy[c.entry.id];
      await refresh();
    }
  }

  function select(delta: number) {
    if (!ordered.length) return;
    const i = ordered.findIndex((e) => e.id === selectedId);
    const next = i < 0 ? (delta > 0 ? 0 : ordered.length - 1) : Math.max(0, Math.min(ordered.length - 1, i + delta));
    selectedId = ordered[next].id;
    requestAnimationFrame(() => document.getElementById("row-" + selectedId)?.scrollIntoView({ block: "nearest" }));
  }

  async function open(e: PortEntry) {
    try { await api.openUrl(url(e)); } catch (err) { toast("error", "Couldn't open the browser", String(err)); }
  }

  async function copy(text: string, what: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast("ok", `${what} copied`, text.length > 80 ? text.slice(0, 80) + "…" : text);
    } catch {
      toast("error", "Clipboard unavailable");
    }
  }

  function cycleTheme() {
    theme = theme === "system" ? "light" : theme === "light" ? "dark" : "system";
  }

  function onKey(e: KeyboardEvent) {
    const typing = document.activeElement === search;
    const mod = e.metaKey || e.ctrlKey;
    if (confirm) {
      if (e.key === "Escape" && !confirm.running) { confirm = null; e.preventDefault(); }
      else if (e.key === "Enter" && !confirm.plan.blocked && !(document.activeElement instanceof HTMLButtonElement && document.activeElement.textContent?.includes("Cancel"))) { runStop(); e.preventDefault(); }
      return;
    }
    if (showHelp) {
      if (e.key === "Escape" || e.key === "?") { showHelp = false; e.preventDefault(); }
      return;
    }
    if ((mod && e.key.toLowerCase() === "k") || (!typing && e.key === "/")) {
      search?.focus(); search?.select(); e.preventDefault(); return;
    }
    if (mod && e.key.toLowerCase() === "r") { refresh(true); e.preventDefault(); return; }
    if (e.key === "ArrowDown" || (!typing && e.key === "j")) { select(1); e.preventDefault(); return; }
    if (e.key === "ArrowUp" || (!typing && e.key === "k")) { select(-1); e.preventDefault(); return; }
    if (e.key === "Escape") {
      if (typing && filters.query) filters.query = "";
      else if (typing) search?.blur();
      else if (filtersActive) { filters.query = ""; filters.dev = filters.mine = filters.exposed = false; filters.proto = "any"; }
      else selectedId = null;
      e.preventDefault();
      return;
    }
    if (e.key === "Enter" && typing) { if (!selected) select(1); search?.blur(); return; }
    if (typing || mod || e.altKey) return;
    const s = selected;
    switch (e.key) {
      case "Backspace":
      case "Delete":
        if (s && (s.process || s.container)) requestStop(s, e.shiftKey);
        break;
      case "o": if (s) open(s); break;
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
    const onMq = () => (systemDark = mq.matches);
    mq.addEventListener("change", onMq);
    const timer = setInterval(() => {
      now = Date.now();
      if (!document.hidden && !confirm && !refreshing) refresh();
    }, 3000);
    const unlisten: Promise<() => void>[] = [
      api.onEvent<{ target: string; line: string }>("stop-progress", (p) => { if (confirm) confirm.log = [...confirm.log, p.line]; }),
      api.onEvent<number>("focus-port", async (port) => {
        await refresh();
        filters.query = "";
        const e = snapshot?.entries.find((x) => x.port === port);
        if (e) { selectedId = e.id; requestAnimationFrame(() => document.getElementById("row-" + e.id)?.scrollIntoView({ block: "center" })); }
      }),
      api.onEvent("refresh", () => refresh()),
    ];
    return () => {
      clearInterval(timer);
      mq.removeEventListener("change", onMq);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });

  const ago = $derived(snapshot ? Math.max(0, Math.round((now - snapshot.taken_at_ms) / 1000)) : null);
  const themeIcon = $derived(theme === "system" ? "monitor" : theme === "light" ? "sun" : "moon");
  const groupCount = (id: Group | "all") => sections.find((s) => s.id === id)?.items.length ?? 0;
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
        placeholder="Search ports, projects, processes…  (:3000, 3000-3999, proto:udp)"
        spellcheck="false"
        autocomplete="off"
        aria-controls="port-list"
      />
      {#if filters.query}
        <button class="clear" aria-label="Clear search" onclick={() => (filters.query = "")}><Icon name="x" size={13} /></button>
      {:else}
        <kbd>{isMac ? "⌘" : "Ctrl"} K</kbd>
      {/if}
    </label>

    <div class="tools">
      <span class="ago" aria-live="off">{#if ago !== null}{refreshing ? "scanning…" : ago < 3 ? "live" : `${ago}s ago`}{/if}</span>
      <button class="icon-btn" onclick={() => refresh(true)} aria-label="Refresh (R)" title="Refresh (R)">
        <span class:spin={refreshing} style="display:inline-flex"><Icon name="refresh" size={16} /></span>
      </button>
      <button class="icon-btn" onclick={cycleTheme} aria-label="Theme: {theme} (⇧L)" title="Theme: {theme} (⇧L)"><Icon name={themeIcon} size={16} /></button>
      <button class="icon-btn" onclick={() => (showHelp = true)} aria-label="Keyboard shortcuts (?)" title="Keyboard shortcuts (?)"><Icon name="keyboard" size={16} /></button>
    </div>
  </header>

  <div class="toolbar" role="toolbar" aria-label="Filters">
    <div class="seg" role="radiogroup" aria-label="Socket states">
      <button role="radio" aria-checked={!filters.all} class:on={!filters.all} onclick={() => { filters.all = false; refresh(); }}>Listening</button>
      <button role="radio" aria-checked={filters.all} class:on={filters.all} onclick={() => { filters.all = true; refresh(); }}>All sockets</button>
    </div>
    <div class="seg" role="radiogroup" aria-label="Protocol">
      {#each [["any", "TCP + UDP"], ["tcp", "TCP"], ["udp", "UDP"]] as [v, l]}
        <button role="radio" aria-checked={filters.proto === v} class:on={filters.proto === v} onclick={() => (filters.proto = v as Filters["proto"])}>{l}</button>
      {/each}
    </div>
    <button class="chip" aria-pressed={filters.dev} class:on={filters.dev} onclick={() => (filters.dev = !filters.dev)}><span class="cdot green"></span>Dev servers <kbd>D</kbd></button>
    <button class="chip" aria-pressed={filters.mine} class:on={filters.mine} onclick={() => (filters.mine = !filters.mine)}>Mine <kbd>M</kbd></button>
    <button class="chip" aria-pressed={filters.exposed} class:on={filters.exposed} onclick={() => (filters.exposed = !filters.exposed)}><Icon name="globe" size={12} />Exposed <kbd>E</kbd></button>

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
    {#if snapshot}
      <div class="stats" aria-label="Summary">
        <span><b>{stats.total}</b> ports</span>
        <span class="g"><b>{stats.dev}</b> dev</span>
        {#if stats.exposed}<span class="w"><b>{stats.exposed}</b> exposed</span>{/if}
      </div>
    {/if}
  </div>

  <main class="content">
    <div class="list" id="port-list" role="listbox" aria-label="Ports in use" aria-activedescendant={selectedId ? "row-" + selectedId : undefined} tabindex="0" bind:this={listEl}>
      {#if error && !snapshot}
        <div class="state">
          <div class="state-icon err"><Icon name="alert" size={26} /></div>
          <h3>Couldn't read the socket table</h3>
          <p class="selectable">{error}</p>
          <button class="btn primary" onclick={() => refresh(true)}><Icon name="refresh" size={14} />Try again</button>
        </div>
      {:else if !snapshot}
        <div class="skeletons" aria-busy="true" aria-label="Loading ports">
          {#each Array(8) as _, i}
            <div class="sk-row" style="animation-delay: {i * 60}ms"><div class="sk a"></div><div class="sk b"></div><div class="sk c"></div></div>
          {/each}
        </div>
      {:else if ordered.length === 0}
        <div class="state">
          {#if freePort && freePort.status === "free"}
            <div class="state-icon ok"><Icon name="check" size={26} /></div>
            <h3>Port {freePort.port} is free</h3>
            <p>Nothing is listening on it. Start your server there:</p>
            <button class="btn" onclick={() => copy(`portwise run -p ${freePort?.port} -- npm run dev`, "Command")}><Icon name="copy" size={13} /><code>portwise run -p {freePort.port} -- npm run dev</code></button>
          {:else if freePort}
            <div class="state-icon warn"><Icon name="info" size={26} /></div>
            <h3>Port {freePort.port}</h3>
            <p class="selectable">{freePort.headline}</p>
            <p class="selectable muted">{freePort.recommendation}</p>
          {:else if filtersActive}
            <div class="state-icon"><Icon name="filter" size={26} /></div>
            <h3>No ports match</h3>
            <p>Nothing matches your search and filters.</p>
            <button class="btn" onclick={() => { filters.query = ""; filters.dev = filters.mine = filters.exposed = false; filters.proto = "any"; }}>Clear filters <kbd>Esc</kbd></button>
          {:else}
            <div class="state-icon ok"><Icon name="plug" size={26} /></div>
            <h3>All quiet</h3>
            <p>Nothing is listening right now. Start a dev server and it'll show up here.</p>
          {/if}
        </div>
      {:else}
        {#each sections as s (s.id)}
          {#if s.title}
            <div class="group" role="presentation">
              <span class="gt">{s.title}</span><span class="gc">{groupCount(s.id)}</span><span class="gh">{s.hint}</span>
            </div>
          {/if}
          {#each s.items as e (e.id)}
            <PortRow
              entry={e}
              selected={e.id === selectedId}
              busy={!!busy[e.id]}
              onselect={() => { selectedId = e.id; listEl?.focus(); }}
              onstop={() => requestStop(e, false)}
              onopen={() => open(e)}
            />
          {/each}
        {/each}
        {#if snapshot.hidden_sockets > 0 || !snapshot.docker_available}
          <p class="foot-note">
            {#if snapshot.hidden_sockets > 0}<Icon name="lock" size={12} /> {snapshot.hidden_sockets} socket{snapshot.hidden_sockets === 1 ? "" : "s"} owned by other users — run portwise with admin rights to see them.{/if}
            {#if !snapshot.docker_available}<span class="nd">Docker isn't running, so container names aren't shown.</span>{/if}
          </p>
        {/if}
      {/if}
    </div>

    <DetailPane
      entry={selected}
      explanation={selected ? explanations[selected.port] ?? null : null}
      loading={explaining}
      busy={selected ? !!busy[selected.id] : false}
      onstop={() => selected && requestStop(selected, false)}
      onkill={() => selected && requestStop(selected, true)}
      onopen={() => selected && open(selected)}
      oncopy={copy}
    />
  </main>
</div>

{#if confirm}
  <ConfirmDialog
    entry={confirm.entry}
    plan={confirm.plan}
    running={confirm.running}
    log={confirm.log}
    onconfirm={runStop}
    oncancel={() => (confirm = null)}
    onoverride={() => confirm && requestStop(confirm.entry, confirm.force, true)}
  />
{/if}
{#if showHelp}<ShortcutsDialog onclose={() => (showHelp = false)} />{/if}
<Toasts {toasts} ondismiss={(id) => (toasts = toasts.filter((t) => t.id !== id))} />

<style>
  .app { display: grid; grid-template-rows: auto auto minmax(0, 1fr); height: 100vh; }
  .titlebar { display: flex; align-items: center; gap: 16px; height: 52px; padding: 0 12px 0 16px; background: var(--surface); border-bottom: 1px solid var(--border); }
  .app.mac .titlebar { padding-left: 84px; }
  .brand { display: flex; align-items: center; gap: 8px; min-width: 120px; }
  .brand img { border-radius: 6px; }
  .name { font-weight: 700; font-size: 14px; letter-spacing: -0.01em; }
  .search { flex: 1; max-width: 620px; margin: 0 auto; display: flex; align-items: center; gap: 8px; height: 34px; padding: 0 8px 0 11px; border-radius: 9px; background: var(--surface-2); border: 1px solid transparent; color: var(--muted); transition: border-color 0.15s, background 0.15s, box-shadow 0.15s; }
  .search:focus-within { background: var(--surface); border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft); }
  .search input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; color: var(--text); font-size: 13px; }
  .search input::placeholder { color: var(--faint); }
  .search input::-webkit-search-cancel-button { display: none; }
  .clear { border: 0; background: var(--surface-3); color: var(--muted); width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; }
  .tools { display: flex; align-items: center; gap: 2px; min-width: 120px; justify-content: flex-end; }
  .ago { font-size: 11.5px; color: var(--faint); margin-right: 6px; font-variant-numeric: tabular-nums; }

  .toolbar { display: flex; align-items: center; gap: 8px; padding: 10px 16px; background: var(--surface); border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .seg { display: inline-flex; padding: 2px; background: var(--surface-2); border-radius: 8px; }
  .seg button { border: 0; background: transparent; height: 26px; padding: 0 10px; border-radius: 6px; color: var(--muted); font-weight: 500; font-size: 12.5px; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgb(0 0 0 / 0.12); }
  .chip { display: inline-flex; align-items: center; gap: 6px; height: 30px; padding: 0 8px 0 10px; border-radius: 8px; border: 1px solid var(--border-strong); background: transparent; color: var(--text-2); font-size: 12.5px; font-weight: 500; }
  .chip:hover { background: var(--surface-2); }
  .chip.on { border-color: var(--accent); background: var(--accent-soft); color: var(--accent); }
  .chip kbd { height: 16px; min-width: 16px; font-size: 10px; }
  .cdot { width: 7px; height: 7px; border-radius: 50%; }
  .cdot.green { background: var(--tone-green); }
  .spacer { flex: 1; }
  .sort { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12px; }
  .sort select { height: 28px; border-radius: 7px; border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); padding: 0 6px; font: inherit; }
  .stats { display: flex; gap: 12px; color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; padding-left: 6px; }
  .stats b { color: var(--text); font-weight: 650; }
  .stats .g b { color: var(--tone-green); }
  .stats .w b { color: var(--warn); }

  .content { display: grid; grid-template-columns: minmax(0, 1fr) clamp(340px, 36vw, 460px); min-height: 0; }
  .list { overflow-y: auto; padding: 6px 0 24px; outline: none; }
  .list:focus-visible { box-shadow: inset 0 0 0 2px var(--ring); border-radius: 0; }
  .group { position: sticky; top: -6px; z-index: 2; display: flex; align-items: baseline; gap: 8px; padding: 14px 22px 6px; background: color-mix(in srgb, var(--bg) 88%, transparent); backdrop-filter: blur(8px); }
  .gt { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; color: var(--text-2); }
  .gc { font-size: 11px; color: var(--muted); background: var(--surface-2); padding: 0 6px; border-radius: 999px; font-variant-numeric: tabular-nums; }
  .gh { font-size: 11.5px; color: var(--faint); }
  .foot-note { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; color: var(--faint); font-size: 12px; padding: 16px 22px 0; margin: 0; }
  .nd { margin-left: 8px; }

  .state { max-width: 380px; margin: 12vh auto 0; text-align: center; color: var(--muted); display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 0 20px; }
  .state h3 { color: var(--text); margin: 14px 0 2px; font-size: 15px; }
  .state p { margin: 0 0 10px; }
  .state .muted { color: var(--faint); }
  .state-icon { width: 60px; height: 60px; border-radius: 18px; display: grid; place-items: center; background: var(--surface-2); color: var(--muted); }
  .state-icon.ok { background: var(--ok-soft); color: var(--ok); }
  .state-icon.err { background: var(--danger-soft); color: var(--danger); }
  .state-icon.warn { background: var(--warn-soft); color: var(--warn); }
  .state code { font-size: 11.5px; }

  .skeletons { padding: 14px 8px; display: grid; gap: 6px; }
  .sk-row { display: grid; grid-template-columns: 60px 1fr 90px; gap: 16px; padding: 12px; align-items: center; animation: pulse 1.4s ease-in-out infinite; }
  .sk { height: 12px; border-radius: 6px; background: var(--surface-3); }
  .sk.a { height: 16px; }
  .sk.b { width: 70%; }
  @keyframes pulse { 50% { opacity: 0.45; } }

  @media (max-width: 860px) {
    .content { grid-template-columns: 1fr; grid-template-rows: minmax(0, 1fr) minmax(0, 45%); }
    .stats { display: none; }
  }
</style>

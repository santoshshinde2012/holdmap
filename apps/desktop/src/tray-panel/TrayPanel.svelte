<script lang="ts">
  // The tray popup panel: live readings, the agents' CPU history, an attention history and the
  // lists a system monitor's popover shows, for holdmap's own data. Every row runs a tray action
  // id from the data; stops still open the reviewed plan in the main window.
  import { onDestroy, onMount, tick } from "svelte";
  import { applyTheme, savedTheme, THEMES } from "./theme";
  import { prefSet } from "../lib/prefs";
  import { humanBytes } from "../lib/format";
  import {
    attentionCells, attentionSummary, chartPaths, chartTop, churnBars, churnSummary, cpuParts, cpuSummary, cpuText, ringArcs, URGENCY_LABEL,
    type PanelData,
  } from "../lib/tray-panel";

  let { load, run, poll = 1000 }: { load: () => Promise<PanelData>; run: (action: string) => Promise<void> | void; poll?: number } = $props();

  let data = $state<PanelData | null>(null);
  let failed = $state(false);
  let readings: HTMLDivElement;
  let refreshing = false;
  let settingsOpen = $state(false);
  let busy = $state(false);
  let actionError = $state("");
  let theme = $state(savedTheme());
  async function changeSetting(action: string) {
    if (busy) return;
    busy = true;
    actionError = "";
    try { await run(action); await refresh(); }
    catch { actionError = action === "refresh" ? "Couldn’t refresh. Please try again." : "Couldn’t save that setting. Please try again."; }
    finally { busy = false; }
  }
  async function chooseTheme(next: typeof theme) {
    await changeSetting(`theme:${next}`);
    if (actionError) return;
    theme = next;
    prefSet("theme", next);
    applyTheme(next);
  }
  let timer: ReturnType<typeof setInterval> | undefined;

  async function refresh() {
    if (document.visibilityState === "hidden" || refreshing) return;
    refreshing = true;
    try {
      data = await load();
      failed = false;
    } catch {
      failed = true;
    } finally { refreshing = false; }
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !e.isComposing) {
      if (settingsOpen) { settingsOpen = false; document.getElementById("quick-settings-toggle")?.focus(); }
      else void run("hide-panel");
    }
  }
  onMount(() => {
    void refresh();
    timer = setInterval(refresh, poll);
    document.addEventListener("visibilitychange", refresh);
  });
  onDestroy(() => {
    clearInterval(timer);
    document.removeEventListener("visibilitychange", refresh);
  });

  const CHART_W = 328;
  const CHART_H = 84;
  const paths = $derived(data ? chartPaths(data.cpuHistory, CHART_W, CHART_H) : { area: "", line: "" });
  const top = $derived(data ? chartTop(data.cpuHistory) : 0);
  const cells = $derived(attentionCells(data?.attentionHistory ?? []));
  const cpu = $derived(cpuParts(data?.cpu ?? null));
  const count = (n: number | null | undefined) => (n === null || n === undefined ? "…" : String(n));

  // The ring: listeners on this computer only, and those open to the network.
  const RING_R = 18;
  const RING_C = Number((2 * Math.PI * RING_R).toFixed(2));
  const arcs = $derived(data?.ready ? ringArcs(data.ports - data.exposed, data.exposed, RING_C) : null);
  // Port changes: opened above the line, closed below, like a network chart.
  const CHURN_H = 48;
  const churn = $derived(churnBars(data?.churn ?? [], CHART_W, CHURN_H / 2 - 2));

  // A click on a menu-bar item opens the panel at that item's section, once per click.
  let shownSeq = 0;
  $effect(() => {
    if (!data?.focus || data.focusSeq === shownSeq) return;
    settingsOpen = false;
    shownSeq = data.focusSeq;
    const section = data.focus;
    // scrollIntoView also scrolls the document (even with overflow:hidden), which moves the
    // entire popup upward in WKWebView. Only the readings viewport may move. Wait for a
    // settings-to-overview switch to finish before measuring its section.
    void tick().then(() => {
      const target = Array.from(readings.querySelectorAll<HTMLElement>("[data-section]"))
        .find(el => el.dataset.section === section);
      if (target) readings.scrollTop += target.getBoundingClientRect().top - readings.getBoundingClientRect().top;
    });
  });
  async function pin() {
    await run("pin-panel");
    await refresh();
  }
</script>

<svelte:window onkeydown={onKey} onfocus={() => theme = savedTheme()} onstorage={() => theme = savedTheme()} />

<main class="panel" aria-label="holdmap at a glance">
  <header class="head">
    <img class="logo" src="/icon.svg" alt="" width="18" height="18" />
    <h1>holdmap</h1>
    <span class="tools">
      <button type="button" class="icon-btn" class:on={data?.pinned} aria-label="Keep open" aria-pressed={!!data?.pinned} title={data?.pinned ? "Kept open: click to let it close" : "Keep open"} onclick={pin}>
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M9 4h6l-1 6 4 4H6l4-4zM12 14v6" fill={data?.pinned ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>
      </button>
      <button type="button" class="icon-btn" aria-label="Open holdmap" title="Open holdmap" onclick={() => run("show")}>
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M14 4h6v6M20 4l-8 8M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>
      </button>
    </span>
  </header>

  <div class="body" bind:this={readings} hidden={settingsOpen}>
    {#if failed && data}<p class="empty" role="status">Readings may be out of date. Reconnecting…</p>{/if}
    {#if failed && !data}
      <p class="empty">Couldn't read the latest scan. It will try again in a moment.</p>
    {/if}

    <section class="hero" aria-label="Now" data-section="top">
      <div class="figure">
        <span class="value">{count(data?.agents)}</span>
        <span class="legend"><i class="dot violet"></i>Agents</span>
      </div>
      <div class="figure">
        <span class="value">{cpu.value}<small>{cpu.unit}</small></span>
        <span class="legend"><i class="dot blue"></i>Agent CPU</span>
      </div>
      <div class="figure">
        <span class="value">{data?.ready ? data.dev.length : "…"}</span>
        <span class="legend"><i class="dot green"></i>Dev servers</span>
      </div>
    </section>

    <h2 class="rule" data-section="cpu">Agent CPU history</h2>
    <figure class="chart" aria-label={cpuSummary(data?.cpuHistory ?? [])} role="img">
      {#if paths.line}
        <svg viewBox="0 0 {CHART_W} {CHART_H}" preserveAspectRatio="none" aria-hidden="true">
          <path class="area" d={paths.area} />
          <path class="line" d={paths.line} />
        </svg>
        <span class="scale top">{top}%</span>
        <span class="scale bottom">0%</span>
      {:else}
        <span class="waiting">Fills in as holdmap scans</span>
      {/if}
    </figure>

    <h2 class="rule">Needs you history</h2>
    <div class="grid" role="img" aria-label={attentionSummary(data?.attentionHistory ?? [])}>
      {#each cells as tone, i (i)}<i class="cell {tone}"></i>{/each}
    </div>

    <h2 class="rule" data-section="needs">Needs you</h2>
    {#if !data?.needs}
      <p class="quiet">Checking agents and ports…</p>
    {:else if data.needs.length === 0}
      <div class="kv"><span>Status</span><span class="pill ok">All clear</span></div>
    {:else}
      <ul class="rows">
        {#each data.needs as need (need.action)}
          <li>
            <button type="button" class="row" onclick={() => run(need.action)}>
              <span class="text" title={need.title}>{need.title}</span>
              <span class="pill {need.urgency}">{URGENCY_LABEL[need.urgency]}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    <h2 class="rule">Ports</h2>
    <div class="ports">
      <svg class="ring" viewBox="0 0 48 48" width="48" height="48" role="img" aria-label={data?.ready ? `${data.ports - data.exposed} on this computer only, ${data.exposed} open to the network` : "Ports not read yet"}>
        <circle cx="24" cy="24" r={RING_R} class="track" />
        {#if arcs}
          <circle cx="24" cy="24" r={RING_R} class="local" stroke-dasharray={arcs.local} transform="rotate(-90 24 24)" />
          <circle cx="24" cy="24" r={RING_R} class="network" stroke-dasharray={arcs.network} stroke-dashoffset={-Number(arcs.local.split(" ")[0])} transform="rotate(-90 24 24)" />
        {/if}
      </svg>
      <div class="ports-rows">
        <div class="kv"><span><i class="dot local"></i>In use</span><span class="num">{data?.ready ? data.ports : "…"}</span></div>
        <div class="kv">
          <span><i class="dot network"></i>Open to the network</span>
          {#if data?.ready}
            <span class="pill {data.exposed ? 'medium' : 'ok'}">{data.exposed ? data.exposed : "None"}</span>
          {:else}<span class="num">…</span>{/if}
        </div>
      </div>
    </div>
    <div class="kv"><span>Agent memory</span><span class="num">{data?.memory === null || data?.memory === undefined ? "…" : humanBytes(data.memory)}</span></div>

    <h2 class="rule">Port changes</h2>
    <figure class="chart churn" role="img" aria-label={churnSummary(data?.churn ?? [])}>
      {#if churn.length}
        <svg viewBox="0 0 {CHART_W} {CHURN_H}" preserveAspectRatio="none" aria-hidden="true">
          <line x1="0" x2={CHART_W} y1={CHURN_H / 2} y2={CHURN_H / 2} class="axis" />
          {#each churn as bar, i (i)}
            {#if bar.up}<rect x={bar.x} y={CHURN_H / 2 - bar.up} width={bar.w} height={bar.up} class="opened" />{/if}
            {#if bar.down}<rect x={bar.x} y={CHURN_H / 2} width={bar.w} height={bar.down} class="closed" />{/if}
          {/each}
        </svg>
        <span class="scale top">opened</span>
        <span class="scale bottom">closed</span>
      {:else}
        <span class="waiting">Fills in as holdmap scans</span>
      {/if}
    </figure>

    <div class="rule-row">
      <h2 class="rule" data-section="dev">Dev servers</h2>
      {#if data?.stopDev}
        <button type="button" class="link" onclick={() => run("stop-all-dev")}>Stop all…</button>
      {/if}
    </div>
    {#if data?.ready && data.dev.length === 0}
      <p class="quiet">No dev servers running.</p>
    {:else if data}
      <ul class="rows">
        {#each data.dev as dev (dev.port)}
          <li>
            <button type="button" class="row" onclick={() => run(dev.action)}>
              <span class="port num">:{dev.port}</span>
              <span class="text">{dev.label}</span>
              <span class="pill {dev.exposed ? 'medium' : 'gray'}">{dev.exposed ? "Network" : "Local"}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    <h2 class="rule" data-section="agents">Top agents</h2>
    {#if data?.agents === 0}
      <p class="quiet">No agents running.</p>
    {:else if data?.topAgents.length}
      <div class="table">
        <div class="thead" aria-hidden="true">
          <span>Agent</span>
          <span class="c"><i class="dot blue"></i>CPU</span>
          <span class="c"><i class="dot violet"></i>Memory</span>
        </div>
        <ul class="rows">
          {#each data.topAgents as agent (agent.action)}
            {@const usage = agent.cpu === null ? "…" : cpuText(agent.cpu)}
            <li>
              <button type="button" class="trow" onclick={() => run(agent.action)}>
                <span class="name"><b>{agent.name}</b>{#if agent.place}{" "}<span class="place">· {agent.place}</span>{/if}</span>
                <span class="c num"><span class="sr-only">CPU</span>{" "}{usage}</span>
                <span class="c num"><span class="sr-only">memory</span>{" "}{humanBytes(agent.memory)}</span>
              </button>
            </li>
          {/each}
        </ul>
        {#if data.moreAgents}
          <button type="button" class="link more" onclick={() => run("show")}>+{data.moreAgents} more in holdmap</button>
        {/if}
      </div>
    {:else}
      <p class="quiet">Looking for agents…</p>
    {/if}
  </div>

  {#if settingsOpen}
    <section class="body settings" aria-label="Quick settings">
      <div class="settings-heading"><h2>Quick settings</h2><button class="link" type="button" onclick={() => run("settings")}>All settings ↗</button></div>
      <p class="quiet">Make this space work for you.</p>
      <h3 class="rule">Appearance</h3>
      <div class="theme-options" role="group" aria-label="Theme">
        {#each THEMES as choice}
          <button type="button" aria-pressed={theme === choice} disabled={busy} onclick={() => chooseTheme(choice)}>{choice === "system" ? "System" : choice === "light" ? "Light" : "Dark"}</button>
        {/each}
      </div>
      {#if data?.settings}
        {@const settings = data.settings}
        {#if settings.bar}
          <h3 class="rule">{settings.bar} widgets</h3>
          {#each settings.items as item (item.action)}
            <button type="button" class="setting-row" role="switch" aria-checked={item.on} disabled={busy} onclick={() => changeSetting(item.action)}><span>{item.name}</span><span class="switch" class:checked={item.on} aria-hidden="true"></span></button>
          {/each}
          {#if settings.combined !== null}
            <button type="button" class="setting-row" role="switch" aria-checked={settings.combined} disabled={busy} onclick={() => changeSetting("combined")}><span>Combine into one item</span><span class="switch" class:checked={settings.combined} aria-hidden="true"></span></button>
          {/if}
        {/if}
        <h3 class="rule">Preferences</h3>
        {#if settings.dock !== null}
          <button type="button" class="setting-row" role="switch" aria-checked={settings.dock} disabled={busy} onclick={() => changeSetting("dock")}><span>Show in the Dock</span><span class="switch" class:checked={settings.dock} aria-hidden="true"></span></button>
        {/if}
        <button type="button" class="setting-row" role="switch" aria-checked={settings.notify} disabled={busy} onclick={() => changeSetting("notify")}><span>Desktop notifications</span><span class="switch" class:checked={settings.notify} aria-hidden="true"></span></button>
      {/if}
    </section>
  {/if}

  <footer class="foot" aria-label="Panel actions">
    {#if actionError}<p class="action-error" role="alert">{actionError}</p>{/if}
    <button type="button" class="btn primary" onclick={() => run("show")}>Open holdmap <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path d="M5 12h14m-5-5 5 5-5 5" /></svg></button>
    <span class="footer-divider" aria-hidden="true"></span>
    <button id="quick-settings-toggle" type="button" class="icon-btn" class:on={settingsOpen} aria-label={settingsOpen ? "Back to overview" : "Quick settings"} title={settingsOpen ? "Back to overview" : "Quick settings"} aria-expanded={settingsOpen} onclick={() => settingsOpen = !settingsOpen}>
      <svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true"><path d="M4 7h16M4 17h16M8 4v6M16 14v6" /></svg>
    </button>
    <button type="button" class="icon-btn" aria-label="Refresh" title="Refresh readings" onclick={() => changeSetting("refresh")} disabled={busy}>
      <svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true"><path d="M20 7v5h-5M4 17v-5h5M6 7a7 7 0 0 1 12-1l2 3M4 15l2 3a7 7 0 0 0 12-1" /></svg>
    </button>
    <button type="button" class="icon-btn quit" aria-label="Quit" title="Quit holdmap" onclick={() => run("quit")}>
      <svg viewBox="0 0 24 24" width="17" height="17" aria-hidden="true"><path d="M12 3v9M7 5.5a8 8 0 1 0 10 0" /></svg>
    </button>
  </footer>
</main>

<style>
  .panel { position: fixed; inset: 0; display: grid; grid-template-rows: auto minmax(0, 1fr) auto; overflow: hidden; background: var(--surface); color: var(--text); font-variant-numeric: tabular-nums; }
  .head { display: grid; grid-template-columns: 60px 1fr 60px; align-items: center; padding: 26px 16px 10px; border-bottom: 1px solid var(--border); -webkit-user-select: none; user-select: none; }
  .tools { display: flex; justify-content: flex-end; gap: 4px; }
  .icon-btn.on { color: var(--accent); background: var(--accent-soft); }
  .ports { display: grid; grid-template-columns: 48px 1fr; gap: 12px; align-items: center; }
  .ring .track, .ring .local, .ring .network { fill: none; stroke-width: 7; }
  .ring .track { stroke: var(--surface-3); }
  .ring .local { stroke: var(--tone-blue); }
  .ring .network { stroke: var(--tone-amber); }
  .dot.local { background: var(--tone-blue); margin-right: 6px; }
  .dot.network { background: var(--tone-amber); margin-right: 6px; }
  .ports-rows .kv span:first-child { display: inline-flex; align-items: center; }
  .chart.churn { height: 60px; }
  .chart.churn svg { height: 48px; }
  .churn .axis { stroke: var(--border-strong); stroke-width: 1; vector-effect: non-scaling-stroke; }
  .churn .opened { fill: var(--tone-green); }
  .churn .closed { fill: var(--tone-amber); }
  .logo { border-radius: 4px; }
  h1 { margin: 0; text-align: center; font-size: var(--fs-title); line-height: var(--lh-title); font-weight: var(--fw-semibold); letter-spacing: var(--ls-title); }
  .body { min-height: 0; overscroll-behavior: contain; overflow-y: auto; padding: 4px 16px 12px; }
  .hero { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; padding: 10px 0 4px; text-align: center; }
  .figure { border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface-2); padding: 10px 2px; display: flex; flex-direction: column; align-items: center; gap: 4px; }
  .value { display: flex; align-items: baseline; justify-content: center; height: 32px; font-size: var(--fs-display); line-height: var(--lh-display); font-weight: var(--fw-regular); letter-spacing: var(--ls-display); }
  .value small { font-size: var(--fs-heading); font-weight: var(--fw-regular); margin-left: 2px; color: var(--text-2); }
  .legend { display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-body-sm); color: var(--text-2); }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 3px; }
  .dot.violet { background: var(--accent); }
  .dot.blue { background: var(--tone-blue); }
  .dot.green { background: var(--tone-green); }
  .rule { display: flex; align-items: center; gap: 10px; margin: 14px 0 8px; font-size: var(--fs-label); line-height: var(--lh-label); font-weight: var(--fw-medium); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--faint); }
  .rule::before, .rule::after { content: ""; flex: 1; height: 1px; background: var(--border-strong); }
  .rule-row { display: flex; align-items: center; gap: 8px; }
  .rule-row .rule { flex: 1; }
  .chart { position: relative; margin: 0; height: 96px; padding: 6px 0; border-radius: var(--r-md); background: var(--surface-2); overflow: hidden; }
  .chart svg { display: block; width: 100%; height: 84px; }
  .chart .area { fill: var(--tone-blue-bg); }
  .chart .line { fill: none; stroke: var(--tone-blue); stroke-width: 1.5; vector-effect: non-scaling-stroke; }
  .scale { position: absolute; left: 8px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .scale.top { top: 4px; }
  .scale.bottom { bottom: 4px; }
  .waiting { display: grid; place-items: center; height: 100%; font-size: var(--fs-body-sm); color: var(--muted); }
  .grid { display: grid; grid-template-columns: repeat(30, 1fr); gap: 3px; }
  .cell { display: block; aspect-ratio: 1; border-radius: 2px; background: var(--surface-3); }
  .cell.ok { background: var(--tone-green); }
  .cell.medium { background: var(--tone-amber); }
  .cell.high { background: var(--danger); }
  .rows { list-style: none; margin: 0; padding: 0; }
  .row, .trow { display: flex; align-items: center; gap: 8px; min-height: 30px; padding: 4px 6px; margin: 0 -6px; width: calc(100% + 12px); border: 0; border-radius: var(--r-sm); background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .row:hover, .trow:hover { background: var(--row-hover); }
  .row:focus-visible, .trow:focus-visible, .link:focus-visible, .btn:focus-visible, .icon-btn:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .port { color: var(--text-2); }
  .kv { display: flex; align-items: center; justify-content: space-between; min-height: 30px; color: var(--text-2); }
  .kv .num { color: var(--text); font-size: var(--fs-heading); }
  .pill { flex: none; padding: 2px 8px; border-radius: var(--r-sm); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); letter-spacing: var(--ls-caption); }
  .pill.ok { background: var(--tone-green); color: var(--surface); }
  .pill.medium { background: var(--tone-amber-bg); color: var(--tone-amber); }
  .pill.high { background: var(--danger); color: var(--danger-fg); }
  .pill.gray { background: var(--tone-gray-bg); color: var(--tone-gray); }
  .quiet, .empty { margin: 4px 0; font-size: var(--fs-body-sm); color: var(--muted); }
  .table { display: flex; flex-direction: column; }
  .thead, .trow { display: grid; grid-template-columns: 1fr 64px 76px; align-items: center; gap: 8px; }
  .thead { padding: 0 0 4px; font-size: var(--fs-body-sm); color: var(--muted); }
  .c { display: inline-flex; align-items: center; justify-content: flex-end; gap: 6px; text-align: right; }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name b { font-weight: var(--fw-medium); }
  .place { color: var(--muted); }
  .link { border: 0; background: none; padding: 2px 4px; color: var(--accent); font: inherit; font-size: var(--fs-body-sm); cursor: pointer; border-radius: var(--r-xs); }
  .link.more { align-self: flex-start; margin-top: 4px; padding-left: 0; }
  .icon-btn { display: grid; place-items: center; width: 28px; height: 28px; border: 0; border-radius: var(--r-sm); background: none; color: var(--text-2); cursor: pointer; }
  .icon-btn:hover { background: var(--row-hover); }
  .foot { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 10px 16px 12px; border-top: 1px solid var(--border); background: var(--surface); }
  .action-error { flex-basis: 100%; margin: 0 0 4px; font-size: var(--fs-body-sm); color: var(--danger); }
  .footer-divider { width: 1px; height: 20px; margin: 0 2px; background: var(--border); }
  .foot .icon-btn { width: 30px; height: 32px; flex: none; }
  .foot svg { fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
  .quit:hover { color: var(--danger); background: var(--danger-soft, var(--surface-2)); }
  .settings-heading { display: flex; align-items: center; justify-content: space-between; margin-top: 12px; }
  .settings-heading h2 { margin: 0; font-size: var(--fs-heading); }
  .theme-options { display: flex; gap: 4px; padding: 4px; background: var(--surface-2); border-radius: var(--r-md); }
  .theme-options button { flex: 1; padding: 6px; border: 1px solid transparent; border-radius: var(--r-sm); background: none; cursor: pointer; }
  .theme-options button[aria-pressed="true"] { background: var(--surface); border-color: var(--border-strong); color: var(--accent); }
  .setting-row { display: flex; align-items: center; justify-content: space-between; width: 100%; min-height: 36px; padding: 6px 4px; border: 0; border-radius: var(--r-sm); background: none; cursor: pointer; }
  .setting-row:hover { background: var(--row-hover); }
  .switch { width: 28px; height: 16px; padding: 2px; border-radius: 12px; background: var(--muted); }
  .switch::after { content: ""; display: block; width: 12px; height: 12px; border-radius: 50%; background: var(--surface); }
  .switch.checked { background: var(--accent); }
  .switch.checked::after { transform: translateX(12px); }
  button:disabled { cursor: wait; opacity: .6; }
  .settings button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .body[hidden] { display: none; }
  .btn { display: inline-flex; align-items: center; justify-content: center; gap: 8px; flex: 1; height: 32px; border: 1px solid var(--input-border); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; font-weight: var(--fw-medium); cursor: pointer; }
  .btn:hover { border-color: var(--input-border-hover); }
  .btn.primary { flex: 2; border-color: transparent; background: var(--accent); color: var(--accent-fg); }
  .btn.primary:hover { background: var(--accent-hover); }
</style>

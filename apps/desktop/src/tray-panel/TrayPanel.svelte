<script lang="ts">
  // The tray popup panel: live readings, the agents' CPU history, an attention history and the
  // lists a system monitor's popover shows, for holdmap's own data. Every row runs a tray action
  // id from the data; stops still open the reviewed plan in the main window.
  import { onDestroy, onMount } from "svelte";
  import { humanBytes } from "../lib/format";
  import {
    attentionCells, attentionSummary, chartPaths, chartTop, cpuParts, cpuSummary, cpuText, URGENCY_LABEL,
    type PanelData,
  } from "../lib/tray-panel";

  let { load, run, poll = 1000 }: { load: () => Promise<PanelData>; run: (action: string) => Promise<void> | void; poll?: number } = $props();

  let data = $state<PanelData | null>(null);
  let failed = $state(false);
  let timer: ReturnType<typeof setInterval> | undefined;

  async function refresh() {
    if (document.visibilityState === "hidden") return;
    try {
      data = await load();
      failed = false;
    } catch {
      failed = true;
    }
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !e.isComposing) void run("hide-panel");
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
</script>

<svelte:window onkeydown={onKey} />

<main class="panel" aria-label="holdmap at a glance">
  <header class="head">
    <img class="logo" src="/icon.svg" alt="" width="18" height="18" />
    <h1>holdmap</h1>
    <button type="button" class="icon-btn" aria-label="Open holdmap" title="Open holdmap" onclick={() => run("show")}>
      <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M14 4h6v6M20 4l-8 8M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>
    </button>
  </header>

  <div class="body">
    {#if failed && !data}
      <p class="empty">Couldn't read the latest scan. It will try again in a moment.</p>
    {/if}

    <section class="hero" aria-label="Now">
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

    <h2 class="rule">Agent CPU history</h2>
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

    <h2 class="rule">Needs you</h2>
    {#if !data?.needs}
      <p class="quiet">Checking agents and ports…</p>
    {:else if data.needs.length === 0}
      <div class="kv"><span>Status</span><span class="pill ok">All clear</span></div>
    {:else}
      <ul class="rows">
        {#each data.needs as need (need.action)}
          <li>
            <button type="button" class="row" onclick={() => run(need.action)}>
              <span class="text">{need.title}</span>
              <span class="pill {need.urgency}">{URGENCY_LABEL[need.urgency]}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    <h2 class="rule">Ports</h2>
    <div class="kv"><span>In use</span><span class="num">{data?.ready ? data.ports : "…"}</span></div>
    <div class="kv">
      <span>Open to the network</span>
      {#if data?.ready}
        <span class="pill {data.exposed ? 'medium' : 'ok'}">{data.exposed ? data.exposed : "None"}</span>
      {:else}<span class="num">…</span>{/if}
    </div>
    <div class="kv"><span>Agent memory</span><span class="num">{data?.memory === null || data?.memory === undefined ? "…" : humanBytes(data.memory)}</span></div>

    <div class="rule-row">
      <h2 class="rule">Dev servers</h2>
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

    <h2 class="rule">Top agents</h2>
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

  <footer class="foot">
    <button type="button" class="btn primary" onclick={() => run("show")}>Open holdmap</button>
    <button type="button" class="btn" onclick={() => run("quit")}>Quit</button>
  </footer>
</main>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--surface); color: var(--text); font-variant-numeric: tabular-nums; }
  .head { display: grid; grid-template-columns: 28px 1fr 28px; align-items: center; padding: 28px 16px 8px; -webkit-user-select: none; user-select: none; }
  .logo { border-radius: 4px; }
  h1 { margin: 0; text-align: center; font-size: var(--fs-title); line-height: var(--lh-title); font-weight: var(--fw-semibold); letter-spacing: var(--ls-title); }
  .body { flex: 1; overflow-y: auto; padding: 4px 16px 12px; }
  .hero { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; padding: 12px 0 16px; text-align: center; }
  .figure { display: flex; flex-direction: column; align-items: center; gap: 4px; }
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
  .foot { display: flex; gap: 8px; padding: 10px 16px 14px; border-top: 1px solid var(--border); }
  .btn { flex: 1; height: 30px; border: 1px solid var(--input-border); border-radius: var(--r-md); background: var(--surface); color: var(--text); font: inherit; font-weight: var(--fw-medium); cursor: pointer; }
  .btn:hover { border-color: var(--input-border-hover); }
  .btn.primary { flex: 2; border-color: transparent; background: var(--accent); color: var(--accent-fg); }
  .btn.primary:hover { background: var(--accent-hover); }
</style>

<script lang="ts">
  // The tray popup panel: a calm summary first (agents, their RAM and CPU), then what needs you,
  // dev servers and the agents using the most RAM, with history at the bottom. Every row runs a
  // tray action id from the data; stops still open the reviewed plan in the main window.
  import { onDestroy, onMount, tick } from "svelte";
  import { applyTheme, savedTheme, THEMES } from "./theme";
  import { prefSet } from "../lib/prefs";
  import { humanBytes } from "../lib/format";
  import {
    agentLabel, agentSummary, attentionCells, attentionSummary, bytesParts, chartPaths, chartTop, churnBars, churnSummary,
    cpuParts, cpuSummary, cpuText, needsStatus, ringArcs, sparkPath, URGENCY_LABEL,
    type PanelData,
  } from "../lib/tray-panel";
  // Compact always shows the icon, and alerts as its dot, so those two are not choices there.
  const COMPACT_FIXED = new Set(["item:icon", "item:needs"]);

  let { load, run, poll = 1000 }: { load: () => Promise<PanelData>; run: (action: string) => Promise<void> | void; poll?: number } = $props();

  let data = $state<PanelData | null>(null);
  let failed = $state(false);
  let readings: HTMLDivElement;
  let refreshing = false;
  let lastJson = "";
  let settingsOpen = $state(false);
  let busy = $state(false);
  let actionError = $state("");
  let theme = $state(savedTheme());
  // Edge fades only where there is more to scroll to, so nothing reads as cut off.
  let fadeTop = $state(false);
  let fadeBottom = $state(false);

  async function changeSetting(action: string) {
    if (busy) return;
    busy = true;
    actionError = "";
    try { await run(action); await refresh(true); }
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

  async function refresh(force = false) {
    if (refreshing || (!force && document.visibilityState === "hidden")) return;
    refreshing = true;
    try {
      const next = await load();
      // Same reading as last time: keep the old object, so nothing re-renders or flickers.
      const json = JSON.stringify(next);
      if (force || json !== lastJson) { data = next; lastJson = json; }
      failed = false;
    } catch {
      failed = true;
    } finally { refreshing = false; }
  }
  const onVisible = () => void refresh();
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !e.isComposing) {
      if (settingsOpen) { settingsOpen = false; document.getElementById("quick-settings-toggle")?.focus(); }
      else void run("hide-panel");
    }
  }
  function measure() {
    if (!readings) return;
    fadeTop = readings.scrollTop > 2;
    fadeBottom = readings.scrollTop + readings.clientHeight < readings.scrollHeight - 2;
  }
  onMount(() => {
    void refresh();
    timer = setInterval(() => void refresh(), poll);
    document.addEventListener("visibilitychange", onVisible);
  });
  onDestroy(() => {
    clearInterval(timer);
    document.removeEventListener("visibilitychange", onVisible);
  });
  $effect(() => { void data; void settingsOpen; void tick().then(measure); });

  const CHART_W = 328;
  const CHART_H = 64;
  const SPARK_W = 64;
  const SPARK_H = 14;
  const paths = $derived(data ? chartPaths(data.cpuHistory, CHART_W, CHART_H) : { area: "", line: "" });
  const top = $derived(data ? chartTop(data.cpuHistory) : 0);
  const spark = $derived(sparkPath(data?.cpuHistory ?? [], SPARK_W, SPARK_H));
  const cells = $derived(attentionCells(data?.attentionHistory ?? []));
  const cpu = $derived(cpuParts(data?.cpu ?? null));
  const ram = $derived(bytesParts(data?.memory ?? null));
  const status = $derived(needsStatus(data?.needs ?? null));
  const tools = $derived(data?.tools ?? []);
  const count = (n: number | null | undefined) => (n === null || n === undefined ? "…" : String(n));

  // The ring: listeners on this computer only, and those open to the network.
  const RING_R = 15;
  const RING_C = Number((2 * Math.PI * RING_R).toFixed(2));
  const arcs = $derived(data?.ready ? ringArcs(data.ports - data.exposed, data.exposed, RING_C) : null);
  // Port changes: opened above the line, closed below, like a network chart.
  const CHURN_H = 40;
  const churn = $derived(churnBars(data?.churn ?? [], CHART_W, CHURN_H / 2 - 2));

  // A click on a menu-bar item opens the panel at that item's section, once per click.
  let shownSeq = 0;
  $effect(() => {
    if (!data?.focus || data.focusSeq === shownSeq) return;
    settingsOpen = false;
    shownSeq = data.focusSeq;
    const section = data.focus;
    // scrollIntoView also scrolls the document (even with overflow:hidden), which moves the
    // entire popup upward in WKWebView. Only the readings viewport may move, and the section's
    // heading lands at the top with a little room above it.
    void tick().then(() => {
      const target = Array.from(readings.querySelectorAll<HTMLElement>("[data-section]"))
        .find(el => el.dataset.section === section);
      if (!target) return;
      const offset = target.getBoundingClientRect().top - readings.getBoundingClientRect().top;
      readings.scrollTop = section === "top" ? 0 : Math.max(0, readings.scrollTop + offset - 8);
      measure();
    });
  });
  async function pin() {
    await run("pin-panel");
    await refresh(true);
  }
</script>

<svelte:window onkeydown={onKey} onfocus={() => theme = savedTheme()} onstorage={() => theme = savedTheme()} />

<main class="panel" aria-label="holdmap at a glance">
  <header class="head">
    <img class="logo" src="/icon.svg" alt="" width="20" height="20" />
    <h1>holdmap</h1>
    <span class="status {status.tone}" role="status" title={status.detail}><i class="status-dot" aria-hidden="true"></i>{status.title}</span>
    <span class="tools">
      <button type="button" class="icon-btn" class:on={data?.pinned} aria-label="Keep open" aria-pressed={!!data?.pinned} title={data?.pinned ? "Kept open: click to let it close" : "Keep open"} onclick={pin}>
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M9 4h6l-1 6 4 4H6l4-4zM12 14v6" fill={data?.pinned ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>
      </button>
    </span>
  </header>

  <div class="body" class:fade-top={fadeTop} class:fade-bottom={fadeBottom} bind:this={readings} onscroll={measure} hidden={settingsOpen}>
    {#if failed && data}<p class="notice" role="status">Readings may be out of date. Reconnecting…</p>{/if}
    {#if failed && !data}
      <p class="notice">Couldn't read the latest scan. It will try again in a moment.</p>
    {/if}

    <section class="hero" aria-label="Now" data-section="top">
      <div class="tile">
        <span class="label"><i class="dot violet" aria-hidden="true"></i>Agents</span>
        <span class="value">{count(data?.agents)}</span>
        <span class="sub">{tools.length ? `+${tools.length} dev ${tools.length === 1 ? "tool" : "tools"}` : "running"}</span>
      </div>
      <div class="tile">
        <span class="label"><i class="dot green" aria-hidden="true"></i>RAM</span>
        <span class="value">{ram.value}<small>{ram.unit}</small></span>
        <span class="sub">all agents</span>
      </div>
      <div class="tile">
        <span class="label"><i class="dot blue" aria-hidden="true"></i>CPU</span>
        <span class="value">{cpu.value}<small>{cpu.unit}</small></span>
        {#if spark}
          <svg class="spark" viewBox="0 0 {SPARK_W} {SPARK_H}" width={SPARK_W} height={SPARK_H} preserveAspectRatio="none" aria-hidden="true"><path d={spark} /></svg>
        {:else}<span class="sub">of one core</span>{/if}
      </div>
    </section>

    <section class="sec" aria-labelledby="sec-needs" data-section="needs">
      <div class="sec-head">
        <h2 id="sec-needs">Needs you</h2>
        {#if data?.needs?.length}<span class="count {status.tone}">{data.needs.length}</span>{/if}
      </div>
      {#if !data?.needs}
        <p class="quiet">Checking agents and ports…</p>
      {:else if data.needs.length === 0}
        <p class="clear"><svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="m5 12 4.5 4.5L19 7" /></svg><span>All clear. Nothing needs you right now.</span></p>
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
    </section>

    <section class="sec" aria-labelledby="sec-dev" data-section="dev">
      <div class="sec-head">
        <h2 id="sec-dev">Dev servers</h2>
        {#if data?.ready && data.dev.length}<span class="count">{data.dev.length}</span>{/if}
        {#if data?.stopDev}
          <button type="button" class="sec-action" title="Opens holdmap to review what will stop before anything does" onclick={() => run("stop-all-dev")}>Stop all…</button>
        {/if}
      </div>
      {#if data?.ready && data.dev.length === 0}
        <p class="quiet">No dev servers running.</p>
      {:else if data}
        <ul class="rows">
          {#each data.dev as dev (dev.port)}
            <li>
              <button type="button" class="row" title={`:${dev.port} ${dev.label}`} onclick={() => run(dev.action)}>
                <span class="port">:{dev.port}</span>
                <span class="text">{dev.label}</span>
                <span class="pill {dev.exposed ? 'medium' : 'gray'}">{dev.exposed ? "Network" : "Local"}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="sec" aria-labelledby="sec-agents" data-section="agents">
      <div class="sec-head">
        <h2 id="sec-agents">Top agents</h2>
        <span class="hint">by RAM</span>
      </div>
      {#if data?.agents === 0 && !tools.length}
        <p class="quiet">No agents running.</p>
      {:else if data?.topAgents.length || tools.length}
        <div class="table">
          <div class="thead" aria-hidden="true">
            <span>Agent</span>
            <span class="c">CPU</span>
            <span class="c">RAM</span>
          </div>
          <ul class="rows">
            {#each data?.topAgents ?? [] as agent (agent.action)}
              {@const usage = agent.cpu === null ? "…" : cpuText(agent.cpu)}
              <li>
                <button type="button" class="trow" title={agentSummary(agent)} onclick={() => run(agent.action)}>
                  <span class="name"><b>{agent.name}</b>{#if agent.place}{" "}<span class="place">· {agent.place}</span>{/if}</span>
                  <span class="c num"><span class="sr-only">CPU</span>{" "}{usage}</span>
                  <span class="c num strong"><span class="sr-only">RAM</span>{" "}{humanBytes(agent.memory)}</span>
                </button>
              </li>
            {/each}
          </ul>
          {#if data?.moreAgents}
            <button type="button" class="link more" onclick={() => run("show")}>+{data.moreAgents} more in holdmap</button>
          {/if}
          {#if tools.length}
            <p class="tools-label" id="dev-tools">Developer tools <span class="hint">not agents, not counted above</span></p>
            <ul class="rows" aria-labelledby="dev-tools">
              {#each tools as tool (tool.action)}
                {@const usage = tool.cpu === null ? "…" : cpuText(tool.cpu)}
                <li>
                  <button type="button" class="trow tool" title={`${agentSummary(tool)} (developer tool)`} onclick={() => run(tool.action)}>
                    <span class="name">{agentLabel(tool)}</span>
                    <span class="c num"><span class="sr-only">CPU</span>{" "}{usage}</span>
                    <span class="c num"><span class="sr-only">RAM</span>{" "}{humanBytes(tool.memory)}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {:else}
        <p class="quiet">Looking for agents…</p>
      {/if}
    </section>

    <section class="sec" aria-labelledby="sec-activity" data-section="cpu">
      <div class="sec-head"><h2 id="sec-activity">Activity</h2><span class="hint">recent scans</span></div>
      <h3 class="sub-head">Agent CPU</h3>
      <figure class="chart" aria-label={cpuSummary(data?.cpuHistory ?? [])} role="img">
        {#if paths.line}
          <svg viewBox="0 0 {CHART_W} {CHART_H}" preserveAspectRatio="none" aria-hidden="true">
            <path class="area" d={paths.area} />
            <path class="line" d={paths.line} />
          </svg>
          <span class="scale top">{top}%</span>
        {:else}
          <span class="waiting">Fills in as holdmap scans</span>
        {/if}
      </figure>

      <h3 class="sub-head">Needs you</h3>
      <div class="grid" role="img" aria-label={attentionSummary(data?.attentionHistory ?? [])}>
        {#each cells as tone, i (i)}<i class="cell {tone}"></i>{/each}
      </div>

      <h3 class="sub-head">Ports</h3>
      <div class="ports">
        <svg class="ring" viewBox="0 0 40 40" width="40" height="40" role="img" aria-label={data?.ready ? `${data.ports - data.exposed} on this computer only, ${data.exposed} open to the network` : "Ports not read yet"}>
          <circle cx="20" cy="20" r={RING_R} class="track" />
          {#if arcs}
            <circle cx="20" cy="20" r={RING_R} class="local" stroke-dasharray={arcs.local} transform="rotate(-90 20 20)" />
            <circle cx="20" cy="20" r={RING_R} class="network" stroke-dasharray={arcs.network} stroke-dashoffset={-Number(arcs.local.split(" ")[0])} transform="rotate(-90 20 20)" />
          {/if}
        </svg>
        <div class="ports-rows">
          <div class="kv"><span><i class="dot local" aria-hidden="true"></i>In use</span><span class="num">{data?.ready ? data.ports : "…"}</span></div>
          <div class="kv">
            <span><i class="dot network" aria-hidden="true"></i>Open to the network</span>
            {#if data?.ready}
              <span class="pill {data.exposed ? 'medium' : 'ok'}">{data.exposed ? data.exposed : "None"}</span>
            {:else}<span class="num">…</span>{/if}
          </div>
        </div>
      </div>

      <h3 class="sub-head">Port changes</h3>
      <figure class="chart churn" role="img" aria-label={churnSummary(data?.churn ?? [])}>
        {#if churn.length}
          <svg viewBox="0 0 {CHART_W} {CHURN_H}" preserveAspectRatio="none" aria-hidden="true">
            <line x1="0" x2={CHART_W} y1={CHURN_H / 2} y2={CHURN_H / 2} class="axis" />
            {#each churn as bar, i (i)}
              {#if bar.up}<rect x={bar.x} y={CHURN_H / 2 - bar.up} width={bar.w} height={bar.up} class="opened" />{/if}
              {#if bar.down}<rect x={bar.x} y={CHURN_H / 2} width={bar.w} height={bar.down} class="closed" />{/if}
            {/each}
          </svg>
        {:else}
          <span class="waiting">Fills in as holdmap scans</span>
        {/if}
      </figure>
      <p class="legend-row" aria-hidden="true"><span><i class="dot green"></i>opened</span><span><i class="dot network"></i>closed</span></p>
    </section>
  </div>

  {#if settingsOpen}
    <section class="body settings" aria-label="Quick settings">
      <div class="settings-heading"><h2>Quick settings</h2><button class="link" type="button" onclick={() => run("settings")}>All settings ↗</button></div>
      <h3 class="sub-head">Appearance</h3>
      <div class="segmented" role="group" aria-label="Theme">
        {#each THEMES as choice}
          <button type="button" aria-pressed={theme === choice} disabled={busy} onclick={() => chooseTheme(choice)}>{choice === "system" ? "System" : choice === "light" ? "Light" : "Dark"}</button>
        {/each}
      </div>
      {#if data?.settings}
        {@const settings = data.settings}
        {#if settings.bar}
          <h3 class="sub-head">{settings.bar}</h3>
          {#if settings.compact !== null}
            <div class="segmented" role="group" aria-label="Menu bar size">
              <button type="button" aria-pressed={settings.compact} disabled={busy} onclick={() => { if (!settings.compact) void changeSetting("compact"); }}>Compact</button>
              <button type="button" aria-pressed={!settings.compact} disabled={busy} onclick={() => { if (settings.compact) void changeSetting("compact"); }}>Every reading</button>
            </div>
            <p class="quiet">{settings.compact ? "The icon, with a dot when something needs you, and the first two readings you pick." : "Each reading you pick is its own menu-bar item."}</p>
          {/if}
          {#each settings.items.filter((i) => !(settings.compact && COMPACT_FIXED.has(i.action))) as item (item.action)}
            <button type="button" class="setting-row" role="switch" aria-checked={item.on} disabled={busy} onclick={() => changeSetting(item.action)}><span>{item.name}</span><span class="switch" class:checked={item.on} aria-hidden="true"></span></button>
          {/each}
          {#if settings.combined !== null && !settings.compact}
            <button type="button" class="setting-row" role="switch" aria-checked={settings.combined} disabled={busy} onclick={() => changeSetting("combined")}><span>Combine into one item</span><span class="switch" class:checked={settings.combined} aria-hidden="true"></span></button>
          {/if}
        {/if}
        <h3 class="sub-head">Preferences</h3>
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

  /* Header: macOS keeps a transparent title bar over the top 26 px, so the row sits below it. */
  .head { display: flex; align-items: center; gap: 8px; padding: 26px 12px 10px 16px; border-bottom: 1px solid var(--border); -webkit-user-select: none; user-select: none; }
  .logo { border-radius: 5px; flex: none; }
  h1 { margin: 0; font-size: var(--fs-title); line-height: var(--lh-title); font-weight: var(--fw-semibold); letter-spacing: var(--ls-title); }
  .status { display: inline-flex; align-items: center; gap: 6px; min-width: 0; margin-left: 4px; padding: 2px 8px; border-radius: 999px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); letter-spacing: var(--ls-caption); white-space: nowrap; background: var(--tone-gray-bg); color: var(--tone-gray); }
  .status-dot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; flex: none; }
  .status.ok { background: var(--tone-green-bg); color: var(--tone-green); }
  .status.medium { background: var(--tone-amber-bg); color: var(--tone-amber); }
  .status.high { background: var(--danger-soft); color: var(--danger); }
  .tools { display: flex; gap: 2px; margin-left: auto; }

  /* Body: one scrolling column; edges fade only where there is more to see. */
  .body { min-height: 0; overflow-y: auto; overscroll-behavior: contain; padding: 12px 16px 16px; scrollbar-width: thin; scrollbar-color: transparent transparent; }
  .body:hover, .body:focus-within { scrollbar-color: var(--border-strong) transparent; }
  .body::-webkit-scrollbar { width: 6px; }
  .body::-webkit-scrollbar-thumb { border-radius: 3px; background: transparent; }
  .body:hover::-webkit-scrollbar-thumb { background: var(--border-strong); }
  .body.fade-top { -webkit-mask-image: linear-gradient(to bottom, transparent, #000 14px); mask-image: linear-gradient(to bottom, transparent, #000 14px); }
  .body.fade-bottom { -webkit-mask-image: linear-gradient(to top, transparent, #000 14px); mask-image: linear-gradient(to top, transparent, #000 14px); }
  .body.fade-top.fade-bottom { -webkit-mask-image: linear-gradient(to bottom, transparent, #000 14px, #000 calc(100% - 14px), transparent); mask-image: linear-gradient(to bottom, transparent, #000 14px, #000 calc(100% - 14px), transparent); }
  .body[hidden] { display: none; }
  .notice { margin: 0 0 8px; padding: 6px 10px; border-radius: var(--r-sm); background: var(--tone-amber-bg); color: var(--tone-amber); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }

  /* Summary tiles. */
  .hero { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px; }
  .tile { display: flex; flex-direction: column; gap: 2px; min-width: 0; padding: 10px 10px 8px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface-2); }
  .label { display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-label); line-height: var(--lh-label); font-weight: var(--fw-medium); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--muted); }
  .value { display: flex; align-items: baseline; font-size: var(--fs-display); line-height: var(--lh-display); font-weight: var(--fw-regular); letter-spacing: var(--ls-display); white-space: nowrap; }
  .value small { margin-left: 3px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); }
  .sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .spark { display: block; width: 100%; height: 14px; margin-top: 1px; }
  .spark path { fill: none; stroke: var(--tone-blue); stroke-width: 1.5; vector-effect: non-scaling-stroke; stroke-linecap: round; stroke-linejoin: round; }
  .dot { display: inline-block; width: 8px; height: 8px; border-radius: 3px; flex: none; }
  .dot.violet { background: var(--accent); }
  .dot.blue, .dot.local { background: var(--tone-blue); }
  .dot.green { background: var(--tone-green); }
  .dot.network { background: var(--tone-amber); }

  /* Sections: a left-aligned heading, its count, and at most one action on the right. */
  .sec { margin-top: 18px; scroll-margin-top: 8px; }
  .sec-head { display: flex; align-items: center; gap: 8px; min-height: 24px; margin-bottom: 6px; }
  .sec-head h2 { margin: 0; font-size: var(--fs-label); line-height: var(--lh-label); font-weight: var(--fw-semibold); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--text-2); }
  .count { min-width: 18px; padding: 0 6px; border-radius: 999px; text-align: center; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); background: var(--tone-gray-bg); color: var(--tone-gray); }
  .count.medium { background: var(--tone-amber-bg); color: var(--tone-amber); }
  .count.high { background: var(--danger-soft); color: var(--danger); }
  .hint { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-weight: var(--fw-regular); text-transform: none; letter-spacing: var(--ls-caption); }
  .sec-action { margin-left: auto; padding: 2px 10px; border: 1px solid var(--border-strong); border-radius: 999px; background: var(--surface); color: var(--danger); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .sec-action:hover { background: var(--danger-soft); border-color: transparent; }
  .sub-head { margin: 12px 0 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); letter-spacing: var(--ls-caption); color: var(--muted); }

  /* Rows. */
  .rows { list-style: none; margin: 0; padding: 0; }
  .row, .trow { display: flex; align-items: center; gap: 8px; min-height: 32px; padding: 4px 8px; margin: 0 -8px; width: calc(100% + 16px); border: 0; border-radius: var(--r-sm); background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .row:hover, .trow:hover { background: var(--row-hover); }
  .row:focus-visible, .trow:focus-visible, .link:focus-visible, .btn:focus-visible, .icon-btn:focus-visible, .sec-action:focus-visible, .settings button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .port { flex: none; min-width: 44px; font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--text-2); }
  .pill { flex: none; padding: 1px 8px; border-radius: 999px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); letter-spacing: var(--ls-caption); }
  .pill.ok { background: var(--tone-green-bg); color: var(--tone-green); }
  .pill.medium { background: var(--tone-amber-bg); color: var(--tone-amber); }
  .pill.high { background: var(--danger); color: var(--danger-fg); }
  .pill.gray { background: var(--tone-gray-bg); color: var(--tone-gray); }
  .quiet { margin: 2px 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); }
  .clear { display: flex; align-items: center; gap: 8px; margin: 0; padding: 8px 10px; border-radius: var(--r-sm); background: var(--tone-green-bg); color: var(--tone-green); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .clear svg { flex: none; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }

  /* Agents table. */
  .table { display: flex; flex-direction: column; }
  .thead, .trow { display: grid; grid-template-columns: minmax(0, 1fr) 52px 64px; align-items: center; gap: 8px; }
  .thead { padding: 0 0 2px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .c { text-align: right; white-space: nowrap; }
  .num { color: var(--text-2); }
  .num.strong { color: var(--text); font-weight: var(--fw-medium); }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name b { font-weight: var(--fw-medium); }
  .place { color: var(--muted); }
  .trow.tool .name { color: var(--text-2); }
  .tools-label { margin: 10px 0 2px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); color: var(--text-2); }
  .link { border: 0; background: none; padding: 2px 4px; color: var(--accent); font: inherit; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); cursor: pointer; border-radius: var(--r-xs); }
  .link.more { align-self: flex-start; margin-top: 4px; padding-left: 0; }

  /* Activity. */
  .chart { position: relative; margin: 0; height: 72px; padding: 4px 0; border-radius: var(--r-md); background: var(--surface-2); overflow: hidden; }
  .chart svg { display: block; width: 100%; height: 64px; }
  .chart .area { fill: var(--tone-blue-bg); }
  .chart .line { fill: none; stroke: var(--tone-blue); stroke-width: 1.5; vector-effect: non-scaling-stroke; }
  .scale { position: absolute; left: 8px; top: 4px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .waiting { display: grid; place-items: center; height: 100%; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); }
  .grid { display: grid; grid-template-columns: repeat(30, 1fr); gap: 3px; }
  .cell { display: block; aspect-ratio: 1; border-radius: 2px; background: var(--surface-3); }
  .cell.ok { background: var(--tone-green); }
  .cell.medium { background: var(--tone-amber); }
  .cell.high { background: var(--danger); }
  .ports { display: grid; grid-template-columns: 40px 1fr; gap: 12px; align-items: center; }
  .ring .track, .ring .local, .ring .network { fill: none; stroke-width: 6; }
  .ring .track { stroke: var(--surface-3); }
  .ring .local { stroke: var(--tone-blue); }
  .ring .network { stroke: var(--tone-amber); }
  .kv { display: flex; align-items: center; justify-content: space-between; min-height: 26px; color: var(--text-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .kv > span:first-child { display: inline-flex; align-items: center; gap: 6px; }
  .kv .num { color: var(--text); }
  .chart.churn { height: 48px; }
  .chart.churn svg { height: 40px; }
  .churn .axis { stroke: var(--border-strong); stroke-width: 1; vector-effect: non-scaling-stroke; }
  .churn .opened { fill: var(--tone-green); }
  .churn .closed { fill: var(--tone-amber); }
  .legend-row { display: flex; gap: 14px; margin: 6px 0 0; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .legend-row span { display: inline-flex; align-items: center; gap: 6px; }

  /* Footer. */
  .foot { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; padding: 10px 12px 12px 16px; border-top: 1px solid var(--border); background: var(--surface); }
  .action-error { flex-basis: 100%; margin: 0 0 4px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--danger); }
  .btn { display: inline-flex; align-items: center; justify-content: center; gap: 8px; flex: 1; height: 32px; margin-right: 6px; border: 1px solid transparent; border-radius: var(--r-md); background: var(--accent); color: var(--accent-fg); font: inherit; font-weight: var(--fw-medium); cursor: pointer; }
  .btn:hover { background: var(--accent-hover); }
  .foot svg { fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
  .icon-btn { display: grid; place-items: center; width: 32px; height: 32px; flex: none; border: 0; border-radius: var(--r-sm); background: none; color: var(--text-2); cursor: pointer; }
  .icon-btn:hover { background: var(--row-hover); color: var(--text); }
  .icon-btn.on { color: var(--accent); background: var(--accent-soft); }
  .quit:hover { color: var(--danger); background: var(--danger-soft); }

  /* Quick settings. */
  .settings-heading { display: flex; align-items: center; justify-content: space-between; }
  .settings-heading h2 { margin: 0; font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); }
  .segmented { display: flex; gap: 4px; padding: 3px; background: var(--surface-2); border-radius: var(--r-md); }
  .segmented button { flex: 1; padding: 5px 6px; border: 1px solid transparent; border-radius: var(--r-sm); background: none; color: var(--text-2); font: inherit; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); cursor: pointer; }
  .segmented button[aria-pressed="true"] { background: var(--surface); border-color: var(--border-strong); color: var(--accent); font-weight: var(--fw-medium); }
  .settings .quiet { margin: 6px 0 4px; }
  .setting-row { display: flex; align-items: center; justify-content: space-between; width: 100%; min-height: 34px; padding: 6px 4px; border: 0; border-radius: var(--r-sm); background: none; color: inherit; font: inherit; cursor: pointer; }
  .setting-row:hover { background: var(--row-hover); }
  .switch { width: 28px; height: 16px; padding: 2px; border-radius: 12px; background: var(--muted); flex: none; }
  .switch::after { content: ""; display: block; width: 12px; height: 12px; border-radius: 50%; background: var(--surface); transition: transform .15s; }
  .switch.checked { background: var(--accent); }
  .switch.checked::after { transform: translateX(12px); }
  button:disabled { cursor: wait; opacity: .6; }
  @media (prefers-reduced-motion: reduce) { .switch::after { transition: none; } }
</style>

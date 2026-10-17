<script lang="ts">
  // One lane per agent on a log time scale ending at now: the agent's uptime bar, a bar per
  // listening port from its process start, and its MCP servers at the live edge. Shares the
  // active agent with the map. j/k move between lanes.
  import Icon from "../Icon.svelte";
  import { ACCESS_LABEL } from "../../lib/home";
  import { accessHeadline, monogram } from "../../lib/agents";
  import { oldest, timeScale, type Lane } from "../../lib/agent-lanes";

  let { lanes, now, colors, active, open = true, ontoggle, onhover, onselect, onopenagent, onport }: {
    lanes: Lane[];
    /** Collapsible band; the header stays so the toggle never moves. */
    open?: boolean;
    ontoggle?: () => void;
    now: number;
    /** Agent colour by id, shared with the map. */
    colors: Map<string, string>;
    active: string | null;
    onhover: (id: string | null) => void;
    onselect: (id: string) => void;
    onopenagent: (id: string) => void;
    /** Opens the port's actions (Details, Stop…) anchored to the clicked element. */
    onport: (entryId: string, anchor: HTMLElement | SVGElement) => void;
  } = $props();

  const scale = $derived(timeScale(now, oldest(lanes, now)));
  const pct = (t: number) => `${(scale.x(t) * 100).toFixed(3)}%`;
  const color = (l: Lane) => colors.get(l.id) ?? "var(--faint)";
  let root = $state<HTMLElement>();
  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "j" && e.key !== "k") return;
    if ((e.target as HTMLElement | null)?.closest("input, textarea, select")) return;
    const nav = [...(root?.querySelectorAll<HTMLElement>("[data-lane]") ?? [])];
    const at = nav.indexOf(document.activeElement as HTMLElement);
    nav[Math.max(0, Math.min(nav.length - 1, at < 0 ? 0 : at + (e.key === "j" ? 1 : -1)))]?.focus();
    e.preventDefault();
  }
</script>

<section class="band" aria-labelledby="lanes-title">
<h2 class="band-head">
  <button type="button" id="lanes-title" aria-expanded={open} aria-controls="agent-lanes" onclick={() => ontoggle?.()}>
    <span class="chev" class:open><Icon name="chevron" size={12} /></span>Timeline<span class="n">{lanes.length}</span>
  </button>
  {#if open}<span class="key" aria-hidden="true"><span><i class="k up"></i>Uptime</span><span><i class="k pt"></i>Port</span><span><i class="k mc"></i>MCP</span></span>{/if}
</h2>
{#if open}
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="lanes" id="agent-lanes" role="table" aria-label="Agent lanes" bind:this={root} {onkeydown}>
  <div class="row axis" role="row">
    <span class="gutter" role="columnheader">Agent</span>
    <div class="track" role="columnheader" aria-label="Time">
      {#each scale.ticks as t (t.label)}<span class="tick" style="left: {(t.x * 100).toFixed(3)}%">{t.label}</span>{/each}
      <span class="tick now" style="left: 100%">now</span>
    </div>
    <span class="edge" role="columnheader">MCP</span>
  </div>
  {#each lanes as l (l.id)}
    {@const c = color(l)}
    {@const head = l.agent ? accessHeadline(l.agent) : null}
    <!-- svelte-ignore a11y_interactive_supports_focus (hover is a convenience; the gutter button carries focus) -->
    <div class="row lane" role="row" style="--agent: {c}" class:active={active !== null && active === l.id} class:dim={active !== null && active !== l.id}
      onmouseenter={() => l.agent && onhover(l.id)} onmouseleave={() => l.agent && onhover(null)}>
      {#if l.agent}
        <button type="button" class="gutter who" data-lane aria-label="{l.agent.name}, up {l.uptime}, {head ? ACCESS_LABEL[head.level] : ''}"
          onfocus={() => onhover(l.id)} onblur={() => onhover(null)} onclick={() => onselect(l.id)} ondblclick={() => onopenagent(l.id)}
          onkeydown={(e) => { if (e.key === "Enter" && active === l.id) { e.preventDefault(); onopenagent(l.id); } }}>
          <span class="mono">{monogram(l.agent.name)}</span>
          <span class="nm"><b>{l.agent.name}</b><small><span>{l.uptime}</span>{#if head}<span class="lv-{head.level}">{` · ${ACCESS_LABEL[head.level]}`}</span>{/if}</small></span>
        </button>
      {:else}
        <span class="gutter who"><span class="mono"><Icon name="server" size={12} /></span><span class="nm"><b>Unowned</b><small>{l.bars.length} dev</small></span></span>
      {/if}
      <div class="track" role="cell">
        {#each scale.ticks as t (t.label)}<span class="grid" style="left: {(t.x * 100).toFixed(3)}%"></span>{/each}
        {#if l.start !== null}
          <div class="sub main"><span class="bar" style="left: {pct(l.start)}"></span><span class="dot" style="left: {pct(l.start)}"></span></div>
        {/if}
        {#each l.bars as b (b.id)}
          {@const x = scale.x(b.start)}
          <div class="sub">
            <span class="bar thin" class:warn={b.warn} style="left: {pct(b.start)}"></span>
            <button type="button" class="pin" class:warn={b.warn} class:short={x > 0.7} style="--x: {pct(b.start)}" aria-haspopup="menu" onclick={(e) => onport(b.entryId, e.currentTarget)} aria-label="Port {b.port} {b.label}{b.warn ? ', open to the network' : ''}">
              <i></i><span>:{b.port} {b.label}</span>
            </button>
          </div>
        {/each}
      </div>
      <div class="edge" role="cell">
        {#each l.mcp as m (m.id)}
          <button type="button" class="mcp" class:on={m.running} class:warn={m.warn} onclick={() => l.agent && onopenagent(l.agent.id)} aria-label="MCP {m.name}, {m.running ? 'running' : 'not running'}{m.warn ? ', unpinned' : ''}"><i></i>{m.name}</button>
        {/each}
      </div>
    </div>
  {/each}
</div>
{/if}
</section>

<style>
  .band { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); overflow: clip; }
  .band-head { margin: 0; display: flex; align-items: center; gap: var(--sp-3); padding: 0 var(--sp-4) 0 var(--sp-2); height: 38px; font: inherit; }
  .band-head button { display: inline-flex; align-items: center; gap: 6px; height: 28px; padding: 0 8px; border: 0; border-radius: var(--r-md); background: none; font: inherit; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); cursor: pointer; }
  .band-head button:hover { background: var(--row-hover); }
  .band-head button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .chev { display: inline-flex; color: var(--muted); transition: transform 160ms ease; }
  .chev.open { transform: rotate(90deg); }
  @media (prefers-reduced-motion: reduce) { .chev { transition: none; } }
  .n { padding: 0 6px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  .key { margin-left: auto; display: flex; gap: var(--sp-3); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .key span { display: inline-flex; align-items: center; gap: 5px; }
  .k { display: inline-block; }
  .k.up { width: 14px; height: 5px; border-radius: 3px; background: var(--text-2); opacity: 0.6; }
  .k.pt { width: 8px; height: 8px; border-radius: 50%; background: var(--text-2); }
  .k.mc { width: 7px; height: 7px; transform: rotate(45deg); border-radius: 1.5px; border: 1.5px solid var(--text-2); }
  .lanes { --gutter: 200px; --edge: 240px; border-top: 1px solid var(--border); }
  .row { display: grid; grid-template-columns: var(--gutter) minmax(0, 1fr) var(--edge); }
  .axis { height: 28px; align-items: center; border-bottom: 1px solid var(--border); color: var(--faint); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .axis .gutter, .axis .edge { padding: 0 var(--sp-4); }
  .track { position: relative; margin: 0 36px 0 14px; min-width: 0; }
  .axis .track { height: 100%; }
  .tick { position: absolute; top: 50%; transform: translate(-50%, -50%); font-variant-numeric: tabular-nums; }
  .tick.now { color: var(--ok); font-weight: var(--fw-semibold); }
  .lane { border-bottom: 1px solid var(--border); transition: opacity 160ms ease, background 160ms ease; }
  .lane:last-child { border-bottom: 0; }
  .lane.active { background: color-mix(in srgb, var(--agent) 6%, var(--surface)); }
  .lane.dim { opacity: 0.4; }
  .lane .track { padding: 6px 0; display: grid; align-content: center; gap: 1px; }
  .lane .track::after { content: ""; position: absolute; top: 0; bottom: 0; right: 0; width: 1.5px; background: color-mix(in srgb, var(--ok) 60%, transparent); }
  .grid { position: absolute; top: 0; bottom: 0; width: 1px; background: var(--border); }
  .who { display: flex; align-items: center; gap: var(--sp-2); padding: 6px var(--sp-4); border: 0; background: none; font: inherit; color: inherit; text-align: left; cursor: pointer; min-width: 0; }
  span.who { cursor: default; }
  .who:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  .mono { width: 26px; height: 26px; flex: none; display: grid; place-items: center; border-radius: 8px; background: color-mix(in srgb, var(--agent) 16%, var(--surface)); color: var(--agent); font-size: var(--fs-caption); font-weight: var(--fw-semibold); }
  .nm { display: grid; min-width: 0; }
  .nm b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); overflow-wrap: anywhere; }
  .nm small span { white-space: nowrap; }
  .nm small { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .lv-elevated { color: var(--danger); } .lv-restricted { color: var(--ok); }
  .sub { position: relative; height: 18px; }
  .sub.main { height: 12px; }
  .bar { position: absolute; right: 0; top: calc(50% - 3px); height: 6px; border-radius: 3px 0 0 3px; background: linear-gradient(to right, color-mix(in srgb, var(--agent) 55%, transparent), color-mix(in srgb, var(--agent) 30%, transparent)); }
  .bar.thin { top: calc(50% - 1px); height: 2px; background: color-mix(in srgb, var(--agent) 45%, transparent); }
  .bar.thin.warn { background: color-mix(in srgb, var(--warn) 55%, transparent); }
  .dot { position: absolute; top: 50%; width: 10px; height: 10px; border-radius: 50%; transform: translate(-50%, -50%); background: var(--surface); border: 2px solid var(--agent); }
  /* Dot at the port's start; label right-aligned at the live edge, or left of the dot on short bars. */
  .pin { position: absolute; inset: 0 0 0 var(--x); border: 0; background: none; padding: 0; font: inherit; color: var(--text-2); cursor: pointer; }
  .pin i { position: absolute; left: 0; top: 50%; transform: translate(-50%, -50%); width: 9px; height: 9px; border-radius: 50%; background: var(--agent); box-shadow: 0 0 0 2px var(--surface); }
  .pin span { position: absolute; right: 6px; top: 50%; transform: translateY(-50%); white-space: nowrap; }
  .pin.short span { right: auto; left: -10px; transform: translate(-100%, -50%); }
  .pin span { font-family: var(--mono); font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 0 4px; border-radius: 4px; background: var(--surface); }
  .pin.warn i { background: var(--warn); box-shadow: 0 0 0 2px var(--surface), 0 0 0 5px var(--warn-soft); }
  .pin.warn span { color: var(--warn); }
  .pin:hover span, .pin:focus-visible span { color: var(--text); text-decoration: underline; }
  .pin:focus-visible { outline: none; } .pin:focus-visible i { outline: 2px solid var(--ring); outline-offset: 2px; }
  .edge { border-left: 1px solid var(--border); padding: 8px var(--sp-4); display: flex; flex-wrap: wrap; align-content: center; gap: 4px 10px; min-width: 0; }
  .axis .edge { height: 100%; align-content: center; }
  .mcp { display: inline-flex; align-items: center; gap: 5px; border: 0; background: none; padding: 0; font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); cursor: pointer; overflow-wrap: anywhere; }
  .mcp i { width: 7px; height: 7px; flex: none; transform: rotate(45deg); border-radius: 1.5px; border: 1.5px solid var(--faint); }
  .mcp.on { color: var(--text); } .mcp.on i { background: var(--ok); border-color: var(--ok); }
  .mcp.warn i { border-color: var(--warn); }
  .mcp:hover { color: var(--text); text-decoration: underline; }
  .mcp:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; border-radius: 3px; }
  @media (max-width: 1100px) { .lanes { --gutter: 168px; --edge: 200px; } }
  @media (max-width: 760px) {
    .axis { display: none; }
    .row.lane { grid-template-columns: minmax(0, 1fr); }
    .lane .track { margin: 0 32px 0 18px; padding-top: 0; }
    .edge { border-left: 0; padding-top: 0; padding-bottom: 12px; }
    .edge:empty { display: none; }
  }
</style>

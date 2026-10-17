<script lang="ts">
  // Concept B, "Agent lanes": one lane per agent across time, from when it started to the live
  // edge on the right. Ports sit where their process started; changes this window saw appear as
  // ticks. The right edge is now: what each agent holds and what needs you. j/k move between lanes.
  import { AGENT_COLORS, monogram } from "../../../lib/agents";
  import { configSummary } from "../../../lib/agent-config";
  import { ACCESS_LABEL, attention } from "../../../lib/home";
  import { lanes } from "../../../lib/home-concepts";
  import { accessHeadline } from "../../../lib/agents";
  import type { ChangeEvent } from "../../../lib/changes";
  import type { AgentsReport, PortEntry } from "../../../lib/types";

  let { entries, report, changes = [], now = Date.now(), onopenagent, onopenentry, onstopentry, onstopagent }: {
    entries: PortEntry[] | null; report: AgentsReport | null; changes?: ChangeEvent[]; now?: number;
    onopenagent: (id: string) => void; onopenentry: (id: string) => void;
    onstopentry?: (id: string) => void; onstopagent?: (id: string) => void;
  } = $props();

  const WINDOWS = [{ id: "1h", ms: 3600_000 }, { id: "6h", ms: 6 * 3600_000 }, { id: "24h", ms: 24 * 3600_000 }, { id: "3d", ms: 72 * 3600_000 }] as const;
  let span = $state<(typeof WINDOWS)[number]["id"]>("24h");
  const width = $derived(WINDOWS.find((w) => w.id === span)!.ms);
  const from = $derived(now - width);
  const rows = $derived(lanes(report, entries, from, now));
  const needs = $derived(attention(entries, report));
  const owned = $derived(new Set((report?.agents ?? []).flatMap((a) => a.ports.map((p) => p.entry_id))));
  const others = $derived((entries ?? []).filter((e) => e.is_dev && !owned.has(e.id)).map((e) => ({ id: e.id, at: Math.max(from, (e.process?.start_time ?? now / 1000) * 1000), label: `:${e.port} ${e.project?.name ?? e.framework?.name ?? e.label}`, warn: e.exposure === "all_interfaces" })).sort((a, b) => a.at - b.at));
  const pct = (t: number) => `${Math.min(100, Math.max(0, ((t - from) / width) * 100))}%`;
  const step = $derived(span === "1h" ? 10 * 60_000 : span === "6h" ? 3600_000 : span === "24h" ? 3 * 3600_000 : 12 * 3600_000);
  const ticks = $derived(Array.from({ length: Math.floor(width / step) }, (_, i) => Math.ceil(from / step) * step + i * step).filter((t) => t < now - step / 3));
  const tickLabel = (t: number) => new Date(t).toLocaleString([], span === "3d" ? { weekday: "short", hour: "numeric" } : { hour: "numeric", minute: span === "1h" ? "2-digit" : undefined });
  /** Hide a label when it would collide with the previous visible one (marker stays). */
  function labelled<T extends { at: number }>(marks: T[]): (T & { show: boolean })[] {
    let last = -Infinity;
    return marks.map((m) => { const p = ((m.at - from) / width) * 100; const show = p - last > 9; if (show) last = p; return { ...m, show }; });
  }
  const agentChanges = (id: string, entryIds: Set<string>) => changes.filter((c) => c.at >= from && c.target && (c.target.view === "agents" ? c.target.agentId === id : entryIds.has(c.target.entryId)));
  let root = $state<HTMLElement>();
  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "j" && e.key !== "k") return;
    const nav = [...(root?.querySelectorAll<HTMLElement>("[data-lane]") ?? [])];
    const at = nav.indexOf(document.activeElement as HTMLElement);
    nav[Math.max(0, Math.min(nav.length - 1, at + (e.key === "j" ? 1 : -1)))]?.focus();
    e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="concept" role="region" aria-label="Overview" bind:this={root} {onkeydown}>
  <header class="bar">
    <div><h1>While you were away</h1><p>What each agent started, held and touched, up to now.</p></div>
    <div class="seg" role="radiogroup" aria-label="Time window">
      {#each WINDOWS as w}<button type="button" role="radio" aria-checked={span === w.id} onclick={() => (span = w.id)}>{w.id}</button>{/each}
    </div>
  </header>

  <div class="grid" role="table" aria-label="Agent lanes">
    <div class="axis" role="row">
      <span class="gutter" role="columnheader">Agent</span>
      <div class="track" role="columnheader" aria-label="Time">{#each ticks as t}<span class="tick" style="left: {pct(t)}">{tickLabel(t)}</span>{/each}<span class="tick now-label" style="left: 100%">now</span></div>
      <span class="edge-h" role="columnheader">Right now</span>
    </div>
    {#each rows as l, i (l.agent.id)}
      {@const a = l.agent}
      {@const head = accessHeadline(a)}
      {@const cfg = configSummary(a)}
      {@const mine = needs.filter((n) => (n.target.view === "agents" && n.target.agentId === a.id) || (n.target.view === "list" && a.ports.some((p) => p.entry_id === (n.target as { entryId: string }).entryId)))}
      {@const ticksFor = agentChanges(a.id, new Set(a.ports.map((p) => p.entry_id)))}
      <div class="lane" role="row" style="--agent: {AGENT_COLORS[i % AGENT_COLORS.length]}">
        <button type="button" class="gutter who" data-lane onclick={() => onopenagent(a.id)} aria-label="{a.name}, up {l.uptime}, {ACCESS_LABEL[head.level]} access">
          <span class="mono">{monogram(a.name)}</span>
          <span class="nm"><b>{a.name}</b><small>up {l.uptime} · <span class="lv-{head.level}">{ACCESS_LABEL[head.level]}</span></small></span>
        </button>
        <div class="track" role="cell">
          <span class="span" style="left: {pct(l.start)}; right: 0" class:clipped={l.clipped}></span>
          {#if !l.clipped}<span class="start" style="left: {pct(l.start)}" title="Started {new Date(l.start).toLocaleString()}"></span>{/if}
          {#if l.clipped}<span class="before">← started {l.uptime} ago</span>{/if}
          {#each labelled(l.marks) as m (m.id)}
            <button type="button" class="mark" class:warn={m.warn} class:before={m.before} style="left: {pct(m.at)}" onclick={() => onopenentry(m.target.entryId)} aria-label="{m.label}, started {new Date(m.at).toLocaleTimeString()}">
              <i></i>{#if m.show}<span>{m.before ? "← " : ""}{m.label}</span>{/if}
            </button>
          {/each}
          {#each ticksFor as c (c.id)}<span class="change" style="left: {pct(c.at)}" title="{c.title} · {c.detail}"></span>{/each}
        </div>
        <div class="edge" role="cell">
          <span class="chips">{#each a.ports as p (p.entry_id)}<button type="button" class="chip" class:warn={p.exposure === "all_interfaces"} onclick={() => onopenentry(p.entry_id)}>:{p.port}</button>{/each}</span>
          {#if cfg}<button type="button" class="mcp" onclick={() => onopenagent(a.id)}><b class:on={cfg.running > 0}>{cfg.running}</b>/{cfg.mcp} MCP</button>{/if}
          {#if mine.length}
            <span class="need" title={mine[0].detail}>{mine[0].title}</span>
            {#if mine[0].stop}<button type="button" class="act" onclick={() => { const s = mine[0].stop!; if (s.kind === "entry") onstopentry?.(s.entryId); else onstopagent?.(s.agentId); }}>Review stop plan</button>{/if}
          {/if}
        </div>
      </div>
    {/each}
    {#if others.length}
      <div class="lane other" role="row">
        <span class="gutter who"><span class="mono">··</span><span class="nm"><b>Not started by an agent</b><small>{others.length} dev servers</small></span></span>
        <div class="track" role="cell">
          {#each labelled(others) as m (m.id)}
            <button type="button" class="mark" class:warn={m.warn} style="left: {pct(m.at)}" onclick={() => onopenentry(m.id)} aria-label="{m.label}"><i></i>{#if m.show}<span>{m.label}</span>{/if}</button>
          {/each}
        </div>
        <div class="edge" role="cell"><span class="chips">{#each others as m (m.id)}<button type="button" class="chip" class:warn={m.warn} onclick={() => onopenentry(m.id)}>{m.label.split(" ")[0]}</button>{/each}</span></div>
      </div>
    {/if}
  </div>
  <p class="foot">Marks sit at the start time of the process holding each port. Changes appear as ticks only for what this window saw while open.</p>
</div>

<style>
  .concept { min-height: 0; overflow-y: auto; padding: var(--sp-5) var(--sp-6); display: grid; gap: var(--sp-4); align-content: start; }
  .bar { display: flex; justify-content: space-between; align-items: end; gap: var(--sp-3); flex-wrap: wrap; }
  h1 { margin: 0; font-size: var(--fs-display); line-height: var(--lh-display); letter-spacing: var(--ls-display); font-weight: var(--fw-semibold); color: var(--text); }
  .bar p { margin: 2px 0 0; color: var(--muted); font-size: var(--fs-body-sm); }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r-md); overflow: hidden; background: var(--surface); }
  .seg button { border: 0; background: none; padding: 4px 12px; font: inherit; font-size: var(--fs-caption); color: var(--muted); cursor: pointer; font-variant-numeric: tabular-nums; }
  .seg button[aria-checked="true"] { background: var(--accent-soft); color: var(--accent); font-weight: var(--fw-semibold); }
  .grid { --gutter: 220px; --edge: 300px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); overflow: hidden; }
  .axis, .lane { display: grid; grid-template-columns: var(--gutter) minmax(0, 1fr) var(--edge); align-items: center; }
  .axis { height: 34px; border-bottom: 1px solid var(--border); color: var(--faint); font-size: var(--fs-caption); }
  .axis .gutter, .edge-h { padding: 0 var(--sp-4); }
  .edge-h { border-left: 1px solid var(--border); height: 100%; display: flex; align-items: center; color: var(--text-2); font-weight: var(--fw-medium); }
  .track { position: relative; height: 100%; margin: 0 28px 0 12px; }
  .axis .track { background: none; }
  .tick { position: absolute; top: 50%; transform: translate(-50%, -50%); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .now-label { color: var(--ok); font-weight: var(--fw-semibold); }
  .lane { min-height: 76px; border-bottom: 1px solid var(--border); }
  .lane:last-child { border-bottom: 0; }
  .lane:hover { background: var(--row-hover); }
  .lane .track { background-image: linear-gradient(to left, color-mix(in srgb, var(--ok) 55%, transparent) 1.5px, transparent 1.5px); background-position: right; background-repeat: no-repeat; }
  .who { display: flex; align-items: center; gap: var(--sp-2); padding: 0 var(--sp-4); height: 100%; border: 0; background: none; font: inherit; color: inherit; text-align: left; cursor: pointer; }
  .who:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  .mono { width: 26px; height: 26px; flex: none; display: grid; place-items: center; border-radius: 8px; background: color-mix(in srgb, var(--agent, var(--faint)) 16%, var(--surface)); color: var(--agent, var(--muted)); font-size: var(--fs-caption); font-weight: var(--fw-semibold); }
  .nm { display: grid; min-width: 0; }
  .nm b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); }
  .nm small { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .lv-elevated { color: var(--danger); } .lv-restricted { color: var(--ok); }
  .span { position: absolute; top: calc(50% - 3px); height: 6px; border-radius: 3px; background: color-mix(in srgb, var(--agent) 30%, transparent); }
  .span.clipped { border-top-left-radius: 0; border-bottom-left-radius: 0; background: linear-gradient(to right, transparent, color-mix(in srgb, var(--agent) 30%, transparent) 40px); }
  .start { position: absolute; top: 50%; width: 10px; height: 10px; border-radius: 50%; transform: translate(-50%, -50%); background: var(--surface); border: 2px solid var(--agent); }
  .before { position: absolute; left: 0; top: calc(50% + 8px); font-size: var(--fs-caption); color: var(--faint); white-space: nowrap; }
  .mark { position: absolute; top: 50%; transform: translate(-5px, -5px); display: flex; align-items: center; gap: 6px; border: 0; background: none; padding: 0; font: inherit; cursor: pointer; color: var(--text-2); }
  .mark i { width: 10px; height: 10px; flex: none; transform: rotate(45deg); border-radius: 2px; background: var(--agent, var(--text-2)); box-shadow: 0 0 0 2px var(--surface); }
  .mark span { position: absolute; left: 0; bottom: 14px; white-space: nowrap; font-size: var(--fs-caption); line-height: var(--lh-caption); font-family: var(--mono); background: var(--surface); padding: 0 3px; border-radius: 3px; }
  .mark.warn i { background: var(--warn); box-shadow: 0 0 0 2px var(--surface), 0 0 0 5px var(--warn-soft); }
  .mark.warn span { color: var(--warn); }
  .mark.before i { opacity: 0.55; }
  .mark:hover span, .mark:focus-visible span { color: var(--text); text-decoration: underline; }
  .mark:focus-visible i { outline: 2px solid var(--ring); outline-offset: 2px; }
  .change { position: absolute; top: calc(50% + 7px); width: 2px; height: 8px; background: var(--muted); border-radius: 1px; }
  .edge { border-left: 1px solid var(--border); height: 100%; padding: 8px var(--sp-4); display: flex; flex-wrap: wrap; align-content: center; align-items: center; gap: 4px 8px; font-size: var(--fs-caption); }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chip { border: 1px solid var(--border); background: var(--surface); padding: 0 6px; border-radius: var(--r-full); font-family: var(--mono); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); cursor: pointer; }
  .chip.warn { color: var(--warn); background: var(--warn-soft); border-color: color-mix(in srgb, var(--warn) 40%, transparent); }
  .mcp { border: 0; background: none; padding: 0; font: inherit; color: var(--muted); cursor: pointer; }
  .mcp b { color: var(--text); } .mcp b.on { color: var(--ok); }
  .need { flex-basis: 100%; color: var(--warn); overflow-wrap: anywhere; }
  .act { border: 1px solid var(--border-strong); background: var(--surface); border-radius: var(--r-md); padding: 1px 8px; font: inherit; color: var(--text-2); cursor: pointer; }
  .other .mark i { background: var(--faint); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .foot { margin: 0; color: var(--faint); font-size: var(--fs-caption); }
  @media (max-width: 1180px) { .grid { --gutter: 180px; --edge: 220px; } }
  @media (max-width: 760px) {
    .concept { padding: var(--sp-3); }
    .axis { display: none; }
    .lane { grid-template-columns: 1fr; }
    .lane .track { height: 56px; margin: 0 20px; }
    .edge { border-left: 0; border-top: 1px dashed var(--border); }
    .who { padding: 10px var(--sp-3) 0; }
  }
</style>

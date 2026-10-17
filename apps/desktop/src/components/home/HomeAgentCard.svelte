<script lang="ts">
  // One agent on Home: identity, access level and attention badge, a footprint constellation with
  // four fixed counters, chips for ports / MCP / projects, and actions pinned to the bottom so
  // cards in a row line up. Click selects (shared with the map); a second click or Enter opens.
  import Icon from "../Icon.svelte";
  import { ACCESS_LABEL, type AttentionItem } from "../../lib/home";
  import { KIND_LABEL, accessHeadline, monogram } from "../../lib/agents";
  import { footprint } from "../../lib/machine-map";
  import { miniLayout, type CardModel } from "../../lib/agent-card";
  import { humanDuration } from "../../lib/format";
  import type { Agent } from "../../lib/types";
  import { countsText, total, worst, type SeverityCounts } from "../../lib/analyzers";

  let { agent, model, needs, findings = null, color, now, selected, active, onhover, onselect, onopen, onport, onstop, oninventory }: {
    agent: Agent;
    model: CardModel;
    /** Attention items about this agent. */
    needs: AttentionItem[];
    /** Open analyzer findings about this agent's skills and packages. */
    findings?: SeverityCounts | null;
    color: string;
    now: number;
    selected: boolean;
    /** Hovered or selected agent anywhere on Home; null lights every card. */
    active: string | null;
    onhover: (id: string | null) => void;
    onselect: (id: string) => void;
    onopen: (id: string) => void;
    /** Opens the port's actions (Details, Stop…) anchored to the clicked element. */
    onport: (entryId: string, anchor: HTMLElement | SVGElement) => void;
    /** Opens the stop plan for review. */
    onstop?: (id: string) => void;
    oninventory?: () => void;
  } = $props();

  const head = $derived(accessHeadline(agent));
  const up = $derived(humanDuration(Math.max(0, now / 1000 - agent.started_at)));
  const mini = $derived(miniLayout(footprint(agent, 16), 44));
  const high = $derived(needs.some((n) => n.severity === "high"));
  const exposed = $derived(model.ports.filter((p) => p.warn).length);
  const found = $derived(findings && total(findings) ? findings : null);
  const label = $derived(`${agent.name}, ${ACCESS_LABEL[head.level]}${needs.length ? `, ${needs.length} need you` : ""}`);
  function headKey(e: KeyboardEvent) { if (e.key === "Enter" && selected) { e.preventDefault(); onopen(agent.id); } }
</script>

{#snippet unknown()}<span class="unk"><span aria-hidden="true">?</span><span class="sr-only">unknown</span></span>{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<article class="card" style="--agent: {color}" aria-label={agent.name} class:selected class:active={active === agent.id} class:dim={active !== null && active !== agent.id}
  onmouseenter={() => onhover(agent.id)} onmouseleave={() => onhover(null)}>
  <header>
    <span class="tile" aria-hidden="true">{monogram(agent.name)}</span>
    <button type="button" class="head" data-card aria-pressed={selected} aria-label={label}
      onclick={() => (selected ? onopen(agent.id) : onselect(agent.id))} onkeydown={headKey} onfocus={() => onhover(agent.id)} onblur={() => onhover(null)}>{agent.name}</button>
    <small class="meta"><span>{KIND_LABEL[agent.kind]}</span><span>{`up ${up}`}</span></small>
    <span class="tags">
      {#if needs.length}<span class="badge" class:high title={needs.map((n) => n.title).join("\n")}><Icon name="alert" size={11} />{needs.length}</span>{/if}
      {#if found}<span class="badge find sev-{worst(found)}" title="Open analyzer findings: {countsText(found, 5)}"><Icon name="shield" size={11} /><span class="sr-only">Analyzer findings: </span>{total(found)}</span>{/if}
      <span class="lv lv-{head.level}" title={head.text}>{ACCESS_LABEL[head.level]}</span>
    </span>
  </header>

  <div class="mid">
    <svg viewBox="-70 -64 140 128" class="fp" aria-hidden="true">
      <circle r="44" class="ring" />
      <circle r="26" class="ring inner" />
      {#each mini as m (m.node.id)}<path d="M0,0 Q{m.x * 0.5 + m.y * 0.12},{m.y * 0.5 - m.x * 0.12} {m.x},{m.y}" class="edge" class:live={m.node.live} class:warn={m.node.warn} />{/each}
      <circle r="17" class="disc" />
      <text y="4" class="mg">{monogram(agent.name)}</text>
      {#each mini as m (m.node.id)}
        <g transform="translate({m.x},{m.y})" class="n k-{m.node.kind}" class:warn={m.node.warn} class:idle={!m.node.live}>
          {#if m.node.warn}<circle r="8.5" class="halo" />{/if}
          {#if m.node.kind === "project"}<rect x="-4.5" y="-4.5" width="9" height="9" rx="2.5" />
          {:else if m.node.kind === "mcp"}<rect x="-4" y="-4" width="8" height="8" rx="1.5" transform="rotate(45)" />
          {:else}<circle r="4.5" class:hollow={m.node.kind === "remote"} />{/if}
        </g>
      {/each}
    </svg>
    <dl class="ctrs">
      <div><dt><Icon name="folder" size={12} />Projects</dt><dd>{model.projects.length}</dd></div>
      <div class:warn={exposed > 0}><dt><Icon name="plug" size={12} />Ports</dt><dd>{model.ports.length}{#if exposed}<small>{`${exposed} exposed`}</small>{/if}</dd></div>
      <div title={model.unknownReason ?? undefined}><dt><Icon name="server" size={12} />MCP</dt>
        <dd>{#if model.mcp}<span class="of" title="{model.mcp.running} of {model.mcp.total} running"><span aria-hidden="true">{model.mcp.running}<small>{`/${model.mcp.total}`}</small></span><span class="sr-only">{`${model.mcp.running} of ${model.mcp.total} running`}</span></span>{:else}{@render unknown()}{/if}</dd></div>
      <div title={model.unknownReason ?? undefined}><dt><Icon name="sparkles" size={12} />Skills</dt>
        <dd>{#if model.skills !== null}{model.skills}{:else}{@render unknown()}{/if}</dd></div>
    </dl>
  </div>

  <ul class="chips" aria-label="{agent.name} footprint">
    {#each model.ports as p (p.entryId)}
      <li><button type="button" class="chip port" class:warn={p.warn} aria-haspopup="menu" onclick={(e) => onport(p.entryId, e.currentTarget)} aria-label="Port {p.port} {p.label}{p.warn ? ', open to the network' : ''}"><i></i><span class="mono-t">:{p.port}</span>{" "}{p.label}</button></li>
    {/each}
    {#each model.mcp?.servers ?? [] as s (s.id)}
      <li><button type="button" class="chip mcp" class:on={s.running} class:warn={s.warn} onclick={() => onopen(agent.id)} title={s.names.length ? s.names.join(", ") : undefined}
        aria-label="MCP {s.name}, {s.running ? 'running' : 'not running'}{s.warn ? ', unpinned' : ''}"><i></i>{s.name}</button></li>
    {/each}
    {#each model.projects as p (p)}<li><span class="chip proj"><Icon name="folder" size={11} />{p}</span></li>{/each}
  </ul>

  <footer>
    <button type="button" class="btn" onclick={() => onopen(agent.id)} aria-label="Open {agent.name}">Open<Icon name="chevron" size={12} /></button>
    {#if model.stoppable && onstop}<button type="button" class="btn ghost" onclick={() => onstop(agent.id)} aria-label="Review stop plan: {agent.name}"><Icon name="stop" size={11} />Stop…</button>{/if}
    {#if model.configOff && oninventory}<button type="button" class="btn ghost accent" onclick={oninventory}><Icon name="plug" size={11} />Read config</button>{/if}
  </footer>
</article>

<style>
  .card { position: relative; display: flex; flex-direction: column; gap: var(--sp-3); min-width: 0; padding: var(--sp-4); border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); transition: opacity 160ms ease, border-color 160ms ease, box-shadow 160ms ease; }
  .card::before { content: ""; position: absolute; inset: 0 0 auto; height: 2px; border-radius: var(--r-lg) var(--r-lg) 0 0; background: linear-gradient(90deg, var(--agent), transparent 70%); opacity: 0.55; }
  .card.active { border-color: color-mix(in srgb, var(--agent) 45%, var(--border)); }
  .card.selected { border-color: var(--agent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--agent) 18%, transparent); }
  .card.dim { opacity: 0.45; }
  @media (prefers-reduced-motion: reduce) { .card { transition: none; } }
  header { display: grid; grid-template-columns: 34px minmax(0, 1fr) auto; grid-template-areas: "tile name tags" "tile meta meta"; column-gap: var(--sp-3); align-items: center; }
  .tile { grid-area: tile; }
  .head { grid-area: name; justify-self: start; min-width: 0; border: 0; padding: 0; background: none; font: inherit; font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); color: var(--text); text-align: left; cursor: pointer; border-radius: var(--r-sm); overflow-wrap: anywhere; }
  .head::after { content: ""; position: absolute; inset: 0; border-radius: var(--r-lg); }
  .head:hover { color: var(--text); text-decoration: underline; text-decoration-color: var(--border-strong); text-underline-offset: 3px; }
  .head:focus-visible { outline: 2px solid var(--ring); outline-offset: 3px; }
  .tile { width: 34px; height: 34px; flex: none; display: grid; place-items: center; border-radius: 10px; background: color-mix(in srgb, var(--agent) 16%, var(--surface)); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--agent) 30%, transparent); color: var(--agent); font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  .meta { grid-area: meta; display: flex; flex-wrap: wrap; column-gap: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .meta span { white-space: nowrap; }
  .meta span + span::before { content: "·"; margin-right: 6px; color: var(--faint); }
  .tags { grid-area: tags; display: flex; align-items: center; gap: 6px; flex: none; }
  .lv, .badge { display: inline-flex; align-items: center; gap: 4px; height: 22px; padding: 0 8px; border-radius: var(--r-full); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); white-space: nowrap; background: var(--surface-2); color: var(--text-2); }
  .lv-restricted { background: var(--ok-soft); color: var(--ok); }
  .lv-elevated { background: var(--danger-soft); color: var(--danger); }
  .lv-unknown { color: var(--muted); }
  .badge { background: var(--warn-soft); color: var(--warn); font-variant-numeric: tabular-nums; }
  .badge.high { background: var(--danger-soft); color: var(--danger); }
  .badge.find.sev-low, .badge.find.sev-info { background: var(--surface-2); color: var(--text-2); }
  .badge.find.sev-high, .badge.find.sev-critical { background: var(--danger-soft); color: var(--danger); }
  .mid { display: grid; grid-template-columns: 128px minmax(0, 1fr); gap: var(--sp-4); align-items: center; padding: var(--sp-2) 0; border-top: 1px solid var(--border); border-bottom: 1px solid var(--border); }
  .fp { width: 128px; height: 117px; display: block; }
  .ring { fill: none; stroke: var(--border); stroke-dasharray: 2 4; }
  .ring.inner { stroke-dasharray: none; opacity: 0.6; }
  .edge { fill: none; stroke: var(--border-strong); stroke-width: 1; stroke-dasharray: 2 2; }
  .edge.live { stroke: color-mix(in srgb, var(--agent) 55%, transparent); stroke-dasharray: none; }
  .edge.warn { stroke: color-mix(in srgb, var(--warn) 65%, transparent); }
  .disc { fill: color-mix(in srgb, var(--agent) 16%, var(--surface)); stroke: var(--agent); stroke-width: 1.5; }
  .mg { text-anchor: middle; font-size: var(--fs-caption); font-weight: var(--fw-semibold); fill: var(--agent); }
  .n rect, .n circle:not(.halo) { fill: var(--surface); stroke: var(--text-2); stroke-width: 1.4; }
  .k-port circle:not(.halo) { fill: var(--agent); stroke: var(--agent); }
  .k-mcp:not(.idle) rect { fill: var(--ok-soft); stroke: var(--ok); }
  .idle rect, .idle circle:not(.halo) { stroke-dasharray: 2 1.5; }
  .n.warn rect, .n.warn circle:not(.halo) { stroke: var(--warn); }
  .k-port.warn circle:not(.halo) { fill: var(--warn); }
  .halo { fill: var(--warn-soft); }
  .ctrs { margin: 0; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--sp-3) var(--sp-4); }
  .ctrs div { display: grid; gap: 2px; min-width: 0; }
  dt { display: flex; align-items: center; gap: 5px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  dt :global(svg) { color: var(--faint); }
  dd { margin: 0; display: flex; align-items: baseline; flex-wrap: wrap; column-gap: 4px; font-size: var(--fs-title); line-height: var(--lh-title); font-weight: var(--fw-semibold); color: var(--text); font-variant-numeric: tabular-nums; }
  dd small { font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-regular); color: var(--muted); white-space: nowrap; }
  .ctrs .warn dd small, .ctrs .warn dt :global(svg) { color: var(--warn); }
  .of small { margin-left: 1px; }
  .unk { color: var(--faint); font-weight: var(--fw-medium); }
  .chips { list-style: none; margin: 0; padding: 0; display: flex; flex-wrap: wrap; align-content: flex-start; gap: 6px; flex: 1; }
  .chip { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 9px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface); color: var(--text-2); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); white-space: nowrap; }
  button.chip { cursor: pointer; }
  button.chip:hover { border-color: var(--border-strong); color: var(--text); background: var(--row-hover); }
  .chip i { width: 7px; height: 7px; flex: none; border-radius: 50%; background: var(--agent); }
  .chip.mcp i { border-radius: 1.5px; transform: rotate(45deg); background: none; border: 1.5px solid var(--faint); width: 6px; height: 6px; }
  .chip.mcp.on i { background: var(--ok); border-color: var(--ok); }
  .chip.mcp.on { color: var(--text); }
  .chip.warn { border-color: color-mix(in srgb, var(--warn) 40%, var(--border)); }
  .chip.port.warn i { background: var(--warn); }
  .chip.mcp.warn i { border-color: var(--warn); }
  .mono-t { font-family: var(--mono); color: var(--text); }
  .chip.port.warn .mono-t { color: var(--warn); }
  .chip.proj { color: var(--muted); border-style: dashed; }
  .chip.proj :global(svg) { color: var(--faint); }
  .chips, footer, .tags { position: relative; z-index: 1; }
  footer { display: flex; gap: 6px; flex-wrap: wrap; }
  .btn { display: inline-flex; align-items: center; gap: 4px; height: 26px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: var(--r-md); background: var(--surface-2); color: var(--text); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); cursor: pointer; white-space: nowrap; }
  .btn.ghost { background: var(--surface); border-color: var(--border); color: var(--text-2); }
  .btn.accent { color: var(--accent); }
  .btn:hover { color: var(--text); border-color: var(--border-strong); background: var(--row-hover); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
</style>

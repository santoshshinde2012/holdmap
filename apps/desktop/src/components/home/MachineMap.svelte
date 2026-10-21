<script lang="ts">
  // Radial map of this computer: agents on an inner ring, each agent's footprint fanned out in its
  // own sector. Attention glows on the exact node. The active agent (hovered or selected, shared
  // with the lanes) stays lit while the others dim. ←/→ move between agents.
  import { ACCESS_LABEL } from "../../lib/home";
  import { accessHeadline, monogram } from "../../lib/agents";
  import Icon from "../Icon.svelte";
  import { MAP_RINGS, MAP_STRETCH, avoidCollisions, mapBounds, type Box, type FootprintNode, type radialLayout } from "../../lib/machine-map";

  type Group = ReturnType<typeof radialLayout>[number];
  let { groups: laidOut, colors, platform, ports, active, selected, hot, onhover, onselect, onopen, onport }: {
    groups: Group[];
    /** Remote-host groups shown one by one. */
    /** Agent colour by id, shared with cards and timeline. */
    colors: Map<string, string>;
    platform: string;
    ports: number | null;
    /** Hovered or selected agent; null shows everything. */
    active: string | null;
    selected: string | null;
    /** Agents with attention about the agent itself (root, elevated, settings). */
    hot: Set<string>;
    onhover: (id: string | null) => void;
    onselect: (id: string) => void;
    onopen: (target: FootprintNode["target"]) => void;
    /** Opens the port's actions (Details, Stop…) anchored to the clicked element. */
    onport: (entryId: string, anchor: HTMLElement | SVGElement) => void;
  } = $props();

  const KIND: Record<FootprintNode["kind"], string> = { project: "Project", port: "Port", mcp: "MCP server", remote: "Remote host" };
  const curve = (ax: number, ay: number, bx: number, by: number) => `M${ax},${ay} Q${((ax + bx) / 2) * 1.08},${((ay + by) / 2) * 1.08} ${bx},${by}`;
  function agentKey(e: KeyboardEvent, i: number, id: string) {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); if (selected === id) onopen({ view: "agents", agentId: id }); else onselect(id); }
    else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const next = groups[(i + (e.key === "ArrowRight" ? 1 : groups.length - 1)) % groups.length];
      document.getElementById(`map-agent-${next.agent.id}`)?.focus();
    }
  }
  // Keep labels legible when the stage is small: counter-scale node bodies (not positions).
  // The view box is then fitted to nodes and labels at that scale, so nothing clips at the edges.
  let w = $state(0), h = $state(0);
  const base = $derived(mapBounds(laidOut, 1));
  const k = $derived(w && h ? Math.min(1.8, Math.max(1, Math.sqrt(1 / Math.min(w / base.w, h / base.h)))) : 1);
  // Labels are pushed apart at the scale they're drawn at, so none overlap.
  const groups = $derived(avoidCollisions(laidOut, k));
  const fit = $derived(mapBounds(groups, k));

  // Zoom and pan: a zoom factor and a centre in map units. Selecting an agent focuses its sector;
  // Fit (or clearing the selection) shows everything again.
  let zoom = $state(1);
  let centre = $state<{ x: number; y: number } | null>(null);
  const focusBox = $derived.by<Box | null>(() => {
    const g = groups.find((g) => g.agent.id === selected);
    if (!g) return null;
    const own = mapBounds([g], k, 40);
    const x0 = Math.min(own.x, -80), x1 = Math.max(own.x + own.w, 80), y0 = Math.min(own.y, -40), y1 = Math.max(own.y + own.h, 40);
    return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
  });
  const frame = $derived(focusBox ?? fit);
  const box = $derived.by<Box>(() => {
    const c = centre ?? { x: frame.x + frame.w / 2, y: frame.y + frame.h / 2 };
    const bw = frame.w / zoom, bh = frame.h / zoom;
    return { x: c.x - bw / 2, y: c.y - bh / 2, w: bw, h: bh };
  });
  $effect(() => { void selected; zoom = 1; centre = null; });
  const ZOOM_MIN = 0.6, ZOOM_MAX = 4;
  function zoomBy(f: number, at?: { x: number; y: number }) {
    const next = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom * f));
    const c = centre ?? { x: box.x + box.w / 2, y: box.y + box.h / 2 };
    const p = at ?? c;
    const r = zoom / next;
    centre = { x: p.x + (c.x - p.x) * r, y: p.y + (c.y - p.y) * r };
    zoom = next;
  }
  function resetView() { zoom = 1; centre = null; if (selected) onselect(selected); }
  let svg: SVGSVGElement | undefined = $state();
  const toMap = (e: { clientX: number; clientY: number }) => {
    const r = svg!.getBoundingClientRect();
    const s = Math.max(box.w / r.width, box.h / r.height);
    return { x: box.x + box.w / 2 + (e.clientX - r.left - r.width / 2) * s, y: box.y + box.h / 2 + (e.clientY - r.top - r.height / 2) * s, s };
  };
  function onwheel(e: WheelEvent) { if (!svg) return; e.preventDefault(); zoomBy(e.deltaY < 0 ? 1.15 : 1 / 1.15, toMap(e)); }
  let drag = $state<{ x: number; y: number; c: { x: number; y: number }; s: number; moved: boolean } | null>(null);
  function ondown(e: PointerEvent) {
    if (!svg || e.button !== 0 || (e.target as Element).closest(".sat, .agent")) return;
    drag = { x: e.clientX, y: e.clientY, c: { x: box.x + box.w / 2, y: box.y + box.h / 2 }, s: toMap(e).s, moved: false };
    svg.setPointerCapture(e.pointerId);
  }
  function onmove(e: PointerEvent) {
    if (!drag) return;
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
    if (drag.moved) centre = { x: drag.c.x - dx * drag.s, y: drag.c.y - dy * drag.s };
  }
  function onup() { drag = null; }
  function mapKey(e: KeyboardEvent) {
    if (e.target !== e.currentTarget) return;
    if (e.key === "+" || e.key === "=") { e.preventDefault(); zoomBy(1.25); }
    else if (e.key === "-") { e.preventDefault(); zoomBy(1 / 1.25); }
    else if (e.key === "0") { e.preventDefault(); resetView(); }
  }
  // Port nodes open the port's actions (so Stop… is one step away); other nodes open their target.
  function activate(t: FootprintNode["target"], el: SVGElement) { if (t.view === "list") onport(t.entryId, el); else onopen(t); }
  const KIND_LABEL = (n: FootprintNode) => n.collapse ? `Hide ${n.count} remote hosts` : n.count ? `${n.count} remote hosts` : KIND[n.kind];
  const nodeKey = (e: KeyboardEvent, t: FootprintNode["target"]) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); activate(t, e.currentTarget as SVGElement); } };
</script>

<div class="fit" bind:clientWidth={w} bind:clientHeight={h}>
<div class="zoom" role="toolbar" aria-label="Map zoom">
  <button type="button" aria-label="Zoom in" title="Zoom in (+)" onclick={() => zoomBy(1.25)}><Icon name="plus" size={13} /></button>
  <button type="button" aria-label="Zoom out" title="Zoom out (−)" onclick={() => zoomBy(1 / 1.25)}><Icon name="minus" size={13} /></button>
  <button type="button" aria-label={selected ? "Show every agent" : "Fit the map"} title={selected ? "Show every agent (0)" : "Fit (0)"} onclick={resetView}><Icon name="fit" size={13} /></button>
</div>
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<svg bind:this={svg} viewBox="{box.x} {box.y} {box.w} {box.h}" class="map" class:grabbing={!!drag} role="group" aria-label="Machine map. Scroll or press + and − to zoom, drag to pan, 0 to fit." style="--k: {k}" tabindex="0"
  {onwheel} onpointerdown={ondown} onpointermove={onmove} onpointerup={onup} onpointercancel={onup} onkeydown={mapKey}>
  <defs>
    <radialGradient id="map-glow"><stop offset="0" stop-color="var(--accent)" stop-opacity="0.10" /><stop offset="1" stop-color="var(--accent)" stop-opacity="0" /></radialGradient>
  </defs>
  <ellipse rx={330 * MAP_STRETCH.x} ry={330 * MAP_STRETCH.y} fill="url(#map-glow)" />
  <ellipse rx={MAP_RINGS.inner * MAP_STRETCH.x} ry={MAP_RINGS.inner * MAP_STRETCH.y} class="orbit" />
  <ellipse rx={MAP_RINGS.outer * MAP_STRETCH.x} ry={MAP_RINGS.outer * MAP_STRETCH.y} class="orbit faint" />
  {#each groups as g (g.agent.id)}
    <g class="cluster" class:dim={active !== null && active !== g.agent.id} style="--agent: {colors.get(g.agent.id)}">
      <line class="spoke" x1="0" y1="0" x2={g.x} y2={g.y} />
      {#each g.nodes as n (n.item.id)}<path class="edge" class:live={n.item.live} d={curve(g.x, g.y, n.x, n.y)} />{/each}
    </g>
  {/each}
  <g class="core"><g class="body">
    <rect x="-70" y="-26" width="140" height="52" rx="13" />
    <text y="-3" class="core-t">This computer</text>
    <text y="15" class="core-s">{platform}{ports !== null ? ` · ${ports} ports` : ""}</text>
  </g></g>
  {#each groups as g, i (g.agent.id)}
    {@const head = accessHeadline(g.agent)}
    <g class="cluster" class:dim={active !== null && active !== g.agent.id} style="--agent: {colors.get(g.agent.id)}">
      {#each g.nodes as n (n.item.id)}
        {@const right = Math.cos(n.angle) >= -0.01}
        <g class="sat k-{n.item.kind}" class:warn={n.item.warn} class:idle={!n.item.live} transform="translate({n.x},{n.y})" role="button" tabindex="0"
          aria-label="{KIND_LABEL(n.item)}{n.item.count ? '' : ` ${n.item.full ?? n.item.label}`}{!n.item.full && n.item.sub ? `, ${n.item.sub}` : ''}{n.item.warn ? ', needs attention' : ''}, {g.agent.name}"
          aria-expanded={n.item.count ? !!n.item.collapse : undefined}
          onmouseenter={() => onhover(g.agent.id)} onmouseleave={() => onhover(null)} onfocus={() => onhover(g.agent.id)} onblur={() => onhover(null)}
          aria-haspopup={n.item.target.view === "list" ? "menu" : undefined} onclick={(e) => activate(n.item.target, e.currentTarget)} onkeydown={(e) => nodeKey(e, n.item.target)}><g class="body">
          <title>{n.item.collapse ? "Group the remote hosts again" : n.item.count ? `${n.item.count} remote hosts: click to show each` : n.item.full ?? (n.item.sub ? `${n.item.label} · ${n.item.sub}` : n.item.label)}</title>
          <circle r="16" class="hit" />
          {#if n.item.warn}<circle class="halo" r="12" />{/if}
          {#if n.item.kind === "project"}<rect x="-5.5" y="-5.5" width="11" height="11" rx="3" />
          {:else if n.item.kind === "mcp"}<rect x="-5" y="-5" width="10" height="10" rx="2" transform="rotate(45)" />
          {:else if n.item.count}<rect class="chip" x="-11" y="-8" width="22" height="16" rx="8" /><text class="chip-n" y="3.5">{n.item.collapse ? "−" : n.item.count}</text>
          {:else}<circle r="5.5" class:ring={n.item.kind === "remote"} />{/if}
          <text x={right ? (n.item.count ? 16 : 12) : (n.item.count ? -16 : -12)} y="-1" text-anchor={right ? "start" : "end"} class="l">{n.item.label}</text>
          {#if n.item.sub}{" "}<text x={right ? (n.item.count ? 16 : 12) : (n.item.count ? -16 : -12)} y="12" text-anchor={right ? "start" : "end"} class="s">{n.item.sub}</text>{/if}
        </g></g>
      {/each}
      <g class="agent" class:hot={hot.has(g.agent.id)} class:selected={selected === g.agent.id} transform="translate({g.x},{g.y})" role="button" tabindex="0" id="map-agent-{g.agent.id}"
        aria-pressed={selected === g.agent.id} aria-label="{g.agent.name}, {ACCESS_LABEL[head.level]}, {g.nodes.length} linked"
        onmouseenter={() => onhover(g.agent.id)} onmouseleave={() => onhover(null)} onfocus={() => onhover(g.agent.id)} onblur={() => onhover(null)}
        onclick={() => (selected === g.agent.id ? onopen({ view: "agents", agentId: g.agent.id }) : onselect(g.agent.id))} onkeydown={(e) => agentKey(e, i, g.agent.id)}><g class="body">
        {#if hot.has(g.agent.id)}<circle class="halo hot" r="38" />{/if}
        <circle class="ring-sel" r="33" />
        <circle class="disc" r="26" />
        <text y={g.y < 0 ? -55 : 46} class="an">{g.agent.name}</text>{" "}
        <text y={g.y < 0 ? -40 : 61} class="as lv-{head.level}">{ACCESS_LABEL[head.level]}</text>
      </g></g>
      <!-- The monogram sits outside the button so its name starts with the words shown under it. -->
      <g class="agent-mono" transform="translate({g.x},{g.y})" aria-hidden="true"><g class="body"><text y="5" class="mono">{monogram(g.agent.name)}</text></g></g>
    </g>
  {/each}
</svg>
</div>

<style>
  .fit { position: relative; width: 100%; height: 100%; }
  .zoom { position: absolute; z-index: 1; top: 8px; right: 8px; display: flex; gap: 2px; padding: 2px; border: 1px solid var(--border); border-radius: 9px; background: var(--surface); box-shadow: var(--shadow-sm); }
  .zoom button { display: grid; place-items: center; width: 26px; height: 26px; border: 0; border-radius: 7px; background: transparent; color: var(--text-2); cursor: pointer; }
  .zoom button:hover { background: var(--row-hover); color: var(--text); }
  .zoom button:focus-visible, .map:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .map { cursor: grab; touch-action: none; } .map.grabbing { cursor: grabbing; }
  .sat .chip { fill: var(--surface); stroke: var(--text-2); stroke-width: 1.5; }
  .chip-n { text-anchor: middle; font-size: var(--fs-caption); font-weight: var(--fw-semibold); fill: var(--text); pointer-events: none; }
  .body { transform: scale(var(--k)); }
  .map { width: 100%; height: 100%; display: block; font-family: inherit; overflow: visible; }
  .orbit { fill: none; stroke: var(--border); stroke-dasharray: 2 6; }
  .orbit.faint { stroke-dasharray: 1 9; }
  .spoke { stroke: color-mix(in srgb, var(--agent) 50%, transparent); stroke-width: 1.5; }
  .edge { fill: none; stroke: var(--border-strong); stroke-width: 1; stroke-dasharray: 3 3; }
  .edge.live { stroke: color-mix(in srgb, var(--agent) 42%, transparent); stroke-dasharray: none; }
  .cluster { transition: opacity 160ms ease; }
  .cluster.dim { opacity: 0.14; }
  .core rect { fill: var(--surface); stroke: var(--border-strong); }
  .core text { text-anchor: middle; }
  .core-t { font-size: var(--fs-body); font-weight: var(--fw-semibold); fill: var(--text); }
  .core-s { font-size: var(--fs-caption); fill: var(--muted); }
  .agent, .sat { cursor: pointer; outline: none; }
  .hit { fill: transparent; stroke: none; }
  .disc { fill: color-mix(in srgb, var(--agent) 15%, var(--surface)); stroke: var(--agent); stroke-width: 1.5; }
  .ring-sel { fill: none; stroke: transparent; stroke-width: 2; }
  .agent.selected .ring-sel, .agent:focus-visible .ring-sel { stroke: var(--agent); stroke-dasharray: 3 3; }
  .agent:hover .disc { stroke-width: 2.5; }
  .agent text, .agent-mono text { text-anchor: middle; }
  .agent-mono { pointer-events: none; }
  .mono { font-size: var(--fs-heading); font-weight: var(--fw-semibold); fill: var(--agent); }
  .an { font-size: var(--fs-body); font-weight: var(--fw-semibold); fill: var(--text); }
  .as { font-size: var(--fs-caption); fill: var(--muted); }
  .as.lv-elevated { fill: var(--danger); } .as.lv-restricted { fill: var(--ok); }
  .sat rect, .sat circle:not(.halo):not(.hit) { fill: var(--surface); stroke: var(--text-2); stroke-width: 1.5; }
  .sat.k-port circle:not(.halo):not(.hit) { fill: color-mix(in srgb, var(--agent) 60%, var(--surface)); stroke: var(--agent); }
  .sat.k-mcp:not(.idle) rect { fill: var(--ok-soft); stroke: var(--ok); }
  .sat.idle rect, .sat.idle circle:not(.halo):not(.hit) { stroke-dasharray: 2 2; }
  .sat.warn rect, .sat.warn circle:not(.halo):not(.hit) { stroke: var(--warn); }
  .halo { fill: var(--warn-soft); stroke: color-mix(in srgb, var(--warn) 50%, transparent); }
  .halo.hot { fill: var(--danger-soft); stroke: color-mix(in srgb, var(--danger) 45%, transparent); }
  .sat:hover .l, .sat:focus-visible .l { text-decoration: underline; }
  .sat:focus-visible rect, .sat:focus-visible circle:not(.halo):not(.hit) { stroke: var(--accent); stroke-width: 2.5; }
  .l { font-size: var(--fs-body-sm); font-weight: var(--fw-medium); fill: var(--text); }
  .s { font-size: var(--fs-caption); fill: var(--muted); }
  @media (prefers-reduced-motion: no-preference) { .halo { animation: breathe 2.8s ease-in-out infinite; } }
  @keyframes breathe { 50% { opacity: 0.45; } }
</style>

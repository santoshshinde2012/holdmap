<script lang="ts">
  // Radial map of this computer: agents on an inner ring, each agent's footprint fanned out in its
  // own sector. Attention glows on the exact node. The active agent (hovered or selected, shared
  // with the lanes) stays lit while the others dim. ←/→ move between agents.
  import { ACCESS_LABEL } from "../../lib/home";
  import { AGENT_COLORS, accessHeadline, monogram } from "../../lib/agents";
  import { MAP_RINGS, MAP_STRETCH, mapBounds, type FootprintNode, type radialLayout } from "../../lib/machine-map";

  type Group = ReturnType<typeof radialLayout>[number];
  let { groups, platform, ports, active, selected, hot, onhover, onselect, onopen }: {
    groups: Group[];
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
  const base = $derived(mapBounds(groups, 1));
  const k = $derived(w && h ? Math.min(1.8, Math.max(1, Math.sqrt(1 / Math.min(w / base.w, h / base.h)))) : 1);
  const box = $derived(mapBounds(groups, k));
  const nodeKey = (e: KeyboardEvent, t: FootprintNode["target"]) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onopen(t); } };
</script>

<div class="fit" bind:clientWidth={w} bind:clientHeight={h}>
<svg viewBox="{box.x} {box.y} {box.w} {box.h}" class="map" role="group" aria-label="Machine map" style="--k: {k}">
  <defs>
    <radialGradient id="map-glow"><stop offset="0" stop-color="var(--accent)" stop-opacity="0.10" /><stop offset="1" stop-color="var(--accent)" stop-opacity="0" /></radialGradient>
  </defs>
  <ellipse rx={330 * MAP_STRETCH.x} ry={330 * MAP_STRETCH.y} fill="url(#map-glow)" />
  <ellipse rx={MAP_RINGS.inner * MAP_STRETCH.x} ry={MAP_RINGS.inner * MAP_STRETCH.y} class="orbit" />
  <ellipse rx={MAP_RINGS.outer * MAP_STRETCH.x} ry={MAP_RINGS.outer * MAP_STRETCH.y} class="orbit faint" />
  {#each groups as g, i (g.agent.id)}
    <g class="cluster" class:dim={active !== null && active !== g.agent.id} style="--agent: {AGENT_COLORS[i % AGENT_COLORS.length]}">
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
    <g class="cluster" class:dim={active !== null && active !== g.agent.id} style="--agent: {AGENT_COLORS[i % AGENT_COLORS.length]}">
      {#each g.nodes as n (n.item.id)}
        {@const right = Math.cos(n.angle) >= -0.01}
        <g class="sat k-{n.item.kind}" class:warn={n.item.warn} class:idle={!n.item.live} transform="translate({n.x},{n.y})" role="button" tabindex="0"
          aria-label="{KIND[n.item.kind]} {n.item.label}{n.item.sub ? `, ${n.item.sub}` : ''}{n.item.warn ? ', needs attention' : ''}, {g.agent.name}"
          onmouseenter={() => onhover(g.agent.id)} onmouseleave={() => onhover(null)} onfocus={() => onhover(g.agent.id)} onblur={() => onhover(null)}
          onclick={() => onopen(n.item.target)} onkeydown={(e) => nodeKey(e, n.item.target)}><g class="body">
          <circle r="16" class="hit" />
          {#if n.item.warn}<circle class="halo" r="12" />{/if}
          {#if n.item.kind === "project"}<rect x="-5.5" y="-5.5" width="11" height="11" rx="3" />
          {:else if n.item.kind === "mcp"}<rect x="-5" y="-5" width="10" height="10" rx="2" transform="rotate(45)" />
          {:else}<circle r="5.5" class:ring={n.item.kind === "remote"} />{/if}
          <text x={right ? 12 : -12} y="-1" text-anchor={right ? "start" : "end"} class="l">{n.item.label}</text>
          {#if n.item.sub}<text x={right ? 12 : -12} y="12" text-anchor={right ? "start" : "end"} class="s">{n.item.sub}</text>{/if}
        </g></g>
      {/each}
      <g class="agent" class:hot={hot.has(g.agent.id)} class:selected={selected === g.agent.id} transform="translate({g.x},{g.y})" role="button" tabindex="0" id="map-agent-{g.agent.id}"
        aria-pressed={selected === g.agent.id} aria-label="{g.agent.name}, {ACCESS_LABEL[head.level]}, {g.nodes.length} linked"
        onmouseenter={() => onhover(g.agent.id)} onmouseleave={() => onhover(null)} onfocus={() => onhover(g.agent.id)} onblur={() => onhover(null)}
        onclick={() => (selected === g.agent.id ? onopen({ view: "agents", agentId: g.agent.id }) : onselect(g.agent.id))} onkeydown={(e) => agentKey(e, i, g.agent.id)}><g class="body">
        {#if hot.has(g.agent.id)}<circle class="halo hot" r="38" />{/if}
        <circle class="ring-sel" r="33" />
        <circle class="disc" r="26" />
        <text y="5" class="mono">{monogram(g.agent.name)}</text>
        <text y={g.y < 0 ? -55 : 46} class="an">{g.agent.name}</text>
        <text y={g.y < 0 ? -40 : 61} class="as lv-{head.level}">{ACCESS_LABEL[head.level]}</text>
      </g></g>
    </g>
  {/each}
</svg>
</div>

<style>
  .fit { width: 100%; height: 100%; }
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
  .agent text { text-anchor: middle; }
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

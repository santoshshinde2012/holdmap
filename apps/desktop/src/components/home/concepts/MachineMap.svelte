<script lang="ts">
  // Concept A, "Machine map": this computer at the centre, agents on an inner ring and everything
  // each one reaches fanned out in its own sector. Attention glows on the exact node. Hover or
  // focus isolates one agent's footprint; ←/→ move between agents, Enter opens.
  import AttentionList from "../AttentionList.svelte";
  import { AGENT_COLORS, monogram } from "../../../lib/agents";
  import { ACCESS_LABEL, attention, overview, type AttentionItem } from "../../../lib/home";
  import { radialLayout, type FootprintNode } from "../../../lib/home-concepts";
  import { accessHeadline } from "../../../lib/agents";
  import type { AgentsReport, PortEntry } from "../../../lib/types";

  let { entries, report, onopenagent, onopenentry, onstopentry, onstopagent }: {
    entries: PortEntry[] | null; report: AgentsReport | null;
    onopenagent: (id: string) => void; onopenentry: (id: string) => void;
    onstopentry?: (id: string) => void; onstopagent?: (id: string) => void;
  } = $props();

  const groups = $derived(radialLayout(report?.agents ?? [], 140, 285));
  const needs = $derived(attention(entries, report));
  const o = $derived(overview(entries, report));
  let focus = $state<string | null>(null);
  const focused = $derived(groups.find((g) => g.agent.id === focus) ?? null);
  /** Agents whose attention is about the agent itself (root, elevated, risky settings); port and MCP issues glow on their own node. */
  const hot = $derived(new Set(needs.flatMap((n) => (n.target.view === "agents" && ["root", "elevated", "settings"].includes(n.rule) ? [n.target.agentId] : []))));

  const SHAPE: Record<FootprintNode["kind"], string> = { project: "Project folder", port: "Listening port", mcp: "MCP server", remote: "Remote host" };
  function open(t: FootprintNode["target"]) { if (t.view === "list") onopenentry(t.entryId); else onopenagent(t.agentId); }
  function openItem(i: AttentionItem) { open(i.target); }
  function stopItem(i: AttentionItem) { if (i.stop?.kind === "entry") onstopentry?.(i.stop.entryId); else if (i.stop) onstopagent?.(i.stop.agentId); }
  function key(e: KeyboardEvent, i: number, go: () => void) {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); go(); }
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const next = groups[(i + (e.key === "ArrowRight" ? 1 : groups.length - 1)) % groups.length];
      document.getElementById(`map-agent-${next.agent.id}`)?.focus();
    }
  }
  const curve = (ax: number, ay: number, bx: number, by: number) => `M${ax},${ay} Q${(ax + bx) / 2 * 1.08},${(ay + by) / 2 * 1.08} ${bx},${by}`;
</script>

<div class="concept" role="region" aria-label="Overview">
  <header class="bar">
    <h1>This computer</h1>
    <span>{o.agents ?? "…"} agents · {o.ports ?? "…"} listening · {o.devServers ?? "…"} dev servers{#if o.mcpConfigured !== null} · {o.mcpRunning}/{o.mcpConfigured} MCP running{/if}</span>
    <span class="legend" aria-label="Legend">
      <span><i class="k project"></i>Project</span><span><i class="k port"></i>Port</span><span><i class="k mcp"></i>MCP</span><span><i class="k remote"></i>Remote</span><span><i class="k warn"></i>Needs you</span>
    </span>
  </header>
  <div class="body">
    <div class="stage">
      <svg viewBox="-520 -420 1040 840" role="group" aria-label="Machine map" class:isolating={focus !== null}>
        {#each groups as g, i (g.agent.id)}
          {@const color = AGENT_COLORS[i % AGENT_COLORS.length]}
          <g class="cluster" class:dim={focus !== null && focus !== g.agent.id} style="--agent: {color}">
            <line class="spoke" x1="0" y1="0" x2={g.x} y2={g.y} />
            {#each g.nodes as n (n.item.id)}<path class="edge" class:live={n.item.live} d={curve(g.x, g.y, n.x, n.y)} />{/each}
          </g>
        {/each}
        <g class="core">
          <rect x="-74" y="-30" width="148" height="60" rx="14" />
          <text y="-4" class="core-t">This computer</text>
          <text y="16" class="core-s">{report?.platform ?? ""} · {o.ports ?? 0} ports</text>
        </g>
        {#each groups as g, i (g.agent.id)}
          {@const color = AGENT_COLORS[i % AGENT_COLORS.length]}
          {@const head = accessHeadline(g.agent)}
          <g class="cluster" class:dim={focus !== null && focus !== g.agent.id} style="--agent: {color}">
            {#each g.nodes as n (n.item.id)}
              {@const right = Math.cos(n.angle) >= -0.01}
              <g class="sat k-{n.item.kind}" class:warn={n.item.warn} class:idle={!n.item.live} transform="translate({n.x},{n.y})" role="button" tabindex="0"
                aria-label="{SHAPE[n.item.kind]} {n.item.label}{n.item.sub ? `, ${n.item.sub}` : ''}{n.item.warn ? ', needs attention' : ''} ({g.agent.name})"
                onmouseenter={() => (focus = g.agent.id)} onmouseleave={() => (focus = null)} onfocus={() => (focus = g.agent.id)} onblur={() => (focus = null)}
                onclick={() => open(n.item.target)} onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); open(n.item.target); } }}>
                {#if n.item.warn}<circle class="halo" r="13" />{/if}
                {#if n.item.kind === "project"}<rect x="-6" y="-6" width="12" height="12" rx="3" />
                {:else if n.item.kind === "mcp"}<rect x="-5.5" y="-5.5" width="11" height="11" rx="2" transform="rotate(45)" />
                {:else if n.item.kind === "remote"}<circle r="6" class="ring" />
                {:else}<circle r="6" />{/if}
                <text x={right ? 12 : -12} y="-1" text-anchor={right ? "start" : "end"} class="l">{n.item.label}</text>
                {#if n.item.sub}<text x={right ? 12 : -12} y="12" text-anchor={right ? "start" : "end"} class="s">{n.item.sub}</text>{/if}
              </g>
            {/each}
            <g class="agent" class:hot={hot.has(g.agent.id)} transform="translate({g.x},{g.y})" role="button" tabindex="0" id="map-agent-{g.agent.id}"
              aria-label="{g.agent.name}, {ACCESS_LABEL[head.level]} access, {g.nodes.length} items in its footprint"
              onmouseenter={() => (focus = g.agent.id)} onmouseleave={() => (focus = null)} onfocus={() => (focus = g.agent.id)} onblur={() => (focus = null)}
              onclick={() => onopenagent(g.agent.id)} onkeydown={(e) => key(e, i, () => onopenagent(g.agent.id))}>
              {#if hot.has(g.agent.id)}<circle class="halo" r="38" />{/if}
              <circle class="disc" r="27" />
              <text y="5" class="mono">{monogram(g.agent.name)}</text>
              <text y="46" class="an">{g.agent.name}</text>
              <text y="61" class="as lv-{head.level}">{ACCESS_LABEL[head.level]}</text>
            </g>
          </g>
        {/each}
      </svg>
    </div>
    <aside class="side">
      <section>
        <h2>Needs you {#if needs.length}<span class="count">{needs.length}</span>{/if}</h2>
        {#if needs.length}<AttentionList items={needs} onopen={openItem} onstop={stopItem} />{:else}<p class="quiet">Nothing needs you.</p>{/if}
      </section>
      <section class="focus">
        {#if focused}
          {@const fh = accessHeadline(focused.agent)}
          <h2>{focused.agent.name}</h2>
          <p class="quiet">{fh.text}</p>
          <ul>{#each Object.keys(SHAPE) as k}{@const c = focused.nodes.filter((n) => n.item.kind === k).length}{#if c}<li><i class="k {k}"></i>{c} {SHAPE[k as FootprintNode["kind"]].toLowerCase()}{c === 1 ? "" : "s"}</li>{/if}{/each}</ul>
        {:else}
          <h2>Footprint</h2><p class="quiet">Hover or focus an agent to isolate what it reaches. ←/→ move between agents.</p>
        {/if}
      </section>
    </aside>
  </div>
</div>

<style>
  .concept { min-height: 0; overflow-y: auto; display: grid; grid-template-rows: auto 1fr; gap: var(--sp-3); padding: var(--sp-4) var(--sp-5); }
  .bar { display: flex; align-items: baseline; gap: var(--sp-3); flex-wrap: wrap; color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  h1 { margin: 0; font-size: var(--fs-title); line-height: var(--lh-title); color: var(--text); font-weight: var(--fw-semibold); }
  .legend { margin-left: auto; display: flex; gap: var(--sp-3); flex-wrap: wrap; font-size: var(--fs-caption); }
  .legend span, .focus li { display: inline-flex; align-items: center; gap: 5px; }
  .k { display: inline-block; width: 9px; height: 9px; border: 1.5px solid var(--text-2); }
  .k.project { border-radius: 2px; } .k.port { border-radius: 50%; background: var(--text-2); } .k.mcp { transform: rotate(45deg) scale(.85); border-radius: 1px; } .k.remote { border-radius: 50%; }
  .k.warn { border-color: var(--warn); border-radius: 50%; box-shadow: 0 0 0 3px var(--warn-soft); }
  .body { display: grid; grid-template-columns: minmax(0, 1fr) 360px; gap: var(--sp-4); min-height: 0; }
  .stage { min-height: 560px; border: 1px solid var(--border); border-radius: var(--r-lg); background: radial-gradient(circle at 50% 50%, var(--accent-softer), transparent 60%), var(--surface); display: grid; }
  svg { width: 100%; height: 100%; max-height: calc(100vh - 170px); font-family: inherit; }
  .spoke { stroke: color-mix(in srgb, var(--agent) 45%, transparent); stroke-width: 1.5; }
  .edge { fill: none; stroke: var(--border-strong); stroke-width: 1; stroke-dasharray: 3 3; }
  .edge.live { stroke: color-mix(in srgb, var(--agent) 38%, transparent); stroke-dasharray: none; }
  .cluster { transition: opacity 160ms ease; }
  .cluster.dim { opacity: 0.16; }
  .core rect { fill: var(--surface); stroke: var(--border-strong); }
  .core text { text-anchor: middle; }
  .core-t { font-size: var(--fs-body); font-weight: var(--fw-semibold); fill: var(--text); }
  .core-s { font-size: var(--fs-caption); fill: var(--muted); }
  .agent, .sat { cursor: pointer; outline: none; }
  .disc { fill: color-mix(in srgb, var(--agent) 14%, var(--surface)); stroke: var(--agent); stroke-width: 1.5; }
  .agent:focus-visible .disc, .agent:hover .disc { stroke-width: 3; }
  .agent text { text-anchor: middle; }
  .mono { font-size: var(--fs-heading); font-weight: var(--fw-semibold); fill: var(--agent); }
  .an { font-size: var(--fs-body); font-weight: var(--fw-semibold); fill: var(--text); }
  .as { font-size: var(--fs-caption); fill: var(--muted); }
  .as.lv-elevated { fill: var(--danger); } .as.lv-restricted { fill: var(--ok); }
  .sat rect, .sat circle:not(.halo) { fill: var(--surface); stroke: var(--text-2); stroke-width: 1.5; }
  .sat.k-port circle:not(.halo) { fill: color-mix(in srgb, var(--agent) 55%, var(--surface)); stroke: var(--agent); }
  .sat.k-mcp:not(.idle) rect { fill: var(--ok-soft); stroke: var(--ok); }
  .sat.idle rect, .sat.idle circle:not(.halo) { stroke-dasharray: 2 2; }
  .sat.warn rect, .sat.warn circle:not(.halo) { stroke: var(--warn); }
  .halo { fill: var(--warn-soft); stroke: color-mix(in srgb, var(--warn) 50%, transparent); }
  .agent.hot .halo { fill: var(--danger-soft); stroke: color-mix(in srgb, var(--danger) 45%, transparent); }
  .sat:focus-visible .l, .sat:hover .l { text-decoration: underline; }
  .sat:focus-visible rect, .sat:focus-visible circle:not(.halo) { stroke: var(--accent); stroke-width: 2.5; }
  .l { font-size: var(--fs-body-sm); font-weight: var(--fw-medium); fill: var(--text); }
  .s { font-size: var(--fs-caption); fill: var(--muted); }
  @media (prefers-reduced-motion: no-preference) { .halo { animation: breathe 2.8s ease-in-out infinite; } }
  @keyframes breathe { 50% { opacity: 0.45; } }
  .side { display: grid; gap: var(--sp-4); align-content: start; }
  .side section { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); padding: var(--sp-3) 0 var(--sp-2); }
  h2 { margin: 0 var(--sp-4) var(--sp-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); display: flex; gap: var(--sp-2); align-items: center; }
  .count { padding: 0 7px; border-radius: var(--r-full); background: var(--warn-soft); color: var(--warn); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .quiet { margin: 0 var(--sp-4); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .focus ul { list-style: none; margin: var(--sp-2) var(--sp-4) 0; padding: 0; display: grid; gap: 4px; font-size: var(--fs-caption); color: var(--text-2); }
  .side :global(.item) { grid-template-columns: 58px minmax(0, 1fr); }
  .side :global(.actions) { grid-column: 2; justify-content: start; }
  @media (max-width: 1100px) { .body { grid-template-columns: minmax(0, 1fr); } .stage { min-height: 0; } svg { max-height: none; } .side { grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); } }
</style>

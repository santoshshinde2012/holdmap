<script lang="ts">
  // Concept C, "Briefing": the home reads like a status brief written from the latest scan, then
  // one card per agent with a small footprint drawing and its next safe action. Every name, port
  // and project in the brief is a link. Tab moves through links and cards in reading order.
  import Icon from "../../Icon.svelte";
  import { AGENT_COLORS, KIND_LABEL, monogram, accessHeadline } from "../../../lib/agents";
  import { configStatus } from "../../../lib/agent-config";
  import { ACCESS_LABEL, type AttentionItem } from "../../../lib/home";
  import { briefing, footprint, type BriefPart, type FootprintNode } from "../../../lib/home-concepts";
  import { humanDuration } from "../../../lib/format";
  import type { AgentsReport, PortEntry } from "../../../lib/types";

  let { entries, report, now = Date.now(), inventory = false, onopenagent, onopenentry, onstopentry, onstopagent, oninventory }: {
    entries: PortEntry[] | null; report: AgentsReport | null; now?: number; inventory?: boolean;
    onopenagent: (id: string) => void; onopenentry: (id: string) => void;
    onstopentry?: (id: string) => void; onstopagent?: (id: string) => void; oninventory?: () => void;
  } = $props();

  const brief = $derived(briefing(entries, report));
  const owned = $derived(new Set((report?.agents ?? []).flatMap((a) => a.ports.map((p) => p.entry_id))));
  const loose = $derived((entries ?? []).filter((e) => e.is_dev && !owned.has(e.id)));
  const when = $derived(new Date(now).toLocaleString([], { weekday: "long", hour: "numeric", minute: "2-digit" }));
  function go(t: BriefPart["target"] | FootprintNode["target"]) { if (!t) return; if (t.view === "list") onopenentry(t.entryId); else onopenagent(t.agentId); }
  function stop(i: AttentionItem) { if (i.stop?.kind === "entry") onstopentry?.(i.stop.entryId); else if (i.stop) onstopagent?.(i.stop.agentId); }
  /** The agent's footprint on a small circle around its disc. */
  function mini(nodes: FootprintNode[]) {
    return nodes.map((n, i) => { const a = -Math.PI / 2 + (i / Math.max(1, nodes.length)) * Math.PI * 2; return { n, x: Math.cos(a) * 48, y: Math.sin(a) * 48 }; });
  }
</script>

<div class="concept" role="region" aria-label="Overview">
  <article class="brief">
    <p class="when">{when}{#if entries} · updated from the latest scan{/if}</p>
    <p class="lede">
      {#each brief.parts as p, i (i)}{#if p.target}<button type="button" class="ref" onclick={() => go(p.target)}>{p.text}</button>{:else}{p.text}{/if}{/each}
    </p>
    {#if brief.items.length}
      <ol class="needs" aria-label="Needs you">
        {#each brief.items as item (item.id)}
          <li class="sev-{item.severity}">
            <span class="dot" aria-hidden="true"></span>
            <div><b>{item.title}</b><span>{item.detail}</span></div>
            <div class="acts">
              {#if item.stop}<button type="button" class="btn" onclick={() => stop(item)}>Review stop plan</button>{/if}
              <button type="button" class="btn ghost" onclick={() => go(item.target)}>Open</button>
            </div>
          </li>
        {/each}
      </ol>
    {/if}
  </article>

  <section class="cards" aria-label="Agents">
    {#each report?.agents ?? [] as a, i (a.id)}
      {@const head = accessHeadline(a)}
      {@const st = configStatus(a)}
      {@const fp = footprint(a, 12)}
      <article class="card" style="--agent: {AGENT_COLORS[i % AGENT_COLORS.length]}" aria-label={a.name}>
        <header>
          <span class="mono">{monogram(a.name)}</span>
          <div class="ttl"><button type="button" class="name" onclick={() => onopenagent(a.id)}>{a.name}</button><small>{KIND_LABEL[a.kind]} · up {humanDuration(Math.max(0, now / 1000 - a.started_at))}</small></div>
          <span class="lv lv-{head.level}">{ACCESS_LABEL[head.level]}</span>
        </header>
        <div class="mid">
          <svg viewBox="-76 -76 152 152" class="fp" role="group" aria-label="{a.name} footprint">
            {#each mini(fp) as m (m.n.id)}<line x1="0" y1="0" x2={m.x} y2={m.y} class:live={m.n.live} />{/each}
            <circle r="17" class="disc" />
            <text y="4">{monogram(a.name)}</text>
            {#each mini(fp) as m (m.n.id)}
              <g transform="translate({m.x},{m.y})" class="n k-{m.n.kind}" class:warn={m.n.warn} class:idle={!m.n.live} role="button" tabindex="-1" aria-label="{m.n.label}" onclick={() => go(m.n.target)} onkeydown={(e) => e.key === "Enter" && go(m.n.target)}>
                {#if m.n.warn}<circle r="9" class="halo" />{/if}
                {#if m.n.kind === "project"}<rect x="-4.5" y="-4.5" width="9" height="9" rx="2" />{:else if m.n.kind === "mcp"}<rect x="-4" y="-4" width="8" height="8" rx="1.5" transform="rotate(45)" />{:else}<circle r="4.5" class:ring={m.n.kind === "remote"} />{/if}
              </g>
            {/each}
          </svg>
          <dl>
            <dt>Works in</dt><dd>{#each [...new Set(a.folders.filter((f) => f.source !== "recent").map((f) => f.project?.name ?? f.label))] as p}<span class="tag"><Icon name="folder" size={11} />{p}</span>{:else}<span class="faint">No project folder</span>{/each}</dd>
            <dt>Holds</dt><dd>{#each a.ports as p (p.entry_id)}<button type="button" class="chip" class:warn={p.exposure === "all_interfaces"} onclick={() => onopenentry(p.entry_id)}>:{p.port}</button>{:else}<span class="faint">No ports</span>{/each}</dd>
            <dt>MCP</dt><dd>{#if st.kind === "read"}<b class:on={st.summary.running > 0}>{st.summary.running}</b>&nbsp;of {st.summary.mcp} running · {st.summary.skills + st.summary.instructions} skills{:else if st.kind === "unsupported"}<span class="faint">Not supported for {a.name}</span>{:else}<span class="faint">Not read</span>{/if}</dd>
            <dt>Access</dt><dd class="why">{head.text}</dd>
          </dl>
        </div>
        <footer>
          <button type="button" class="btn ghost" onclick={() => onopenagent(a.id)}>Open {a.name}</button>
          {#if a.ports.length}<button type="button" class="btn ghost" onclick={() => onstopagent?.(a.id)}>Review stop plan</button>{/if}
          {#if st.kind === "off" && oninventory && !inventory}<button type="button" class="btn ghost" onclick={oninventory}>Read config</button>{/if}
        </footer>
      </article>
    {/each}
  </section>
  {#if loose.length}
    <p class="loose">Also listening, not started by an agent:
      {#each loose as e (e.id)}<button type="button" class="chip" class:warn={e.exposure === "all_interfaces"} onclick={() => onopenentry(e.id)}>:{e.port} {e.project?.name ?? e.framework?.name ?? e.label}</button>{/each}
    </p>
  {/if}
</div>

<style>
  .concept { min-height: 0; overflow-y: auto; padding: var(--sp-8) var(--sp-8) var(--sp-8); display: grid; gap: var(--sp-6); align-content: start; max-width: 1360px; width: 100%; margin: 0 auto; }
  .brief { display: grid; grid-template-columns: minmax(0, 1.25fr) minmax(320px, 1fr); grid-template-areas: "when when" "lede needs"; gap: var(--sp-3) var(--sp-8); align-items: start; }
  .when { grid-area: when; } .lede { grid-area: lede; } .needs { grid-area: needs; }
  @media (max-width: 1180px) { .brief { grid-template-columns: minmax(0, 1fr); grid-template-areas: "when" "lede" "needs"; } }
  .when { margin: 0; color: var(--muted); font-size: var(--fs-caption); letter-spacing: var(--ls-label); text-transform: uppercase; font-weight: var(--fw-medium); }
  .lede { margin: 0; font-size: var(--fs-display); line-height: var(--lh-display); letter-spacing: var(--ls-display); color: var(--text-2); font-weight: var(--fw-regular); text-wrap: pretty; }
  .ref { border: 0; padding: 0; background: none; font: inherit; color: var(--text); font-weight: var(--fw-semibold); cursor: pointer; text-decoration: underline; text-decoration-color: var(--border-strong); text-decoration-thickness: 1.5px; text-underline-offset: 4px; }
  .ref:hover { text-decoration-color: var(--accent); }
  .ref:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; border-radius: 3px; }
  .needs { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--sp-2); }
  .needs li { display: grid; grid-template-columns: 8px minmax(0, 1fr) auto; gap: var(--sp-3); align-items: center; padding: var(--sp-2) var(--sp-3); border-radius: var(--r-md); background: var(--surface); border: 1px solid var(--border); }
  .needs .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--warn); }
  .needs .sev-high .dot { background: var(--danger); box-shadow: 0 0 0 3px var(--danger-soft); }
  .needs li > div:nth-child(2) { display: grid; min-width: 0; }
  .needs b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); overflow-wrap: anywhere; }
  .needs span { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow-wrap: anywhere; }
  .acts { display: flex; gap: 6px; flex-wrap: wrap; }
  .btn { border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); border-radius: var(--r-md); padding: 3px 10px; font: inherit; font-size: var(--fs-caption); font-weight: var(--fw-medium); cursor: pointer; white-space: nowrap; }
  .btn.ghost { border-color: var(--border); color: var(--text-2); }
  .btn:hover { background: var(--row-hover); color: var(--text); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(360px, 1fr)); gap: var(--sp-4); }
  .card { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); padding: var(--sp-4); display: grid; gap: var(--sp-3); }
  .card header { display: flex; align-items: center; gap: var(--sp-2); }
  .mono { width: 30px; height: 30px; flex: none; display: grid; place-items: center; border-radius: 9px; background: color-mix(in srgb, var(--agent) 16%, var(--surface)); color: var(--agent); font-size: var(--fs-caption); font-weight: var(--fw-semibold); }
  .ttl { display: grid; flex: 1; min-width: 0; }
  .name { border: 0; padding: 0; background: none; font: inherit; text-align: left; font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); color: var(--text); cursor: pointer; }
  .ttl small { color: var(--muted); font-size: var(--fs-caption); }
  .lv { font-size: var(--fs-caption); padding: 1px 8px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); font-weight: var(--fw-medium); }
  .lv-restricted { background: var(--ok-soft); color: var(--ok); } .lv-elevated { background: var(--danger-soft); color: var(--danger); }
  .mid { display: grid; grid-template-columns: 128px minmax(0, 1fr); gap: var(--sp-4); align-items: center; }
  .fp { width: 128px; height: 128px; }
  .fp line { stroke: var(--border-strong); stroke-dasharray: 2 2; }
  .fp line.live { stroke: color-mix(in srgb, var(--agent) 45%, transparent); stroke-dasharray: none; }
  .fp .disc { fill: color-mix(in srgb, var(--agent) 14%, var(--surface)); stroke: var(--agent); stroke-width: 1.5; }
  .fp text { text-anchor: middle; font-size: var(--fs-caption); font-weight: var(--fw-semibold); fill: var(--agent); }
  .fp .n { cursor: pointer; }
  .fp .n rect, .fp .n circle:not(.halo) { fill: var(--surface); stroke: var(--text-2); stroke-width: 1.4; }
  .fp .k-port circle { fill: var(--agent) !important; stroke: var(--agent) !important; }
  .fp .k-mcp:not(.idle) rect { fill: var(--ok-soft); stroke: var(--ok); }
  .fp .idle rect, .fp .idle circle:not(.halo) { stroke-dasharray: 2 1.5; }
  .fp .warn rect, .fp .warn circle:not(.halo) { stroke: var(--warn) !important; }
  .fp .halo { fill: var(--warn-soft); }
  dl { margin: 0; display: grid; grid-template-columns: 64px minmax(0, 1fr); gap: 6px var(--sp-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  dt { color: var(--faint); }
  dd { margin: 0; display: flex; flex-wrap: wrap; gap: 4px; align-items: center; color: var(--text-2); min-width: 0; }
  dd b { color: var(--text); } dd b.on { color: var(--ok); }
  .why { overflow-wrap: anywhere; }
  .tag { display: inline-flex; align-items: center; gap: 3px; }
  .faint { color: var(--muted); }
  .chip { border: 1px solid var(--border); background: var(--surface); padding: 0 6px; border-radius: var(--r-full); font-family: var(--mono); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); cursor: pointer; }
  .chip.warn { color: var(--warn); background: var(--warn-soft); border-color: color-mix(in srgb, var(--warn) 40%, transparent); }
  .card footer { display: flex; gap: 6px; flex-wrap: wrap; border-top: 1px solid var(--border); padding-top: var(--sp-3); }
  .loose { margin: 0; display: flex; flex-wrap: wrap; gap: 6px; align-items: center; color: var(--muted); font-size: var(--fs-caption); }
  @media (max-width: 1100px) { .concept { padding: var(--sp-6); } .lede { font-size: var(--fs-heading); line-height: var(--lh-display); } }
  @media (max-width: 640px) { .concept { padding: var(--sp-4); } .cards { grid-template-columns: 1fr; } .mid { grid-template-columns: 1fr; } .fp { justify-self: center; } .needs li { grid-template-columns: 8px minmax(0, 1fr); } .acts { grid-column: 2; } }
</style>

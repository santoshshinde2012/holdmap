<script lang="ts">
  // Overview: what's running, what needs a look, and one click into any agent. Counts come from
  // the latest scan; anything not yet loaded reads as unknown rather than zero.
  import Icon from "../Icon.svelte";
  import { AGENT_COLORS, KIND_LABEL, accessHeadline, counts, countsLine, monogram } from "../../lib/agents";
  import { attention, overview, type AttentionItem } from "../../lib/home";
  import type { AgentsReport, PortEntry } from "../../lib/types";

  let { entries, report, agentsLoading = false, onopenagent, onopenentry, onview }: {
    entries: PortEntry[] | null;
    report: AgentsReport | null;
    agentsLoading?: boolean;
    onopenagent: (id: string) => void;
    onopenentry: (id: string) => void;
    onview: (view: "list" | "agents" | "graph") => void;
  } = $props();

  const o = $derived(overview(entries, report));
  const items = $derived(attention(entries, report));
  const show = (n: number | null) => (n === null ? "—" : String(n));
  function open(item: AttentionItem) {
    if (item.target.view === "agents") onopenagent(item.target.agentId);
    else onopenentry(item.target.entryId);
  }
  const tiles = $derived([
    { id: "agents", label: "Agents & tools", value: o.agents, icon: "bot", go: () => onview("agents") },
    { id: "dev", label: "Dev servers", value: o.devServers, icon: "code", go: () => onview("list") },
    { id: "ports", label: "Listening ports", value: o.ports, icon: "list", go: () => onview("list") },
    { id: "exposed", label: "Open to the network", value: o.exposed, icon: "globe", go: () => onview("list") },
  ]);
</script>

<div class="home" role="region" aria-label="Overview">
  <section class="tiles" aria-label="At a glance">
    {#each tiles as t (t.id)}
      <button type="button" class="tile" class:warn={t.id === "exposed" && !!t.value} onclick={t.go} aria-label="{t.label}: {t.value === null ? 'unknown' : t.value}">
        <span class="ti"><Icon name={t.icon} size={15} /></span>
        <span class="tv">{show(t.value)}</span>
        <span class="tl">{t.label}</span>
      </button>
    {/each}
  </section>

  <section class="panel" aria-labelledby="home-attention">
    <h2 id="home-attention">Needs a look {#if items.length}<span class="count">{items.length}</span>{/if}</h2>
    {#if items.length}
      <ul class="rows">
        {#each items as item (item.id)}
          <li><button type="button" class="row sev-{item.severity}" onclick={() => open(item)}>
            <span class="dot" aria-hidden="true"></span>
            <span class="main"><span class="title">{item.title}</span><span class="sub">{item.detail}</span></span>
            <Icon name="chevron" size={13} />
          </button></li>
        {/each}
      </ul>
    {:else if entries && report}
      <p class="quiet"><Icon name="check" size={14} />Nothing needs a look in the latest scan.</p>
    {:else}
      <p class="quiet" aria-busy="true">Waiting for the first scan…</p>
    {/if}
  </section>

  <section class="panel" aria-labelledby="home-agents">
    <h2 id="home-agents">Agents &amp; tools</h2>
    {#if report?.agents.length}
      <ul class="rows">
        {#each report.agents as agent, i (agent.id)}
          {@const head = accessHeadline(agent)}
          <li><button type="button" class="row agent" style="--agent: {AGENT_COLORS[i % AGENT_COLORS.length]}" onclick={() => onopenagent(agent.id)}>
            <span class="mono" aria-hidden="true">{monogram(agent.name)}</span>
            <span class="main"><span class="title">{agent.name} <span class="kind">{KIND_LABEL[agent.kind]}</span></span><span class="sub">{countsLine(counts(agent))}</span></span>
            <span class="acc lv-{head.level}">{head.text}</span>
            <Icon name="chevron" size={13} />
          </button></li>
        {/each}
      </ul>
    {:else if report}
      <p class="quiet">No agents or tools are running.</p>
    {:else}
      <p class="quiet" aria-busy={agentsLoading}>{agentsLoading ? "Looking for agents…" : "Agents not loaded yet."}</p>
    {/if}
  </section>
</div>

<style>
  .home { overflow-y: auto; min-height: 0; padding: var(--sp-5) var(--sp-5) var(--sp-8); display: grid; gap: var(--sp-5); align-content: start; max-width: 980px; width: 100%; margin: 0 auto; }
  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: var(--sp-3); }
  .tile { display: grid; grid-template-columns: auto 1fr; grid-template-areas: "icon value" "label label"; align-items: center; gap: var(--sp-1) var(--sp-2); padding: var(--sp-4); border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .tile:hover { border-color: var(--border-strong); }
  .tile:focus-visible, .row:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .ti { grid-area: icon; color: var(--muted); display: inline-flex; }
  .tv { grid-area: value; font-size: var(--fs-title); line-height: var(--lh-title); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
  .tl { grid-area: label; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .tile.warn .tv, .tile.warn .ti { color: var(--warn); }
  .panel { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); padding: var(--sp-3) var(--sp-2); }
  h2 { margin: 0 var(--sp-3) var(--sp-2); font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; font-weight: var(--fw-semibold); color: var(--muted); display: flex; align-items: center; gap: var(--sp-2); }
  .count { padding: 0 6px; border-radius: var(--r-full); background: var(--warn-soft); color: var(--warn); font-variant-numeric: tabular-nums; }
  .rows { list-style: none; margin: 0; padding: 0; display: grid; gap: 1px; }
  .row { width: 100%; display: flex; align-items: center; gap: var(--sp-3); padding: var(--sp-2) var(--sp-3); border: 0; border-radius: var(--r-md); background: transparent; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .row:hover { background: var(--row-hover); }
  .row > :global(svg) { color: var(--faint); flex: none; }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; background: var(--warn); }
  .sev-high .dot { background: var(--danger); }
  .main { flex: 1; min-width: 0; display: grid; }
  .title { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); overflow-wrap: anywhere; }
  .sub, .kind { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .kind { font-weight: var(--fw-regular); margin-left: 4px; }
  .mono { width: 28px; height: 28px; flex: none; display: grid; place-items: center; border-radius: 8px; background: color-mix(in srgb, var(--agent) 16%, var(--surface)); color: var(--agent); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); }
  .acc { color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); white-space: nowrap; }
  .acc.lv-elevated { color: var(--danger); }
  .quiet { margin: 0; padding: var(--sp-2) var(--sp-3); color: var(--muted); display: flex; align-items: center; gap: var(--sp-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  @container (max-width: 560px) { .acc { display: none; } }
  @media (max-width: 560px) { .acc { display: none; } .home { padding: var(--sp-3); } }
</style>

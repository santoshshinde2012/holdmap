<script lang="ts">
  // Home: the machine map as the hero, "Needs you" beside it and agent lanes below. Hovering or
  // selecting an agent lights it in the map, the lanes and its attention items together.
  // Narrow screens swap the map for a compact list; lanes stack. Esc clears the selection.
  import Icon from "../Icon.svelte";
  import HomeSummary, { type SummaryStat } from "./HomeSummary.svelte";
  import MachineMap from "./MachineMap.svelte";
  import MapList from "./MapList.svelte";
  import NeedsList from "./NeedsList.svelte";
  import AgentLanes from "./AgentLanes.svelte";
  import { attention, attentionAgent, hotAgents, overview, type AttentionItem } from "../../lib/home";
  import { radialLayout, type FootprintNode } from "../../lib/machine-map";
  import { lanes } from "../../lib/agent-lanes";
  import { AGENT_COLORS } from "../../lib/agents";
  import type { AgentsReport, PortEntry } from "../../lib/types";

  type Section = "overview" | "capabilities";
  let { entries, report, inventory = false, now = Date.now(), onopenagent, onopenentry, onview, oninventory, onstopentry, onstopagent }: {
    entries: PortEntry[] | null;
    report: AgentsReport | null;
    /** Whether reading agents' MCP/skills/permission files is turned on. */
    inventory?: boolean;
    /** Time of the latest scan. */
    now?: number;
    onopenagent: (id: string, section?: Section) => void;
    onopenentry: (id: string) => void;
    onview: (view: "list" | "agents" | "graph") => void;
    oninventory?: () => void;
    /** Both open the stop plan for review; neither stops anything directly. */
    onstopentry?: (id: string) => void;
    onstopagent?: (id: string) => void;
  } = $props();

  const o = $derived(overview(entries, report));
  const needs = $derived(attention(entries, report));
  const groups = $derived(radialLayout(report?.agents ?? [], 140, 285));
  const rows = $derived(lanes(report, entries, now));
  const hot = $derived(hotAgents(needs));
  const colors = $derived(new Map((report?.agents ?? []).map((a, i) => [a.id, AGENT_COLORS[i % AGENT_COLORS.length]])));

  let hovered = $state<string | null>(null);
  let selected = $state<string | null>(null);
  const active = $derived(hovered ?? selected);
  const activeNeeds = $derived(active ? new Set(needs.filter((n) => attentionAgent(n, report) === active).map((n) => n.id)) : null);

  const stats = $derived<SummaryStat[]>([
    { id: "agents", value: o.agents, label: "agents", go: () => onview("agents") },
    { id: "ports", value: o.ports, label: "listening", go: () => onview("list") },
    { id: "dev", value: o.devServers, label: "dev", go: () => onview("list") },
    { id: "exposed", value: o.exposed, label: "exposed", tone: o.exposed ? "warn" : undefined, go: () => onview("list") },
    ...(o.mcpConfigured !== null ? [{ id: "mcp", value: o.mcpRunning, of: o.mcpConfigured, label: "MCP running", go: () => onview("agents") } satisfies SummaryStat] : []),
  ]);
  const showInventory = $derived(!inventory && !!oninventory && (report?.agents ?? []).some((a) => !a.capabilities));

  function open(t: FootprintNode["target"]) { if (t.view === "list") onopenentry(t.entryId); else onopenagent(t.agentId); }
  function openItem(i: AttentionItem) { open(i.target); }
  function stopItem(i: AttentionItem) { if (i.stop?.kind === "entry") onstopentry?.(i.stop.entryId); else if (i.stop) onstopagent?.(i.stop.agentId); }
  function select(id: string) { selected = selected === id ? null : id; }
  function onkeydown(e: KeyboardEvent) { if (e.key === "Escape" && selected) { selected = null; e.stopPropagation(); } }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="home" role="region" aria-label="Overview" {onkeydown}>
  <header class="top">
    <HomeSummary {stats} />
    <span class="legend" aria-label="Legend">
      <span><i class="k project"></i>Project</span><span><i class="k port"></i>Port</span><span><i class="k mcp"></i>MCP</span><span><i class="k remote"></i>Remote</span><span><i class="k warn"></i>Needs you</span>
    </span>
    {#if showInventory}<button type="button" class="inv" onclick={oninventory}><Icon name="plug" size={12} />Read MCP config</button>{/if}
  </header>

  <div class="hero">
    <div class="stage">
      <div class="map-wrap">
        <MachineMap {groups} platform={report?.platform ?? ""} ports={o.ports} {active} {selected} {hot} onhover={(id) => (hovered = id)} onselect={select} onopen={open} />
      </div>
      <div class="map-list"><MapList {groups} {active} onhover={(id) => (hovered = id)} onopenagent={(id) => onopenagent(id)} /></div>
    </div>
    <aside class="panel" aria-labelledby="home-needs">
      <h2 id="home-needs">Needs you{#if needs.length}<span class="count">{needs.length}</span>{/if}</h2>
      {#if needs.length}
        <div class="needs-scroll">
          <NeedsList items={needs} lit={activeNeeds} onopen={openItem} onstop={stopItem} onhover={(i) => (hovered = i ? attentionAgent(i, report) : null)} />
        </div>
      {:else if entries && report}
        <p class="clear"><Icon name="check" size={13} />All clear</p>
      {:else}
        <p class="clear" aria-busy="true">…</p>
      {/if}
    </aside>
  </div>

  {#if rows.length}
    <AgentLanes lanes={rows} {now} {colors} {active} onhover={(id) => (hovered = id)} onselect={select} onopenagent={(id) => onopenagent(id)} {onopenentry} />
  {:else}
    <p class="clear lanes-empty" aria-busy={!report}>{report ? "No agents" : "…"}</p>
  {/if}
</div>

<style>
  .home { overflow-y: auto; min-height: 0; padding: var(--sp-4) var(--sp-5) var(--sp-6); display: grid; gap: var(--sp-4); align-content: start; width: 100%; max-width: 1680px; margin: 0 auto; }
  .top { display: flex; align-items: center; gap: var(--sp-2) var(--sp-4); flex-wrap: wrap; min-height: 30px; }
  .legend { margin-left: auto; display: flex; gap: var(--sp-3); flex-wrap: wrap; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .legend span { display: inline-flex; align-items: center; gap: 5px; }
  .k { display: inline-block; width: 9px; height: 9px; border: 1.5px solid var(--text-2); }
  .k.project { border-radius: 2.5px; } .k.port { border-radius: 50%; background: var(--text-2); } .k.mcp { transform: rotate(45deg) scale(0.85); border-radius: 1.5px; } .k.remote { border-radius: 50%; }
  .k.warn { border-color: var(--warn); border-radius: 50%; box-shadow: 0 0 0 3px var(--warn-soft); }
  .inv { display: inline-flex; align-items: center; gap: 5px; height: 26px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: var(--r-md); background: var(--surface); color: var(--accent); font: inherit; font-size: var(--fs-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .inv:hover { background: var(--accent-softer); }
  .inv:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .hero { display: grid; grid-template-columns: minmax(0, 1fr) 320px; gap: var(--sp-4); height: clamp(380px, 47vh, 580px); }
  .stage { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); min-width: 0; min-height: 0; overflow: hidden; }
  .map-wrap { width: 100%; height: 100%; padding: var(--sp-2); }
  .map-list { display: none; }
  .panel { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); display: grid; grid-template-rows: auto minmax(0, 1fr); min-height: 0; }
  h2 { margin: 0; padding: var(--sp-3) var(--sp-3) var(--sp-2); display: flex; align-items: center; gap: var(--sp-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); border-bottom: 1px solid var(--border); }
  .count { padding: 0 7px; border-radius: var(--r-full); background: var(--warn-soft); color: var(--warn); font-size: var(--fs-caption); line-height: var(--lh-caption); font-variant-numeric: tabular-nums; }
  .needs-scroll { overflow-y: auto; min-height: 0; }
  .clear { margin: 0; padding: var(--sp-3); display: flex; align-items: center; gap: var(--sp-2); color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .clear :global(svg) { color: var(--ok); }
  .lanes-empty { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  @media (max-width: 1100px) { .hero { grid-template-columns: minmax(0, 1fr) 280px; height: clamp(360px, 44vh, 520px); } }
  @media (max-width: 860px) {
    .hero { grid-template-columns: minmax(0, 1fr); height: auto; }
    .stage { height: 440px; }
    .panel { max-height: none; }
  }
  @media (max-width: 760px) {
    .home { padding: var(--sp-3); gap: var(--sp-3); }
    .legend { display: none; }
    .stage { height: auto; }
    .map-wrap { display: none; }
    .map-list { display: block; }
  }
</style>

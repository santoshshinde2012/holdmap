<script lang="ts">
  // Home: one card per agent, with Needs you beside them. The Agents view is the spatial map
  // of the same report, so Home does not draw a second map or a 3D machine.
  import { insightsStore } from "../../lib/insights-store.svelte";
  import ConsentGate from "../ui/ConsentGate.svelte";
  import { trustStore } from "../../lib/trust-store.svelte";
  import { securityTab } from "../../lib/security-tab.svelte";
  import * as api from "../../lib/api";
  import Icon from "../Icon.svelte";
  import { Button, CollapsiblePanel, IconButton } from "../ui";
  import EmptyPanel from "../ui/EmptyPanel.svelte";
  import ExportActions from "../ui/ExportActions.svelte";
  import { homeReport, homeSummaryText, reportFilename } from "../../lib/report-export";
  import HomeSummary, { type SummaryStat } from "./HomeSummary.svelte";
  import AgentCards from "./AgentCards.svelte";
  import NeedsList from "./NeedsList.svelte";
  import PortMenu from "./PortMenu.svelte";
  import { attention, attentionAgent, overview, type AttentionItem } from "../../lib/home";
  import { agentColors, agentMatches } from "../../lib/agents";
  import { prefGetBoolean, prefSet } from "../../lib/prefs";
  import type { AgentJump } from "../../lib/agent-jump";
  import type { AgentsReport, PortEntry } from "../../lib/types";
  let { entries, report, inventory = false, now = Date.now(), query = "", onopenagent, onopenentry, onview, onsecurity, oninventory, onstopentry, onstopagent, selectedAgent = null, onselectagent, agentsError = null, onretryagents }: {
    entries: PortEntry[] | null;
    report: AgentsReport | null;
    /** Whether reading agents' MCP/skills/permission files is turned on. */
    inventory?: boolean;
    /** Time of the latest scan. */
    now?: number;
    /** Home search: agents, folders and PIDs. A port number is handled by the app shell. */
    query?: string;
    onopenagent: (id: string, jump?: AgentJump, capabilityId?: string) => void;
    onopenentry: (id: string) => void;
    onview: (view: "list" | "agents" | "graph") => void;
    /** Opens Security → Findings (the skill-security tile). */
    onsecurity?: () => void;
    oninventory?: () => void;
    /** Both open the stop plan for review; neither stops anything directly. */
    onstopentry?: (id: string) => void;
    onstopagent?: (id: string) => void;
    /** The app's shared selected agent, so a pick here is still selected on Agents. */
    selectedAgent?: string | null;
    onselectagent?: (id: string | null) => void;
    /** Why the agents lookup failed, while there's no report to show. */
    agentsError?: string | null;
    onretryagents?: () => void;
  } = $props();

  const o = $derived(overview(entries, report));
  // The TypeScript mirror paints instantly; in the app the Rust rules then replace it.
  $effect(() => { if (!trustStore.report) void trustStore.load(); });
  const trustAlerts = $derived(trustStore.alerting);
  const trustChanged = $derived(trustAlerts.filter((s) => s.state === "changed").length);
  function openSkills() { onsecurity?.(); securityTab.value = "skills"; }
  const quick = $derived(attention(entries, report, insightsStore.insights));
  let decided = $state<{ from: AttentionItem[]; items: AttentionItem[] } | null>(null);
  const needs = $derived(decided && decided.from === quick ? decided.items : quick);
  $effect(() => {
    const from = quick;
    let live = true;
    api.needs(entries, report, insightsStore.insights).then(
      (items) => { if (live) decided = { from, items }; },
      () => {},
    );
    return () => { live = false; };
  });
  /** Search filters what is drawn. Headline counts stay for the whole machine. */
  const shown = $derived(report ? { ...report, agents: report.agents.filter((a) => agentMatches(a, query)) } : null);
  /** Nothing observed at all: the first minute on a quiet machine. */
  const idle = $derived(!!report && !!entries && report.agents.length === 0 && entries.length === 0);
  const colors = $derived(agentColors(shown?.agents ?? []));

  let hovered = $state<string | null>(null);
  let localSelected = $state<string | null>(null);
  const selected = $derived(onselectagent ? selectedAgent : localSelected);
  function setSelected(id: string | null) { if (onselectagent) onselectagent(id); else localSelected = id; }
  const active = $derived(hovered ?? selected);
  const activeNeeds = $derived(active ? new Set(needs.filter((n) => attentionAgent(n, report) === active).map((n) => n.id)) : null);

  /** Skill security at a glance: open findings and the skills they're about (only once an analyzer is on or has run). */
  const skillSec = $derived.by(() => {
    const on = insightsStore.analyzers.some((a) => a.state === "approved");
    const results = insightsStore.insights?.results ?? [];
    if (!on && !results.length) return null;
    const open = results.flatMap((r) => r.findings).filter((f) => f.state === "open");
    return { open: open.length, skills: new Set(open.map((f) => f.subject)).size };
  });
  const stats = $derived<SummaryStat[]>([
    { id: "agents", value: o.agents, label: "agents", go: () => onview("agents") },
    { id: "ports", value: o.ports, label: "listening", go: () => onview("list") },
    { id: "dev", value: o.devServers, label: "dev servers", go: () => onview("list") },
    { id: "exposed", value: o.exposedDev, label: "exposed dev", meta: o.exposedOther ? `${o.exposedOther} system` : undefined, tone: o.exposedDev ? "warn" : undefined, go: () => onview("list") },
    ...(skillSec ? [{ id: "skill-findings", value: skillSec.open, label: "skill findings", meta: skillSec.skills ? `in ${skillSec.skills} ${skillSec.skills === 1 ? "skill" : "skills"}` : undefined, tone: skillSec.open ? "warn" : "ok", go: () => onsecurity?.() ?? onview("agents") } satisfies SummaryStat] : []),
    ...(o.mcpConfigured !== null ? [{ id: "mcp", value: o.mcpRunning, of: o.mcpConfigured, label: "MCP running", go: () => onview("agents") } satisfies SummaryStat] : []),
  ]);
  const showInventory = $derived(!inventory && !!oninventory && (report?.agents ?? []).some((a) => !a.capabilities));

  function open(t: AttentionItem["target"]) {
    if (t.view === "list") onopenentry(t.entryId);
    else onopenagent(t.agentId);
  }
  function openItem(i: AttentionItem) { if (i.rule === "analyzer" && i.target.view === "agents") onopenagent(i.target.agentId, "insights"); else open(i.target); }
  function stopItem(i: AttentionItem) { if (i.stop?.kind === "entry") onstopentry?.(i.stop.entryId); else if (i.stop) onstopagent?.(i.stop.agentId); }
  let portMenu = $state<{ entry: PortEntry; anchor: HTMLElement | SVGElement } | null>(null);
  function port(entryId: string, anchor: HTMLElement | SVGElement) {
    const entry = entries?.find((e) => e.id === entryId);
    if (entry) portMenu = { entry, anchor }; else onopenentry(entryId);
  }
  function select(id: string) { setSelected(selected === id ? null : id); }
  function onkeydown(e: KeyboardEvent) { if (e.key === "Escape" && selected) { setSelected(null); e.stopPropagation(); } }

  let summaryOpen = $state(prefGetBoolean("home.summary.open", true));
  $effect(() => prefSet("home.summary.open", summaryOpen ? "1" : "0"));
  let sideOpen = $state(prefGetBoolean("home.side.open", true));
  $effect(() => prefSet("home.side.open", sideOpen ? "1" : "0"));
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="home" id="home-overview" role="region" aria-label="Overview" {onkeydown}>
  <header class="top">
    <IconButton icon={summaryOpen ? "chevron-up" : "chevron-down"} label={summaryOpen ? "Fold the summary" : "Show the summary"} size="sm" aria-expanded={summaryOpen} onclick={() => (summaryOpen = !summaryOpen)} />
    {#if summaryOpen}<HomeSummary {stats} />{:else}<span class="folded">Summary</span>{/if}
    <div class="tools">
      <ExportActions label="Export overview" disabled={!report || !entries} copy={() => homeSummaryText(o, needs, Date.now())}
        files={[{ label: "Download JSON", filename: reportFilename("overview", Date.now(), "json"), build: () => homeReport(o, needs, report, Date.now()) }]} />
      {#if showInventory && oninventory}<ConsentGate onconsent={oninventory} compact />{/if}
      <IconButton icon="sidebar" label={sideOpen ? "Hide the side panels" : "Show the side panels"} size="sm" aria-expanded={sideOpen} onclick={() => (sideOpen = !sideOpen)} />
    </div>
  </header>

  <div class="body" class:focused={!sideOpen}>
    <div class="main">
      {#if idle}
        <EmptyPanel icon="bot" title="No agents or dev servers running" hint="Start Claude Code, Cursor, Codex or a dev server and it shows up here within a few seconds." />
      {:else if query.trim() && shown && shown.agents.length === 0}
        <EmptyPanel icon="search" title="No matching agents" hint={`No agent, folder or PID matches “${query.trim()}”.`} />
      {:else if shown}
        <AgentCards report={shown} {entries} {needs} {colors} {now} {selected} {active} onhover={(id) => (hovered = id)} onselect={select}
          onopen={(id) => onopenagent(id)} onport={port} onstop={onstopagent} />
      {:else if agentsError}
        <EmptyPanel icon="alert" title="Couldn't look up the agents" hint={agentsError}>
          {#if onretryagents}<Button size="sm" onclick={onretryagents}>Try again</Button>{/if}
        </EmptyPanel>
      {:else}
        <p class="clear wait" aria-busy="true">…</p>
      {/if}
    </div>
    {#if sideOpen}
    <aside class="side" aria-label="Side panels">
      <CollapsiblePanel id="home-needs" title="Needs you" count={needs.length} pref="home.needs.open" fill>
        {#if trustAlerts.length && onsecurity}
          <button type="button" class="trust-alert" class:changed={trustChanged > 0} onclick={openSkills} data-trust-alert>
            <Icon name="hash" size={13} />
            <span>{#if trustChanged}{trustChanged} skill{trustChanged === 1 ? "" : "s"} changed since you trusted {trustChanged === 1 ? "it" : "them"}{:else}{trustAlerts.length} skill{trustAlerts.length === 1 ? "" : "s"} not trusted yet{/if}</span>
            <span class="go">Review</span>
          </button>
        {/if}
        {#if needs.length}
          <div class="needs-scroll">
            <NeedsList items={needs} lit={activeNeeds} onopen={openItem} onstop={stopItem} onhover={(i) => (hovered = i ? attentionAgent(i, report) : null)} />
          </div>
        {:else if entries && report && !trustAlerts.length}
          <EmptyPanel compact tone="ok" icon="check" title="All clear" hint="Nothing needs you right now." />
        {:else}
          <p class="clear" aria-busy="true">…</p>
        {/if}
      </CollapsiblePanel>
    </aside>
    {/if}
  </div>

  {#each portMenu ? [portMenu] : [] as m (m.entry.id)}
    <PortMenu entry={m.entry} anchor={m.anchor} ondetails={() => onopenentry(m.entry.id)} onstop={() => onstopentry?.(m.entry.id)} onclose={() => (portMenu = null)} />
  {/each}
</div>

<style>
  .home { overflow-y: auto; min-height: 0; padding: var(--sp-4) var(--sp-5) var(--sp-6); display: grid; grid-auto-rows: max-content; gap: var(--sp-4); align-content: start; width: 100%; max-width: 1680px; margin: 0 auto; }
  .top { display: flex; align-items: center; gap: var(--sp-2) var(--sp-4); flex-wrap: wrap; min-height: 30px; }
  .tools { margin-left: auto; display: flex; align-items: center; gap: var(--sp-3); flex-wrap: wrap; justify-content: flex-end; }
  .body { display: grid; grid-template-columns: minmax(0, 1fr) 320px; gap: var(--sp-4); align-items: start; }
  .body.focused { grid-template-columns: minmax(0, 1fr); }
  .main { min-width: 0; min-height: 0; }
  .side { position: sticky; top: 0; max-height: calc(100vh - 220px); display: flex; flex-direction: column; gap: var(--sp-3); min-height: 0; }
  .side :global(.cpanel > .body > :last-child) { flex: 1 1 auto; min-height: 0; }
  .side :global(.cpanel > .body > .trust-alert) { margin: var(--sp-2) var(--sp-2) 0; width: auto; }
  .needs-scroll { overflow-y: auto; min-height: 0; }
  .folded { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); letter-spacing: var(--ls-label); text-transform: uppercase; }
  .clear { margin: 0; padding: var(--sp-3); display: flex; align-items: center; gap: var(--sp-2); color: var(--muted); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .clear :global(svg) { color: var(--ok); }
  .wait { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); }
  .trust-alert { display: flex; align-items: center; gap: 8px; width: 100%; min-height: 32px; margin: 0 0 8px; padding: 6px 10px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface-2); color: var(--text); font: inherit; font-size: var(--fs-caption); text-align: left; cursor: pointer; }
  .trust-alert.changed { border-color: var(--danger-soft); }
  .trust-alert.changed :global(svg) { color: var(--danger); }
  .trust-alert:hover { background: var(--row-hover); }
  .trust-alert:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .trust-alert span:first-of-type { flex: 1; min-width: 0; }
  .trust-alert .go { color: var(--accent); }
  @media (max-width: 1100px) { .body { grid-template-columns: minmax(0, 1fr) 280px; } }
  @media (max-width: 860px) {
    .body { grid-template-columns: minmax(0, 1fr); }
    .side { position: static; order: -1; max-height: none; }
  }
</style>

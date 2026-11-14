<script lang="ts">
  // The selected agent: header, summary strip and four sections (Overview, Config, Connections,
  // Findings). Their names differ from the whole-machine workspaces (MCP & skills, Flow, Security). Each section is its own component in ./inspector, so each has one reason to change.
  import Icon from "../Icon.svelte";
  import Tabs, { panelId, tabId } from "../ui/Tabs.svelte";
  import StatStrip from "../ui/StatStrip.svelte";
  import OverviewTab from "./inspector/OverviewTab.svelte";
  import CapabilitiesTab from "./inspector/CapabilitiesTab.svelte";
  import FlowTab from "./inspector/FlowTab.svelte";
  import SecurityTab from "./inspector/SecurityTab.svelte";
  import { trustStore } from "../../lib/trust-store.svelte";
  import { insightsStore } from "../../lib/insights-store.svelte";
  import { activityRuns } from "../../lib/agent-flow/build";
  import { agentPids, spanBelongsTo } from "../../lib/agent-scope";
  import { agentCounts, total } from "../../lib/analyzers";
  import { agentDomId, detailStats } from "../../lib/agent-details";
  import { accessHeadline, agentMeta, monogram, stoppableEntries } from "../../lib/agents";
  import type { ActivitySnapshot, AgentFlowReport } from "../../lib/agent-flow/model";
  import type { Agent, AgentsReport, PortEntry } from "../../lib/types";
  import type { TabItem } from "../ui/Tabs.svelte";
  type DetailTab = "overview" | "capabilities" | "flow" | "security";

  let {
    agent,
    report,
    entries,
    color,
    onclose,
    onentry,
    onreveal,
    oneditor,
    onfolder,
    onstop,
    onstopall,
    onnavigate,
    oncapabilities,
    oninsights,
    oninventory,
    onflow,
    flowReport = null,
    activity = null,
    tab = $bindable("overview"),
    now = Date.now(),
  }: {
    agent: Agent;
    report: AgentsReport;
    entries: PortEntry[];
    color: string;
    onclose?: () => void;
    onentry: (e: PortEntry) => void;
    onreveal: (path: string) => void | Promise<void | boolean>;
    oneditor: (path: string) => void | Promise<void | boolean>;
    onfolder?: (path: string) => void;
    onstop: (e: PortEntry) => void;
    onstopall: () => void;
    onnavigate?: (id: string) => void;
    oncapabilities?: () => void;
    oninsights?: () => void;
    oninventory?: () => void;
    /** Open the full Flow workspace (scoped to this agent). */
    onflow?: () => void;
    /** Observed process and socket relationships, when Flow data has been read. */
    flowReport?: AgentFlowReport | null;
    /** Reported activity from the opt-in recorder, when available. */
    activity?: ActivitySnapshot | null;
    /** The active section; kept by the parent so switching agents keeps it. */
    tab?: DetailTab;
    now?: number;
  } = $props();

  let heading = $state<HTMLElement | undefined>();
  const base = $derived(agentDomId(agent.id));
  const caps = $derived(agent.capabilities);
  const capCount = $derived(caps?.inspected ? caps.mcp_servers.length + caps.more_mcp_servers + caps.skills.length + caps.more_skills : null);
  const pids = $derived(agentPids(agent));
  const runs = $derived(activityRuns(activity?.spans ?? []).filter((r) => r.spans.some((s) => spanBelongsTo(s, agent, pids))));
  const ownCalls = $derived(runs.reduce((n, r) => n + r.spans.filter((s) => s.kind === "tool" && spanBelongsTo(s, agent, pids)).length, 0));
  const flowCount = $derived(agent.links.length + agent.more_links + ownCalls);
  const severity = $derived(agentCounts(insightsStore.insights, agent.product));
  const securityCount = $derived(total(severity) + trustStore.forAgent(agent.product).length);
  const sectionTabs = $derived<TabItem<DetailTab>[]>([
    { id: "overview", label: "Overview", icon: "panel" },
    { id: "capabilities", label: "Config", icon: "plug", count: capCount },
    { id: "flow", label: "Connections", icon: "activity", count: flowCount || null },
    { id: "security", label: "Findings", icon: "shield", count: securityCount || null, alert: severity.critical + severity.high > 0 },
  ]);
  // Skill trust is read once for the sections that show it; the store dedupes the request.
  $effect(() => { if ((tab === "security" || tab === "capabilities") && !trustStore.report && !trustStore.busy) void trustStore.load(); });

  const head = $derived(accessHeadline(agent));
  const stats = $derived(detailStats(agent));
  const stoppable = $derived(stoppableEntries(agent, entries));
  const inspect = $derived(oninventory ?? oncapabilities);

  /** Focus the name so screen readers announce the newly selected agent. */
  export function focusHeading() { heading?.focus({ preventScroll: true }); }
</script>

<div class="details" style="--agent: {color}">
  <header class="dhead">
    <span class="tile" aria-hidden="true">{monogram(agent.name)}</span>
    <div class="who">
      <h3 class="name" tabindex="-1" bind:this={heading}>{agent.name}</h3>
      <span class="meta">{agentMeta(agent, now)}</span>
    </div>
    {#if onclose}<button type="button" class="x" aria-label="Close agent details" title="Close (Esc)" onclick={onclose}><Icon name="x" size={14} /></button>{/if}
    <span class="pill lv-{head.level}" title={head.text}><i aria-hidden="true"></i>{head.text}</span>
  </header>

  <StatStrip compact label="{agent.name} summary" {stats} />

  {#if stoppable.length}
    <div class="actions" role="group" aria-label="Agent actions">
      <button type="button" class="act stop-all" title="Review a plan to stop every unprotected port this agent started" onclick={onstopall}><Icon name="stop" size={12} />Stop all ({stoppable.length})</button>
    </div>
  {/if}

  <div class="section-tabs">
    <Tabs tabs={sectionTabs} bind:value={tab} {base} label="Agent sections" />
  </div>

  <div class="tab-panel" role="tabpanel" id={panelId(base, tab)} aria-labelledby={tabId(base, tab)}>
    {#if tab === "overview"}
      <OverviewTab {agent} {report} {base} {onreveal} {oneditor} {onfolder} oninspect={inspect} />
    {:else if tab === "capabilities"}
      <CapabilitiesTab {agent} {base} oninspect={inspect} onopen={oncapabilities} {onreveal} />
    {:else if tab === "flow"}
      <FlowTab {agent} {report} {entries} {base} {pids} {runs} {flowReport} {activity} {onentry} {onstop} {onnavigate} onopen={onflow} />
    {:else}
      <SecurityTab {agent} {base} onopen={oninsights} />
    {/if}
  </div>
</div>

<style>
  .details { display: grid; gap: 10px; padding: 14px 16px 18px; min-width: 0; }
  .dhead { display: grid; grid-template-columns: auto 1fr auto; grid-template-areas: "tile who x" "pill pill pill"; gap: 8px 12px; align-items: center; }
  .tile { grid-area: tile; width: 40px; height: 40px; display: grid; place-items: center; border-radius: 12px; border: 1px solid color-mix(in srgb, var(--agent) 30%, transparent); background: color-mix(in srgb, var(--agent) 12%, var(--surface)); color: var(--agent); font-weight: var(--fw-semibold); font-size: var(--fs-body); }
  .who { grid-area: who; min-width: 0; display: grid; }
  .name { margin: 0; font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); font-weight: var(--fw-semibold); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; outline: none; }
  .meta { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .x { grid-area: x; align-self: start; display: grid; place-items: center; width: 28px; height: 28px; border: 0; border-radius: 8px; background: transparent; color: var(--muted); cursor: pointer; }
  .x:hover { background: var(--surface-2); color: var(--text); }
  .x:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .pill { grid-area: pill; justify-self: start; display: inline-flex; align-items: center; gap: 6px; padding: 2px 10px; border-radius: 999px; border: 1px solid var(--border); background: var(--surface-2); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .pill i { width: 7px; height: 7px; border-radius: 50%; background: var(--muted); }
  .pill.lv-restricted i { background: var(--ok); }
  .pill.lv-standard i { background: var(--tone-blue); }
  .pill.lv-elevated { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 30%, var(--border)); background: color-mix(in srgb, var(--danger) 7%, var(--surface)); }
  .pill.lv-elevated i { background: var(--danger); }
  .actions { display: flex; flex-wrap: wrap; gap: 6px; }
  .lv-restricted i { background: var(--ok); }
  .lv-standard i { background: var(--tone-blue); }
  .lv-elevated i { background: var(--danger); }
  .lv-unknown i { background: var(--muted); }
  .act { display: inline-flex; align-items: center; gap: 4px; min-height: 24px; padding: 2px 8px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface-2); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; }
  .act:hover { color: var(--text); border-color: var(--border-strong, var(--border)); }
  .act:disabled { opacity: 0.5; cursor: wait; }
  .act.stop-all { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 28%, var(--border)); }
  .act.stop-all:hover { background: color-mix(in srgb, var(--danger) 10%, var(--surface)); }
  .section-tabs { position: sticky; top: 0; z-index: 3; margin: 0 -16px; background: var(--surface); overflow-x: auto; scrollbar-width: none; }
  .section-tabs :global(.tabs) { padding: 0 8px; }
  .section-tabs :global(.tabs button) { padding: 0 7px; }
  .tab-panel { display: grid; gap: 10px; min-width: 0; outline: none; }
  .act:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
</style>

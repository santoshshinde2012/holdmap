<script lang="ts">
  // The one details screen for a selected agent: who it is, what it is using, and the ports it
  // serves. MCP & skills, Flow and Security are their own screens, so this does not copy them.
  import Icon from "../Icon.svelte";
  import StatStrip from "../ui/StatStrip.svelte";
  import OpenFull from "./inspector/OpenFull.svelte";
  import OverviewTab from "./inspector/OverviewTab.svelte";
  import { FoldSection } from "../ui";
  import { agentDomId, detailStats } from "../../lib/agent-details";
  import { accessHeadline, agentMeta, monogram, stoppableEntries } from "../../lib/agents";
  import type { Agent, AgentsReport, PortEntry } from "../../lib/types";

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
    oncapabilities,
    oninsights,
    oninventory,
    onflow,
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
    oncapabilities?: () => void;
    oninsights?: () => void;
    oninventory?: () => void;
    /** Open Flow, scoped to this agent. */
    onflow?: () => void;
    now?: number;
  } = $props();

  let heading = $state<HTMLElement | undefined>();
  const base = $derived(agentDomId(agent.id));
  const head = $derived(accessHeadline(agent));
  const stats = $derived(detailStats(agent));
  const stoppable = $derived(stoppableEntries(agent, entries));
  const inspect = $derived(oninventory ?? oncapabilities);
  const entryFor = (id: string | null) => entries.find((e) => e.id === id) ?? null;
  const portRole = (role: string) => role === "dev_server" ? "dev server" : role === "agent" ? "agent's own port" : "service";
  const reach = (exposure: string) => exposure === "all_interfaces" ? " · reachable from the network" : exposure === "specific" ? " · bound to a specific interface" : "";

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

  <div class="go" role="group" aria-label="Open this agent">
    <OpenFull label="Open in MCP & skills" icon="plug" hint="Servers, skills and file locations" onopen={oncapabilities} />
    <OpenFull label="Open in Flow" icon="branch" hint="Connections and recorded calls" onopen={onflow} />
    <OpenFull label="Open in Security" icon="shield" hint="Findings and analyzer results" onopen={oninsights} />
  </div>

  <OverviewTab {agent} {report} {base} {onreveal} {oneditor} {onfolder} oninspect={inspect} />

  {#if agent.ports.length}
    <section class="sec-box" id="ports-{base}">
      <FoldSection id="ports-head-{base}" title="Ports it serves" count={agent.ports.length}>
      <ul class="rows">
        {#each agent.ports as p (`${p.port}/${p.protocol}`)}
          {@const e = entryFor(p.entry_id)}
          {@const label = p.project ?? p.framework ?? p.process ?? p.label}
          <li class="port-row">
            {#if e}
              <button class="row-btn" onclick={() => onentry(e)} title="Open :{e.port} in the details pane"><Icon name="server" size={13} /><span class="row-main"><span class="row-label">{label}</span><span class="row-sub mono">:{p.port} · {portRole(p.role)}{reach(p.exposure)}</span></span></button>
            {:else}
              <span class="row-btn"><Icon name="server" size={13} /><span class="row-main"><span class="row-label">{label}</span><span class="row-sub mono">:{p.port} · {portRole(p.role)}</span></span></span>
            {/if}
            {#if e && !e.protected && (p.role === "dev_server" || p.role === "service")}
              <button type="button" class="act stop" title="Stop :{e.port}" onclick={() => onstop(e)}><Icon name="stop" size={12} />Stop</button>
            {/if}
          </li>
        {/each}
      </ul>
      </FoldSection>
    </section>
  {/if}
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
  .go { display: grid; gap: 6px; }
  .sec-box { display: grid; gap: 8px; padding: 11px 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); min-width: 0; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .port-row { display: grid; grid-template-columns: 1fr auto; align-items: start; gap: 4px; }
  .row-btn { width: 100%; display: flex; gap: 8px; align-items: flex-start; padding: 4px 6px; margin: 0 -6px; border: 0; border-radius: 8px; background: transparent; text-align: left; color: var(--text); font: inherit; }
  button.row-btn { cursor: pointer; }
  button.row-btn:hover { background: var(--surface-2); }
  button.row-btn:focus-visible, .act:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .row-main { min-width: 0; display: grid; }
  .row-label { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-sub { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .act.stop { margin-top: 4px; color: var(--danger); border-color: color-mix(in srgb, var(--danger) 28%, var(--border)); }
  .act.stop:hover { background: color-mix(in srgb, var(--danger) 10%, var(--surface)); }
  .lv-restricted i { background: var(--ok); }
  .lv-standard i { background: var(--tone-blue); }
  .lv-elevated i { background: var(--danger); }
  .lv-unknown i { background: var(--muted); }
  .act { display: inline-flex; align-items: center; gap: 4px; min-height: 24px; padding: 2px 8px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface-2); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; }
  .act:hover { color: var(--text); border-color: var(--border-strong, var(--border)); }
  .act:disabled { opacity: 0.5; cursor: wait; }
  .act.stop-all { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 28%, var(--border)); }
  .act.stop-all:hover { background: color-mix(in srgb, var(--danger) 10%, var(--surface)); }
  .act:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
</style>

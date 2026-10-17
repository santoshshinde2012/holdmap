<script lang="ts">
  // The control tower: a one-line summary, every running agent with its projects, ports, MCP
  // servers, skills and posture, then what needs attention and what changed. Figures come from the
  // latest scan; anything not loaded yet reads "scanning…", never zero. j/k or ↑/↓ move between rows.
  import Icon from "../Icon.svelte";
  import AgentRow from "./AgentRow.svelte";
  import EventList, { type EventItem } from "./EventList.svelte";
  import { AGENT_COLORS } from "../../lib/agents";
  import { agentRow, attention, overview, type AttentionTarget } from "../../lib/home";
  import { ago, type ChangeEvent } from "../../lib/changes";
  import type { AgentsReport, PortEntry } from "../../lib/types";

  type Section = "overview" | "capabilities";
  let { entries, report, agentsLoading = false, changes = [], inventory = false, now = Date.now(), onopenagent, onopenentry, onview, oninventory }: {
    entries: PortEntry[] | null;
    report: AgentsReport | null;
    agentsLoading?: boolean;
    /** Observed since this window opened, newest first. */
    changes?: ChangeEvent[];
    /** Whether reading agents' MCP/skills/permission files is turned on. */
    inventory?: boolean;
    now?: number;
    onopenagent: (id: string, section?: Section) => void;
    onopenentry: (id: string) => void;
    onview: (view: "list" | "agents" | "graph") => void;
    oninventory?: () => void;
  } = $props();

  const o = $derived(overview(entries, report));
  const rows = $derived((report?.agents ?? []).map(agentRow));
  const go = (t: AttentionTarget | null) => (t ? () => (t.view === "agents" ? onopenagent(t.agentId) : onopenentry(t.entryId)) : undefined);
  const needs = $derived<EventItem[]>(attention(entries, report).map((i) => ({ id: i.id, tone: i.severity, title: i.title, detail: i.detail, open: go(i.target) })));
  const recent = $derived<EventItem[]>(changes.slice(0, 12).map((c) => ({
    id: c.id,
    tone: c.kind === "port_opened" || c.kind === "agent_started" ? "ok" : "neutral",
    title: c.title,
    detail: c.detail,
    meta: ago(c.at, now),
    open: go(c.target),
  })));
  const uninspected = $derived(!inventory && rows.some((r) => !r.config));

  type Stat = { id: string; n: number | null; label: string; warn?: boolean; go: () => void; of?: number | null };
  const stats = $derived<Stat[]>([
    { id: "agents", n: o.agents, label: o.agents === 1 ? "agent" : "agents", go: () => onview("agents") },
    { id: "dev", n: o.devServers, label: o.devServers === 1 ? "dev server" : "dev servers", go: () => onview("list") },
    { id: "ports", n: o.ports, label: "listening", go: () => onview("list") },
    { id: "exposed", n: o.exposed, label: "open to the network", warn: !!o.exposed, go: () => onview("list") },
    ...(o.mcpConfigured !== null ? [{ id: "mcp", n: o.mcpRunning, of: o.mcpConfigured, label: "MCP running", go: () => onview("agents") }] : []),
  ]);

  let root = $state<HTMLElement>();
  function onkeydown(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const down = e.key === "j" || e.key === "ArrowDown";
    const up = e.key === "k" || e.key === "ArrowUp";
    if (!down && !up) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest("input, textarea, select, [contenteditable]")) return;
    const nav = [...(root?.querySelectorAll<HTMLElement>("[data-home-nav]") ?? [])];
    if (!nav.length) return;
    const at = nav.findIndex((el) => el === document.activeElement || el.contains(document.activeElement));
    const next = at < 0 ? 0 : Math.min(nav.length - 1, Math.max(0, at + (down ? 1 : -1)));
    e.preventDefault();
    nav[next].focus();
    nav[next].scrollIntoView({ block: "nearest" });
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="home" role="region" aria-label="Overview" bind:this={root} {onkeydown}>
  <nav class="summary" aria-label="Summary">
    {#each stats as s, i (s.id)}
      {#if i}<span class="sep" aria-hidden="true">·</span>{/if}
      {#if s.n === null}
        <span class="stat pending" aria-busy="true">{s.label} <em>scanning…</em></span>
      {:else}
        <button type="button" class="stat" class:warn={s.warn} onclick={s.go}><b>{s.n}</b>{#if s.of != null}<span class="of">/{s.of}</span>{/if} {s.label}</button>
      {/if}
    {/each}
    <span class="hint" aria-hidden="true">j/k to move · Enter to open</span>
  </nav>

  <section class="panel" aria-labelledby="home-agents">
    <header><h2 id="home-agents">Running agents</h2>{#if rows.length}<span class="n">{rows.length}</span>{/if}
      <button type="button" class="link" onclick={() => onview("agents")}>Open agents <Icon name="chevron" size={11} /></button></header>
    {#if uninspected && oninventory}
      <p class="callout"><Icon name="plug" size={13} /><span>See each agent's MCP servers, skills and permission rules. Names and metadata only: never secret values, prompts or chats, and no server is started.</span><button type="button" class="link" onclick={oninventory}>Turn on</button></p>
    {/if}
    {#if rows.length}
      <ul class="agents">
        {#each rows as row, i (row.agent.id)}
          <AgentRow {row} color={AGENT_COLORS[i % AGENT_COLORS.length]} onopen={(section) => onopenagent(row.agent.id, section)} onentry={onopenentry} />
        {/each}
      </ul>
    {:else if report}
      <p class="quiet">No coding agents or AI tools are running.</p>
    {:else}
      <p class="quiet" aria-busy={agentsLoading}>{agentsLoading ? "Looking for agents…" : "Scanning for agents…"}</p>
    {/if}
  </section>

  <div class="split">
    <section class="panel" aria-labelledby="home-attention">
      <header><h2 id="home-attention">Needs attention</h2>{#if needs.length}<span class="n warn">{needs.length}</span>{/if}</header>
      {#if needs.length}
        <EventList items={needs} />
      {:else if entries && report}
        <p class="quiet"><Icon name="check" size={13} />Nothing in the latest scan.</p>
      {:else}
        <p class="quiet" aria-busy="true">Waiting for the first scan…</p>
      {/if}
    </section>
    <section class="panel" aria-labelledby="home-changes">
      <header><h2 id="home-changes">Recent changes</h2></header>
      {#if recent.length}
        <EventList items={recent} />
      {:else}
        <p class="quiet">No changes since this window opened.</p>
      {/if}
    </section>
  </div>
</div>

<style>
  .home { overflow-y: auto; min-height: 0; padding: var(--sp-4) var(--sp-5) var(--sp-8); display: grid; gap: var(--sp-4); align-content: start; max-width: 1120px; width: 100%; margin: 0 auto; }
  .home:focus-visible { outline: none; }
  .summary { display: flex; align-items: baseline; flex-wrap: wrap; gap: 2px var(--sp-1); padding: 0 var(--sp-1); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--muted); }
  .stat { border: 0; background: none; padding: 1px 4px; border-radius: var(--r-sm); font: inherit; color: var(--text-2); cursor: pointer; }
  .stat b { color: var(--text); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
  .stat:hover { background: var(--row-hover); color: var(--text); }
  .stat.warn, .stat.warn b { color: var(--warn); }
  .stat:focus-visible, .link:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .stat.pending { cursor: default; color: var(--muted); }
  .stat em { font-style: normal; color: var(--faint); }
  .of { color: var(--muted); font-variant-numeric: tabular-nums; }
  .sep { color: var(--faint); }
  .hint { margin-left: auto; color: var(--faint); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .panel { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); padding: var(--sp-2) var(--sp-1) var(--sp-2); min-width: 0; }
  header { display: flex; align-items: center; gap: var(--sp-2); padding: 2px var(--sp-3) var(--sp-2); }
  h2 { margin: 0; font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; font-weight: var(--fw-semibold); color: var(--muted); }
  .n { padding: 0 6px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); font-variant-numeric: tabular-nums; }
  .n.warn { background: var(--warn-soft); color: var(--warn); }
  .link { margin-left: auto; display: inline-flex; align-items: center; gap: 2px; border: 0; background: none; padding: 0 2px; border-radius: var(--r-sm); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--accent); cursor: pointer; white-space: nowrap; }
  .agents { list-style: none; margin: 0; padding: 0; display: grid; gap: 1px; }
  .callout { margin: 0 var(--sp-2) var(--sp-2); padding: var(--sp-2) var(--sp-3); display: flex; align-items: center; gap: var(--sp-2); border-radius: var(--r-md); background: var(--accent-softer); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .callout :global(svg) { color: var(--accent); flex: none; }
  .split { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: var(--sp-4); align-items: start; }
  .quiet { margin: 0; padding: var(--sp-1) var(--sp-3) var(--sp-1); color: var(--muted); display: flex; align-items: center; gap: var(--sp-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  @media (max-width: 560px) { .home { padding: var(--sp-3); } .hint { display: none; } }
</style>

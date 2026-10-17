<script lang="ts">
  // The control tower: a summary header, what needs attention, every running agent in an aligned
  // table, and a rail with recent activity and dev servers. Figures come from the latest scan;
  // anything not loaded yet reads "scanning…", never zero. j/k or ↑/↓ move between rows.
  import Icon from "../Icon.svelte";
  import HomeSummary, { type SummaryStat } from "./HomeSummary.svelte";
  import AgentsTable from "./AgentsTable.svelte";
  import AttentionList from "./AttentionList.svelte";
  import EventList, { type EventItem } from "./EventList.svelte";
  import { agentRow, attention, overview, services, type AttentionItem, type AttentionTarget } from "../../lib/home";
  import { ago, type ChangeEvent } from "../../lib/changes";
  import type { AgentsReport, PortEntry } from "../../lib/types";

  type Section = "overview" | "capabilities";
  let { entries, report, agentsLoading = false, changes = [], inventory = false, now = Date.now(), since = null, onopenagent, onopenentry, onview, oninventory, onstopentry, onstopagent }: {
    entries: PortEntry[] | null;
    report: AgentsReport | null;
    agentsLoading?: boolean;
    /** Observed since this window opened, newest first. */
    changes?: ChangeEvent[];
    /** Whether reading agents' MCP/skills/permission files is turned on. */
    inventory?: boolean;
    /** Time of the latest scan. */
    now?: number;
    /** When this window started watching (the baseline scan). */
    since?: number | null;
    onopenagent: (id: string, section?: Section) => void;
    onopenentry: (id: string) => void;
    onview: (view: "list" | "agents" | "graph") => void;
    oninventory?: () => void;
    /** Both open the stop plan for review; neither stops anything directly. */
    onstopentry?: (id: string) => void;
    onstopagent?: (id: string) => void;
  } = $props();

  const o = $derived(overview(entries, report));
  const rows = $derived((report?.agents ?? []).map(agentRow));
  const needs = $derived(attention(entries, report));
  const devs = $derived(services(entries, now));
  const go = (t: AttentionTarget | null) => (t ? () => (t.view === "agents" ? onopenagent(t.agentId) : onopenentry(t.entryId)) : undefined);
  const recent = $derived<EventItem[]>(changes.slice(0, 12).map((c) => ({
    id: c.id,
    tone: c.kind === "port_opened" || c.kind === "agent_started" ? "ok" : c.kind === "stopped" ? "warn" : "neutral",
    title: c.title,
    detail: c.detail,
    meta: ago(c.at, now),
    open: go(c.target),
  })));
  const clock = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });

  const stats = $derived<SummaryStat[]>([
    { id: "agents", value: o.agents, label: o.agents === 1 ? "agent running" : "agents running", go: () => onview("agents") },
    { id: "attention", value: entries && report ? needs.length : null, label: "need attention", tone: needs.length ? "warn" : "ok", go: () => document.getElementById("home-attention")?.scrollIntoView({ block: "start" }) },
    { id: "dev", value: o.devServers, label: o.devServers === 1 ? "dev server" : "dev servers", go: () => onview("list") },
    { id: "ports", value: o.ports, label: "listening ports", go: () => onview("list") },
    { id: "exposed", value: o.exposed, label: "open to network", tone: o.exposed ? "warn" : undefined, go: () => onview("list") },
    ...(o.mcpConfigured !== null ? [{ id: "mcp", value: o.mcpRunning, of: o.mcpConfigured, label: "MCP running", go: () => onview("agents") } satisfies SummaryStat] : []),
  ]);

  function openItem(item: AttentionItem) { go(item.target)?.(); }
  function stopItem(item: AttentionItem) {
    if (item.stop?.kind === "entry") onstopentry?.(item.stop.entryId);
    else if (item.stop?.kind === "agent") onstopagent?.(item.stop.agentId);
  }

  let root = $state<HTMLElement>();
  function onkeydown(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const down = e.key === "j" || e.key === "ArrowDown";
    const up = e.key === "k" || e.key === "ArrowUp";
    if (!down && !up) return;
    if ((e.target as HTMLElement | null)?.closest("input, textarea, select, [contenteditable]")) return;
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
  <HomeSummary {stats} updated={entries ? ago(now, Date.now()) : null} />

  <div class="layout">
    <div class="main">
      <section class="panel" aria-labelledby="home-attention">
        <header><h2 id="home-attention">Needs attention</h2>{#if needs.length}<span class="count warn">{needs.length}</span>{/if}</header>
        {#if needs.length}
          <AttentionList items={needs} onopen={openItem} onstop={stopItem} />
        {:else if entries && report}
          <p class="quiet ok"><Icon name="check" size={13} />All clear: no exposed ports, elevated agents, risky settings or unpinned servers in the latest scan.</p>
        {:else}
          <p class="quiet" aria-busy="true">Waiting for the first scan…</p>
        {/if}
      </section>

      <section class="panel" aria-labelledby="home-agents">
        <header><h2 id="home-agents">Running agents</h2>{#if rows.length}<span class="count">{rows.length}</span>{/if}
          <span class="grow"></span><span class="hint">j/k move · → expand · Enter open</span>
          <button type="button" class="link" onclick={() => onview("agents")}>All agent details <Icon name="chevron" size={11} /></button></header>
        {#if !inventory && oninventory && rows.some((r) => !r.agent.capabilities)}
          <p class="callout"><Icon name="plug" size={13} /><span>See each agent's MCP servers, skills and permission rules. Names and metadata only: never secret values, prompts or chats, and no server is started.</span><button type="button" class="cta" onclick={oninventory}>Turn on</button></p>
        {/if}
        {#if rows.length}
          <AgentsTable {rows} {inventory} onopen={onopenagent} onentry={onopenentry} {oninventory} />
        {:else if report}
          <p class="quiet">No coding agents or AI tools are running.</p>
        {:else}
          <p class="quiet" aria-busy={agentsLoading}>{agentsLoading ? "Looking for agents…" : "Scanning for agents…"}</p>
        {/if}
      </section>
    </div>

    <aside class="rail" aria-label="Activity and services">
      <section class="panel" aria-labelledby="home-changes">
        <header><h2 id="home-changes">Activity</h2></header>
        {#if recent.length}
          <EventList items={recent} />
        {/if}
        {#if since !== null}
          <p class="baseline"><span class="pulse" aria-hidden="true"></span>Watching since {clock(since)}. New ports, agent starts and exits, and stops appear here.</p>
        {:else}
          <p class="baseline">Starts with the first scan.</p>
        {/if}
      </section>
      <section class="panel" aria-labelledby="home-services">
        <header><h2 id="home-services">Dev servers</h2>{#if devs.length}<span class="count">{devs.length}</span>{/if}
          <span class="grow"></span><button type="button" class="link" onclick={() => onview("list")}>All ports <Icon name="chevron" size={11} /></button></header>
        {#if devs.length}
          <ul class="services">
            {#each devs.slice(0, 8) as s (s.id)}
              <li><button type="button" data-home-nav onclick={() => onopenentry(s.id)} aria-label="{s.name} on port {s.port}">
                <span class="port" class:exposed={s.exposed}>:{s.port}</span>
                <span class="svc"><span class="sname">{s.name}</span>{#if s.detail}<span class="sdetail">{s.detail}</span>{/if}</span>
              </button></li>
            {/each}
          </ul>
          {#if devs.length > 8}<button type="button" class="more" onclick={() => onview("list")}>{devs.length - 8} more in the port list</button>{/if}
        {:else if entries}
          <p class="quiet">No dev servers are listening.</p>
        {:else}
          <p class="quiet" aria-busy="true">Scanning…</p>
        {/if}
      </section>
    </aside>
  </div>
</div>

<style>
  .home { overflow-y: auto; min-height: 0; padding: var(--sp-5) var(--sp-6) var(--sp-8); display: grid; gap: var(--sp-5); align-content: start; width: 100%; max-width: 1680px; margin: 0 auto; }
  .layout { display: grid; grid-template-columns: minmax(0, 1fr) minmax(280px, 340px); gap: var(--sp-5); align-items: start; }
  .main, .rail { display: grid; gap: var(--sp-5); min-width: 0; }
  .rail { position: sticky; top: 0; }
  .panel { border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); padding: var(--sp-3) 0 var(--sp-2); min-width: 0; }
  header { display: flex; align-items: center; gap: var(--sp-2); padding: 0 var(--sp-4) var(--sp-2); flex-wrap: wrap; }
  h2 { margin: 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); }
  .count { padding: 0 7px; border-radius: var(--r-full); background: var(--surface-2); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); font-variant-numeric: tabular-nums; }
  .count.warn { background: var(--warn-soft); color: var(--warn); }
  .grow { flex: 1; }
  .hint { color: var(--faint); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .link { display: inline-flex; align-items: center; gap: 2px; border: 0; background: none; padding: 0 2px; border-radius: var(--r-sm); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--accent); cursor: pointer; white-space: nowrap; }
  .link:focus-visible, .cta:focus-visible, .more:focus-visible, .services button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .callout { margin: 0 var(--sp-3) var(--sp-2); padding: var(--sp-2) var(--sp-3); display: flex; align-items: center; gap: var(--sp-2); flex-wrap: wrap; border-radius: var(--r-md); background: var(--accent-softer); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .callout span { flex: 1 1 260px; }
  .callout :global(svg) { color: var(--accent); flex: none; }
  .cta { border: 0; border-radius: var(--r-md); background: var(--accent); color: var(--accent-fg); padding: 3px 10px; font: inherit; font-weight: var(--fw-medium); cursor: pointer; }
  .quiet { margin: 0; padding: var(--sp-1) var(--sp-4) var(--sp-2); color: var(--muted); display: flex; align-items: center; gap: var(--sp-2); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .quiet.ok :global(svg) { color: var(--ok); flex: none; }
  .baseline { margin: 0; padding: var(--sp-2) var(--sp-4) var(--sp-1); display: flex; align-items: baseline; gap: var(--sp-2); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .pulse { width: 7px; height: 7px; flex: none; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 3px var(--ok-soft); transform: translateY(-1px); }
  .services { list-style: none; margin: 0; padding: 0; display: grid; }
  .services button { width: 100%; display: grid; grid-template-columns: 58px minmax(0, 1fr); gap: var(--sp-2); align-items: start; padding: 6px var(--sp-4); border: 0; background: none; font: inherit; color: inherit; text-align: left; cursor: pointer; }
  .services button:hover { background: var(--row-hover); }
  .port { font-family: var(--mono); font-size: var(--fs-caption); line-height: var(--lh-body-sm); color: var(--text-2); font-variant-numeric: tabular-nums; }
  .port.exposed { color: var(--warn); }
  .svc { display: grid; min-width: 0; }
  .sname { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); color: var(--text); overflow-wrap: anywhere; }
  .sdetail { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow-wrap: anywhere; }
  .more { margin: 4px var(--sp-4) 0; border: 0; background: none; padding: 0; color: var(--accent); font: inherit; font-size: var(--fs-caption); cursor: pointer; }
  @media (max-width: 1180px) {
    .layout { grid-template-columns: minmax(0, 1fr); }
    .rail { position: static; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); align-items: start; }
  }
  @media (max-width: 640px) { .home { padding: var(--sp-3); gap: var(--sp-3); } .hint { display: none; } .rail { grid-template-columns: minmax(0, 1fr); } }
</style>

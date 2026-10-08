<script lang="ts">
  // One agent in the rail: monogram, access headline and footprint counts, expanding to the full
  // access picture, folders, ports and process tree. Figures come from the OS; anything not
  // observed is labelled as such.
  import Icon from "../Icon.svelte";
  import { humanBytes, humanDuration, tildify } from "../../lib/format";
  import { EVIDENCE_LABEL, KIND_LABEL, LEVEL_LABEL, TOPIC_LABEL, accessFacts, accessHeadline, counts, countsLine, monogram } from "../../lib/agents";
  import type { Agent, AgentProcess, AgentsReport, PortEntry } from "../../lib/types";

  let {
    agent,
    report,
    entries,
    selected,
    color,
    onselect,
    onentry,
    now = Date.now(),
  }: {
    agent: Agent;
    report: AgentsReport;
    /** Live ports, so a card's port can open the details pane. */
    entries: PortEntry[];
    selected: boolean;
    /** The agent's colour in the graph. */
    color: string;
    onselect: () => void;
    onentry: (e: PortEntry) => void;
    now?: number;
  } = $props();

  let el = $state<HTMLElement | undefined>();
  // Keyboard selection (↑/↓) may land on a card scrolled out of the rail.
  $effect(() => { if (selected) el?.scrollIntoView?.({ block: "nearest" }); });
  const head = $derived(accessHeadline(agent));
  const c = $derived(counts(agent));
  const facts = $derived(accessFacts(agent));
  const parent = $derived(report.agents.find((a) => a.id === agent.parent) ?? null);
  const up = $derived(agent.started_at ? humanDuration(now / 1000 - agent.started_at) : null);
  const meta = $derived([KIND_LABEL[agent.kind], agent.vendor !== "unknown" ? agent.vendor : null, `pid ${agent.pid}`, up ? `up ${up}` : null].filter(Boolean).join(" · "));
  const entry = (id: string | null) => entries.find((e) => e.id === id) ?? null;

  function role(p: AgentProcess): string {
    return p.role === "helper" ? "helper" : p.role === "child" ? "started by it" : "agent";
  }
</script>

{#snippet rowBody(icon: string, label: string, sub: string)}
  <span class="ic"><Icon name={icon} size={13} /></span>
  <span class="row-main"><span class="row-label">{label}</span><span class="row-sub mono">{sub}</span></span>
{/snippet}
{#snippet row(e: PortEntry | null, icon: string, label: string, sub: string)}
  {#if e}
    <button class="row-btn" onclick={() => onentry(e)} title="Open :{e.port} in the details pane">{@render rowBody(icon, label, sub)}</button>
  {:else}
    <span class="row-btn">{@render rowBody(icon, label, sub)}</span>
  {/if}
{/snippet}

<article class="card" bind:this={el} class:on={selected} style="--agent: {color}" aria-label={agent.name}>
  <button class="head" aria-pressed={selected} onclick={onselect}>
    <span class="tile" aria-hidden="true">{monogram(agent.name)}</span>
    <span class="who">
      <span class="name">{agent.name}</span>
      <span class="meta">{meta}</span>
    </span>
    <span class="acc lv-{head.level}"><i aria-hidden="true"></i>{head.text}</span>
    <span class="nums">{countsLine(c)}</span>
  </button>

  {#if selected}
    <div class="body">
      <dl class="facts">
        {#each facts as f}
          <div class="fact lv-{f.level}">
            <dt>{TOPIC_LABEL[f.topic]}</dt>
            <dd>
              <span class="lv">{LEVEL_LABEL[f.level]}</span>
              <span class="sum">{f.summary}</span>
              {#if f.evidence === "inferred"}<span class="ev">{EVIDENCE_LABEL[f.evidence]}</span>{/if}
            </dd>
          </div>
        {/each}
      </dl>

      {#if agent.folders.length}
        <h3>Folders</h3>
        <ul class="rows">
          {#each agent.folders as f}
            <li class="row-btn">
              <span class="ic"><Icon name="folder" size={13} /></span>
              <span class="row-main">
                <span class="row-label">{f.label}{#if f.evidence !== "observed"}<em> · {EVIDENCE_LABEL[f.evidence]}</em>{/if}</span>
                <span class="row-sub">{tildify(f.path)}{f.project?.git_branch ? ` · ⎇ ${f.project.git_branch}` : ""}{f.source === "recent" ? " · recent" : ""}{f.privacy_area ? ` · ${f.privacy_area}` : ""}</span>
                {#if f.note}<span class="row-note">{f.note}</span>{/if}
              </span>
            </li>
          {/each}
        </ul>
      {/if}

      {#if agent.ports.length}
        <h3>Ports it serves</h3>
        <ul class="rows">
          {#each agent.ports as p}
            <li>
              {@render row(entry(p.entry_id), "server", p.project ?? p.framework ?? p.process ?? p.label, `:${p.port} · ${p.role === "dev_server" ? "dev server" : p.role === "agent" ? "agent's own port" : "service"}${p.exposure === "all_interfaces" ? " · reachable from the network" : ""}`)}
            </li>
          {/each}
        </ul>
      {/if}

      {#if agent.links.length}
        <h3>Talks to</h3>
        <ul class="rows">
          {#each agent.links as l}
            <li>
              {@render row(entry(l.entry_id), l.kind === "remote" ? "globe" : "plug", l.label, `${l.kind === "remote" ? l.address : `:${l.port}`} · ×${l.connections}${l.service ? ` · ${l.service}` : ""}`)}
            </li>
          {/each}
          {#if agent.more_links}<li class="more">+{agent.more_links} more connections not listed</li>{/if}
        </ul>
      {/if}

      <h3>Processes</h3>
      <code class="cmd" title={agent.command}>{agent.command}</code>
      <ul class="procs">
        {#each agent.processes as p}
          <li>
            <span class="pname">{p.name} <span class="mono pid">{p.pid}</span></span>
            <span class="prole">{role(p)}{#if p.cwd} · {tildify(p.cwd)}{/if}</span>
            <span class="pfig mono">{p.cpu_percent.toFixed(p.cpu_percent < 10 ? 1 : 0)}% · {humanBytes(p.memory_bytes)}</span>
          </li>
        {/each}
        {#if agent.more_processes}<li class="more">+{agent.more_processes} more processes</li>{/if}
      </ul>
      {#if parent}<p class="from">Started from <b>{parent.name}</b> (pid {parent.pid}).</p>{/if}
    </div>
  {/if}
</article>

<style>
  .card { border: 1px solid var(--border); border-radius: 14px; background: var(--surface); overflow: hidden; }
  .card.on { border-color: var(--agent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--agent) 16%, transparent); }
  .head:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; border-radius: 14px; }
  .head { width: 100%; display: grid; grid-template-columns: auto 1fr; grid-template-areas: "tile who" "tile acc" "nums nums"; gap: 2px 10px; padding: 10px 12px; border: 0; background: transparent; text-align: left; cursor: pointer; color: var(--text); }
  .tile { grid-area: tile; width: 36px; height: 36px; margin-top: 2px; display: grid; place-items: center; border-radius: 10px; background: color-mix(in srgb, var(--agent) 16%, var(--surface)); color: var(--agent); font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); letter-spacing: var(--ls-body); }
  .who { grid-area: who; min-width: 0; display: grid; }
  .name { font-weight: var(--fw-semibold); font-size: var(--fs-body); line-height: var(--lh-body); letter-spacing: var(--ls-body); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .acc { grid-area: acc; display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .acc i { width: 7px; height: 7px; border-radius: 50%; background: var(--muted); flex: none; }
  .lv-restricted i { background: var(--ok); }
  .lv-standard i { background: var(--tone-blue); }
  .lv-elevated i { background: var(--danger); }
  .acc.lv-elevated { color: var(--danger); }
  .lv-unknown i { background: var(--muted); }
  .nums { grid-area: nums; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .body { padding: 2px 12px 12px; display: grid; gap: 6px; border-top: 1px solid var(--border); }
  h3 { margin: 8px 0 0; font-size: var(--fs-label); line-height: var(--lh-label); font-weight: var(--fw-semibold); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--muted); }
  .facts { margin: 8px 0 0; display: grid; gap: 7px; }
  .fact { display: grid; grid-template-columns: 76px 1fr; gap: 8px; align-items: baseline; }
  dt { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  dd { margin: 0; display: flex; flex-wrap: wrap; gap: 2px 6px; align-items: baseline; min-width: 0; }
  .lv { font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text); }
  .fact.lv-elevated .lv { color: var(--danger); }
  .fact.lv-restricted .lv { color: var(--ok); }
  .fact.lv-unknown .lv { color: var(--muted); }
  .sum, .ev { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .ev { color: var(--warn); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .row-btn { width: 100%; display: flex; gap: 8px; align-items: flex-start; padding: 4px 6px; margin: 0 -6px; border: 0; border-radius: 8px; background: transparent; text-align: left; color: var(--text); font: inherit; }
  button.row-btn { cursor: pointer; }
  button.row-btn:hover { background: var(--surface-2); }
  .ic { margin-top: 2px; color: var(--muted); display: inline-flex; }
  .row-main { min-width: 0; display: grid; }
  .row-label { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-label em { font-style: normal; color: var(--warn); font-weight: var(--fw-regular); }
  .row-sub, .row-note { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-note { white-space: normal; }
  .procs li { display: grid; grid-template-columns: 1fr auto; grid-template-areas: "n f" "r r"; gap: 0 8px; padding: 3px 0; }
  .pname { grid-area: n; } .pfig { grid-area: f; } .prole { grid-area: r; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pid { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-regular); }
  .cmd { display: block; padding: 6px 8px; border-radius: 8px; background: var(--surface-2); color: var(--text-2); font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pname { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); }
  .prole, .more { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .pfig { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); font-variant-numeric: tabular-nums; }
  .from { margin: 4px 0 0; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .from b { font-weight: var(--fw-semibold); color: var(--text); }
</style>

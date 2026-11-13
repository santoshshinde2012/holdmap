<script lang="ts">
  // Flow: who this agent is related to, the ports it serves, what it talks to, its live
  // sockets and any recorded calls attributed to it.
  import Icon from "../../Icon.svelte";
  import AgentRelations from "../AgentRelations.svelte";
  import OpenFull from "./OpenFull.svelte";
  import { agentSocketLinks, spanBelongsTo } from "../../../lib/agent-scope";
  import type { ActivityRun } from "../../../lib/agent-flow/build";
  import type { ActivitySnapshot, AgentFlowReport } from "../../../lib/agent-flow/model";
  import type { Agent, AgentsReport, PortEntry } from "../../../lib/types";

  let { agent, report, entries, base, pids, runs, flowReport = null, activity = null, onentry, onstop, onnavigate, onopen }: {
    agent: Agent;
    report: AgentsReport;
    entries: PortEntry[];
    base: string;
    pids: Set<number>;
    runs: ActivityRun[];
    flowReport?: AgentFlowReport | null;
    activity?: ActivitySnapshot | null;
    onentry: (e: PortEntry) => void;
    onstop: (e: PortEntry) => void;
    onnavigate?: (id: string) => void;
    onopen?: () => void;
  } = $props();

  const sockets = $derived(agentSocketLinks(flowReport, agent));
  const calls = $derived(runs.reduce((n, r) => n + r.spans.filter((s) => s.kind === "tool" && spanBelongsTo(s, agent, pids)).length, 0));
  const entry = (id: string | null) => entries.find((e) => e.id === id) ?? null;
  const at = (ms: number | null) => ms == null ? "time not reported" : new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const portRole = (role: string) => role === "dev_server" ? "dev server" : role === "agent" ? "agent's own port" : "service";
  const reach = (exposure: string) => exposure === "all_interfaces" ? " · reachable from the network" : exposure === "specific" ? " · bound to a specific interface" : "";
</script>

{#snippet rowBody(icon: string, label: string, sub: string)}
  <span class="ic"><Icon name={icon} size={13} /></span>
  <span class="row-main"><span class="row-label">{label}</span><span class="row-sub mono">{sub}</span></span>
{/snippet}

<AgentRelations {agent} {report} {onnavigate} />

{#if agent.ports.length}
  <section class="sec-box" aria-labelledby="ports-head-{base}">
    <h4 id="ports-head-{base}"><Icon name="server" size={13} />Ports it serves <span class="sec-n">{agent.ports.length}</span></h4>
    <ul class="rows">
      {#each agent.ports as p (`${p.port}/${p.protocol}`)}
        {@const e = entry(p.entry_id)}
        {@const label = p.project ?? p.framework ?? p.process ?? p.label}
        <li class="port-row">
          {#if e}
            <button class="row-btn" onclick={() => onentry(e)} title="Open :{e.port} in the details pane">{@render rowBody("server", label, `:${p.port} · ${portRole(p.role)}${reach(p.exposure)}`)}</button>
          {:else}
            <span class="row-btn static">{@render rowBody("server", label, `:${p.port} · ${portRole(p.role)}`)}</span>
          {/if}
          {#if e && !e.protected && (p.role === "dev_server" || p.role === "service")}
            <button type="button" class="act stop" title="Stop :{e.port}" onclick={() => onstop(e)}><Icon name="stop" size={12} />Stop</button>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}

{#if agent.links.length || agent.more_links}
  <section class="sec-box" aria-labelledby="links-head-{base}">
    <h4 id="links-head-{base}"><Icon name="globe" size={13} />Talks to <span class="sec-n">{agent.links.length}</span></h4>
    <ul class="rows">
      {#each agent.links as l (l.id)}
        {@const e = entry(l.entry_id)}
        {@const sub = `${l.kind === "remote" ? l.address : `:${l.port}`} · ×${l.connections}${l.service ? ` · ${l.service}` : ""}`}
        <li>
          {#if e}
            <button class="row-btn" onclick={() => onentry(e)} title="Open :{e.port} in the details pane">{@render rowBody(l.kind === "remote" ? "globe" : "plug", l.label, sub)}</button>
          {:else}
            <span class="row-btn static">{@render rowBody(l.kind === "remote" ? "globe" : "plug", l.label, sub)}</span>
          {/if}
        </li>
      {/each}
      {#if agent.more_links}<li class="more">+{agent.more_links} links not listed</li>{/if}
    </ul>
  </section>
{/if}

<section class="sec-box" aria-labelledby="sockets-head-{base}">
  <h4 id="sockets-head-{base}"><Icon name="link" size={13} />Live socket links <span class="sec-n">{sockets.length}</span></h4>
  {#if !flowReport}
    <p class="note" role="status">Reading this agent's observed connections…</p>
  {:else if sockets.length}
    <ul class="rows socket-list">
      {#each sockets.slice(0, 12) as s (s.edge.id)}
        <li><span class="ic"><Icon name={s.to.kind === "remote" ? "globe" : "plug"} size={13} /></span><span class="row-main"><span class="row-label">{s.from.label} ↔ {s.to.label}</span><span class="row-sub mono">{s.to.address ?? "address unknown"}{s.to.port != null ? `:${s.to.port}` : ""} · {s.edge.connections} live {s.edge.connections === 1 ? "socket" : "sockets"}</span></span></li>
      {/each}
      {#if sockets.length > 12}<li class="more">+{sockets.length - 12} more in Flow</li>{/if}
    </ul>
  {:else}
    <p class="note">No live sockets observed for {agent.name} in the last scan.</p>
  {/if}
  <p class="fine">Sockets show that a connection exists, not messages or data direction.</p>
</section>

<section class="sec-box" aria-labelledby="calls-head-{base}">
  <h4 id="calls-head-{base}"><Icon name="activity" size={13} />Recorded calls <span class="sec-n">{calls}</span></h4>
  {#if runs.length}
    <ul class="rows run-list">
      {#each runs.slice(0, 6) as r (r.id)}
        {@const mine = r.spans.filter((s) => spanBelongsTo(s, agent, pids))}
        {@const tools = mine.filter((s) => s.kind === "tool").length}
        <li><span class="ic"><Icon name="activity" size={13} /></span><span class="row-main"><span class="row-label">{r.spans.find((s) => s.kind === "agent")?.name ?? mine[0]?.name ?? r.label}</span><span class="row-sub">{r.label} · {tools} tool {tools === 1 ? "call" : "calls"} · {mine.length} {mine.length === 1 ? "record" : "records"}{r.errors ? ` · ${r.errors} failed` : ""} · {at(r.first_at_ms)}</span></span></li>
      {/each}
    </ul>
  {:else}
    <p class="note">{activity ? `No recorded calls are attributed to ${agent.name}.` : "Recorded calls appear here when the opt-in recorder is on."}</p>
  {/if}
</section>
<OpenFull label="Open in Flow" icon="activity" {onopen} />

<style>
  .sec-box { display: grid; gap: 8px; padding: 11px 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); min-width: 0; scroll-margin-top: 10px; }
  h4 { margin: 0; display: flex; align-items: center; gap: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .sec-n { margin-left: auto; padding: 0 7px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .port-row { display: grid; gap: 2px; }
  .row-btn { width: 100%; display: flex; gap: 8px; align-items: flex-start; padding: 4px 6px; margin: 0 -6px; border: 0; border-radius: 8px; background: transparent; text-align: left; color: var(--text); font: inherit; }
  button.row-btn { cursor: pointer; }
  button.row-btn:hover { background: var(--surface-2); }
  button.row-btn:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  button.row-btn:disabled { cursor: default; }
  .row-btn.static { cursor: default; }
  .act { display: inline-flex; align-items: center; gap: 4px; min-height: 24px; padding: 2px 8px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface-2); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; }
  .act:hover { color: var(--text); border-color: var(--border-strong, var(--border)); }
  .act:disabled { opacity: 0.5; cursor: wait; }
  .act.stop { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 28%, var(--border)); }
  .act.stop:hover { background: color-mix(in srgb, var(--danger) 10%, var(--surface)); }
  .port-row { grid-template-columns: 1fr auto; align-items: start; gap: 4px; }
  .port-row .act.stop { margin-top: 4px; }
  .ic { margin-top: 2px; color: var(--muted); display: inline-flex; }
  .row-main { min-width: 0; display: grid; }
  .row-label { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-sub { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .more { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .act:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .note { margin: 0; color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .fine { margin: 0; color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .socket-list li, .run-list li { display: flex; gap: 8px; align-items: flex-start; padding: 3px 0; }
</style>

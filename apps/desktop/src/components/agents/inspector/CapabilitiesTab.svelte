<script lang="ts">
  // MCP & skills: what this agent can use first (servers and skills), then where its
  // configuration lives (the documented locations), folded by default.
  import Icon from "../../Icon.svelte";
  import AgentLocations from "../AgentLocations.svelte";
  import ConsentGate from "../../ui/ConsentGate.svelte";
  import OpenFull from "./OpenFull.svelte";
  import { createCopy } from "../../../lib/use-copy.svelte";
  import { tildify } from "../../../lib/format";
  import type { Agent, AgentMcpServer } from "../../../lib/types";

  let { agent, base, oninspect, onopen }: { agent: Agent; base: string; oninspect?: () => void; onopen?: () => void } = $props();

  const caps = $derived(agent.capabilities);
  const total = $derived(caps?.inspected ? caps.mcp_servers.length + caps.more_mcp_servers + caps.skills.length + caps.more_skills : null);
  let filter = $state<"all" | "mcp" | "skills">("all");
  let query = $state("");
  const needle = $derived(query.trim().toLocaleLowerCase());
  const match = (...parts: (string | null | undefined)[]) => !needle || parts.filter(Boolean).join(" ").toLocaleLowerCase().includes(needle);
  const servers = $derived(filter === "skills" ? [] : (caps?.mcp_servers ?? []).filter((m) => match(m.name, m.transport, m.launch?.package, m.remote_host, m.source_path)));
  const skills = $derived(filter === "mcp" ? [] : (caps?.skills ?? []).filter((k) => match(k.name, k.declared_name, k.description, k.source_path)));
  const clip = createCopy();
  $effect(() => () => clip.dispose());

  const launchText = (m: AgentMcpServer) => m.launch ? `${m.launch.runner}${m.launch.package ? ` ${m.launch.package}` : ""}` : m.remote_host ? `${m.transport.toUpperCase()} · ${m.remote_host}` : m.transport === "unknown" ? "Launch unknown" : m.transport.toUpperCase();
  const enabledText = (m: AgentMcpServer) => m.enabled === true ? "Enabled" : m.enabled === false ? "Disabled" : "Unknown";
  const enabledTone = (m: AgentMcpServer) => m.enabled === true ? "on" : m.enabled === false ? "off" : "unknown";
  const scopeText = (scope: string) => scope === "user" ? "User" : scope === "project" ? "Project" : scope === "local" ? "Local" : scope;
</script>

{#snippet pathRow(path: string, name: string)}
  <div class="cap-path"><span class="mono" title={path}>{tildify(path)}</span><button type="button" class="act" aria-label="Copy path for {name}" onclick={() => clip.copy(path)}><Icon name={clip.copied === path ? "check" : "copy"} size={11} />{clip.copied === path ? "Copied" : "Copy path"}</button></div>
{/snippet}

<section class="sec-box" aria-labelledby="caps-head-{base}">
  <h4 id="caps-head-{base}"><Icon name="plug" size={13} />What {agent.name} can use{#if total != null}<span class="sec-n">{total}</span>{/if}</h4>
  {#if !caps?.inspected}
    <p class="note">MCP servers and skills for {agent.name} aren't read yet.</p>
    {#if oninspect}<ConsentGate onconsent={oninspect} />{/if}
  {:else}
    <div class="cap-tools">
      <div class="seg" role="group" aria-label="Show">
        {#each [["all", "All"], ["mcp", "MCP"], ["skills", "Skills"]] as const as [value, label] (value)}
          <button type="button" class:on={filter === value} aria-pressed={filter === value} onclick={() => (filter = value)}>{label}</button>
        {/each}
      </div>
      <label class="cap-search"><Icon name="search" size={12} /><span class="sr-only">Filter {agent.name}'s MCP servers and skills</span><input type="search" placeholder="Filter" bind:value={query} /></label>
    </div>
    {#if servers.length}
      <h5 class="cap-group">MCP servers <span>{servers.length}</span></h5>
      <ul class="cap-list" aria-label="{agent.name} MCP servers">
        {#each servers as m (m.id)}
          <li class="cap-card">
            <div class="cap-top"><b>{m.name}</b><span class="badge {enabledTone(m)}">{enabledText(m)}</span>{#if m.runtime?.state === "running"}<span class="badge run">Running</span>{/if}<span class="tag">{scopeText(m.scope)}</span></div>
            <span class="row-sub mono" title={launchText(m)}>{launchText(m)}</span>
            {@render pathRow(m.source_path, m.name)}
          </li>
        {/each}
      </ul>
    {/if}
    {#if skills.length}
      <h5 class="cap-group">Skills <span>{skills.length}</span></h5>
      <ul class="cap-list" aria-label="{agent.name} skills">
        {#each skills as k (k.id)}
          <li class="cap-card">
            <div class="cap-top"><b>{k.declared_name ?? k.name}</b><span class="tag">{scopeText(k.scope)}</span></div>
            <span class="cap-desc">{k.description ?? "No declared purpose"}</span>
            {@render pathRow(k.source_path, k.declared_name ?? k.name)}
          </li>
        {/each}
      </ul>
    {/if}
    {#if !servers.length && !skills.length}<p class="note">{needle || filter !== "all" ? "Nothing matches this filter." : "No MCP servers or skills in the inspected locations."}</p>{/if}
    {#if caps.more_mcp_servers || caps.more_skills}<p class="more">+{caps.more_mcp_servers + caps.more_skills} more not listed</p>{/if}
  {/if}
</section>

<!-- Where it lives: every documented location, closest layer first. -->
<details class="sec-box where" open>
  <summary><Icon name="layers" size={13} />Where it lives</summary>
  <AgentLocations {agent} {oninspect} />
</details>
<OpenFull label="Open in MCP & skills" icon="plug" {onopen} />

<style>
  .sec-box { display: grid; gap: 8px; padding: 11px 12px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); min-width: 0; scroll-margin-top: 10px; }
  h4 { margin: 0; display: flex; align-items: center; gap: 6px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .sec-n { margin-left: auto; padding: 0 7px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .act { display: inline-flex; align-items: center; gap: 4px; min-height: 24px; padding: 2px 8px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface-2); color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; }
  .act:hover { color: var(--text); border-color: var(--border-strong, var(--border)); }
  .act:disabled { opacity: 0.5; cursor: wait; }
  .row-sub { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tag { padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface-2); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); white-space: nowrap; }
  .tag.mono { font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); }
  .more { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .act:focus-visible, .seg button:focus-visible, .cap-search:focus-within { outline: 2px solid var(--ring); outline-offset: 1px; }
  .note { margin: 0; color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .cap-tools { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .seg { display: inline-flex; gap: 2px; padding: 2px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface-2); }
  .seg button { border: 0; border-radius: var(--r-full); padding: 2px 10px; background: transparent; color: var(--text-2); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: var(--shadow-sm); }
  .cap-search { flex: 1 1 120px; min-width: 0; display: inline-flex; align-items: center; gap: 6px; padding: 2px 10px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface); color: var(--muted); }
  .cap-search input { flex: 1; min-width: 0; border: 0; outline: none; background: transparent; color: var(--text); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); padding: 2px 0; }
  .cap-group { margin: 4px 0 0; display: flex; gap: 6px; align-items: baseline; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); }
  .cap-group span { color: var(--muted); font-weight: var(--fw-regular); }
  .cap-list { gap: 6px; }
  .cap-card { display: grid; gap: 3px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface-2); min-width: 0; }
  .cap-top { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; min-width: 0; }
  .cap-top b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); overflow-wrap: anywhere; margin-right: auto; }
  .cap-desc { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); }
  .cap-path { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .cap-path .mono { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: var(--fs-mono-sm); line-height: var(--lh-mono-sm); color: var(--muted); }
  .cap-path .act { flex: none; min-height: 22px; }
  .badge { padding: 0 7px; border-radius: var(--r-full); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); border: 1px solid transparent; white-space: nowrap; }
  .badge.on, .badge.run { color: var(--ok); background: color-mix(in srgb, var(--ok) 12%, var(--surface)); }
  .badge.off { color: var(--text-2); background: var(--surface-3, var(--surface)); border-color: var(--border); }
  .badge.unknown { color: var(--muted); background: var(--surface); border-color: var(--border); }
  .badge.warn { color: var(--warn); background: var(--warn-soft); }
  .where summary { display: flex; align-items: center; gap: 6px; cursor: pointer; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-2); border-radius: var(--r-sm); }
  .where summary :global(svg) { color: var(--agent); }
  .where summary:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
</style>

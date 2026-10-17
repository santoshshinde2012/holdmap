<script lang="ts">
  // One running agent on the overview: where it works, the ports it holds, its MCP servers
  // (running of configured), skills and rules, and its posture. Every figure opens its detail.
  import Icon from "../Icon.svelte";
  import { KIND_LABEL, monogram } from "../../lib/agents";
  import type { AgentRow } from "../../lib/home";

  let { row, color, onopen, onentry }: {
    row: AgentRow;
    color: string;
    /** Opens the agent; "capabilities" opens its MCP/skills/permissions section. */
    onopen: (section: "overview" | "capabilities") => void;
    onentry: (entryId: string) => void;
  } = $props();

  const a = $derived(row.agent);
  const c = $derived(row.config);
  const shownPorts = $derived(row.ports.slice(0, 3));
</script>

<li class="row" style="--agent: {color}">
  <button type="button" class="who" data-home-nav onclick={() => onopen("overview")} aria-label="{a.name}, {KIND_LABEL[a.kind]}, {row.posture.text}">
    <span class="mono" aria-hidden="true">{monogram(a.name)}</span>
    <span class="name">{a.name}</span>
    <span class="kind">{KIND_LABEL[a.kind]}</span>
  </button>
  <span class="projects" title={row.projects.join(", ")}>
    {#if row.projects.length}
      <Icon name="folder" size={12} />{row.projects.join(", ")}{#if row.moreProjects}<span class="more">+{row.moreProjects}</span>{/if}
    {:else}<span class="faint">no project folder seen</span>{/if}
  </span>
  <span class="ports">
    {#each shownPorts as p (p.entry_id)}
      <button type="button" class="chip" class:exposed={p.exposure === "all_interfaces"} onclick={() => onentry(p.entry_id)} aria-label="Port {p.port}{p.exposure === 'all_interfaces' ? ', open to the network' : ''}">:{p.port}</button>
    {/each}
    {#if row.ports.length > shownPorts.length}<button type="button" class="chip more" onclick={() => onopen("overview")}>+{row.ports.length - shownPorts.length}</button>{/if}
    {#if !row.ports.length}<span class="faint">no ports</span>{/if}
  </span>
  <span class="config">
    {#if c}
      <button type="button" class="stat" onclick={() => onopen("capabilities")} aria-label="MCP servers: {c.running} running of {c.mcp} configured">
        <Icon name="plug" size={12} /><b class:on={c.running > 0}>{c.running}</b><span class="of">/{c.mcp}</span> MCP
      </button>
      <button type="button" class="stat" onclick={() => onopen("capabilities")} aria-label="{c.skills} skills, {c.instructions} instruction files, {c.rules} permission rules">
        <Icon name="sparkles" size={12} /><b>{c.skills + c.instructions}</b> skills · <b>{c.rules}</b> rules
      </button>
    {:else}
      <span class="faint">config not read</span>
    {/if}
  </span>
  <span class="posture lv-{row.posture.level}" title={row.posture.text}>{row.posture.text}</span>
</li>

<style>
  .row { display: grid; grid-template-columns: minmax(190px, 1.3fr) minmax(110px, 1fr) minmax(110px, 0.8fr) minmax(190px, auto) minmax(0, 240px); align-items: center; gap: var(--sp-3); padding: 5px var(--sp-3); border-radius: var(--r-md); min-width: 0; }
  .row:hover, .row:focus-within { background: var(--row-hover); }
  button { font: inherit; color: inherit; cursor: pointer; }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .who { display: flex; align-items: center; gap: var(--sp-2); min-width: 0; border: 0; padding: 2px; margin: -2px; border-radius: var(--r-sm); background: none; text-align: left; }
  .mono { width: 22px; height: 22px; flex: none; display: grid; place-items: center; border-radius: 6px; background: color-mix(in srgb, var(--agent) 16%, var(--surface)); color: var(--agent); font-size: var(--fs-caption); line-height: 1; font-weight: var(--fw-semibold); }
  .name { flex: none; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .kind { overflow: hidden; text-overflow: ellipsis; }
  .kind, .faint, .more, .of { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); white-space: nowrap; }
  .projects { display: flex; align-items: center; gap: 4px; min-width: 0; color: var(--text-2); font-size: var(--fs-caption); line-height: var(--lh-caption); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .projects :global(svg) { color: var(--faint); flex: none; }
  .ports, .config { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; min-width: 0; }
  .chip { border: 1px solid var(--border); background: var(--surface); padding: 0 6px; border-radius: var(--r-full); font-family: var(--mono); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); font-variant-numeric: tabular-nums; }
  .chip:hover, .stat:hover { border-color: var(--border-strong); color: var(--text); }
  .chip.exposed { border-color: color-mix(in srgb, var(--warn) 45%, transparent); color: var(--warn); background: var(--warn-soft); }
  .stat { display: inline-flex; align-items: center; gap: 3px; border: 1px solid transparent; background: none; padding: 0 5px; border-radius: var(--r-sm); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); white-space: nowrap; }
  .stat :global(svg) { color: var(--faint); }
  .stat b { font-weight: var(--fw-semibold); color: var(--text); font-variant-numeric: tabular-nums; }
  .stat b.on { color: var(--ok); }
  .posture { justify-self: end; max-width: 100%; overflow: hidden; text-overflow: ellipsis; font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); white-space: nowrap; }
  .posture.lv-elevated { color: var(--danger); }
  @media (max-width: 760px) {
    .row { grid-template-columns: 1fr auto; row-gap: 2px; }
    .projects, .ports, .config { grid-column: 1 / -1; }
    .posture { grid-row: 1; grid-column: 2; max-width: 16ch; }
  }
</style>

<script lang="ts">
  // The map on narrow screens: one row per agent with what it reaches, by kind.
  import Icon from "../Icon.svelte";
  import { ACCESS_LABEL } from "../../lib/home";
  import { accessHeadline, monogram } from "../../lib/agents";
  import type { radialLayout } from "../../lib/machine-map";

  type Group = ReturnType<typeof radialLayout>[number];
  let { groups, colors, active, onhover, onopenagent }: { groups: Group[]; colors: Map<string, string>; active: string | null; onhover: (id: string | null) => void; onopenagent: (id: string) => void } = $props();
  const KINDS = [{ k: "project", icon: "folder" }, { k: "port", icon: "plug" }, { k: "mcp", icon: "server" }, { k: "remote", icon: "globe" }] as const;
</script>

<ul class="list" aria-label="Machine map">
  {#each groups as g (g.agent.id)}
    {@const head = accessHeadline(g.agent)}
    <li style="--agent: {colors.get(g.agent.id)}" class:active={active === g.agent.id}>
      <button type="button" onclick={() => onopenagent(g.agent.id)} onfocus={() => onhover(g.agent.id)} onblur={() => onhover(null)} aria-label="{g.agent.name}, {ACCESS_LABEL[head.level]}">
        <span class="mono">{monogram(g.agent.name)}</span>
        <span class="nm"><b>{g.agent.name}</b><small class="lv-{head.level}">{ACCESS_LABEL[head.level]}</small></span>
        <span class="counts">
          {#each KINDS as { k, icon }}
            {@const nodes = g.nodes.filter((n) => n.item.kind === k)}
            {#if nodes.length}<span class:warn={nodes.some((n) => n.item.warn)}><Icon name={icon} size={11} />{nodes.length}</span>{/if}
          {/each}
        </span>
      </button>
    </li>
  {/each}
</ul>

<style>
  .list { list-style: none; margin: 0; padding: 4px 0; display: grid; }
  button { width: 100%; display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: var(--sp-2); padding: 8px var(--sp-3); border: 0; background: none; font: inherit; color: inherit; text-align: left; cursor: pointer; }
  li.active button, button:hover { background: var(--row-hover); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  .mono { width: 26px; height: 26px; display: grid; place-items: center; border-radius: 8px; background: color-mix(in srgb, var(--agent) 16%, var(--surface)); color: var(--agent); font-size: var(--fs-caption); font-weight: var(--fw-semibold); }
  .nm { display: grid; min-width: 0; }
  .nm b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); overflow-wrap: anywhere; }
  .nm small { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .lv-elevated { color: var(--danger) !important; } .lv-restricted { color: var(--ok) !important; }
  .counts { display: flex; gap: 10px; color: var(--text-2); font-size: var(--fs-caption); font-variant-numeric: tabular-nums; }
  .counts span { display: inline-flex; align-items: center; gap: 3px; }
  .counts :global(svg) { color: var(--faint); }
  .counts .warn, .counts .warn :global(svg) { color: var(--warn); }
</style>

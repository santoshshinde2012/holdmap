<script lang="ts">
  // The map on narrow screens: one row per agent with what it reaches, by kind, and its ports
  // as buttons that open the same port menu as the map (Details, Stop…).
  import Icon from "../Icon.svelte";
  import { ACCESS_LABEL } from "../../lib/home";
  import { accessHeadline, monogram } from "../../lib/agents";
  import type { radialLayout } from "../../lib/machine-map";

  type Group = ReturnType<typeof radialLayout>[number];
  let { groups, colors, active, onhover, onopenagent, onport }: {
    groups: Group[];
    colors: Map<string, string>;
    active: string | null;
    onhover: (id: string | null) => void;
    onopenagent: (id: string) => void;
    onport: (entryId: string, anchor: HTMLElement) => void;
  } = $props();
  const NOUN: Record<string, [string, string]> = { project: ["project", "projects"], port: ["port", "ports"], mcp: ["MCP server", "MCP servers"], remote: ["remote host", "remote hosts"] };
  /** Visible words first (name, access), then the counts the icons show. */
  function mapListLabel(g: Group): string {
    const counts = KINDS.map(({ k }) => tally(g, k))
      .flatMap((n, i) => (n ? [`${n} ${NOUN[KINDS[i].k][n === 1 ? 0 : 1]}`] : []));
    return [g.agent.name, ACCESS_LABEL[accessHeadline(g.agent).level], ...counts].join(", ");
  }
  /** Nodes of one kind, counting a collapsed remote group as the hosts it stands for. */
  const tally = (g: Group, k: string) => g.nodes.filter((n) => n.item.kind === k).reduce((sum, n) => sum + (n.item.collapse ? 0 : n.item.count ?? 1), 0);
  const KINDS = [{ k: "project", icon: "folder" }, { k: "port", icon: "plug" }, { k: "mcp", icon: "server" }, { k: "remote", icon: "globe" }] as const;
</script>

<ul class="list" aria-label="Machine map">
  {#each groups as g (g.agent.id)}
    {@const head = accessHeadline(g.agent)}
    {@const ports = g.nodes.filter((n) => n.item.target.view === "list")}
    <li style="--agent: {colors.get(g.agent.id)}" class:active={active === g.agent.id}>
      <button type="button" onclick={() => onopenagent(g.agent.id)} onfocus={() => onhover(g.agent.id)} onblur={() => onhover(null)} aria-label={mapListLabel(g)}>
        <span class="mono" aria-hidden="true" data-glyph={monogram(g.agent.name)}></span>
        <span class="nm"><b>{g.agent.name}</b>{" "}<small class="lv-{head.level}">{ACCESS_LABEL[head.level]}</small></span>
        <span class="counts">
          {#each KINDS as { k, icon }}
            {@const nodes = g.nodes.filter((n) => n.item.kind === k && !n.item.collapse)}
            {#if nodes.length}<span class:warn={nodes.some((n) => n.item.warn)}><Icon name={icon} size={11} /><span data-glyph={tally(g, k)}></span></span>{/if}
          {/each}
        </span>
      </button>
      {#if ports.length}
        <ul class="ports" aria-label="{g.agent.name} ports">
          {#each ports as n (n.item.id)}
            {@const t = n.item.target}
            <li><button type="button" class="port" class:warn={n.item.warn} aria-haspopup="menu" onclick={(e) => t.view === "list" && onport(t.entryId, e.currentTarget)}
              aria-label="Port {n.item.label.slice(1)}{n.item.sub ? ` ${n.item.sub}` : ''}{n.item.warn ? ', open to the network' : ''}"><i></i><span class="pn">{n.item.label}</span>{" "}{n.item.sub ?? ""}</button></li>
          {/each}
        </ul>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .list { list-style: none; margin: 0; padding: 4px 0; display: grid; }
  li > button { width: 100%; display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: var(--sp-2); padding: 8px var(--sp-3); border: 0; background: none; font: inherit; color: inherit; text-align: left; cursor: pointer; }
  li.active > button, li > button:hover { background: var(--row-hover); }
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
  .ports { list-style: none; margin: 0; padding: 0 var(--sp-3) 8px calc(var(--sp-3) + 26px + var(--sp-2)); display: flex; flex-wrap: wrap; gap: 6px; }
  .port { display: inline-flex; align-items: center; gap: 5px; min-height: 24px; padding: 2px 8px; border: 1px solid var(--border); border-radius: var(--r-full); background: var(--surface); color: var(--text-2); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); cursor: pointer; overflow-wrap: anywhere; text-align: left; }
  .port:hover { background: var(--row-hover); color: var(--text); }
  .port:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .port i { width: 6px; height: 6px; border-radius: 50%; background: var(--ok); flex: none; }
  .port.warn { border-color: color-mix(in srgb, var(--warn) 45%, var(--border)); }
  .port.warn i { background: var(--warn); }
  .pn { font-family: var(--mono); color: var(--text); }
</style>

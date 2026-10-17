<script lang="ts">
  // Cards layout of Home: one card per agent in a responsive grid (rows share a height), plus a
  // card for dev servers no agent started. ←/→ move between cards.
  import Icon from "../Icon.svelte";
  import HomeAgentCard from "./HomeAgentCard.svelte";
  import { attentionAgent, unownedDevServers, type AttentionItem } from "../../lib/home";
  import { cardModel } from "../../lib/agent-card";
  import type { AgentsReport, PortEntry } from "../../lib/types";

  let { report, entries, needs, colors, now, selected, active, onhover, onselect, onopen, onport, onstop, oninventory }: {
    report: AgentsReport;
    entries: PortEntry[] | null;
    needs: AttentionItem[];
    colors: Map<string, string>;
    now: number;
    selected: string | null;
    active: string | null;
    onhover: (id: string | null) => void;
    onselect: (id: string) => void;
    onopen: (id: string) => void;
    /** Opens the port's actions (Details, Stop…) anchored to the clicked element. */
    onport: (entryId: string, anchor: HTMLElement | SVGElement) => void;
    onstop?: (id: string) => void;
    oninventory?: () => void;
  } = $props();

  const loose = $derived(unownedDevServers(entries, report));
  let root = $state<HTMLElement>();
  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    const heads = [...(root?.querySelectorAll<HTMLElement>("[data-card]") ?? [])];
    const at = heads.indexOf(document.activeElement as HTMLElement);
    if (at < 0) return;
    heads[(at + (e.key === "ArrowRight" ? 1 : heads.length - 1)) % heads.length]?.focus();
    e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="cards" aria-label="Agents" bind:this={root} {onkeydown}>
  {#each report.agents as a (a.id)}
    <HomeAgentCard agent={a} model={cardModel(a, entries)} needs={needs.filter((n) => attentionAgent(n, report) === a.id)} color={colors.get(a.id) ?? "var(--accent)"}
      {now} selected={selected === a.id} {active} {onhover} {onselect} {onopen} {onport} {onstop} {oninventory} />
  {/each}
  {#if loose.length}
    <article class="loose" aria-label="Unowned dev servers">
      <header><span class="tile"><Icon name="server" size={14} /></span><span class="ttl"><b>Unowned</b><small>{`${loose.length} dev`}</small></span></header>
      <ul>
        {#each loose as e (e.id)}
          <li><button type="button" class="row" class:warn={e.exposure === "all_interfaces"} aria-haspopup="menu" onclick={(ev) => onport(e.id, ev.currentTarget)}
            aria-label="Port {e.port} {e.project?.name ?? e.framework?.name ?? e.label}{e.framework?.name && e.project?.name ? ` ${e.framework.name}` : ''}{e.exposure === 'all_interfaces' ? ', exposed: open to the network' : ''}">
            <i></i><span class="port">:{e.port}</span>{" "}<span class="nm">{e.project?.name ?? e.framework?.name ?? e.label}</span>
            {#if e.framework?.name && e.project?.name}{" "}<span class="fw">{e.framework.name}</span>{/if}
            {#if e.exposure === "all_interfaces"}{" "}<span class="exp">exposed</span>{/if}
          </button></li>
        {/each}
      </ul>
    </article>
  {/if}
</section>

<style>
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 300px), 1fr)); gap: var(--sp-4); align-content: start; min-width: 0; }
  .loose { display: flex; flex-direction: column; gap: var(--sp-3); padding: var(--sp-4); border: 1px dashed var(--border-strong); border-radius: var(--r-lg); background: color-mix(in srgb, var(--surface) 60%, transparent); min-width: 0; }
  header { display: flex; align-items: center; gap: var(--sp-3); }
  .tile { width: 34px; height: 34px; display: grid; place-items: center; border-radius: 10px; background: var(--surface-2); color: var(--muted); }
  .ttl { display: grid; }
  .ttl b { font-size: var(--fs-heading); line-height: var(--lh-heading); font-weight: var(--fw-semibold); color: var(--text); }
  .ttl small { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; border-top: 1px solid var(--border); padding-top: var(--sp-2); }
  .row { width: 100%; display: flex; align-items: center; gap: var(--sp-2); padding: 6px 8px; border: 0; border-radius: var(--r-md); background: none; font: inherit; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text-2); text-align: left; cursor: pointer; flex-wrap: wrap; }
  .row:hover { background: var(--row-hover); color: var(--text); }
  .row:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .row i { width: 7px; height: 7px; border-radius: 50%; background: var(--faint); flex: none; }
  .row.warn i { background: var(--warn); box-shadow: 0 0 0 3px var(--warn-soft); }
  .port { font-family: var(--mono); color: var(--text); }
  .row.warn .port { color: var(--warn); }
  .nm { overflow-wrap: anywhere; }
  .fw { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .exp { margin-left: auto; padding: 0 7px; border-radius: var(--r-full); background: var(--warn-soft); color: var(--warn); font-size: var(--fs-caption); line-height: var(--lh-caption); }
</style>

<script lang="ts">
  // The details pane with nothing selected: a summary of the machine instead of a blank pane.
  // Counts, the ports other devices can reach (one click selects them) and the keys to start with.
  import Icon from "../Icon.svelte";
  import Kbd from "../ui/Kbd.svelte";
  import FrameworkIcon from "../FrameworkIcon.svelte";
  import { glance } from "../../lib/detail";
  import { title } from "../../lib/format";
  import type { PortEntry } from "../../lib/types";

  let { entries, onselect, mod = "Ctrl" }: { entries: PortEntry[]; onselect: (e: PortEntry) => void; mod?: string } = $props();
  const g = $derived(glance(entries));
</script>

<div class="glance">
  <h3>At a glance</h3>
  <div class="counts">
    <div><span class="n">{g.total}</span><span class="l">listening</span></div>
    <div><span class="n">{g.dev}</span><span class="l">dev servers</span></div>
    <div class:warn={g.exposedCount > 0}><span class="n">{g.exposedCount}</span><span class="l">exposed</span></div>
  </div>

  {#if g.exposed.length}
    <section aria-labelledby="glance-exposed">
      <h4 id="glance-exposed"><Icon name="globe" size={12} />Reachable from your network</h4>
      <ul>
        {#each g.exposed as e (e.id)}
          <li><button type="button" onclick={() => onselect(e)}>
            <span class="port">{e.port}</span>
            <FrameworkIcon entry={e} size={18} />
            <span class="t">{title(e)}</span>
            <Icon name="chevron" size={12} />
          </button></li>
        {/each}
      </ul>
      {#if g.exposedCount > g.exposed.length}<p class="more">and {g.exposedCount - g.exposed.length} more (press E to filter)</p>{/if}
    </section>
  {:else}
    <p class="calm"><Icon name="check" size={12} />Nothing is reachable from other devices.</p>
  {/if}

  <div class="keys"><span><Kbd keys={["↑", "↓"]} size="sm" /> select a port</span><span><Kbd keys={[mod, "K"]} size="sm" /> commands</span><span><Kbd keys="?" size="sm" /> shortcuts</span></div>
</div>

<style>
  .glance { padding: var(--sp-6) var(--sp-5); display: grid; gap: var(--sp-5); color: var(--muted); }
  h3 { margin: 0; color: var(--text); font-size: var(--fs-title); font-weight: var(--fw-semibold); letter-spacing: var(--ls-title); line-height: var(--lh-title); }
  .counts { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; }
  .counts div { display: grid; gap: 2px; padding: 10px 12px; box-shadow: -1px 0 0 var(--border); }
  .n { color: var(--text); font-size: var(--fs-heading); line-height: var(--lh-heading); letter-spacing: var(--ls-heading); font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
  .counts .warn .n { color: var(--warn); }
  .l { font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; font-weight: var(--fw-medium); }
  h4 { display: flex; align-items: center; gap: 6px; margin: 0 0 var(--sp-2); font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; font-weight: var(--fw-medium); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  button { width: 100%; display: flex; align-items: center; gap: var(--sp-2); height: 34px; padding: 0 var(--sp-2); border: 0; border-radius: var(--r-md); background: transparent; color: var(--text); font: inherit; text-align: left; cursor: pointer; transition: background var(--dur-1) var(--ease); }
  button:hover { background: var(--row-hover); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  button :global(svg:last-child) { color: var(--faint); margin-left: auto; flex: none; }
  .port { width: 48px; font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
  .t { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .more, .calm { margin: var(--sp-1) 0 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .calm { display: flex; align-items: center; gap: 6px; }
  .calm :global(svg) { color: var(--ok); }
  .keys { display: flex; flex-wrap: wrap; gap: 6px 14px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .keys span { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
</style>

<script lang="ts">
  // The details pane with nothing selected: a summary of the machine instead of a blank pane.
  // Counts, the ports other devices can reach (one click selects them) and the keys to start with.
  import Icon from "../Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Kbd from "../ui/Kbd.svelte";
  import StatStrip from "../ui/StatStrip.svelte";
  import FrameworkIcon from "../FrameworkIcon.svelte";
  import { glance } from "../../lib/detail";
  import { title } from "../../lib/format";
  import type { PortEntry } from "../../lib/types";

  let { entries, onselect, mod = "Ctrl", onhide }: { entries: PortEntry[]; onselect: (e: PortEntry) => void; mod?: string; onhide?: () => void } = $props();
  const g = $derived(glance(entries));
</script>

<div class="glance">
  <div class="head">
    <h2>At a glance</h2>
    {#if onhide}<IconButton icon="panel" size="sm" label="Hide details pane" kbd="B" onclick={onhide} />{/if}
  </div>
  <StatStrip compact label="Ports summary" stats={[
    { id: "total", label: "Listening", value: g.total },
    { id: "dev", label: "Dev servers", value: g.dev },
    { id: "exposed", label: "Exposed", value: g.exposedCount, tone: g.exposedCount > 0 ? "warn" : "plain" },
  ]} />

  {#if g.exposed.length}
    <section aria-labelledby="glance-exposed">
      <h3 id="glance-exposed"><Icon name="globe" size={12} />Reachable from your network</h3>
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
  .head { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-2); }
  h2 { margin: 0; color: var(--text); font-size: var(--fs-title); font-weight: var(--fw-semibold); letter-spacing: var(--ls-title); line-height: var(--lh-title); }
  h3 { display: flex; align-items: center; gap: 6px; margin: 0 0 var(--sp-2); color: var(--text); font-size: var(--fs-body-sm); font-weight: var(--fw-semibold); }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  button { width: 100%; display: flex; align-items: center; gap: var(--sp-2); height: 34px; padding: 0 var(--sp-2); border: 0; border-radius: var(--r-md); background: transparent; color: var(--text); font: inherit; text-align: left; cursor: pointer; transition: background var(--dur-1) var(--ease); }
  button:hover { background: var(--row-hover); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  button :global(svg:last-child) { color: var(--faint); margin-left: auto; flex: none; }
  button :global(.tile) { border-radius: 5px; }
  button :global(.corner) { border-color: var(--surface); }
  .port { width: 48px; font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
  .t { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .more, .calm { margin: var(--sp-1) 0 0; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .calm { display: flex; align-items: center; gap: 6px; }
  .calm :global(svg) { color: var(--ok); }
  .keys { display: flex; flex-wrap: wrap; gap: 6px 14px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .keys span { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
</style>

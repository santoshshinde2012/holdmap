<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { GraphNode, PortEntry } from "../lib/types";
  import { brandFor, brandForNode } from "../lib/frameworks";

  let { entry, node, size = 32 }: { entry?: PortEntry; node?: GraphNode; size?: number } = $props();
  const b = $derived(node ? brandForNode(node) : brandFor(entry!));
</script>

<span
  class="tile"
  class:hidden={b.kind === "hidden"}
  style="--bg: {b.bg}; --fg: {b.fg}; --tile: {size}px; width: {size}px; height: {size}px; --glyph: {b.glyph.length > 1 ? 0.36 : 0.44}"
  aria-hidden="true"
>
  {#if b.kind === "hidden"}
    <Icon name="lock" size={Math.round(size * 0.45)} />
  {:else if b.glyph === ""}
    <Icon name="shield" size={Math.round(size * 0.45)} />
  {:else}
    {b.glyph}
  {/if}
  {#if b.kind === "container"}<span class="corner"><Icon name="box" size={9} /></span>{/if}
</span>

<style>
  .tile {
    position: relative; flex: none; display: inline-grid; place-items: center;
    border-radius: 9px; background: var(--bg); color: var(--fg);
    font-family: var(--font); font-weight: var(--fw-semibold); line-height: 1;
    font-size: calc(var(--tile) * var(--glyph)); /* type-exempt: the monogram is artwork that scales with its tile */
    letter-spacing: -0.02em; /* type-exempt: optical tracking of the monogram */
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.08), inset 0 1px 0 rgb(255 255 255 / 0.12), 0 1px 2px rgb(0 0 0 / 0.12);
  }
  .tile.hidden { background: var(--surface-2); color: var(--muted); box-shadow: inset 0 0 0 1px var(--border); }
  .corner {
    position: absolute; right: -4px; bottom: -4px; width: 16px; height: 16px; border-radius: 6px;
    display: grid; place-items: center; background: #1d63b8; color: #fff; border: 2px solid var(--surface);
  }
</style>

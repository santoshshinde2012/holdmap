<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { PortEntry } from "../lib/types";
  import { brandFor } from "../lib/frameworks";

  let { entry, size = 32 }: { entry: PortEntry; size?: number } = $props();
  const b = $derived(brandFor(entry));
</script>

<span
  class="tile"
  class:hidden={b.kind === "hidden"}
  style="--bg: {b.bg}; --fg: {b.fg}; width: {size}px; height: {size}px; font-size: {Math.round(size * (b.glyph.length > 1 ? 0.36 : 0.44))}px"
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
    font-weight: 750; letter-spacing: -0.02em; font-family: var(--font);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.08), inset 0 1px 0 rgb(255 255 255 / 0.12), 0 1px 2px rgb(0 0 0 / 0.12);
  }
  .tile.hidden { background: var(--surface-2); color: var(--muted); box-shadow: inset 0 0 0 1px var(--border); }
  .corner {
    position: absolute; right: -4px; bottom: -4px; width: 16px; height: 16px; border-radius: 6px;
    display: grid; place-items: center; background: #1d63b8; color: #fff; border: 2px solid var(--surface);
  }
</style>

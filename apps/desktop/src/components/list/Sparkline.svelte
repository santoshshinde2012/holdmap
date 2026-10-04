<script lang="ts">
  // Tiny trend line (decorative; the value is in the parent's tooltip and accessible name).
  // `range` scales between the samples' min and max (memory) instead of from zero (CPU %).
  import { rangePoints, sparkPoints } from "../../lib/rows";
  let { values, width = 32, height = 14, range = false }: { values: number[]; width?: number; height?: number; range?: boolean } = $props();
  const pts = $derived(range ? rangePoints(values, width, height) : sparkPoints(values, width, height));
  const hot = $derived(!range && (values.at(-1) ?? 0) >= 50);
</script>

<svg class="spark" class:hot {width} {height} viewBox="0 0 {width} {height}" aria-hidden="true">
  {#if pts}
    <polyline points={pts} />
  {/if}
</svg>

<style>
  .spark { flex: none; overflow: visible; }
  polyline { fill: none; stroke: var(--accent); stroke-width: 1.25; stroke-linecap: round; stroke-linejoin: round; opacity: 0.75; }
  .hot polyline { stroke: var(--warn); }
</style>

<script lang="ts">
  // A small severity badge: "2 high" in the worst severity's colour. Renders nothing at zero.
  import { countsText, total, worst, type SeverityCounts } from "../../lib/analyzers";
  let { counts }: { counts: SeverityCounts } = $props();
  const level = $derived(worst(counts));
</script>

{#if total(counts)}<span class="badge sev-{level}" title="Open analyzer findings">{countsText(counts, 1)}</span>{/if}

<style>
  .badge { display: inline-flex; align-items: center; padding: 2px 7px; border-radius: var(--r-full); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); color: var(--muted); background: var(--surface-2); border: 1px solid var(--border); white-space: nowrap; }
  .sev-medium { color: var(--warn); border-color: color-mix(in srgb, var(--warn) 40%, var(--border)); }
  .sev-high, .sev-critical { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 40%, var(--border)); background: color-mix(in srgb, var(--danger) 8%, var(--surface)); }
</style>

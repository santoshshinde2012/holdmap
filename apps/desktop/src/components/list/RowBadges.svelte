<script lang="ts">
  // Right-aligned badge column: at most `max` chips, the rest folded into "+N" with a tooltip.
  import Icon from "../Icon.svelte";
  import { tooltip } from "../../lib/tooltip";
  import { splitBadges, type RowBadge } from "../../lib/rows";
  let { badges, max = 2 }: { badges: RowBadge[]; max?: number } = $props();
  const parts = $derived(splitBadges(badges, max));
</script>

<span class="badges">
  {#each parts.shown as b (b.id)}
    <span class="chip tone-{b.tone}" use:tooltip={b.tip}>{#if b.icon}<Icon name={b.icon} size={11} />{/if}<span class="t">{b.label}</span></span>
  {/each}
  {#if parts.overflow}
    <span class="chip more" use:tooltip={parts.overflow} aria-label="{parts.rest.length} more: {parts.overflow}">+{parts.rest.length}</span>
  {/if}
</span>

<style>
  .badges { display: flex; align-items: center; justify-content: flex-end; gap: 4px; min-width: 0; overflow: hidden; }
  .chip {
    display: inline-flex; align-items: center; gap: 4px; flex: none; height: 20px; padding: 0 7px; border-radius: var(--r-sm);
    font-size: var(--fs-caption); line-height: var(--lh-caption); letter-spacing: var(--ls-caption); font-weight: var(--fw-medium);
    color: var(--muted); background: var(--surface-2); box-shadow: inset 0 0 0 1px var(--border); white-space: nowrap; font-variant-numeric: tabular-nums;
  }
  .chip :global(svg) { opacity: 0.85; }
  .t { max-width: 88px; overflow: hidden; text-overflow: ellipsis; }
  .tone-amber { color: var(--tone-amber); background: var(--tone-amber-bg); box-shadow: none; }
  .tone-blue { color: var(--tone-blue); background: var(--tone-blue-bg); box-shadow: none; }
  .tone-violet { color: var(--tone-violet); background: var(--tone-violet-bg); box-shadow: none; }
  .tone-quiet { background: transparent; box-shadow: none; color: var(--faint); padding: 0 4px; }
  .more { padding: 0 6px; color: var(--text-2); }
</style>

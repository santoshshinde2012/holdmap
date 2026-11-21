<script lang="ts">
  // One disclosure for a details section: the heading holds the chevron, the body opens in place.
  // Open by default, so a screen never hides its facts until the viewer folds them.
  import type { Snippet } from "svelte";
  import Icon from "../Icon.svelte";

  let { id, title, count = null, open = $bindable(true), children }: {
    /** Section id. The heading is `{id}-h` and the body is `{id}-b`. */
    id: string;
    title: string;
    count?: number | null;
    open?: boolean;
    children: Snippet;
  } = $props();
</script>

<section class="foldsec" aria-labelledby="{id}-h">
  <h4 id="{id}-h">
    <button type="button" class="fold" aria-expanded={open} aria-controls="{id}-b" onclick={() => (open = !open)}>
      <span class="chev" aria-hidden="true"><Icon name="chevron-down" size={14} /></span>
      <span class="t">{title}</span>
      {#if count != null}<span class="n">{count}</span>{/if}
    </button>
  </h4>
  {#if open}<div class="body" id="{id}-b">{@render children()}</div>{/if}
</section>

<style>
  .foldsec { display: grid; gap: 8px; min-width: 0; }
  h4 { margin: 0; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); }
  .fold { display: flex; align-items: center; gap: 6px; width: 100%; min-height: 28px; padding: 2px 2px; border: 0; border-radius: var(--r-sm); background: transparent; color: var(--text-2); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); letter-spacing: var(--ls-label); text-transform: uppercase; text-align: left; cursor: pointer; }
  .fold:hover { background: var(--row-hover); color: var(--text); }
  .fold:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .chev { display: inline-flex; color: var(--muted); transition: transform 160ms ease; }
  .fold[aria-expanded="false"] .chev { transform: rotate(-90deg); }
  @media (prefers-reduced-motion: reduce) { .chev { transition: none; } }
  .t { min-width: 0; }
  .n { margin-left: auto; padding: 0 7px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-weight: var(--fw-medium); font-variant-numeric: tabular-nums; letter-spacing: normal; text-transform: none; }
  .body { min-width: 0; }
</style>

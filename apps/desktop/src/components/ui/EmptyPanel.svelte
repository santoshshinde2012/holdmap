<script lang="ts">
  // The calm inline empty state shared by Security, Needs you and the side panels: a soft icon
  // tile, a one-line title, an optional short hint and an optional action.
  import type { Snippet } from "svelte";
  import Icon from "../Icon.svelte";
  let { icon, title, hint = "", tone = "accent", compact = false, children }:
    { icon: string; title: string; hint?: string; tone?: "accent" | "ok"; compact?: boolean; children?: Snippet } = $props();
</script>

<div class="empty tone-{tone}" class:compact role="status">
  <span class="ic" aria-hidden="true"><Icon name={icon} size={compact ? 14 : 18} /></span>
  <p>{title}</p>
  {#if hint}<span class="hint">{hint}</span>{/if}
  {@render children?.()}
</div>

<style>
  .empty { display: grid; align-content: center; justify-items: center; gap: 6px; padding: 40px 16px; border: 1px dashed var(--border-strong); border-radius: var(--r-lg); text-align: center; color: var(--muted); font-size: var(--fs-body-sm); }
  .compact { padding: 20px 12px; gap: 4px; }
  .hint { max-width: 360px; }
  .ic { display: grid; place-items: center; width: 36px; height: 36px; margin-bottom: 4px; border-radius: 50%; background: var(--accent-soft); color: var(--accent); }
  .compact .ic { width: 28px; height: 28px; }
  .tone-ok .ic { background: var(--ok-soft); color: var(--ok); }
  p { margin: 0; color: var(--text); font-size: var(--fs-body); font-weight: var(--fw-semibold); }
  .empty :global(.ui-btn) { margin-top: 8px; }
</style>

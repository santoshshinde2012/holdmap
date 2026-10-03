<script lang="ts">
  // Sticky section header for the port list: chevron (collapse), label, count, optional detail.
  import Icon from "../Icon.svelte";
  import { tooltip } from "../../lib/tooltip";
  let {
    id,
    title,
    count,
    hint = "",
    showHint = false,
    collapsed = false,
    ontoggle,
  }: {
    id: string;
    title: string;
    count: number;
    hint?: string;
    /** Render the hint inline (cluster details) instead of as a tooltip (generic group blurbs). */
    showHint?: boolean;
    collapsed?: boolean;
    ontoggle: () => void;
  } = $props();
</script>

<div class="gh" class:collapsed role="presentation">
  <button
    type="button"
    class="toggle"
    tabindex="-1"
    aria-expanded={!collapsed}
    aria-controls="grp-{id}"
    use:tooltip={!showHint && hint ? hint : null}
    onclick={ontoggle}
  >
    <span class="chev" aria-hidden="true"><Icon name="chevron" size={12} /></span>
    <span class="label" id="gl-{id}">{title}</span>
    <span class="count" aria-label="{count} port{count === 1 ? '' : 's'}">{count}</span>
    {#if showHint && hint}<span class="hint">{hint}</span>{/if}
  </button>
</div>

<style>
  .gh {
    position: sticky; top: 0; z-index: 3; height: 34px; display: flex; align-items: flex-end; padding: 0 var(--sp-3) 0 var(--sp-2);
    background: color-mix(in srgb, var(--bg) 88%, transparent); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);
  }
  .toggle {
    display: inline-flex; align-items: center; gap: 6px; height: 26px; margin-bottom: 2px; padding: 0 8px 0 4px; border: 0; border-radius: var(--r-sm);
    background: transparent; color: var(--text-2); cursor: pointer; min-width: 0; max-width: 100%; transition: background var(--dur-1) var(--ease), color var(--dur-1) var(--ease);
  }
  .toggle:hover { background: var(--row-hover); color: var(--text); }
  .toggle:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  .chev { display: inline-grid; place-items: center; width: 16px; color: var(--faint); transform: rotate(90deg); transition: transform var(--dur-2) var(--ease); }
  .collapsed .chev { transform: none; }
  .label { font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); font-weight: var(--fw-medium); text-transform: uppercase; white-space: nowrap; }
  .count { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--faint); font-variant-numeric: tabular-nums; font-weight: var(--fw-medium); }
  .hint { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; margin-left: 4px; }
  @container portlist (max-width: 560px) { .hint { display: none; } }
</style>

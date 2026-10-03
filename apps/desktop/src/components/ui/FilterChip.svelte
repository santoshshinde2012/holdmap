<script lang="ts">
  // Toggleable filter pill (aria-pressed) with optional icon / colour dot and a count.
  import Icon from "../Icon.svelte";
  import { tooltip } from "../../lib/tooltip";

  let {
    pressed = $bindable(false),
    label,
    icon,
    dot,
    count,
    tone = "accent",
    title,
    kbd,
    onchange,
  }: {
    pressed?: boolean;
    label: string;
    icon?: string;
    /** CSS colour for a leading dot. */
    dot?: string;
    count?: number;
    /** `warn` tints the icon when there is something to warn about. */
    tone?: "accent" | "warn";
    title?: string;
    kbd?: string;
    onchange?: (pressed: boolean) => void;
  } = $props();
</script>

<button type="button" class="chip {tone}" class:on={pressed} aria-pressed={pressed} use:tooltip={title ? { text: title, kbd } : null} onclick={() => { pressed = !pressed; onchange?.(pressed); }}>
  {#if dot}<span class="dot" style="background: {dot}" aria-hidden="true"></span>{:else if icon}<Icon name={icon} size={12} />{/if}
  <span>{label}</span>
  {#if count !== undefined}<span class="count">{count}</span>{/if}
</button>

<style>
  .chip { flex: none; display: inline-flex; align-items: center; gap: 6px; height: var(--h-sm); padding: 0 6px 0 10px; border-radius: var(--r-full); border: 1px solid var(--input-border); background: var(--surface); color: var(--text-2); font: inherit; font-size: var(--fs-xs); font-weight: 550; cursor: pointer; transition: background var(--dur-1), border-color var(--dur-1), color var(--dur-1); }
  .chip:hover { background: var(--surface-2); color: var(--text); border-color: var(--input-border-hover); }
  .chip:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .chip.on { border-color: color-mix(in srgb, var(--accent) 55%, transparent); background: var(--accent-soft); color: var(--accent); }
  .warn:not(.on) :global(svg) { color: var(--warn); }
  .dot { width: 7px; height: 7px; border-radius: 50%; }
  .count { min-width: 20px; height: 18px; padding: 0 5px; border-radius: var(--r-full); background: var(--surface-2); color: var(--muted); font-size: 10.5px; display: inline-grid; place-items: center; font-variant-numeric: tabular-nums; font-weight: 650; }
  .on .count { background: var(--accent); color: var(--accent-fg); }
</style>

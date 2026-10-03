<script lang="ts">
  // Square icon-only button. `label` is required: it is the accessible name and the tooltip.
  import type { HTMLButtonAttributes } from "svelte/elements";
  import Icon from "../Icon.svelte";
  import { tooltip } from "../../lib/tooltip";

  let {
    icon,
    label,
    kbd,
    size = "md",
    variant = "ghost",
    pressed,
    tip = true,
    el = $bindable(),
    class: cls = "",
    ...rest
  }: Omit<HTMLButtonAttributes, "class"> & {
    icon: string;
    label: string;
    kbd?: string | string[];
    size?: "xs" | "sm" | "md";
    variant?: "ghost" | "outline";
    /** Toggle state; renders aria-pressed. */
    pressed?: boolean;
    tip?: boolean;
    el?: HTMLButtonElement;
    class?: string;
  } = $props();
  const px = $derived(size === "xs" ? 12 : size === "sm" ? 14 : 16);
</script>

<button bind:this={el} type="button" class="ib {size} {variant} {cls}" class:on={pressed} aria-label={label} aria-pressed={pressed} use:tooltip={tip ? { text: label, kbd } : null} {...rest}>
  <Icon name={icon} size={px} />
</button>

<style>
  .ib { display: inline-grid; place-items: center; flex: none; width: var(--h-md); height: var(--h-md); border-radius: var(--r-md); border: 1px solid transparent; background: transparent; color: var(--muted); cursor: pointer; transition: background var(--dur-1), color var(--dur-1), border-color var(--dur-1); }
  .ib:hover:not(:disabled) { background: var(--row-hover); color: var(--text); }
  .ib:active:not(:disabled) { background: var(--surface-3); }
  .ib:disabled { opacity: 0.45; cursor: not-allowed; }
  .ib:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .sm { width: var(--h-sm); height: var(--h-sm); border-radius: var(--r-sm); }
  .xs { width: var(--h-xs); height: var(--h-xs); border-radius: var(--r-sm); }
  .outline { border-color: var(--input-border); background: var(--surface); box-shadow: var(--control-shadow); }
  .outline:hover:not(:disabled) { border-color: var(--input-border-hover); background: var(--surface-2); }
  .on { color: var(--accent); }
</style>

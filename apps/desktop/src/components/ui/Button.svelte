<script lang="ts">
  // Button: variants × sizes, leading/trailing icon, keyboard hint, loading and disabled states.
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";
  import Icon from "../Icon.svelte";
  import Kbd from "./Kbd.svelte";
  import { tooltip, type TooltipOptions } from "../../lib/tooltip";

  export type Variant = "primary" | "secondary" | "ghost" | "danger" | "danger-outline" | "soft";
  export type Size = "xs" | "sm" | "md" | "lg";

  let {
    variant = "secondary",
    size = "md",
    icon,
    iconRight,
    kbd,
    loading = false,
    loadingText,
    full = false,
    tip,
    disabled = false,
    type = "button",
    el = $bindable(),
    children,
    class: cls = "",
    ...rest
  }: Omit<HTMLButtonAttributes, "class"> & {
    variant?: Variant;
    size?: Size;
    icon?: string;
    iconRight?: string;
    kbd?: string | string[];
    loading?: boolean;
    /** Label while loading (defaults to the normal label). */
    loadingText?: string;
    full?: boolean;
    /** Tooltip (use instead of `title`, which renders an inconsistent native tooltip). */
    tip?: string | TooltipOptions;
    el?: HTMLButtonElement;
    children?: Snippet;
    class?: string;
  } = $props();

  const iconSize = $derived(size === "xs" ? 12 : size === "sm" ? 13 : size === "lg" ? 15 : 14);
  const inverse = $derived(variant === "primary" || variant === "danger");
</script>

<button bind:this={el} {type} class="ui-btn {variant} {size} {cls}" class:full class:loading disabled={disabled || loading} aria-busy={loading || undefined} aria-disabled={disabled || loading || undefined} use:tooltip={tip} {...rest}>
  {#if loading}<span class="spinner" aria-hidden="true"></span>{:else if icon}<Icon name={icon} size={iconSize} />{/if}
  {#if children}<span class="lbl">{#if loading && loadingText}{loadingText}{:else}{@render children()}{/if}</span>{/if}
  {#if iconRight && !loading}<Icon name={iconRight} size={iconSize} />{/if}
  {#if kbd && !loading && size !== "xs"}<Kbd keys={kbd} size="sm" tone={inverse ? "inverse" : "default"} />{/if}
</button>

<style>
  .ui-btn {
    --bh: var(--h-md);
    position: relative; display: inline-flex; align-items: center; justify-content: center; gap: 7px;
    height: var(--bh); padding: 0 12px; border-radius: var(--r-md);
    border: 1px solid transparent; font: inherit; font-size: var(--fs-sm); font-weight: 550; line-height: 1;
    white-space: nowrap; user-select: none; cursor: pointer; flex: none;
    transition: background var(--dur-1) var(--ease), border-color var(--dur-1), color var(--dur-1), box-shadow var(--dur-1), transform var(--dur-1);
  }
  .ui-btn:active:not(:disabled) { transform: translateY(0.5px); }
  .ui-btn:focus-visible { outline: 2px solid var(--ring); outline-offset: 2px; }
  .ui-btn:disabled { cursor: not-allowed; opacity: 0.5; box-shadow: none; }
  .ui-btn.loading { cursor: progress; opacity: 0.85; }
  .lbl { display: inline-flex; align-items: center; gap: 6px; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .full { width: 100%; }

  .xs { --bh: var(--h-xs); padding: 0 8px; font-size: var(--fs-xs); gap: 5px; border-radius: var(--r-sm); }
  .sm { --bh: var(--h-sm); padding: 0 10px; font-size: var(--fs-xs); gap: 6px; border-radius: var(--r-sm); }
  .lg { --bh: var(--h-lg); padding: 0 16px; font-size: var(--fs-base); gap: 8px; }

  .secondary { background: var(--surface); color: var(--text); border-color: var(--input-border); box-shadow: var(--control-shadow); }
  .secondary:hover:not(:disabled) { background: var(--surface-2); border-color: var(--input-border-hover); }
  .primary { background: var(--accent); color: var(--accent-fg); box-shadow: var(--control-shadow), inset 0 1px 0 rgb(255 255 255 / 0.12); }
  .primary:hover:not(:disabled) { background: var(--accent-hover); }
  .danger { background: var(--danger); color: var(--danger-fg); box-shadow: var(--control-shadow), inset 0 1px 0 rgb(255 255 255 / 0.12); }
  .danger:hover:not(:disabled) { background: var(--danger-hover); }
  .danger:focus-visible { outline-color: color-mix(in srgb, var(--danger) 60%, transparent); }
  .danger-outline { background: transparent; color: var(--danger); border-color: color-mix(in srgb, var(--danger) 38%, transparent); }
  .danger-outline:hover:not(:disabled) { background: var(--danger-soft); border-color: color-mix(in srgb, var(--danger) 55%, transparent); }
  .ghost { background: transparent; color: var(--text-2); }
  .ghost:hover:not(:disabled) { background: var(--row-hover); color: var(--text); }
  .soft { background: var(--accent-soft); color: var(--accent); }
  .soft:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 18%, transparent); }

  .spinner { width: 13px; height: 13px; border-radius: 50%; border: 2px solid currentColor; border-right-color: transparent; animation: spin 0.7s linear infinite; flex: none; }
  .xs .spinner, .sm .spinner { width: 11px; height: 11px; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>

<script lang="ts" module>
  import type { Segment } from "./SegmentedControl.svelte";
  export type { Segment };
</script>

<script lang="ts" generics="T extends string">
  // Exclusive action buttons that belong to a parent view. Not a second segmented track:
  // each choice is its own pill, hinged to the control on the left.
  import Icon from "../Icon.svelte";
  import { nextIndex } from "../../lib/roving";
  import { tooltip } from "../../lib/tooltip";
  import { isComposingKey } from "../../lib/keyboard";

  let {
    value = $bindable(),
    options,
    label,
    hinge = true,
    animate = true,
    size = "md",
    appearance = "pills",
    onchange,
  }: {
    value: T;
    options: Segment<T>[];
    label: string;
    hinge?: boolean;
    animate?: boolean;
    size?: "md" | "compact";
    /** Pills are sibling actions. Tabs are a section bar: text, icon, and an underline. */
    appearance?: "pills" | "tabs";
    onchange?: (v: T) => void;
  } = $props();

  let buttons: HTMLButtonElement[] = $state([]);
  const idx = $derived(Math.max(0, options.findIndex((o) => o.value === value)));

  function select(i: number, focus = false) {
    const o = options[i];
    if (!o || o.disabled) return;
    if (focus) buttons[i]?.focus();
    if (o.value !== value) { value = o.value; onchange?.(o.value); }
  }
  function key(e: KeyboardEvent) {
    if (isComposingKey(e) || e.altKey || e.ctrlKey || e.metaKey) return;
    const n = nextIndex(idx, e.key, options.length, { orientation: "both", disabled: options.map((o) => !!o.disabled) });
    if (n !== null) { select(n, true); e.preventDefault(); }
  }
</script>

<div class="dock" class:animate class:compact={size === "compact"} class:tabs={appearance === "tabs"} role="radiogroup" aria-label={label} tabindex="-1" onkeydown={key}>
  {#if hinge && appearance !== "tabs"}<span class="hinge" aria-hidden="true"></span>{/if}
  {#each options as o, i (o.value)}
    <button
      bind:this={buttons[i]}
      type="button"
      role="radio"
      class:action-pill={appearance !== "tabs"}
      class:tab={appearance === "tabs"}
      class:compact={size === "compact" && appearance !== "tabs"}
      aria-checked={o.value === value}
      tabindex={i === idx ? 0 : -1}
      class:on={o.value === value}
      disabled={o.disabled}
      title={o.title}
      use:tooltip={o.title ? { text: o.title, placement: "bottom" } : null}
      onclick={() => select(i)}
    >
      {#if o.icon}<Icon name={o.icon} size={appearance === "tabs" ? 14 : 12} />{/if}<span>{o.label}</span>{#if o.count != null}<span class="count" aria-hidden={o.countLabel ? "true" : undefined}>{o.count}</span>{#if o.countLabel}<span class="sr-only">, {o.countLabel}</span>{/if}{/if}
    </button>
  {/each}
</div>

<style>
  .dock { display: inline-flex; align-items: center; gap: 4px; flex: none; outline: none; }
  .dock.animate { animation: in var(--dur-2) var(--ease); }
  .hinge { width: 1px; height: 14px; margin: 0 6px 0 4px; border-radius: 1px; background: var(--border-strong); flex: none; }
  .dock.compact { gap: 3px; }
  .dock.compact .hinge { height: 12px; margin: 0 4px; }
  .dock.tabs { flex-wrap: wrap; align-items: stretch; gap: 0; flex: 1 1 auto; min-width: 0; max-width: 100%; }
  .tab { display: inline-flex; align-items: center; gap: 6px; height: 40px; padding: 0 10px; flex: none; border: 0; border-bottom: 2px solid transparent; border-radius: var(--r-sm) var(--r-sm) 0 0; background: transparent; box-shadow: none; color: var(--muted); font: inherit; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-medium); white-space: nowrap; cursor: pointer; }
  .tab:hover:not(:disabled) { color: var(--text); background: var(--surface-2); }
  .tab.on { color: var(--text); border-bottom-color: var(--accent); background: transparent; }
  .tab.on:hover:not(:disabled) { color: var(--text); background: transparent; }
  .tab:disabled { opacity: 0.45; cursor: not-allowed; }
  .tab:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; }
  .tab .count { font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); min-width: 16px; padding: 0 5px; border-radius: var(--r-full); background: var(--surface-3); color: var(--text-2); font-variant-numeric: tabular-nums; }
  .tab.on .count { background: var(--accent-soft); color: var(--accent); }
  @keyframes in { from { opacity: 0; transform: translateX(-6px); } to { opacity: 1; transform: none; } }
  @media (prefers-reduced-motion: reduce) { .dock.animate { animation: none; } }
</style>

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
    onchange,
  }: {
    value: T;
    options: Segment<T>[];
    label: string;
    hinge?: boolean;
    animate?: boolean;
    size?: "md" | "compact";
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

<div class="dock" class:animate class:compact={size === "compact"} role="radiogroup" aria-label={label} tabindex="-1" onkeydown={key}>
  {#if hinge}<span class="hinge" aria-hidden="true"></span>{/if}
  {#each options as o, i (o.value)}
    <button
      bind:this={buttons[i]}
      type="button"
      role="radio"
      class="action-pill"
      class:compact={size === "compact"}
      aria-checked={o.value === value}
      tabindex={i === idx ? 0 : -1}
      class:on={o.value === value}
      disabled={o.disabled}
      title={o.title}
      use:tooltip={o.title ? { text: o.title, placement: "bottom" } : null}
      onclick={() => select(i)}
    >
      {#if o.icon}<Icon name={o.icon} size={12} />{/if}<span>{o.label}</span>{#if o.count != null}<span class="count" aria-hidden={o.countLabel ? "true" : undefined}>{o.count}</span>{#if o.countLabel}<span class="sr-only">, {o.countLabel}</span>{/if}{/if}
    </button>
  {/each}
</div>

<style>
  .dock { display: inline-flex; align-items: center; gap: 4px; flex: none; outline: none; }
  .dock.animate { animation: in var(--dur-2) var(--ease); }
  .hinge { width: 1px; height: 14px; margin: 0 6px 0 4px; border-radius: 1px; background: var(--border-strong); flex: none; }
  .dock.compact { gap: 3px; }
  .dock.compact .hinge { height: 12px; margin: 0 4px; }
  @keyframes in { from { opacity: 0; transform: translateX(-6px); } to { opacity: 1; transform: none; } }
  @media (prefers-reduced-motion: reduce) { .dock.animate { animation: none; } }
</style>

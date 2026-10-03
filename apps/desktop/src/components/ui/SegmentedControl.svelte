<script lang="ts" module>
  export interface Segment<T extends string = string> { value: T; label: string; icon?: string; count?: number | null; title?: string; disabled?: boolean }
</script>

<script lang="ts" generics="T extends string">
  // Radio group styled as a segmented control, with a sliding selection indicator.
  // Arrow keys move and select (roving tabindex), Home/End jump.
  import Icon from "../Icon.svelte";
  import { nextIndex } from "../../lib/roving";
  import { tooltip } from "../../lib/tooltip";

  let {
    value = $bindable(),
    options,
    label,
    size = "sm",
    full = false,
    onchange,
  }: {
    value: T;
    options: Segment<T>[];
    /** Accessible name of the group. */
    label: string;
    size?: "sm" | "md";
    full?: boolean;
    onchange?: (v: T) => void;
  } = $props();

  let buttons: HTMLButtonElement[] = $state([]);
  let ind = $state({ left: 0, width: 0, ready: false });
  const idx = $derived(Math.max(0, options.findIndex((o) => o.value === value)));

  $effect(() => {
    void value; void options.length;
    const b = buttons[idx];
    if (!b) return;
    const measure = () => (ind = { left: b.offsetLeft, width: b.offsetWidth, ready: b.offsetWidth > 0 });
    measure();
    const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(measure) : null;
    ro?.observe(b);
    return () => ro?.disconnect();
  });

  function select(i: number, focus = false) {
    const o = options[i];
    if (!o || o.disabled) return;
    if (focus) buttons[i]?.focus();
    if (o.value !== value) { value = o.value; onchange?.(o.value); }
  }
  function key(e: KeyboardEvent) {
    const n = nextIndex(idx, e.key, options.length, { orientation: "both", disabled: options.map((o) => !!o.disabled) });
    if (n !== null) { select(n, true); e.preventDefault(); }
  }
</script>

<div class="seg {size}" class:full role="radiogroup" aria-label={label} tabindex="-1" onkeydown={key}>
  {#if ind.ready}<span class="ind" style="transform: translateX({ind.left - 3}px); width: {ind.width}px" aria-hidden="true"></span>{/if}
  {#each options as o, i (o.value)}
    <button
      bind:this={buttons[i]}
      type="button"
      role="radio"
      aria-checked={o.value === value}
      tabindex={i === idx ? 0 : -1}
      class:on={o.value === value}
      class:noind={!ind.ready}
      disabled={o.disabled}
      use:tooltip={o.title ?? null}
      onclick={() => select(i)}
    >
      {#if o.icon}<Icon name={o.icon} size={size === "sm" ? 12 : 14} />{/if}<span>{o.label}</span>{#if o.count != null}<span class="count">{o.count}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .seg { position: relative; display: inline-flex; padding: 2px; gap: 1px; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r-md); flex: none; isolation: isolate; outline: none; }
  .seg.full { display: flex; }
  .full button { flex: 1; }
  .ind { position: absolute; top: 2px; bottom: 2px; left: 3px; z-index: -1; border-radius: var(--r-sm); background: var(--surface); box-shadow: var(--shadow-sm), 0 0 0 1px var(--border); transition: transform var(--dur-2) var(--ease), width var(--dur-2) var(--ease); }
  :global([data-theme="dark"]) .ind { background: var(--surface-3); }
  button { position: relative; display: inline-flex; align-items: center; justify-content: center; gap: 6px; border: 0; background: transparent; height: calc(var(--h-sm) - 6px); padding: 0 10px; border-radius: var(--r-sm); color: var(--muted); font: inherit; font-weight: var(--fw-medium); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); white-space: nowrap; cursor: pointer; transition: color var(--dur-1); }
  .md button { height: calc(var(--h-md) - 6px); font-size: var(--fs-body); line-height: var(--lh-body); padding: 0 12px; }
  button:hover:not(:disabled) { color: var(--text); }
  button.on { color: var(--text); }
  button.on.noind { background: var(--surface); box-shadow: var(--shadow-sm), 0 0 0 1px var(--border); }
  button:disabled { opacity: 0.45; cursor: not-allowed; }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .count { font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); padding: 1px 5px; border-radius: 999px; background: var(--accent-soft); color: var(--accent); font-variant-numeric: tabular-nums; }
</style>

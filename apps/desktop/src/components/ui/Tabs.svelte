<script lang="ts" module>
  export interface TabItem<T extends string = string> { id: T; label: string; count?: number | null; icon?: string; alert?: boolean }
  /** ids that pair a tab with its panel: <div role="tabpanel" id={panelId(base, id)} aria-labelledby={tabId(base, id)}> */
  export const tabId = (base: string, id: string) => `${base}-tab-${id}`;
  export const panelId = (base: string, id: string) => `${base}-panel-${id}`;
</script>

<script lang="ts" generics="T extends string">
  // Underlined tab list with automatic activation, roving focus and an animated indicator.
  import Icon from "../Icon.svelte";
  import { nextIndex } from "../../lib/roving";

  let { tabs, value = $bindable(), base, label, onchange }: { tabs: TabItem<T>[]; value: T; base: string; label: string; onchange?: (v: T) => void } = $props();

  let els: HTMLButtonElement[] = $state([]);
  let ind = $state({ left: 0, width: 0 });
  const idx = $derived(Math.max(0, tabs.findIndex((t) => t.id === value)));

  $effect(() => {
    void value; void tabs.length;
    const b = els[idx];
    if (!b) return;
    const m = () => (ind = { left: b.offsetLeft, width: b.offsetWidth });
    m();
    const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(m) : null;
    ro?.observe(b);
    return () => ro?.disconnect();
  });

  function go(i: number) {
    const t = tabs[i];
    if (!t) return;
    els[i]?.focus();
    els[i]?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    if (t.id !== value) { value = t.id; onchange?.(t.id); }
  }
  function key(e: KeyboardEvent) {
    const n = nextIndex(idx, e.key, tabs.length);
    if (n !== null) { go(n); e.preventDefault(); }
  }
</script>

<div class="tabs" role="tablist" aria-label={label} tabindex="-1" onkeydown={key}>
  {#each tabs as t, i (t.id)}
    <button
      bind:this={els[i]}
      type="button"
      role="tab"
      id={tabId(base, t.id)}
      aria-selected={t.id === value}
      aria-controls={panelId(base, t.id)}
      tabindex={t.id === value ? 0 : -1}
      class:on={t.id === value}
      onclick={() => go(i)}
    >
      {#if t.icon}<Icon name={t.icon} size={13} />{/if}{t.label}{#if t.count}<span class="count">{t.count}</span>{/if}{#if t.alert}<span class="alert" aria-label="needs attention"></span>{/if}
    </button>
  {/each}
  <span class="ind" style="transform: translateX({ind.left}px); width: {ind.width}px" aria-hidden="true"></span>
</div>

<style>
  .tabs { position: relative; display: flex; gap: 0; overflow-x: auto; scrollbar-width: none; border-bottom: 1px solid var(--border); padding: 0 var(--sp-3); outline: none; }
  .tabs::-webkit-scrollbar { display: none; }
  button { position: relative; display: inline-flex; align-items: center; gap: 6px; height: 38px; padding: 0 9px; flex: none; border: 0; background: transparent; color: var(--muted); font: inherit; font-size: var(--fs-sm); font-weight: 550; white-space: nowrap; cursor: pointer; border-radius: var(--r-sm) var(--r-sm) 0 0; transition: color var(--dur-1); }
  button:hover { color: var(--text); }
  button.on { color: var(--text); }
  button:focus-visible { outline: 2px solid var(--ring); outline-offset: -4px; }
  .count { font-size: 10.5px; font-weight: 650; min-width: 18px; height: 17px; padding: 0 5px; border-radius: 999px; display: inline-grid; place-items: center; background: var(--surface-3); color: var(--text-2); font-variant-numeric: tabular-nums; }
  .on .count { background: var(--accent-soft); color: var(--accent); }
  .alert { width: 6px; height: 6px; border-radius: 50%; background: var(--warn); }
  .ind { position: absolute; left: 0; bottom: -1px; height: 2px; border-radius: 2px; background: var(--accent); transition: transform var(--dur-2) var(--ease), width var(--dur-2) var(--ease); pointer-events: none; }
</style>

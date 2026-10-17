<script lang="ts">
  // Actions for one port, anchored to whatever was clicked on Home (card chip, map node,
  // timeline pin): Details, and Stop… which always opens the stop plan for review first; the plan
  // guards protected and operating-system owners. Stop… is off only when there is no owner to stop.
  // Arrow keys move, Esc or a click outside closes and returns focus to the anchor.
  import Icon from "../Icon.svelte";
  import { isComposingKey } from "../../lib/keyboard";
  import { stopState } from "../../lib/detail";
  import type { PortEntry } from "../../lib/types";

  let { entry, anchor, ondetails, onstop, onclose }: {
    entry: PortEntry;
    anchor: HTMLElement | SVGElement;
    ondetails: () => void;
    /** Opens the stop plan; nothing stops without confirmation. */
    onstop: () => void;
    onclose: () => void;
  } = $props();

  let menu = $state<HTMLDivElement>();
  const stop = $derived(stopState(entry, null));
  const name = $derived(entry.project?.name ?? entry.framework?.name ?? entry.label);
  // Below the anchor, flipped up near the bottom edge and kept inside the window.
  const pos = $derived.by(() => {
    const r = anchor.getBoundingClientRect();
    const w = typeof window === "undefined" ? 1024 : window.innerWidth, h = typeof window === "undefined" ? 768 : window.innerHeight;
    const left = Math.max(8, Math.min(r.left, w - 228));
    return r.bottom + 120 > h ? { left, top: null, bottom: h - r.top + 6 } : { left, top: r.bottom + 6, bottom: null };
  });
  const items = () => [...(menu?.querySelectorAll<HTMLButtonElement>("[role=menuitem]:not(:disabled)") ?? [])];
  $effect(() => { if (menu) requestAnimationFrame(() => items()[0]?.focus()); });

  function close(focus = true) { onclose(); if (focus && "focus" in anchor) (anchor as HTMLElement).focus?.(); }
  function key(e: KeyboardEvent) {
    if (isComposingKey(e) || e.altKey || e.ctrlKey || e.metaKey) return;
    const list = items(), i = list.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); list[(i + 1) % list.length]?.focus(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); list[(i - 1 + list.length) % list.length]?.focus(); }
    else if (e.key === "Tab") close(false);
  }
  function outside(e: PointerEvent) { if (!menu?.contains(e.target as Node) && !anchor.contains(e.target as Node)) close(false); }
  function run(fn: () => void) { fn(); close(false); }
</script>

<svelte:window onpointerdown={outside} onresize={() => close(false)} />

<!-- svelte-ignore a11y_interactive_supports_focus -->
<div class="menu" role="menu" aria-label=":{entry.port} {name}" tabindex="-1" bind:this={menu} onkeydown={key}
  style="left: {pos.left}px; {pos.top !== null ? `top: ${pos.top}px` : `bottom: ${pos.bottom}px`}">
  <div class="head" aria-hidden="true"><span class="port">:{entry.port}</span><span class="nm">{name}</span>{#if entry.exposure === "all_interfaces"}<span class="exp">exposed</span>{/if}</div>
  <button type="button" role="menuitem" tabindex="-1" onclick={() => run(ondetails)}><Icon name="info" size={14} /><span class="l">Details</span></button>
  <button type="button" role="menuitem" tabindex="-1" class="stop" disabled={!stop.stoppable} title={stop.reason ?? undefined} onclick={() => run(onstop)}>
    <Icon name="stop" size={14} /><span class="l">Stop…</span>{#if !stop.stoppable}<span class="sr-only">, {stop.reason}</span>{/if}
  </button>
</div>

<style>
  .menu { position: fixed; z-index: 40; width: 220px; padding: 4px; display: grid; gap: 1px; background: var(--surface-raised, var(--surface)); border: 1px solid var(--border-strong); border-radius: var(--r-lg); box-shadow: var(--shadow-lg, 0 8px 24px rgb(0 0 0 / 0.18)); }
  .head { display: flex; align-items: center; gap: 6px; padding: 6px 10px 7px; margin-bottom: 2px; border-bottom: 1px solid var(--border); font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); min-width: 0; }
  .port { font-family: var(--mono); color: var(--text); }
  .nm { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .exp { padding: 0 6px; border-radius: var(--r-full); background: var(--warn-soft); color: var(--warn); }
  button { display: flex; align-items: center; gap: 9px; height: 30px; padding: 0 10px; border: 0; border-radius: var(--r-md); background: transparent; color: var(--text); font: inherit; font-size: var(--fs-body); line-height: var(--lh-body); text-align: left; cursor: pointer; white-space: nowrap; }
  button :global(svg) { color: var(--muted); flex: none; }
  button:hover:not(:disabled), button:focus-visible { background: var(--row-hover); outline: none; }
  .stop, .stop :global(svg) { color: var(--danger); }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
  .l { flex: 1; }
</style>

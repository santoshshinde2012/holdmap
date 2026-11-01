<script lang="ts">
  // Choose part of the window to snapshot or record. Drag to draw, drag inside to move, drag a
  // handle to resize; click a panel to take exactly its area. Arrow keys move the area (Shift: 10
  // px), Alt with the arrows resizes it, Enter confirms and Esc cancels. The picker leaves the
  // window before anything is captured.
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import Button from "./ui/Button.svelte";
  import { clampRegion, fromDrag, MIN_REGION, regionLabel, snapArea, type CaptureRegion } from "../lib/capture-region";

  let { purpose, confirmLabel, initial = null, even = false, onconfirm, oncancel }: {
    /** What the area is for, read out with the instructions ("snapshot", "recording"). */
    purpose: string;
    confirmLabel: string;
    /** The last area used, offered again. */
    initial?: CaptureRegion | null;
    /** Video needs even sizes. */
    even?: boolean;
    onconfirm: (region: CaptureRegion) => void;
    oncancel: () => void;
  } = $props();

  let vw = $state(typeof window === "undefined" ? 1280 : window.innerWidth);
  let vh = $state(typeof window === "undefined" ? 800 : window.innerHeight);
  const fit = (r: CaptureRegion | null) => (r ? clampRegion(r, { width: vw, height: vh }, even) : null);
  // svelte-ignore state_referenced_locally
  let region = $state<CaptureRegion | null>(fit(initial));
  let hover = $state<CaptureRegion | null>(null);
  let root = $state<HTMLDivElement>();
  type Drag = { mode: "draw" | "move" | "resize"; x: number; y: number; start: CaptureRegion | null; edges: string; moved: boolean; id: number };
  let drag: Drag | null = null;

  onMount(() => {
    // A closing dialog hands focus back to its opener after this mounts: take it a frame later.
    root?.focus();
    const frame = requestAnimationFrame(() => root?.focus());
    const resize = () => { vw = window.innerWidth; vh = window.innerHeight; if (region) region = fit(region); };
    window.addEventListener("resize", resize);
    return () => { cancelAnimationFrame(frame); window.removeEventListener("resize", resize); };
  });

  function down(e: PointerEvent, mode: Drag["mode"], edges = "") {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    drag = { mode, x: e.clientX, y: e.clientY, start: region ? { ...region } : null, edges, moved: false, id: e.pointerId };
    (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!drag) { hover = snapArea(e.clientX, e.clientY); return; }
    if (e.pointerId !== drag.id) return;
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    if (!drag.moved && Math.abs(dx) + Math.abs(dy) < 4) return;
    drag.moved = true;
    hover = null;
    if (drag.mode === "draw") { region = fromDrag(drag.x, drag.y, e.clientX, e.clientY); return; }
    const s = drag.start!;
    if (drag.mode === "move") { region = fit({ ...s, x: s.x + dx, y: s.y + dy }); return; }
    // Resize from the dragged edges; the opposite edges stay put.
    let { x, y, width: w, height: h } = s;
    if (drag.edges.includes("w")) { const nx = Math.min(x + w - MIN_REGION, x + dx); w += x - nx; x = nx; }
    if (drag.edges.includes("e")) w = Math.max(MIN_REGION, w + dx);
    if (drag.edges.includes("n")) { const ny = Math.min(y + h - MIN_REGION, y + dy); h += y - ny; y = ny; }
    if (drag.edges.includes("s")) h = Math.max(MIN_REGION, h + dy);
    region = { x, y, width: w, height: h };
  }
  function up(e: PointerEvent) {
    if (!drag || e.pointerId !== drag.id) return;
    const d = drag;
    drag = null;
    // A click (no drag) on the backdrop takes the panel under it, whole.
    if (!d.moved && d.mode === "draw") { const snap = snapArea(e.clientX, e.clientY); if (snap) region = fit(snap); return; }
    region = fit(region);
  }
  // Keys are read on the window while the picker is open, ahead of the app's own shortcuts.
  function key(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); oncancel(); return; }
    if (e.key === "Tab") return;
    if (e.key === "Enter" && region && !(e.target instanceof HTMLButtonElement)) { e.preventDefault(); e.stopPropagation(); confirm(); return; }
    const arrows: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
    const step = arrows[e.key];
    if (!step) return;
    e.preventDefault();
    e.stopPropagation();
    const n = e.shiftKey ? 10 : 1;
    const base = region ?? { x: vw / 4, y: vh / 4, width: vw / 2, height: vh / 2 };
    region = fit(e.altKey ? { ...base, width: base.width + step[0] * n, height: base.height + step[1] * n } : { ...base, x: base.x + step[0] * n, y: base.y + step[1] * n });
  }
  function confirm() {
    const r = fit(region);
    if (r) onconfirm(r);
  }
  const whole = () => { region = fit({ x: 0, y: 0, width: vw, height: vh }); };
  const HANDLES = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
  // The toolbar sits under the area, or inside its bottom edge when there's no room below.
  const bar = $derived(region ? { left: Math.min(Math.max(8, region.x + region.width - 360), vw - 368), top: region.y + region.height + 54 > vh ? Math.max(8, region.y + region.height - 50) : region.y + region.height + 8 } : null);
</script>

<svelte:window onkeydowncapture={key} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_no_noninteractive_tabindex -->
<div class="picker" bind:this={root} role="dialog" aria-modal="true" aria-label="Choose an area for the {purpose}" tabindex="-1" data-snapshot-ui
  onpointerdown={(e) => down(e, "draw")} onpointermove={move} onpointerup={up} onpointercancel={up}>
  {#if !region}<div class="veil" aria-hidden="true"></div>{/if}
  {#if hover && !region}<div class="hint-box" style="left: {hover.x}px; top: {hover.y}px; width: {hover.width}px; height: {hover.height}px" aria-hidden="true"></div>{/if}
  <p class="lead" id="region-help">
    <Icon name="fit" size={14} />
    {#if region}Drag to move, or a handle to resize. Arrows nudge, Alt with arrows resizes. Enter to confirm, Esc to cancel.
    {:else}Drag to choose an area, or click a panel to take it whole. Esc to cancel.{/if}
  </p>
  {#if region}
    <div class="box" style="left: {region.x}px; top: {region.y}px; width: {region.width}px; height: {region.height}px" onpointerdown={(e) => down(e, "move")} aria-hidden="true">
      <!-- Pointer handles; the keyboard resizes with Alt and the arrows. -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      {#each HANDLES as h (h)}<span class="handle {h}" onpointerdown={(e) => down(e, "resize", h)}></span>{/each}
      <span class="size">{regionLabel(region)}</span>
    </div>
    <div class="bar" style="left: {bar!.left}px; top: {bar!.top}px" role="toolbar" aria-label="Area" tabindex="-1" onpointerdown={(e) => e.stopPropagation()}>
      <span class="readout" aria-live="polite">{regionLabel(region)}{even ? " · even size for video" : ""}</span>
      <Button size="sm" variant="ghost" onclick={whole}>Whole window</Button>
      <Button size="sm" onclick={oncancel}>Cancel</Button>
      <Button size="sm" variant="primary" icon="check" onclick={confirm}>{confirmLabel}</Button>
    </div>
  {:else}
    <div class="bar start" role="toolbar" aria-label="Area" tabindex="-1" onpointerdown={(e) => e.stopPropagation()}>
      <Button size="sm" variant="ghost" onclick={whole}>Whole window</Button>
      <Button size="sm" onclick={oncancel}>Cancel</Button>
    </div>
  {/if}
</div>

<style>
  .picker { position: fixed; inset: 0; z-index: 2147483000; cursor: crosshair; outline: none; touch-action: none; user-select: none; }
  .veil { position: absolute; inset: 0; background: rgb(0 0 0 / 0.42); }
  .hint-box { position: absolute; pointer-events: none; outline: 2px dashed var(--accent); outline-offset: -2px; background: color-mix(in srgb, var(--accent) 10%, transparent); border-radius: var(--r-md); }
  .lead { position: absolute; top: 14px; left: 50%; transform: translateX(-50%); z-index: 2; display: inline-flex; align-items: center; gap: 8px; margin: 0; max-width: calc(100vw - 32px); padding: 8px 14px;
    border-radius: var(--r-full); background: color-mix(in srgb, var(--surface) 94%, transparent); border: 1px solid var(--border); box-shadow: var(--shadow-md);
    color: var(--text); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); pointer-events: none; }
  .lead :global(svg) { color: var(--accent); flex: none; }
  /* Everything outside the area dims; the area itself shows the window as it is. */
  .box { position: absolute; cursor: move; box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.42); outline: 1.5px solid var(--accent); outline-offset: 0; }
  .handle { position: absolute; width: 12px; height: 12px; margin: -6px 0 0 -6px; border: 2px solid var(--accent); border-radius: 3px; background: var(--surface); }
  .handle.nw { left: 0; top: 0; cursor: nwse-resize; } .handle.n { left: 50%; top: 0; cursor: ns-resize; } .handle.ne { left: 100%; top: 0; cursor: nesw-resize; }
  .handle.e { left: 100%; top: 50%; cursor: ew-resize; } .handle.se { left: 100%; top: 100%; cursor: nwse-resize; } .handle.s { left: 50%; top: 100%; cursor: ns-resize; }
  .handle.sw { left: 0; top: 100%; cursor: nesw-resize; } .handle.w { left: 0; top: 50%; cursor: ew-resize; }
  .size { position: absolute; left: 0; top: -26px; padding: 2px 7px; border-radius: var(--r-sm); background: var(--accent); color: var(--accent-fg); font-family: var(--mono); font-size: var(--fs-caption); line-height: var(--lh-caption); white-space: nowrap; }
  .bar { position: absolute; z-index: 2; display: flex; align-items: center; gap: 6px; padding: 6px; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); box-shadow: var(--shadow-md); cursor: default; }
  .bar.start { left: 50%; top: 60px; transform: translateX(-50%); }
  .readout { padding: 0 6px; color: var(--text-2); font-family: var(--mono); font-size: var(--fs-caption); font-variant-numeric: tabular-nums; white-space: nowrap; }
</style>

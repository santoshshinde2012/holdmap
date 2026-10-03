<script lang="ts">
  // Vertical resize handle for a right-hand pane: drag, or focus and use ←/→ (Shift ×4),
  // Home/End; double-click resets. Width is clamped to [min, max].
  let { width = $bindable(420), min = 340, max = 680, initial = 420, label = "Resize details pane", onchange }: { width?: number; min?: number; max?: number; initial?: number; label?: string; onchange?: (w: number) => void } = $props();
  let dragging = $state(false);
  let startX = 0, startW = 0;
  const clamp = (w: number) => Math.round(Math.min(max, Math.max(min, w)));
  function set(w: number) { width = clamp(w); onchange?.(width); }
  function down(e: PointerEvent) { dragging = true; startX = e.clientX; startW = width; (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); e.preventDefault(); }
  function move(e: PointerEvent) { if (dragging) width = clamp(startW - (e.clientX - startX)); }
  function up() { if (dragging) { dragging = false; onchange?.(width); } }
  $effect(() => { document.body.classList.toggle("pw-resizing", dragging); });
  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 64 : 16;
    if (e.key === "ArrowLeft") set(width + step);
    else if (e.key === "ArrowRight") set(width - step);
    else if (e.key === "Home") set(min);
    else if (e.key === "End") set(max);
    else return;
    e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="split"
  class:dragging
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  aria-valuenow={width}
  aria-valuemin={min}
  aria-valuemax={max}
  tabindex="0"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  ondblclick={() => set(initial)}
  onkeydown={key}
></div>

<style>
  .split { position: absolute; top: 0; bottom: 0; left: -4px; width: 8px; cursor: col-resize; z-index: 5; outline: none; touch-action: none; }
  .split::after { content: ""; position: absolute; top: 0; bottom: 0; left: 3px; width: 2px; background: transparent; transition: background var(--dur-2) var(--ease); }
  .split:hover::after, .split.dragging::after { background: var(--accent); transition-delay: 120ms; }
  .split.dragging::after { transition-delay: 0s; }
  .split:focus-visible::after { background: var(--ring); }
  :global(body.pw-resizing) { cursor: col-resize; }
  :global(body.pw-resizing *) { user-select: none !important; }
</style>

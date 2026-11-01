<script lang="ts">
  // The WebGL canvas, its orbit controls and the labels that follow it. Everything shown is
  // decided by the scene; this component turns pointer and keyboard input into camera moves and
  // reports picks. The tour drives the camera here and hands control back on any input.
  import { onMount, untrack } from "svelte";
  import { MachineStage, type StageView } from "../../../lib/machine3d/stage";
  import { readColors } from "../../../lib/machine3d/theme";
  import { labelPriority, labelSize, placeLabels as fitLabels, type LabelCandidate } from "../../../lib/machine3d/labels";
  import { LAYER_LABEL, type Layer, type MachineScene, type Part } from "../../../lib/machine3d/scene";
  import type { MachineLayout } from "../../../lib/machine3d/layout";
  import type { Pose } from "../../../lib/machine3d/camera";
  import { TOUR_RESUME_S, tourBeats, tourFrame, type TourBeat } from "../../../lib/machine3d/tour";
  import { shorten } from "../../../lib/machine-map";
  import { partChip } from "../../../lib/machine3d/describe";
  import { PART_KIND_LABEL } from "../../../lib/machine3d/scene";

  let { scene, layout, colors, view, flight, tour = false, insets = { top: 0, bottom: 0 }, onpick, onhover, onfail, oncaption, ontourfocus, onreset }: {
    scene: MachineScene;
    layout: MachineLayout;
    /** Agent colours as CSS values, shared with cards and the map. */
    colors: ReadonlyMap<string, string>;
    view: StageView;
    /** Where the camera should be; a new object starts a flight there. */
    flight: { pose: (aspect: number) => Pose; instant?: boolean } | null;
    tour?: boolean;
    /** Overlays along the top and bottom edges, and the inspector on the right, in CSS pixels: the model is framed between them. */
    insets?: { top: number; bottom: number; right?: number };
    onpick: (id: string | null, at: { x: number; y: number }) => void;
    onhover: (id: string | null) => void;
    /** WebGL could not start or was lost. */
    onfail: (reason: string) => void;
    oncaption: (text: string | null) => void;
    /** The agent the tour is showing (null for the whole machine), or undefined when the viewer drives. */
    ontourfocus?: (id: string | null | undefined) => void;
    onreset: () => void;
  } = $props();

  let host = $state<HTMLDivElement>();
  let canvas = $state<HTMLCanvasElement>();
  let stage = $state<MachineStage | null>(null);
  let width = $state(0), height = $state(0);
  const labelEls: Record<string, HTMLDivElement | undefined> = {};

  onMount(() => {
    let s: MachineStage;
    try {
      s = new MachineStage(canvas!, readColors(host!, colors));
    } catch (e) {
      onfail(e instanceof Error ? e.message : String(e));
      return;
    }
    s.onlost = () => onfail("The graphics context was lost");
    s.onrender = placeLabels;
    stage = s;
    // Theme changes (data-theme or the system setting) recolour the scene.
    const recolour = () => s.setColors(readColors(host!, colors));
    const media = matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", recolour);
    const themeWatch = new MutationObserver(recolour);
    themeWatch.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "class"] });
    const wake = () => s.wake();
    document.addEventListener("visibilitychange", wake);
    return () => {
      media.removeEventListener("change", recolour);
      themeWatch.disconnect();
      document.removeEventListener("visibilitychange", wake);
      s.dispose();
      stage = null;
    };
  });

  // Size is read on the next frame, as elsewhere in the app: resizing inside the observer
  // callback would trigger a "ResizeObserver loop" notice.
  $effect(() => {
    const el = host;
    if (!el || typeof ResizeObserver === "undefined") return;
    let frame = 0;
    const read = () => { width = el.clientWidth; height = el.clientHeight; };
    read();
    const ro = new ResizeObserver(() => { cancelAnimationFrame(frame); frame = requestAnimationFrame(read); });
    ro.observe(el);
    return () => { ro.disconnect(); cancelAnimationFrame(frame); };
  });
  $effect(() => { stage?.resize(width, height); });
  $effect(() => { stage?.setInsets(insets.top, insets.bottom, insets.right ?? 0); });
  // Poses are fitted to the free area between the overlays, then pulled back to the full canvas.
  const freeHeight = $derived(Math.max(120, height - insets.top - insets.bottom));
  const freeWidth = $derived(Math.max(160, width - (insets.right ?? 0)));
  const framed = (p: Pose): Pose => ({ ...p, distance: p.distance * (height / freeHeight) });
  $effect(() => { stage?.setScene(scene, layout); });
  // Agent colours follow the report order. Home builds a new map every scan, so recolour (and
  // rebuild the board) only when an agent's colour actually changes.
  const colorKey = $derived(JSON.stringify([...colors]));
  $effect(() => { void colorKey; untrack(() => { if (stage && host) stage.setColors(readColors(host, colors)); }); });

  // Tour: a pure function of tour time. Input pauses it; after a quiet spell the camera flies
  // back from wherever the viewer left it, and the tour carries on where it stopped.
  let pausedUntil = 0, tourTime = 0;
  /** The tour's focus while it drives the view, so labels follow it; undefined when the viewer drives. */
  let tourFocus = $state<string | null | undefined>(undefined);
  // Rebuilt with each scan (captions carry live counts), read by the running tour without restarting it.
  let beats: TourBeat[] = [];
  $effect(() => { beats = width && height ? tourBeats(scene, layout, freeWidth / freeHeight).map((b) => ({ ...b, pose: framed(b.pose) })) : []; });
  $effect(() => {
    if (!stage || !tour) { oncaption(null); tourFocus = undefined; return; }
    const s = stage;
    let raf = 0, last = 0, resumeUntil = 0, paused = true;
    const step = (now: number) => {
      raf = requestAnimationFrame(step);
      const dt = last ? Math.min(0.1, (now - last) / 1000) : 0;
      last = now;
      if (now < pausedUntil || !beats.length) { paused = true; tourFocus = undefined; return; }
      // A fresh start begins on the first beat's own view, not on the loop's way in from the last.
      if (tourTime === 0) tourTime = beats[0].fly;
      const f = tourFrame(beats, tourTime)!;
      const tourView = { ...view, explode: f.explode, xray: f.xray, focus: f.focus, selected: f.focus, highlight: null };
      if (paused) {
        // Starting, or resuming after input: fly there from the current camera first.
        paused = false;
        s.setView(tourView); s.flyTo(f.pose, 1200);
        resumeUntil = now + 1200;
      }
      if (tourFocus !== f.focus) tourFocus = f.focus;
      oncaption(f.caption);
      if (now < resumeUntil) return;
      tourTime += dt;
      s.setPose(f.pose);
      s.setView(tourView, { immediate: true });
    };
    raf = requestAnimationFrame(step);
    return () => { cancelAnimationFrame(raf); tourFocus = undefined; };
  });
  $effect(() => { if (!tour) tourTime = 0; });
  $effect(() => { if (stage && tourFocus === undefined) stage.setView(view); });
  $effect(() => { ontourfocus?.(tourFocus); });

  // Camera flights requested by the view (selection, angle, explode, Fit), and the first fit once
  // the canvas has a size. Until the viewer turns, pans or zooms, the camera also keeps that view
  // fitted while data and overlays settle (recorded calls arriving, the readings wrapping, a
  // resize); after that it stays where they put it until they ask for another view.
  const ready = $derived(!!stage && width > 0 && height > 0);
  let opened = false, moved = false;
  $effect(() => {
    const f = flight;
    if (!ready || !f || tour) return;
    moved = false;
    untrack(() => {
      const target = framed(f.pose(freeWidth / freeHeight));
      // Opening move, once the first scan is in: settle onto the machine from higher up while it
      // builds, unless motion is reduced. Until then (an instant fit) the camera just waits.
      if (!opened && !f.instant) {
        opened = true;
        if (!view.reduced) {
          stage!.setPose({ ...target, phi: Math.min(1.35, target.phi + 0.55), distance: target.distance * 1.35, theta: target.theta - 0.5 });
          stage!.flyTo(target, 1600);
          return;
        }
      }
      stage!.flyTo(target, f.instant ? 0 : 1100);
    });
  });
  const settleKey = $derived(JSON.stringify([layout.board, layout.calls, insets.top, insets.bottom, insets.right, width, height]));
  $effect(() => {
    void settleKey;
    untrack(() => {
      const f = flight;
      if (!ready || !f || f.instant || tour || moved || !opened) return;
      stage!.flyTo(framed(f.pose(freeWidth / freeHeight)), 600);
    });
  });

  /** Zoom buttons beside the canvas (the wheel and +/− keys do the same). */
  export function zoomBy(factor: number) { interact(); stage?.zoom(factor); }

  function interact() {
    moved = true;
    if (tour) { pausedUntil = performance.now() + TOUR_RESUME_S * 1000; tourFocus = undefined; oncaption(null); stage?.setView(view); }
  }

  // ---------- labels ----------
  interface Label { id: string; part: Part | null; text: string; sub: string | null; priority: number; size: { w: number; h: number }; kind: string; warn: boolean; owner: string | null; lane?: boolean }
  const labels = $derived.by<Label[]>(() => {
    const touring = tourFocus !== undefined;
    // Canvas hover shows the hover card, so the hovered part's label doesn't repeat it.
    const state = { focus: touring ? tourFocus! : view.focus, selected: touring ? tourFocus! : view.selected, hovered: null,
      hiddenLayers: view.hidden as ReadonlySet<string>, highlight: touring ? null : view.highlight };
    // The overview names things; detail lines appear once you focus, select, hover or light up a reading.
    const detailed = !!(state.focus || state.highlight);
    const out: Label[] = [];
    for (const p of scene.parts) {
      const priority = labelPriority(p, state);
      if (priority === null) continue;
      // Labels stay short; the hover card, inspector and parts list carry the full text.
      const text = shorten(p.label, 28);
      const tower = p.kind === "agent" || p.kind === "dormant";
      const full = detailed || p.id === state.selected || p.id === state.hovered;
      const sub = tower ? (p.warn ? "Needs you" : full ? shorten(p.sub, 34) : null) : full ? shorten(p.sub, 34) : null;
      out.push({ id: p.id, part: p, text, sub, priority, size: labelSize(text, sub), kind: p.kind, warn: !!p.warn, owner: p.owner });
    }
    for (const layer of ["runtime", "config", "calls"] as Layer[]) {
      if (view.hidden.has(layer) || layer === "calls" && !layout.calls) continue;
      const text = LAYER_LABEL[layer], sub = layer === "runtime" ? "Processes running now" : layer === "config" ? "Files on disk" : "Not matched to processes";
      out.push({ id: `plate:${layer}`, part: null, text, sub, priority: 85, size: labelSize(text, sub), kind: "plate", lane: layer === "calls", warn: false, owner: null });
    }
    out.push({ id: "hub", part: null, text: "This computer", sub: null, priority: 20, size: labelSize("This computer", null), kind: "hub", warn: false, owner: null });
    return out;
  });

  /** Gap between a front edge and the label hanging below it. */
  const BELOW = 4;
  function placeLabels() {
    if (!stage) return;
    const list = labels;
    const explode = stage.explodeValue;
    const anchors = stage.anchors(list.map((l) => l.id));
    const cands: LabelCandidate[] = [];
    for (const l of list) {
      const a = anchors.get(l.id);
      if (!a) continue;
      // Plate names only read once the plates are apart; the calls lane is above them either way.
      if (l.kind === "plate" && !l.lane && explode < 0.5) continue;
      // Nameplates and port numbers hang below a front edge; everything else sits above its part.
      // A nameplate that would overlap its neighbour's steps down a line, then another.
      const alts = l.kind === "agent" || l.kind === "dormant" ? [l.size.h + 6, (l.size.h + 6) * 2] : undefined;
      cands.push({ id: l.id, x: a.end ? a.x - l.size.w / 2 : a.x, y: a.below ? a.y + BELOW + l.size.h : a.y, w: l.size.w, h: l.size.h, priority: l.priority, behind: a.behind, alts });
    }
    const shown = fitLabels(cands, { width, height, top: insets.top, bottom: insets.bottom, right: insets.right ?? 0 });
    for (const l of list) {
      const el = labelEls[l.id];
      if (!el) continue;
      const a = anchors.get(l.id);
      if (!a || !shown.has(l.id)) { el.style.visibility = "hidden"; continue; }
      el.style.visibility = "visible";
      const dy = shown.get(l.id) ?? 0;
      el.style.transform = `translate(${Math.round(a.end ? a.x - l.size.w : a.x - l.size.w / 2)}px, ${Math.round((a.below ? a.y + BELOW : a.y - l.size.h) + dy)}px)`;
    }
  }
  // New label candidates need positions before the next camera move.
  $effect(() => { void labels; stage?.wake(); queueMicrotask(placeLabels); });

  // ---------- input ----------
  let drag: { x: number; y: number; px: number; py: number; pan: boolean; moved: boolean; id: number } | null = null;
  let hoverRaf = 0;
  const local = (e: { clientX: number; clientY: number }) => { const r = canvas!.getBoundingClientRect(); return { x: e.clientX - r.left, y: e.clientY - r.top }; };

  // One pointer drives the camera; a second finger or pen is ignored until the first lifts.
  function ondown(e: PointerEvent) {
    if (!stage || e.button > 2 || drag) return;
    drag = { x: e.clientX, y: e.clientY, px: e.clientX, py: e.clientY, pan: e.button === 2 || e.shiftKey, moved: false, id: e.pointerId };
    canvas!.setPointerCapture(e.pointerId);
  }
  function onmove(e: PointerEvent) {
    if (!stage) return;
    if (drag && e.pointerId !== drag.id) return;
    if (drag) {
      const dx = e.clientX - drag.px, dy = e.clientY - drag.py;
      // A drag turns the camera, not the hover card: let go of the hovered part.
      if (!drag.moved && Math.abs(e.clientX - drag.x) + Math.abs(e.clientY - drag.y) > 3) { drag.moved = true; interact(); onhover(null); }
      if (drag.moved) {
        if (drag.pan) stage.pan(dx, dy); else stage.orbit(-dx * 0.006, dy * 0.005);
      }
      drag.px = e.clientX; drag.py = e.clientY;
      return;
    }
    if (hoverRaf) return;
    const at = local(e);
    hoverRaf = requestAnimationFrame(() => { hoverRaf = 0; pointer = at; onhover(stage?.pick(at.x, at.y) ?? null); });
  }
  function onup(e: PointerEvent) {
    if (drag && e.pointerId !== drag.id) return;
    const d = drag;
    drag = null;
    if (canvas?.hasPointerCapture(e.pointerId)) canvas.releasePointerCapture(e.pointerId);
    if (!d || d.moved || !stage || e.button !== 0) return;
    const at = local(e);
    interact();
    onpick(stage.pick(at.x, at.y), at);
  }
  function onleave() { pointer = null; if (!drag) onhover(null); }

  // ---------- hover card ----------
  /** The pointer over the canvas; the card shows beside it. Pointer-only: the parts list and the
   *  inspector carry the same facts for the keyboard and screen readers. */
  let pointer = $state<{ x: number; y: number } | null>(null);
  const hovered = $derived(view.hovered && pointer && tourFocus === undefined ? scene.parts.find((p) => p.id === view.hovered) ?? null : null);
  const card = $derived(hovered ? { part: hovered, chip: partChip(hovered), x: Math.min(pointer!.x + 16, width - 250), y: Math.min(pointer!.y + 16, height - 120) } : null);
  function onwheel(e: WheelEvent) {
    if (!stage) return;
    e.preventDefault();
    interact();
    // Toward the pointer, as maps do.
    stage.zoom(Math.exp(Math.max(-60, Math.min(60, e.deltaY)) * 0.0022), local(e));
  }
  function onkey(e: KeyboardEvent) {
    if (!stage || e.target !== e.currentTarget || e.altKey || e.ctrlKey || e.metaKey) return;
    const turn = 0.12;
    const act: Record<string, () => void> = {
      ArrowLeft: () => stage!.orbit(turn, 0), ArrowRight: () => stage!.orbit(-turn, 0),
      ArrowUp: () => stage!.orbit(0, 0.08), ArrowDown: () => stage!.orbit(0, -0.08),
      "+": () => stage!.zoom(1 / 1.18), "=": () => stage!.zoom(1 / 1.18), "-": () => stage!.zoom(1.18), "0": onreset,
    };
    const fn = act[e.key];
    if (!fn) return;
    e.preventDefault();
    interact();
    fn();
  }
  // Non-passive, so the page doesn't scroll while zooming the model.
  $effect(() => {
    const c = canvas;
    if (!c) return;
    c.addEventListener("wheel", onwheel, { passive: false });
    return () => c.removeEventListener("wheel", onwheel);
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div class="stage3d" bind:this={host} role="group" tabindex="0"
  aria-label="3D machine. Drag to turn, right-drag or Shift-drag to pan, scroll to zoom. Arrow keys turn, + and − zoom, 0 resets. The parts list beside it has every part as a button."
  onkeydown={onkey}>
  <canvas bind:this={canvas} class:grab={!!view.hovered} onpointerdown={ondown} onpointermove={onmove} onpointerup={onup} onpointercancel={onup}
    onpointerleave={onleave} oncontextmenu={(e) => e.preventDefault()} aria-hidden="true"></canvas>
  {#if card}
    <div class="card" style="transform: translate({Math.max(8, card.x)}px, {Math.max(8, card.y)}px)" aria-hidden="true">
      <small>{PART_KIND_LABEL[card.part.kind]}</small>
      <b>{card.part.label}</b>
      {#if card.part.sub}<span>{card.part.sub}</span>{/if}
      <span class="how t-{card.chip.tone}"><i></i>{card.chip.label}</span>
      {#if card.part.warn}<span class="warn">{card.part.warn}</span>{/if}
    </div>
  {/if}
  <div class="labels" aria-hidden="true">
    {#each labels as l (l.id)}
      <div class="lbl k-{l.kind}" class:lane={l.lane} class:warn={l.warn} class:sel={l.id === view.selected} style="visibility: hidden; --agent: {l.owner ? colors.get(l.owner) ?? 'var(--muted)' : 'var(--muted)'}" bind:this={labelEls[l.id]}>
        <b>{#if l.kind === "agent" || l.kind === "dormant"}<i></i>{/if}{l.text}</b>{#if l.sub}<span>{l.sub}</span>{/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .stage3d { position: absolute; inset: 0; outline: none; }
  .stage3d:focus-visible { outline: 2px solid var(--ring); outline-offset: -2px; border-radius: var(--r-lg); }
  canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; cursor: grab; touch-action: none; }
  canvas:active { cursor: grabbing; }
  canvas.grab { cursor: pointer; }
  .labels { position: absolute; inset: 0; pointer-events: none; overflow: hidden; }
  .lbl { position: absolute; left: 0; top: 0; display: grid; justify-items: center; padding: 2px 7px; border-radius: var(--r-sm); background: color-mix(in srgb, var(--surface) 86%, transparent);
    border: 1px solid var(--border); box-shadow: var(--shadow-sm); white-space: nowrap; will-change: transform; }
  .lbl b { display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); color: var(--text); }
  .lbl span { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--muted); }
  .lbl i { width: 8px; height: 8px; border-radius: 50%; background: var(--agent); }
  .lbl.k-dormant i { background: transparent; border: 1.5px dashed var(--muted); }
  /* Port numbers: a light tag, so a row of them doesn't bury the towers behind. */
  .lbl.k-port { padding: 0 5px; box-shadow: none; background: color-mix(in srgb, var(--surface) 78%, transparent); }
  .lbl.k-port b { font-family: var(--mono); font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); }
  .lbl.k-plate { background: transparent; border: 0; box-shadow: none; }
  .lbl.k-plate b { color: var(--text-2); font-size: var(--fs-label); letter-spacing: var(--ls-label); text-transform: uppercase; }
  .lbl.k-hub { background: transparent; border: 0; box-shadow: none; }
  .lbl.k-hub b { color: var(--muted); font-weight: var(--fw-medium); }
  .lbl.warn { border-color: color-mix(in srgb, var(--warn) 55%, transparent); }
  .lbl.warn span { color: var(--warn); }
  .lbl.sel { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-soft), var(--shadow-sm); }
  .lbl.lane b { color: var(--accent); }
  .card { position: absolute; left: 0; top: 0; z-index: 2; display: grid; gap: 1px; width: max-content; max-width: 240px; padding: 7px 10px 8px; pointer-events: none;
    border: 1px solid var(--border-strong); border-radius: var(--r-md); background: color-mix(in srgb, var(--surface) 96%, transparent); box-shadow: var(--shadow-md); }
  .card small { font-size: var(--fs-label); line-height: var(--lh-label); letter-spacing: var(--ls-label); text-transform: uppercase; color: var(--muted); font-weight: var(--fw-semibold); }
  .card b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); color: var(--text); font-weight: var(--fw-semibold); overflow-wrap: anywhere; }
  .card span { font-size: var(--fs-caption); line-height: var(--lh-caption); color: var(--text-2); overflow-wrap: anywhere; }
  .card .how { display: inline-flex; align-items: center; gap: 5px; margin-top: 3px; color: var(--muted); }
  .card .how i { width: 6px; height: 6px; border-radius: 50%; background: var(--muted); }
  .card .t-seen i { background: var(--ok); } .card .t-inferred i { background: var(--tone-amber); } .card .t-configured i { background: var(--tone-blue); }
  .card .warn { color: var(--warn); }
</style>

<script lang="ts">
  // Home as a 3D machine you can take apart: agents stand on the board with the ports and folders
  // they use, their processes float on the runtime plate above, and configuration files on disk sit
  // on top. Pull it apart, X-ray it, pick any part, or let the tour walk through it. The figures
  // around it read the same scene that places the parts. Loaded on demand; without WebGL it shows
  // the 2D map instead.
  import { onMount, tick, untrack, type Snippet } from "svelte";
  import Icon from "../../Icon.svelte";
  import { ActionDock, ActionPill, Callout, IconButton } from "../../ui";
  import Stage3D from "./Stage3D.svelte";
  import PartsRail from "./PartsRail.svelte";
  import Readings from "./Readings.svelte";
  import PartInspector from "./PartInspector.svelte";
  import { buildScene, gaugeParts, readings, type Gauge, type Layer, type Part } from "../../../lib/machine3d/scene";
  import { layoutScene, type MachineLayout } from "../../../lib/machine3d/layout";
  import { agentPose, overviewPose, partPose, topPose } from "../../../lib/machine3d/camera";
  import { webglSupport, type StageView, type Support } from "../../../lib/machine3d/stage";
  import { prefGetBoolean, prefSet } from "../../../lib/prefs";
  import { shortcutContext } from "../../../lib/keyboard";
  import type { SkillTrustView } from "../../../lib/trust";
  import type { ActivitySnapshot, RecordingStatus } from "../../../lib/agent-flow/model";
  import type { AgentsReport, PortEntry } from "../../../lib/types";

  let { report, entries, hot, needs, colors, trust = [], activity = null, recording = null, selected, active, inventory = false, singleKeys = true, expanded = false,
    onexpand, oninventory, onselectagent, onhover, onopenagent, onport, fallback }: {
    report: AgentsReport | null;
    entries: PortEntry[] | null;
    /** Agents whose "Needs you" items are about the agent itself. */
    hot: ReadonlySet<string>;
    /** "Needs you" titles by agent. */
    needs: ReadonlyMap<string, readonly string[]>;
    /** Agent colour by id, shared with cards, map and timeline. */
    colors: ReadonlyMap<string, string>;
    trust?: readonly SkillTrustView[];
    /** Opt-in recorded calls, drawn in their own lane; never joined to processes or configuration. */
    activity?: ActivitySnapshot | null;
    recording?: RecordingStatus | null;
    /** Focus: Home hides its summary and Needs you so the machine fills the space. */
    expanded?: boolean;
    onexpand?: (expanded: boolean) => void;
    /** The app's selected agent. */
    selected: string | null;
    /** Hovered or selected agent from elsewhere on Home (Needs you); its parts stay lit. */
    active: string | null;
    inventory?: boolean;
    /** The app's single-key shortcuts setting: E, X, T and F follow it. */
    singleKeys?: boolean;
    oninventory?: () => void;
    onselectagent: (id: string | null) => void;
    onhover: (id: string | null) => void;
    onopenagent: (id: string) => void;
    onport: (entryId: string, anchor: HTMLElement) => void;
    /** The 2D map, shown when WebGL is unavailable. */
    fallback: Snippet;
  } = $props();

  // ---------- capability ----------
  let support = $state<Support | "pending">("pending");
  let forced = $state(false);
  let failure = $state<string | null>(null);
  let reduced = $state(false);
  onMount(() => {
    support = webglSupport();
    const media = matchMedia("(prefers-reduced-motion: reduce)");
    reduced = media.matches;
    const change = () => (reduced = media.matches);
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  });
  const usable = $derived(!failure && (support === "ok" || support === "slow" && forced));

  // ---------- scene ----------
  const scene = $derived(buildScene({ report, entries, hot, needs, trust, activity, recording }));
  // Layout keeps its identity across scans whose parts didn't change.
  let layoutCache: MachineLayout | null = null;
  const layout = $derived.by(() => {
    if (layoutCache?.signature !== scene.signature) layoutCache = layoutScene(scene);
    return layoutCache;
  });

  // ---------- view state ----------
  let explode = $state(prefGetBoolean("home.3d.exploded", false));
  let xray = $state(prefGetBoolean("home.3d.xray", false));
  $effect(() => prefSet("home.3d.exploded", explode ? "1" : "0"));
  $effect(() => prefSet("home.3d.xray", xray ? "1" : "0"));
  let top = $state(false);
  let tour = $state(false);
  let hidden = $state<ReadonlySet<Layer>>(new Set());
  let legend = $state(prefGetBoolean("home.3d.legend", true));
  $effect(() => prefSet("home.3d.legend", legend ? "1" : "0"));
  /** A selected part that isn't an agent, or an agent that isn't running. */
  let partId = $state<string | null>(null);
  let hoverPart = $state<string | null>(null);
  let caption = $state<string | null>(null);
  let fit = $state(0);

  const part = $derived(partId ? scene.parts.find((p) => p.id === partId) ?? null : null);
  $effect(() => { if (partId && !part) partId = null; });
  const selectedAgent = $derived(selected && scene.agents.some((a) => a.id === selected) ? selected : null);
  const focus = $derived(part ? part.owner : selectedAgent);
  const inspected = $derived(part ?? (selectedAgent ? scene.parts.find((p) => p.id === selectedAgent) ?? null : null));
  // A pressed reading lights up exactly the parts it counts (for the agent in focus, if any).
  let gauge = $state<Gauge | null>(null);
  const highlight = $derived(gauge ? gaugeParts(scene, gauge, focus) : null);
  function pickGauge(next: Gauge | null) {
    stopTour();
    gauge = next;
    // Bring back a hidden plate when its parts are the ones asked for.
    const lit = next ? gaugeParts(scene, next, focus) : null;
    if (lit) { const layers = new Set(scene.parts.filter((p) => lit.has(p.id)).map((p) => p.layer)); if ([...hidden].some((l) => layers.has(l))) hidden = new Set([...hidden].filter((l) => !layers.has(l))); }
  }
  const view = $derived<StageView>({ explode: explode ? 1 : 0, xray, focus: focus ?? active, selected: inspected?.id ?? null, hovered: hoverPart, hidden, reduced, highlight });

  // Fly only on an explicit change (selection, angle, explode, Fit, the first agent scan), so a
  // scan never pulls the camera away from where the viewer turned it. Recorded calls can arrive
  // before the agents; the first fit waits for the agents.
  const hasParts = $derived(scene.parts.length > 0);
  const scanned = $derived(!!report);
  const flight = $derived.by(() => {
    const key = [partId, selectedAgent, explode, top, fit, scanned];
    void key;
    return untrack(() => {
      const l = layout, s = scene, ex = explode ? 1 : 0, id = partId, agent = selectedAgent, isTop = top;
      const target = id ? s.parts.find((p) => p.id === id) : null;
      return { pose: (aspect: number) => isTop ? topPose(l, aspect)
        : target && target.kind !== "dormant" ? partPose(l, s, target.id, ex, aspect)
          : target ? agentPose(l, target.id, ex, aspect)
            : agent ? agentPose(l, agent, ex, aspect) : overviewPose(l, ex, aspect), instant: !scanned };
    });
  });

  // Focus changes the stage's size on purpose, so fit the machine to it once the layout settles.
  let lastExpanded = untrack(() => expanded);
  $effect(() => {
    if (expanded === lastExpanded) return;
    lastExpanded = expanded;
    const raf = requestAnimationFrame(() => requestAnimationFrame(() => { top = false; fit++; }));
    return () => cancelAnimationFrame(raf);
  });

  // ---------- readings ----------
  /** While the tour drives, the readings follow the agent it shows. */
  let tourFocus = $state<string | null | undefined>(undefined);
  const readingFocus = $derived(tourFocus !== undefined ? tourFocus : focus);
  const scope = $derived(readingFocus ? scene.agents.find((a) => a.id === readingFocus)?.label ?? "This computer" : "This computer");
  const figures = $derived(readings(scene, readingFocus, !!report));
  let cpuHistory = $state<Map<string, number[]>>(new Map());
  let lastSample = 0;
  $effect(() => {
    const r = report;
    if (!r || r.taken_at_ms === lastSample) return;
    lastSample = r.taken_at_ms;
    untrack(() => {
      const next = new Map(cpuHistory);
      const push = (id: string, v: number) => next.set(id, [...(next.get(id) ?? []), v].slice(-40));
      push("", r.agents.reduce((n, a) => n + a.cpu_percent, 0));
      for (const a of r.agents) push(a.id, a.cpu_percent);
      cpuHistory = next;
    });
  });

  // ---------- actions ----------
  function stopTour() { if (tour) { tour = false; fit++; } }
  function pickAgent(id: string) {
    stopTour();
    const agent = scene.agents.find((a) => a.id === id);
    if (!agent) return;
    if (agent.kind === "dormant") { partId = partId === id ? null : id; onselectagent(null); return; }
    partId = null;
    onselectagent(selectedAgent === id ? null : id);
  }
  function pickPart(p: Part, anchor: HTMLElement) {
    stopTour();
    if (p.kind === "agent" || p.kind === "dormant") { pickAgent(p.id); return; }
    partId = p.id;
    if (selectedAgent !== p.owner && scene.agents.find((a) => a.id === p.owner)?.kind === "agent") onselectagent(p.owner);
    if (p.target?.view === "list") onport(p.target.entryId, anchor);
  }
  let pointAnchor = $state<HTMLSpanElement>();
  let anchorAt = $state({ x: 0, y: 0 });
  function onpick(id: string | null, at: { x: number; y: number }) {
    if (!id) { if (partId || selectedAgent) { partId = null; onselectagent(null); } return; }
    const p = scene.parts.find((x) => x.id === id);
    if (!p) return;
    // Like the map: a selected agent opens on Agents with a second click.
    if (p.kind === "agent" && selectedAgent === p.id && !partId) { onopenagent(p.id); return; }
    // The port menu anchors to where the part was clicked, once the anchor has moved there.
    anchorAt = at;
    void tick().then(() => pickPart(p, pointAnchor!));
  }
  function clear() { partId = null; onselectagent(null); }
  function toggleLayer(layer: Layer) {
    const next = new Set(hidden);
    if (!next.delete(layer)) next.add(layer);
    hidden = next;
  }
  // The same policy as the app's shortcuts: no keys while composing or editing; letters are
  // single-key shortcuts, so they follow that setting and leave focused native controls alone.
  // Listened for on the window, in the capture phase: WebKit doesn't focus a clicked button, so
  // focus often stays on the page, and Esc must unwind the tour, a part and a reading before
  // Home's own Esc clears the agent. Dialogs and menus keep their keys.
  function onkey(e: KeyboardEvent) {
    if (!usable) return;
    const target = e.target instanceof Element ? e.target : null;
    if (target?.closest("[role=dialog], [role=alertdialog], [role=menu]") || document.querySelector("[role=dialog][aria-modal=true], [role=alertdialog]")) return;
    const c = shortcutContext(e, document.activeElement);
    if (c.blocked || c.editing) return;
    if (e.key === "Escape" && c.navigation && (partId || tour || gauge)) { e.stopPropagation(); if (tour) stopTour(); else if (partId) partId = null; else gauge = null; return; }
    if (!singleKeys || !c.navigation || c.control) return;
    const act: Record<string, () => void> = { e: () => { stopTour(); explode = !explode; }, x: () => { stopTour(); xray = !xray; }, t: () => { if (!reduced && hasParts) tour = !tour; }, f: () => { stopTour(); top = false; fit++; } };
    const fn = act[e.key];
    if (!fn) return;
    e.preventDefault();
    e.stopPropagation();
    fn();
  }
  // Overlays along the top and bottom of the canvas: the camera frames the model between them.
  let viewport = $state<HTMLDivElement>();
  let stage3d = $state<ReturnType<typeof Stage3D>>();
  let insets = $state({ top: 0, bottom: 0 });
  $effect(() => {
    const vp = viewport;
    if (!vp || typeof ResizeObserver === "undefined") return;
    const measure = () => {
      const box = vp.getBoundingClientRect();
      let top = 0, bottom = 0;
      for (const el of vp.querySelectorAll<HTMLElement>("[data-inset]")) {
        const r = el.getBoundingClientRect();
        if (!r.height) continue;
        if (r.top + r.height / 2 < box.top + box.height / 2) top = Math.max(top, r.bottom - box.top + 4);
        else bottom = Math.max(bottom, box.bottom - r.top + 4);
      }
      // Untracked: a new measurement must not re-subscribe this effect.
      untrack(() => { if (top !== insets.top || bottom !== insets.bottom) insets = { top, bottom }; });
    };
    // Measure on the next frame, not inside the observer callback: the update moves the camera's
    // frame and WebKit reports a resize loop if that happens in the same layout pass.
    let raf = 0;
    const ro = new ResizeObserver(() => { cancelAnimationFrame(raf); raf = requestAnimationFrame(measure); });
    ro.observe(vp);
    for (const el of vp.querySelectorAll("[data-inset]")) ro.observe(el);
    measure();
    return () => { cancelAnimationFrame(raf); ro.disconnect(); };
  });
  const MODES = [{ value: "assembled" as const, label: "Assembled", icon: "box", title: "Assembled · E" }, { value: "exploded" as const, label: "Exploded", icon: "layers", title: "Pulled apart · E" }];
</script>

<svelte:window onkeydowncapture={onkey} />

<div class="m3d">
  {#if support === "pending"}
    <p class="wait" aria-busy="true">Starting 3D…</p>
  {:else if !usable}
    <div class="fallback">
      <div class="note">
        <Callout tone="info" size="sm" title="3D isn't available in this window">
          {#if failure}The graphics context stopped ({failure}). Showing the Map instead.
          {:else if support === "slow"}Only software rendering is available here, so 3D would be slow. Showing the Map instead.
          {:else}WebGL 2 is turned off or unsupported here. Showing the Map instead.{/if}
          {#snippet actions()}
            {#if support === "slow" || failure}<button type="button" class="link" onclick={() => { failure = null; forced = true; }}>Use 3D anyway</button>{/if}
          {/snippet}
        </Callout>
      </div>
      <div class="fallback-map">{@render fallback()}</div>
    </div>
  {:else}
    <PartsRail {scene} {colors} {focus} selected={inspected?.id ?? null} onagent={pickAgent} onpart={pickPart} onhover={onhover} />
    <div class="viewport" class:xray bind:this={viewport}>
      <Stage3D bind:this={stage3d} {scene} {layout} {colors} {view} {flight} {tour} {insets} onpick={onpick} onhover={(id) => (hoverPart = id)} onfail={(reason) => (failure = reason)}
        oncaption={(c) => (caption = c)} ontourfocus={(id) => (tourFocus = id)} onreset={() => { stopTour(); top = false; clear(); fit++; }} />
      <span class="anchor" bind:this={pointAnchor} style="left: {anchorAt.x}px; top: {anchorAt.y}px" aria-hidden="true"></span>
      <div class="hud top" data-inset>
        <Readings readings={figures} {scope} cpu={cpuHistory.get(readingFocus ?? "") ?? []} active={gauge} onpick={pickGauge} />
      </div>
      <div class="hud bottom-left" data-inset>
        {#if caption}
          <p class="caption" aria-live="polite"><Icon name="play" size={11} />{caption}</p>
        {:else if legend}
          <ul class="legend" id="m3d-legend" aria-label="Legend">
            <li><i class="solid"></i>Seen now</li><li><i class="faded"></i>Inferred</li><li><i class="blue"></i>File on disk</li>
            <li><i class="beam"></i>MCP running</li><li><i class="amber"></i>Needs you</li><li><i class="dots"></i>Open connections</li>
            {#if scene.calls}<li><i class="call"></i>Reported call</li>{/if}
          </ul>
        {/if}
        <div class="bar" role="group" aria-label="Layers">
          <ActionPill label="Processes" toggle pressed={!hidden.has("runtime")} size="compact" title="Show or hide the runtime plate" onclick={() => toggleLayer("runtime")} />
          <ActionPill label="Configuration" toggle pressed={!hidden.has("config")} size="compact" title="Show or hide configuration files on disk" onclick={() => toggleLayer("config")} />
          {#if scene.calls}<ActionPill label="Calls" toggle pressed={!hidden.has("calls")} size="compact" title="Show or hide recorded calls" onclick={() => toggleLayer("calls")} />{/if}
          <ActionPill icon="info" label="Legend" toggle pressed={legend} size="compact" expanded={legend} onclick={() => (legend = !legend)} />
          {#if !inventory && oninventory && !scene.inspected}<button type="button" class="link" onclick={oninventory}><Icon name="plug" size={12} />Read MCP config</button>{/if}
        </div>
      </div>
      <div class="hud bottom-right bar" role="toolbar" aria-label="3D view" data-inset>
        <ActionDock value={explode ? "exploded" : "assembled"} options={MODES} label="Assembly" hinge={false} animate={false} size="compact" onchange={(v) => { stopTour(); explode = v === "exploded"; }} />
        <ActionPill icon="radar" label="X-ray" toggle pressed={xray} size="compact" kbd="X" title="See through solid parts; configuration and running beams stand out" onclick={() => { stopTour(); xray = !xray; }} />
        <ActionPill icon="layers" label="Top" toggle pressed={top} size="compact" title="Look straight down: the same picture as the Map" onclick={() => { stopTour(); top = !top; }} />
        <ActionPill icon="play" label="Tour" toggle pressed={tour} size="compact" kbd="T" disabled={reduced || !hasParts} title={reduced ? "The tour is off while reduced motion is on" : "Walk through the machine; any input hands you control"} onclick={() => (tour = !tour)} />
        {#if onexpand}<ActionPill icon="sidebar" label="Focus" toggle pressed={expanded} size="compact" title={expanded ? "Show the summary and Needs you again" : "Hide the summary and Needs you so the machine fills Home"} onclick={() => onexpand?.(!expanded)} />{/if}
      </div>
      <div class="hud zoom" role="toolbar" aria-label="Zoom" aria-orientation="vertical">
        <IconButton icon="plus" label="Zoom in" kbd="+" size="sm" onclick={() => stage3d?.zoomBy(1 / 1.25)} />
        <IconButton icon="minus" label="Zoom out" kbd="−" size="sm" onclick={() => stage3d?.zoomBy(1.25)} />
        <IconButton icon="fit" label="Fit" kbd="F" size="sm" onclick={() => { stopTour(); top = false; fit++; }} />
      </div>
      {#if inspected}
        <div class="hud side" style="top: {insets.top + 6}px; bottom: {insets.bottom + 6}px">
          <PartInspector part={inspected} onclose={clear} {onopenagent} {onport} />
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .m3d { position: relative; display: grid; grid-template-columns: 216px minmax(0, 1fr); height: 100%; min-height: 0; container-type: inline-size; }
  .wait { margin: 0; padding: var(--sp-4); color: var(--muted); grid-column: 1 / -1; }
  .fallback { grid-column: 1 / -1; display: grid; grid-template-rows: auto minmax(0, 1fr); min-height: 0; height: 100%; }
  .note { padding: var(--sp-2) var(--sp-2) 0; }
  .fallback-map { min-height: 0; padding: var(--sp-2); }
  .viewport { position: relative; min-width: 0; min-height: 0; overflow: hidden;
    background: radial-gradient(120% 90% at 50% 38%, var(--surface) 0%, var(--surface-2) 70%, var(--surface-3) 100%); }
  :global([data-theme="dark"]) .viewport { background: radial-gradient(120% 90% at 50% 38%, var(--surface-2) 0%, var(--surface) 62%, var(--bg) 100%); }
  @media (prefers-color-scheme: dark) { :global(:root:not([data-theme="light"])) .viewport { background: radial-gradient(120% 90% at 50% 38%, var(--surface-2) 0%, var(--surface) 62%, var(--bg) 100%); } }
  .anchor { position: absolute; width: 1px; height: 1px; pointer-events: none; }
  .hud { position: absolute; pointer-events: none; z-index: 1; }
  .hud > :global(*) { pointer-events: auto; }
  .top { top: 10px; left: 10px; right: 10px; }
  .bottom-left { left: 10px; bottom: 10px; display: grid; gap: 6px; justify-items: start; max-width: calc(100% - 470px); }
  .bottom-right { right: 10px; bottom: 10px; }
  .side { right: 52px; display: flex; align-items: flex-end; }
  /* Zoom sits on the right edge, clear of the readings above and the controls below. */
  .zoom { right: 10px; top: 50%; transform: translateY(-50%); display: grid; gap: 2px; padding: 3px; border: 1px solid var(--border); border-radius: var(--r-lg);
    background: color-mix(in srgb, var(--surface) 90%, transparent); box-shadow: var(--shadow-sm); }
  .bar { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; padding: 3px; border: 1px solid var(--border); border-radius: var(--r-lg); background: color-mix(in srgb, var(--surface) 90%, transparent); box-shadow: var(--shadow-sm); }
  .bottom-right.bar { flex-wrap: nowrap; }
  .legend { list-style: none; margin: 0; padding: 4px 8px; display: flex; flex-wrap: wrap; gap: 4px 12px; border-radius: var(--r-md); background: color-mix(in srgb, var(--surface) 80%, transparent); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .legend li { display: inline-flex; align-items: center; gap: 5px; }
  .legend i { width: 10px; height: 10px; border-radius: 2px; border: 1.5px solid var(--text-2); }
  .legend .solid { background: var(--text-2); }
  .legend .faded { background: color-mix(in srgb, var(--text-2) 35%, transparent); }
  .legend .blue { border-color: var(--tone-blue); background: var(--tone-blue-bg); }
  .legend .beam { width: 4px; border: 0; border-radius: 2px; background: var(--ok); height: 12px; }
  .legend .amber { border-color: var(--warn); background: var(--warn-soft); border-radius: 50%; }
  .legend .dots { border: 0; width: 14px; background: radial-gradient(circle, var(--text-2) 1.5px, transparent 2px) 0 50% / 5px 5px repeat-x; }
  .legend .call { width: 14px; height: 7px; border-radius: 4px; border: 0; background: color-mix(in srgb, var(--accent) 75%, transparent); }
  .caption { margin: 0; display: inline-flex; align-items: center; gap: 6px; max-width: 100%; padding: 6px 10px; border-radius: var(--r-md); background: color-mix(in srgb, var(--surface) 92%, transparent); border: 1px solid var(--border); box-shadow: var(--shadow-sm);
    color: var(--text); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .caption :global(svg) { color: var(--accent); flex: none; }
  .link { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 8px; border: 0; border-radius: var(--r-md); background: transparent; color: var(--accent); font: inherit; font-size: var(--fs-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .link:hover { background: var(--accent-softer); }
  .link:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  /* Narrow: the view controls sit above the layer toggles, and the parts list narrows. */
  @container (max-width: 900px) {
    .m3d { grid-template-columns: 180px minmax(0, 1fr); }
    .bottom-left { max-width: calc(100% - 20px); bottom: 52px; }
    .bottom-right { left: 10px; right: auto; }
  }
</style>

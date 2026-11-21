<script lang="ts">
  // Home as a 3D machine you can take apart: agents stand on the board with the ports and folders
  // they use, their processes float on the runtime plate above, and configuration files on disk sit
  // on top. Pull it apart, X-ray it, pick any part, or let the tour walk through it. The figures
  // around it read the same scene that places the parts. Loaded on demand; without WebGL it shows
  // the 2D map instead.
  import { onMount, tick, untrack, type Snippet } from "svelte";
  import ConsentGate from "../../ui/ConsentGate.svelte";
  import Icon from "../../Icon.svelte";
  import { ActionDock, ActionPill, Callout, IconButton } from "../../ui";
  import Stage3D from "./Stage3D.svelte";
  import PartsRail from "./PartsRail.svelte";
  import Readings from "./Readings.svelte";
  import PartInspector from "./PartInspector.svelte";
  import { buildScene, agentSummary, gaugeParts, LENS_LABEL, LENS_TITLE, LENSES, lensView, readings, type Gauge, type Lens, type Layer, type Part } from "../../../lib/machine3d/scene";
  import { componentScene } from "../../../lib/machine3d/preview";
  import { layoutScene, type MachineLayout } from "../../../lib/machine3d/layout";
  import { agentPose, overviewPose, topPose } from "../../../lib/machine3d/camera";
  import { webglSupport, type StageView, type Support } from "../../../lib/machine3d/stage";
  import { partJump, type AgentJump } from "../../../lib/agent-jump";
  import { prefGetBoolean, prefSet } from "../../../lib/prefs";
  import { shortcutContext } from "../../../lib/keyboard";
  import type { SkillTrustView } from "../../../lib/trust";
  import type { ActivitySnapshot, RecordingStatus } from "../../../lib/agent-flow/model";
  import type { AgentsReport, PortEntry } from "../../../lib/types";

  let { report, entries, hot, needs, colors, trust = [], activity = null, recording = null, selected, active, inventory = false, singleKeys = true, expanded = false,
    onexpand, oninventory, onselectagent, onhover, onopenagent, onport, ondetails, fallback }: {
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
    onopenagent: (id: string, jump?: AgentJump, capabilityId?: string) => void;
    onport: (entryId: string, anchor: HTMLElement) => void;
    /** Home shows the readings and the selected part in its side column, above Needs you; the
     *  view hands it that content as a snippet (null while there's nothing to show). */
    ondetails?: (details: Snippet | null) => void;
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

  // ---------- view state ----------
  // Pulled apart reads best: every floor clear of the one below. Assembled is the compact view.
  let explode = $state(prefGetBoolean("home.3d.exploded", true));
  let xray = $state(prefGetBoolean("home.3d.xray", false));
  $effect(() => prefSet("home.3d.exploded", explode ? "1" : "0"));
  $effect(() => prefSet("home.3d.xray", xray ? "1" : "0"));
  let top = $state(false);
  let tour = $state(false);
  // One concern at a time: a lens lights its parts and sets aside the floors in its way.
  let lens = $state<Lens>("all");
  const lensed = $derived(lensView(scene, lens));
  // A typed find lights matching parts and brings their floors back, so a search isn't hidden by a lens.
  let find = $state("");
  const found = $derived.by(() => {
    const q = find.trim().toLowerCase();
    if (!q) return null;
    return new Set(scene.parts.filter((p) => `${p.label} ${p.sub ?? ""}`.toLowerCase().includes(q)).map((p) => p.id));
  });
  const hidden = $derived<ReadonlySet<Layer>>(found ? new Set() : lensed.hidden);
  const LENS_ICON: Record<Lens, string> = { all: "layers", network: "globe", runtime: "cpu", config: "plug", needs: "alert", calls: "activity" };
  const lensOptions = $derived(LENSES.filter((l) => l !== "calls" || scene.calls).map((l) => ({ value: l, label: LENS_LABEL[l], icon: LENS_ICON[l], title: `${LENS_TITLE[l]} · L` })));
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
  const highlight = $derived(found ?? (gauge ? gaugeParts(scene, gauge, focus) : lensed.lit));
  /** One agent, drawn the way the whole machine is: its district, floors and parts, and nothing else. */
  const agentView = $derived(focus ? componentScene(scene, focus) : null);
  const stageScene = $derived(agentView?.scene ?? scene);
  let layoutCache: MachineLayout | null = null;
  const layout = $derived.by(() => {
    if (layoutCache?.signature !== stageScene.signature) layoutCache = layoutScene(stageScene);
    return layoutCache;
  });
  const agentName = $derived(agentView?.focus ? scene.agents.find((a) => a.id === agentView.focus)?.label ?? "" : "");
  // Recorded calls belong to the whole machine, so an agent's screen doesn't offer that lens.
  const lenses = $derived(lensOptions.filter((l) => l.value !== "calls" || !agentView));
  $effect(() => { if (lens === "calls" && (!scene.calls || agentView)) lens = "all"; });
  function pickGauge(next: Gauge | null) {
    stopTour();
    gauge = next;
    // A lens that sets aside the floor those parts stand on steps back to All.
    const lit = next ? gaugeParts(scene, next, focus) : null;
    if (lit && scene.parts.some((p) => lit.has(p.id) && hidden.has(p.layer))) lens = "all";
  }
  function pickLens(next: Lens) { stopTour(); gauge = null; lens = next; }
  const view = $derived<StageView>({ explode: explode ? 1 : 0, xray, focus: focus ?? active, selected: inspected?.id ?? null, hovered: hoverPart, hidden, reduced, highlight });

  // Fly only on an explicit change (selection, angle, explode, Fit, the first agent scan), so a
  // scan never pulls the camera away from where the viewer turned it. Recorded calls can arrive
  // before the agents; the first fit waits for the agents.
  const hasParts = $derived(scene.parts.length > 0);
  const scanned = $derived(!!report);
  const flight = $derived.by(() => {
    // Reading the layout here keeps the camera on the picture that is actually drawn.
    const l = layout;
    const key = [agentView?.focus ?? "", explode, top, fit, scanned, l.signature];
    void key;
    return untrack(() => {
      const ex = explode ? 1 : 0, agent = agentView?.focus ?? null, isTop = top;
      return { pose: (aspect: number) => isTop ? topPose(l, aspect) : agent ? agentPose(l, agent, ex, aspect) : overviewPose(l, ex, aspect), instant: !scanned };
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
  function go(p: Part) {
    // A recorded call isn't an agent's part, so it opens the calls timeline without selecting one.
    const jump = partJump(p.kind, p.variant);
    if (p.capabilityId) onopenagent(p.kind === "call" ? "" : p.owner, jump, p.capabilityId);
    else onopenagent(p.kind === "call" ? "" : p.owner, jump);
  }
  function pickAgent(id: string) {
    stopTour();
    const agent = scene.agents.find((a) => a.id === id);
    if (!agent) return;
    if (agent.kind === "dormant") {
      if (partId === id) { go(agent); return; }
      partId = id; onselectagent(null); return;
    }
    // First click selects. The next click on the same agent opens its overview.
    if (selectedAgent === id && !partId) { go(agent); return; }
    partId = null;
    onselectagent(id);
  }
  function pickPart(p: Part, anchor: HTMLElement) {
    stopTour();
    if (p.kind === "agent" || p.kind === "dormant") { pickAgent(p.id); return; }
    if (partId === p.id) { go(p); return; }
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
    // A second click opens the screen this part belongs to: overview, MCP & skills, flow, or calls.
    if ((p.kind === "agent" && selectedAgent === p.id && !partId) || (p.kind !== "agent" && partId === p.id)) { go(p); return; }
    // The port menu anchors to where the part was clicked, once the anchor has moved there.
    anchorAt = at;
    void tick().then(() => pickPart(p, pointAnchor!));
  }
  function clear() { partId = null; onselectagent(null); }
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
    const nextLens = () => { const all = lenses.map((o) => o.value); pickLens(all[(all.indexOf(lens) + 1) % all.length]); };
    const act: Record<string, () => void> = { e: () => { stopTour(); explode = !explode; }, x: () => { stopTour(); xray = !xray }, t: () => { if (!reduced && hasParts) tour = !tour; }, f: () => { stopTour(); top = false; fit++; }, l: nextLens };
    const fn = act[e.key];
    if (!fn) return;
    e.preventDefault();
    e.stopPropagation();
    fn();
  }
  // Overlays along the top and bottom of the canvas: the camera frames the model between them.
  let viewport = $state<HTMLDivElement>();
  let stage3d = $state<ReturnType<typeof Stage3D>>();
  let insets = $state({ top: 0, bottom: 0, right: 0 });
  $effect(() => {
    const vp = viewport;
    if (!vp || typeof ResizeObserver === "undefined") return;
    const measure = () => {
      const box = vp.getBoundingClientRect();
      let top = 0, bottom = 0, right = 0;
      for (const el of vp.querySelectorAll<HTMLElement>("[data-inset]")) {
        const r = el.getBoundingClientRect();
        if (!r.height) continue;
        // The inspector takes the right side: the picture centres in what's left of it.
        if (el.dataset.inset === "right") right = Math.max(right, box.right - r.left + 8);
        else if (r.top + r.height / 2 < box.top + box.height / 2) top = Math.max(top, r.bottom - box.top + 4);
        else bottom = Math.max(bottom, box.bottom - r.top + 4);
      }
      // Untracked: a new measurement must not re-subscribe this effect.
      untrack(() => { if (top !== insets.top || bottom !== insets.bottom || right !== insets.right) insets = { top, bottom, right }; });
    };
    // Measure on the next frame, not inside the observer callback: the update moves the camera's
    // frame and WebKit reports a resize loop if that happens in the same layout pass.
    let raf = 0;
    const ro = new ResizeObserver(() => { cancelAnimationFrame(raf); raf = requestAnimationFrame(measure); });
    ro.observe(vp);
    const watch = () => { for (const el of vp.querySelectorAll("[data-inset]")) ro.observe(el); };
    watch();
    // The inspector comes and goes: watch new overlays as they appear.
    const mo = new MutationObserver(() => { watch(); cancelAnimationFrame(raf); raf = requestAnimationFrame(measure); });
    mo.observe(vp, { childList: true, subtree: true });
    measure();
    return () => { cancelAnimationFrame(raf); ro.disconnect(); mo.disconnect(); };
  });
  // The parts list folds to a strip of agent buttons; remembered.
  let railOpen = $state(prefGetBoolean("home.3d.rail", true));
  $effect(() => prefSet("home.3d.rail", railOpen ? "1" : "0"));
  // One details surface. Home's side column holds it when that column is open. Otherwise the same
  // dossier sits beside the picture, and folds away without leaving the agent.
  const sideDetails = $derived(!!ondetails && !expanded);
  let dossierOpen = $state(prefGetBoolean("home.3d.dossier", true));
  $effect(() => prefSet("home.3d.dossier", dossierOpen ? "1" : "0"));
  const ownDossier = $derived(!!inspected && !sideDetails);
  $effect(() => {
    const host = ondetails;
    if (!host) return;
    host(usable ? details : null);
    return () => host(null);
  });
  const MODES = [{ value: "assembled" as const, label: "Assembled", icon: "box", title: "Assembled · E" }, { value: "exploded" as const, label: "Exploded", icon: "layers", title: "Pulled apart · E" }];
</script>

<svelte:window onkeydowncapture={onkey} />

{#snippet details()}
  <!-- The doors come first: the readings are long, and the way out of the picture has to be on screen. -->
  {#if inspected}
    <PartInspector flat part={inspected} onclose={clear} {onopenagent} {onport}>
      <Readings flat titled={inspected.id !== focus} readings={figures} {scope} cpu={cpuHistory.get(readingFocus ?? "") ?? []} active={gauge} onpick={pickGauge} />
    </PartInspector>
  {:else}
    <Readings flat readings={figures} {scope} cpu={cpuHistory.get(readingFocus ?? "") ?? []} active={gauge} onpick={pickGauge} />
  {/if}
{/snippet}

<div class="m3d" class:folded={!railOpen} class:open={ownDossier && dossierOpen} class:peek={ownDossier && !dossierOpen}>
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
    <PartsRail {scene} {colors} {focus} query={find} onquery={(q) => { find = q; if (q.trim()) gauge = null; }} selected={inspected?.id ?? null} collapsed={!railOpen} ontoggle={() => (railOpen = !railOpen)} onagent={pickAgent} onpart={pickPart} onhover={onhover} />
    <div class="viewport" class:xray bind:this={viewport} data-capture-area>
      <Stage3D bind:this={stage3d} scene={stageScene} {layout} {colors} {view} {flight} {tour} {insets} onpick={onpick} onhover={(id) => (hoverPart = id)} onfail={(reason) => (failure = reason)}
        oncaption={(c) => (caption = c)} ontourfocus={(id) => (tourFocus = id)} onreset={() => { stopTour(); top = false; clear(); fit++; }} />
      <span class="anchor" bind:this={pointAnchor} style="left: {anchorAt.x}px; top: {anchorAt.y}px" aria-hidden="true"></span>
      {#if agentView || !sideDetails || (sideDetails && !inventory && oninventory && !scene.inspected)}
        <div class="hud top" data-inset>
          {#if agentView}
            <div class="screen">
              <button type="button" class="back" onclick={() => { stopTour(); clear(); }}><Icon name="arrow-left" size={14} />All agents</button>
              <span class="who"><i style="background: {colors.get(agentView.focus!) ?? 'var(--muted)'}" aria-hidden="true"></i><span><b>{agentName}</b> <small>{agentSummary(scene, agentView.focus!)}</small></span></span>
            </div>
          {/if}
          {#if sideDetails && !inventory && oninventory && !scene.inspected}
            <div class="ask"><ConsentGate onconsent={oninventory} /></div>
          {/if}
          {#if !sideDetails && !inspected}
            <Readings readings={figures} {scope} cpu={cpuHistory.get(readingFocus ?? "") ?? []} active={gauge} onpick={pickGauge} />
          {/if}
        </div>
      {/if}
      <!-- One overlay along the bottom: the legend (or tour caption) across the width, then the
           layer bar and the view controls, which wrap onto two rows when space runs out. -->
      <div class="hud bottom" data-inset>
        {#if caption}
          <p class="caption" aria-live="polite"><Icon name="play" size={11} />{caption}</p>
        {:else if legend}
          <ul class="legend" id="m3d-legend" aria-label="Legend">
            <li><i class="solid"></i>Seen now</li><li><i class="faded"></i>Inferred</li><li><i class="blue"></i>On disk</li>
            <li><i class="gauge"></i>CPU</li><li><i class="beam"></i>MCP running</li><li><i class="flow"></i>Connection</li>
            <li><i class="ripple"></i>Open to network</li><li><i class="pin"></i>Needs you</li>
            {#if stageScene.calls}<li><i class="call"></i>Reported call</li>{/if}
          </ul>
        {/if}
        <div class="row">
        <div class="bar lenses" role="group" aria-label="View">
          <ActionDock value={lens} options={lenses} label="Lens" hinge={false} animate={false} size="compact" onchange={pickLens} />
          <ActionPill icon="info" label="Legend" toggle pressed={legend} size="compact" expanded={legend} onclick={() => (legend = !legend)} />
          {#if !sideDetails && !inventory && oninventory && !scene.inspected}<ConsentGate onconsent={oninventory} compact />{/if}
        </div>
        <div class="bar tools" role="toolbar" aria-label="3D view">
        <ActionDock value={explode ? "exploded" : "assembled"} options={MODES} label="Assembly" hinge={false} animate={false} size="compact" onchange={(v) => { stopTour(); explode = v === "exploded"; }} />
        <ActionPill icon="radar" label="X-ray" toggle pressed={xray} size="compact" kbd="X" title="See through solid parts; configuration and running beams stand out" onclick={() => { stopTour(); xray = !xray; }} />
        <ActionPill icon="hash" label="Plan" toggle pressed={top} size="compact" title="Look straight down: the floor plan, one district per agent" onclick={() => { stopTour(); top = !top; }} />
        <ActionPill icon="play" label="Tour" toggle pressed={tour} size="compact" kbd="T" disabled={reduced || !hasParts} title={reduced ? "The tour is off while reduced motion is on" : "Walk through the machine; any input hands you control"} onclick={() => (tour = !tour)} />
        {#if onexpand}<ActionPill icon="sidebar" label="Focus" toggle pressed={expanded} size="compact" title={expanded ? "Show the summary and Needs you again" : "Hide the summary and Needs you so the machine fills Home"} onclick={() => onexpand?.(!expanded)} />{/if}
        </div>
        </div>
      </div>
      <div class="hud zoom" role="toolbar" aria-label="Zoom" aria-orientation="vertical">
        <IconButton icon="plus" label="Zoom in" kbd="+" size="sm" onclick={() => stage3d?.zoomBy(1 / 1.25)} />
        <IconButton icon="minus" label="Zoom out" kbd="−" size="sm" onclick={() => stage3d?.zoomBy(1.25)} />
        <IconButton icon="fit" label="Fit" kbd="F" size="sm" onclick={() => { stopTour(); top = false; fit++; }} />
      </div>
    </div>
    {#if inspected && ownDossier && dossierOpen}
      <aside class="dossier" aria-label="Details">
        <div class="dhead">
          <button type="button" class="fold" aria-expanded="true" aria-controls="m3d-dossier" title="Fold details" onclick={() => (dossierOpen = false)}>
            <Icon name="chevron-down" size={14} /><span>Details</span>
          </button>
        </div>
        <div class="dbody" id="m3d-dossier">
          <PartInspector flat part={inspected} onclose={clear} {onopenagent} {onport}>
            <Readings flat titled={inspected.id !== focus} readings={figures} {scope} cpu={cpuHistory.get(readingFocus ?? "") ?? []} active={gauge} onpick={pickGauge} />
          </PartInspector>
        </div>
      </aside>
    {:else if ownDossier}
      <div class="peek-col">
        <button type="button" class="fold" aria-expanded="false" title="Show details" onclick={() => (dossierOpen = true)}>
          <Icon name="chevron-down" size={14} /><span class="sr-only">Show details</span>
        </button>
      </div>
    {/if}
  {/if}
</div>

<style>
  .m3d { position: relative; display: grid; grid-template-columns: 216px minmax(0, 1fr); height: 100%; min-height: 0; container-type: inline-size; }
  .m3d.open { grid-template-columns: 216px minmax(0, 1fr) 300px; }
  .m3d.peek { grid-template-columns: 216px minmax(0, 1fr) 44px; }
  .m3d.folded { grid-template-columns: 44px minmax(0, 1fr); }
  .m3d.folded.open { grid-template-columns: 44px minmax(0, 1fr) 300px; }
  .m3d.folded.peek { grid-template-columns: 44px minmax(0, 1fr) 44px; }
  /* The dossier sits beside the picture and scrolls on its own. Folding it keeps the agent. */
  .dossier { min-width: 0; min-height: 0; display: flex; flex-direction: column; border-left: 1px solid var(--border); background: var(--surface); }
  .dhead { display: flex; align-items: center; padding: 6px 8px 0; flex: none; }
  .dbody { min-height: 0; overflow: auto; }
  .dossier :global(header) { position: sticky; top: 0; z-index: 1; margin: 0 -10px; padding: 0 10px 6px; background: var(--surface); }
  .fold { display: inline-flex; align-items: center; gap: 4px; min-height: 28px; padding: 0 8px; border: 0; border-radius: var(--r-md); background: transparent; color: var(--muted); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); cursor: pointer; }
  .fold:hover { background: var(--row-hover); color: var(--text); }
  .fold:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .fold :global(svg) { transition: transform 160ms ease; }
  .peek-col .fold :global(svg) { transform: rotate(-90deg); }
  @media (prefers-reduced-motion: reduce) { .fold :global(svg) { transition: none; } }
  .peek-col { display: flex; justify-content: center; padding-top: 8px; border-left: 1px solid var(--border); background: var(--surface); }
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
  .top { top: 10px; left: 10px; right: 10px; display: grid; gap: 6px; justify-items: start; min-width: 0; }
  .top :global(.readings) { width: 100%; max-width: 100%; min-width: 0; justify-self: stretch; }
  .screen { display: flex; align-items: center; gap: 8px; max-width: min(100%, 480px); min-height: 40px; padding: 4px 12px 4px 4px; border: 1px solid var(--border); border-radius: var(--r-lg); background: color-mix(in srgb, var(--surface) 94%, transparent); box-shadow: var(--shadow-sm); }
  .back { display: inline-flex; align-items: center; gap: 5px; min-height: 32px; padding: 0 10px; border: 0; border-radius: var(--r-md); background: var(--surface-2); color: var(--text); font: inherit; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-medium); cursor: pointer; flex: none; }
  .back:hover { background: var(--row-hover); }
  .back:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  .who { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .who i { width: 10px; height: 10px; border-radius: 3px; flex: none; }
  .who > span { display: grid; min-width: 0; }
  .who b, .who small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .who b { font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); font-weight: var(--fw-semibold); }
  .who small { color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .ask :global(.consent) { padding: 6px 10px; border: 1px solid var(--border); border-radius: var(--r-lg); background: color-mix(in srgb, var(--surface) 92%, transparent); }
  .bottom { left: 10px; right: 10px; bottom: 10px; display: grid; gap: 6px; justify-items: start; }
  .row { justify-self: stretch; display: flex; flex-wrap: wrap; align-items: flex-end; gap: 6px; }
  .row .tools { margin-left: auto; flex-wrap: nowrap; }
  /* Zoom sits on the right edge, clear of the readings above and the controls below. */
  .zoom { right: 10px; top: 50%; transform: translateY(-50%); display: grid; gap: 2px; padding: 3px; border: 1px solid var(--border); border-radius: var(--r-lg);
    background: color-mix(in srgb, var(--surface) 90%, transparent); box-shadow: var(--shadow-sm); }
  .bar { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; padding: 3px; border: 1px solid var(--border); border-radius: var(--r-lg); background: color-mix(in srgb, var(--surface) 90%, transparent); box-shadow: var(--shadow-sm); }
  .legend { list-style: none; margin: 0; padding: 4px 8px; display: flex; flex-wrap: wrap; gap: 4px 12px; border-radius: var(--r-md); background: color-mix(in srgb, var(--surface) 80%, transparent); color: var(--muted); font-size: var(--fs-caption); line-height: var(--lh-caption); }
  .legend li { display: inline-flex; align-items: center; gap: 5px; }
  .legend i { width: 10px; height: 10px; border-radius: 2px; border: 1.5px solid var(--text-2); }
  .legend .solid { background: var(--text-2); }
  .legend .faded { background: color-mix(in srgb, var(--text-2) 35%, transparent); }
  .legend .blue { border-color: var(--tone-blue); background: var(--tone-blue-bg); }
  .legend .beam { width: 4px; border: 0; border-radius: 2px; background: var(--ok); height: 12px; }
  .legend .gauge { border-radius: 50%; border-width: 2px; border-color: var(--border-strong) var(--border-strong) var(--accent) var(--accent); transform: rotate(-45deg); }
  .legend .flow { border: 0; width: 16px; background: radial-gradient(circle, var(--tone-blue) 1.6px, transparent 2.1px) 0 50% / 5px 6px repeat-x; }
  .legend .ripple { width: 6px; height: 6px; border-color: var(--warn); border-radius: 50%; box-shadow: 0 0 0 2px var(--surface), 0 0 0 3.5px color-mix(in srgb, var(--warn) 55%, transparent); margin: 0 2px; }
  .legend .pin { width: 8px; height: 8px; border: 0; border-radius: 50% 50% 50% 0; transform: rotate(-45deg); background: var(--warn); }
  .legend .call { width: 14px; height: 7px; border-radius: 4px; border: 0; background: color-mix(in srgb, var(--accent) 75%, transparent); }
  .caption { margin: 0; display: inline-flex; align-items: center; gap: 6px; max-width: 100%; padding: 6px 10px; border-radius: var(--r-md); background: color-mix(in srgb, var(--surface) 92%, transparent); border: 1px solid var(--border); box-shadow: var(--shadow-sm);
    color: var(--text); font-size: var(--fs-body-sm); line-height: var(--lh-body-sm); }
  .caption :global(svg) { color: var(--accent); flex: none; }
  .link { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 8px; border: 0; border-radius: var(--r-md); background: transparent; color: var(--accent); font: inherit; font-size: var(--fs-caption); font-weight: var(--fw-medium); cursor: pointer; }
  .link:hover { background: var(--accent-softer); }
  .link:focus-visible { outline: 2px solid var(--ring); outline-offset: 1px; }
  /* Narrower: the view controls show their icons only. Their names stay as visually hidden text,
     so each button keeps its accessible name, and its tooltip still shows it. */
  /* Lenses show their icons only first; each keeps its name as visually hidden text and in its tooltip. */
  @container (max-width: 1200px) {
    /* The lens in use keeps its name; the others stay icons so the bar still fits. */
    .lenses :global([role="radio"]:not([aria-checked="true"]) > span) { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; border: 0; }
  }
  @container (max-width: 1100px) {
    .m3d.open { grid-template-columns: 168px minmax(0, 1fr) 272px; }
    .m3d.peek { grid-template-columns: 168px minmax(0, 1fr) 44px; }
    .m3d.folded.open { grid-template-columns: 44px minmax(0, 1fr) 272px; }
    .m3d.folded.peek { grid-template-columns: 44px minmax(0, 1fr) 44px; }
  }
  @container (max-width: 1000px) {
    .m3d:not(.folded):not(.open) { grid-template-columns: 180px minmax(0, 1fr); }
    .tools :global(.action-pill > span) { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; border: 0; }
  }
</style>

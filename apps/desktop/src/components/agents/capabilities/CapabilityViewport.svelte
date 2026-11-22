<script lang="ts">
  import { untrack } from "svelte";
  import { useSvelteFlow } from "@xyflow/svelte";
  import { cardInside, cardZoom, fitZoom } from "../../../lib/flow-camera";
  import { FIT_PADDING } from "../../../lib/flow";
  import { CAPABILITY_MAP_NODE_HEIGHT, CAPABILITY_MAP_NODE_WIDTH, type CapabilityGraphPoint } from "../../../lib/agent-capability-map";

  let { token, focus, hub, width, height, reduced }: {
    token: string;
    focus: CapabilityGraphPoint | null;
    hub: CapabilityGraphPoint | null;
    width: number;
    height: number;
    reduced: boolean;
  } = $props();
  const flow = useSvelteFlow();
  const READABLE_ZOOM = .85;
  let last = "";

  function showIfNeeded(target: CapabilityGraphPoint, duration: number) {
    if (width <= 0 || height <= 0) return;
    const viewport = flow.getViewport();
    const margin = 16;
    const card = { width: CAPABILITY_MAP_NODE_WIDTH, height: CAPABILITY_MAP_NODE_HEIGHT };
    const inset = { left: margin, right: margin, top: margin, bottom: margin };
    if (cardInside(viewport, target, card, { width, height }, inset, READABLE_ZOOM)) return;
    const zoom = cardZoom(viewport.zoom, READABLE_ZOOM, fitZoom(card, width - margin * 2, height - margin * 2));
    void flow.setCenter(target.x, target.y, { zoom, duration });
  }

  $effect(() => {
    if (token === last) return;
    last = token;
    // Only structural/size tokens trigger fitting; sample refreshes never move the map.
    const narrow = untrack(() => width < 960);
    const duration = untrack(() => reduced ? 0 : 320);
    let cancelled = false;
    let generation = 0;
    const fit = async () => {
      const current = ++generation;
      await flow.fitView({ padding: FIT_PADDING, duration, minZoom: narrow ? READABLE_ZOOM : .08, maxZoom: 1.2 });
      if (cancelled || current !== generation) return;
      // A clamped fit would otherwise centre blank space between the parallel lanes.
      const target = untrack(() => focus ?? (narrow && flow.getZoom() <= READABLE_ZOOM + .0001 ? hub : null));
      if (target) untrack(() => showIfNeeded(target, 0));
    };
    const first = setTimeout(() => { void fit(); }, 60);
    const measured = setTimeout(() => { void fit(); }, 400);
    return () => { cancelled = true; clearTimeout(first); clearTimeout(measured); };
  });

  $effect(() => {
    const target = focus;
    if (!target) return;
    // Reading the viewport inside this effect would subscribe it to its own updates.
    untrack(() => showIfNeeded(target, reduced ? 0 : 300));
  });
</script>

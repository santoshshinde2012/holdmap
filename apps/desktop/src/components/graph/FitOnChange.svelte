<script lang="ts">
  // Lives inside <SvelteFlow> so it can reach the viewport: re-fits when `token` changes
  // (new layout or structure) and centres on `focus` when the selection comes from the list.
  // A panel floating over the map (narrow windows: `obstacle`) is left out of the fit, so the
  // whole graph shows beside or above it; a docked panel just resizes the pane instead.
  import { untrack } from "svelte";
  import { useSvelteFlow } from "@xyflow/svelte";
  import { FIT_PADDING } from "../../lib/flow";
  let { token, focus, reduced, open = false, obstacle = () => null }: { token: string; focus: { x: number; y: number } | null; reduced: boolean; open?: boolean; obstacle?: () => HTMLElement | null } = $props();
  const flow = useSvelteFlow();
  let last = "";
  let covered = false;
  /** Fit padding for the map as it is right now. */
  function padding(): Record<string, string> {
    const p: Record<string, string> = { top: FIT_PADDING.top, bottom: FIT_PADDING.bottom, right: FIT_PADDING.x, left: FIT_PADDING.x };
    const el = open ? obstacle() : null;
    const pane = document.querySelector<HTMLElement>(".agents .svelte-flow")?.getBoundingClientRect();
    covered = !!el && !!pane && getComputedStyle(el).position === "absolute";
    if (covered && el && pane) {
      const o = el.getBoundingClientRect();
      // Reserve whichever side the panel took: the right edge for a side panel, the bottom for a sheet.
      if (o.width < pane.width * 0.9) p.right = `${Math.ceil(pane.right - o.left) + 16}px`;
      else p.bottom = `${Math.ceil(pane.bottom - o.top) + 16}px`;
    }
    return p;
  }
  const fit = () => flow.fitView({ padding: padding(), duration: reduced ? 0 : 320, maxZoom: 1.2 });
  $effect(() => {
    if (token === last) return;
    last = token;
    // Nodes need a frame to be measured before the bounds are right; fit twice to be safe.
    const a = setTimeout(() => untrack(fit), 60);
    const b = setTimeout(() => untrack(fit), 400);
    return () => { clearTimeout(a); clearTimeout(b); };
  });
  // The floating panel opened or closed: fit into what is left, or back into the whole pane.
  let wasCovered = false;
  $effect(() => {
    void open;
    const t = setTimeout(() => untrack(() => {
      const p = padding();
      if (!covered && !wasCovered) return;
      wasCovered = covered;
      flow.fitView({ padding: p, duration: reduced ? 0 : 320, maxZoom: 1.1 });
    }), 80);
    return () => clearTimeout(t);
  });
  $effect(() => {
    const f = focus;
    if (!f) return;
    // Only `focus` drives this effect. Reading the zoom subscribes to the viewport, and with
    // reduced motion `setCenter` writes it synchronously, so untracked to avoid a re-run loop.
    untrack(() => flow.setCenter(f.x, f.y, { zoom: flow.getZoom(), duration: reduced ? 0 : 300 }));
  });
</script>

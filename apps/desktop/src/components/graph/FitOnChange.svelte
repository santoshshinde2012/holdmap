<script lang="ts">
  // Lives inside <SvelteFlow> so it can reach the viewport: re-fits when `token` changes
  // (new layout or structure) and centres on `focus` when the selection comes from the list.
  // A panel floating over the map (narrow windows: `obstacle`) is left out of the fit, so the
  // whole graph shows beside or above it; a docked panel just resizes the pane instead.
  import { untrack } from "svelte";
  import { useSvelteFlow } from "@xyflow/svelte";
  import { FIT_MIN_ZOOM, FIT_PADDING } from "../../lib/flow";
  let { token, focus, reduced, open = false, obstacle = () => null, near = [] }: { token: string; focus: { x: number; y: number } | null; reduced: boolean; open?: boolean; obstacle?: () => HTMLElement | null; near?: string[] } = $props();
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
  /** Under a floating panel the map may crop rather than shrink below this: tiny diagrams are unreadable. */
  const COVERED_MIN_ZOOM = 0.6;
  /** Whether fitting every node would shrink the map below the readable floor in the pane as it is now. */
  function tooSmall(p: Record<string, string>): boolean {
    const pane = document.querySelector<HTMLElement>(".agents .svelte-flow")?.getBoundingClientRect();
    const all = flow.getNodes();
    if (!pane || all.length === 0) return false;
    const b = flow.getNodesBounds(all);
    const px = (v: string) => parseFloat(v) || 0;
    const zoom = Math.min((pane.width - px(p.left) - px(p.right)) / Math.max(b.width, 1), (pane.height - px(p.top) - px(p.bottom)) / Math.max(b.height, 1));
    return zoom < COVERED_MIN_ZOOM;
  }
  /** What to fit: the whole map, or (a panel is open and the whole map would be too small to read) just the picked agent and what it touches. */
  function target(p: Record<string, string>) {
    const ids = near.filter((id) => !!flow.getInternalNode(id));
    return open && ids.length > 1 && tooSmall(p) ? { nodes: ids.map((id) => ({ id })), minZoom: COVERED_MIN_ZOOM } : { minZoom: covered ? COVERED_MIN_ZOOM : FIT_MIN_ZOOM };
  }
  const fit = () => {
    const p = padding();
    return flow.fitView({ padding: p, duration: reduced ? 0 : 320, maxZoom: 1.2, ...target(p) });
  };
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
    void near.join();
    const t = setTimeout(() => untrack(() => {
      const p = padding();
      const t = target(p);
      if (!covered && !wasCovered && !t.nodes) return;
      wasCovered = covered || !!t.nodes;
      flow.fitView({ padding: p, duration: reduced ? 0 : 320, maxZoom: 1.1, ...t });
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

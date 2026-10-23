<script lang="ts">
  import { untrack } from "svelte";
  import { useSvelteFlow, useStore } from "@xyflow/svelte";
  import { FIT_PADDING } from "../../../lib/flow";
  let { token, focus, reduced }: { token: string; focus: { x: number; y: number } | null; reduced: boolean } = $props();
  const flow = useSvelteFlow();
  /** 11px captions stay about 9px: the floor below which a busy graph is panned instead of shrunk. */
  const READABLE_ZOOM = .8;
  const store = useStore();
  $effect(() => {
    void token;
    const width = store.width, height = store.height;
    if (!width || !height) return;
    const fit = () => untrack(() => {
      const bounds = flow.getNodesBounds(flow.getNodes());
      if (bounds.width * READABLE_ZOOM > width - 56 || bounds.height * READABLE_ZOOM > height - 112) {
        // Keep the starting participants readable. Fit-all remains available in Controls.
        void flow.setViewport({ x: 28 - bounds.x * READABLE_ZOOM, y: 84 - bounds.y * READABLE_ZOOM, zoom: READABLE_ZOOM }, { duration: reduced ? 0 : 250 });
      } else {
        void flow.fitView({ padding: FIT_PADDING, duration: reduced ? 0 : 250, maxZoom: 1.15 });
      }
    });
    const first = setTimeout(fit, 60), settled = setTimeout(fit, 400);
    return () => { clearTimeout(first); clearTimeout(settled); };
  });
  $effect(() => {
    const point = focus;
    if (point) untrack(() => void flow.setCenter(point.x, point.y, { zoom: Math.max(READABLE_ZOOM, flow.getZoom()), duration: reduced ? 0 : 250 }));
  });
</script>

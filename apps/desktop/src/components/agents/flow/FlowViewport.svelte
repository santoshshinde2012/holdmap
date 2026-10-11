<script lang="ts">
  import { untrack } from "svelte";
  import { useSvelteFlow, useStore } from "@xyflow/svelte";
  import { FIT_PADDING } from "../../../lib/flow";
  let { token, focus, reduced }: { token: string; focus: { x: number; y: number } | null; reduced: boolean } = $props();
  const flow = useSvelteFlow();
  const store = useStore();
  $effect(() => {
    void token;
    const width = store.width, height = store.height;
    if (!width || !height) return;
    const fit = () => untrack(() => {
      const bounds = flow.getNodesBounds(flow.getNodes());
      if (bounds.width * .65 > width - 56 || bounds.height * .65 > height - 112) {
        // Keep the starting participants readable. Fit-all remains available in Controls.
        void flow.setViewport({ x: 28 - bounds.x * .65, y: 84 - bounds.y * .65, zoom: .65 }, { duration: reduced ? 0 : 250 });
      } else {
        void flow.fitView({ padding: FIT_PADDING, duration: reduced ? 0 : 250, maxZoom: 1.15 });
      }
    });
    const first = setTimeout(fit, 60), settled = setTimeout(fit, 400);
    return () => { clearTimeout(first); clearTimeout(settled); };
  });
  $effect(() => {
    const point = focus;
    if (point) untrack(() => void flow.setCenter(point.x, point.y, { zoom: Math.max(.65, flow.getZoom()), duration: reduced ? 0 : 250 }));
  });
</script>

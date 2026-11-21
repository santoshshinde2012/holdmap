<script lang="ts">
  import { untrack } from "svelte";
  import { useSvelteFlow, useStore } from "@xyflow/svelte";
  import { pinnedView, tooWide } from "../../../lib/flow-camera";
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
      // 56px and 112px are the side and vertical chrome. Past them, sit on the first participants.
      if (tooWide(bounds, { width, height }, READABLE_ZOOM, 56, 112)) {
        void flow.setViewport(pinnedView(bounds, READABLE_ZOOM, { x: 28, y: 84 }), { duration: reduced ? 0 : 250 });
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

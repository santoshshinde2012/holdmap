<script lang="ts">
  // Lives inside <SvelteFlow> so it can reach the viewport: re-fits when `token` changes
  // (new layout or structure) and centres on `focus` when the selection comes from the list.
  import { untrack } from "svelte";
  import { useSvelteFlow } from "@xyflow/svelte";
  import { FIT_PADDING } from "../../lib/graph";
  let { token, focus, reduced }: { token: string; focus: { x: number; y: number } | null; reduced: boolean } = $props();
  const flow = useSvelteFlow();
  let last = "";
  $effect(() => {
    if (token === last) return;
    last = token;
    // Nodes need a frame to be measured before the bounds are right; fit twice to be safe.
    const fit = () => flow.fitView({ padding: FIT_PADDING, duration: reduced ? 0 : 320, maxZoom: 1.2 });
    const a = setTimeout(fit, 60);
    const b = setTimeout(fit, 400);
    return () => { clearTimeout(a); clearTimeout(b); };
  });
  $effect(() => {
    const f = focus;
    if (!f) return;
    // Only `focus` drives this effect. Reading the zoom subscribes to the viewport, and with
    // reduced motion `setCenter` writes it synchronously, so untracked to avoid a re-run loop.
    untrack(() => flow.setCenter(f.x, f.y, { zoom: flow.getZoom(), duration: reduced ? 0 : 300 }));
  });
</script>

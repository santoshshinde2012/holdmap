<script lang="ts">
  // Lives inside <SvelteFlow> so it can reach the viewport: re-fits when `token` changes
  // (new layout or structure) and centres on `focus` when the selection comes from the list.
  import { useSvelteFlow } from "@xyflow/svelte";
  let { token, focus, reduced }: { token: string; focus: { x: number; y: number } | null; reduced: boolean } = $props();
  const flow = useSvelteFlow();
  let last = "";
  $effect(() => {
    if (token === last) return;
    last = token;
    requestAnimationFrame(() => flow.fitView({ padding: 0.14, duration: reduced ? 0 : 320, maxZoom: 1.15 }));
  });
  $effect(() => {
    if (focus) flow.setCenter(focus.x, focus.y, { zoom: Math.max(flow.getZoom(), 0.85), duration: reduced ? 0 : 300 });
  });
</script>

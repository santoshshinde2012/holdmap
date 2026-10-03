<script lang="ts">
  // Minimap that appears only when part of the graph is off-screen (zoomed in or panned away).
  // After fit-view everything is visible, so it stays out of the way instead of covering nodes.
  import { MiniMap, useNodes, useStore, useSvelteFlow, useViewport } from "@xyflow/svelte";
  import { needsMiniMap, type FlowNode } from "../../lib/graph";

  let { dark }: { dark: boolean } = $props();
  const flow = useSvelteFlow();
  const store = useStore();
  const nodes = useNodes();
  const viewport = useViewport();
  const show = $derived.by(() => {
    const list = nodes.current;
    if (!list.length) return false;
    return needsMiniMap(flow.getNodesBounds(list), viewport.current, store.width, store.height);
  });
</script>

{#if show}
  <MiniMap
    position="bottom-right"
    pannable
    zoomable
    ariaLabel="Graph overview"
    nodeColor={(n) => (n.type === "cluster" ? "transparent" : (n.data as FlowNode["data"]).node?.is_dev ? "var(--tone-green)" : "var(--border-strong)")}
    nodeStrokeColor={(n) => (n.type === "cluster" ? "var(--accent)" : "transparent")}
    maskColor={dark ? "rgb(0 0 0 / 0.55)" : "rgb(240 240 245 / 0.65)"}
    width={160}
    height={104}
  />
{/if}

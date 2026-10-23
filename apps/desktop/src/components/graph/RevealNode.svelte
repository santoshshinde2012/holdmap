<script lang="ts">
  // Lives inside <SvelteFlow>: when `id` changes (a selection from the list, ↑/↓, search or a
  // deep link) and that node is not clearly in view, pan to centre it at the current zoom.
  // "In view" excludes whatever floats over the map (`obstacle`, the narrow-window details
  // sheet), so a selected node is never left hidden behind it.
  import { untrack } from "svelte";
  import { useSvelteFlow } from "@xyflow/svelte";
  import { freeRegion, type Box } from "./reveal";
  let { id, reduced, obstacle = () => null }: { id: string | null; reduced: boolean; obstacle?: () => HTMLElement | null } = $props();
  const flow = useSvelteFlow();
  const box = (r: DOMRect): Box => ({ left: r.left, top: r.top, right: r.right, bottom: r.bottom });
  $effect(() => {
    const target = id;
    if (!target) return;
    // Wait a frame so a just-rendered node has been measured.
    const t = setTimeout(() => untrack(() => {
      const node = flow.getInternalNode(target);
      const pane = document.querySelector<HTMLElement>(".agents .svelte-flow");
      if (!node || !pane) return;
      const w = node.measured.width ?? 0;
      const h = node.measured.height ?? 0;
      const { x, y } = node.internals.positionAbsolute;
      const a = flow.flowToScreenPosition({ x, y });
      const b = flow.flowToScreenPosition({ x: x + w, y: y + h });
      const paneBox = box(pane.getBoundingClientRect());
      const floating = obstacle();
      const free = freeRegion(paneBox, floating && getComputedStyle(floating).position === "absolute" ? box(floating.getBoundingClientRect()) : null);
      const visible = a.x >= free.left && a.y >= free.top && b.x <= free.right && b.y <= free.bottom;
      if (visible) return;
      // Put the node in the middle of the free region: shift the viewport centre by the gap
      // between the pane's middle and the free region's middle.
      const zoom = flow.getZoom();
      const dx = ((paneBox.left + paneBox.right) - (free.left + free.right)) / 2 / zoom;
      const dy = ((paneBox.top + paneBox.bottom) - (free.top + free.bottom)) / 2 / zoom;
      flow.setCenter(x + w / 2 + dx, y + h / 2 + dy, { zoom, duration: reduced ? 0 : 280 });
    }), 30);
    return () => clearTimeout(t);
  });
</script>

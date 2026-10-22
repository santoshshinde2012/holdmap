<script lang="ts">
  // Lives inside <SvelteFlow>: when `id` changes (a selection from the list, ↑/↓, search or a
  // deep link) and that node is off screen, pan to centre it at the current zoom.
  import { untrack } from "svelte";
  import { useSvelteFlow } from "@xyflow/svelte";
  let { id, reduced, inset = 0 }: { id: string | null; reduced: boolean; inset?: number } = $props();
  const flow = useSvelteFlow();
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
      const box = pane.getBoundingClientRect();
      const visible = a.x >= box.left && a.y >= box.top && b.x <= box.right - inset && b.y <= box.bottom;
      if (!visible) flow.setCenter(x + w / 2, y + h / 2, { zoom: flow.getZoom(), duration: reduced ? 0 : 280 });
    }), 30);
    return () => clearTimeout(t);
  });
</script>

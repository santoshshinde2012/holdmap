<script lang="ts">
  import { untrack } from "svelte";
  import { Controls, ControlButton, useNodesInitialized, useSvelteFlow, useViewport, useViewportInitialized } from "@xyflow/svelte";
  import Icon from "../Icon.svelte";
  import { NODE_H, NODE_W, type Point } from "../../lib/graph";

  let { viewKey, focus, anchor, width, height, reduced, pending = false }: {
    viewKey: string; focus: Point | null; anchor: Point | null;
    width: number; height: number; reduced: boolean;
    pending?: boolean;
  } = $props();
  const flow = useSvelteFlow();
  const viewport = useViewport();
  const initialized = useNodesInitialized();
  const viewportInitialized = useViewportInitialized();
  const padding = { top: "24px", bottom: "72px", x: "24px" } as const;
  const readableZoom = .85;
  let fittedKey: string | null = null;
  const revealed = { point: null as Point | null, width: 0, height: 0 };

  function reveal(point: Point, duration: number) {
    const view = flow.getViewport();
    const x = point.x * view.zoom + view.x, y = point.y * view.zoom + view.y;
    if (view.zoom >= readableZoom && x - NODE_W * view.zoom / 2 >= 20
      && x + NODE_W * view.zoom / 2 <= width - 20
      && y - NODE_H * view.zoom / 2 >= 20 && y + NODE_H * view.zoom / 2 <= height - 64) return;
    void flow.setCenter(point.x, point.y, { zoom: Math.max(readableZoom, view.zoom), duration });
  }

  function readable(reset = false) {
    const duration = reduced ? 0 : 250;
    const point = focus ?? anchor;
    if (reset && point) {
      void flow.setCenter(point.x, point.y, { zoom: 1, duration });
      return;
    }
    const bounds = flow.getNodesBounds(flow.getNodes());
    if (bounds.width * readableZoom > width - 48 || bounds.height * readableZoom > height - 96) {
      if (width < 600 && point) {
        // A narrow canvas starts on a complete service card, with its links available by pan.
        void flow.setCenter(point.x, point.y, { zoom: readableZoom, duration });
        return;
      }
      // Start at the first compact group instead of centring empty space between islands.
      void flow.setViewport({ x: 24 - bounds.x * readableZoom, y: 24 - bounds.y * readableZoom, zoom: readableZoom }, { duration });
      if (focus) reveal(focus, duration);
    } else {
      void flow.fitView({ padding, minZoom: readableZoom, maxZoom: 1.1, duration });
    }
  }

  $effect(() => {
    const ready = initialized.current && viewportInitialized.current && !pending;
    const key = viewKey;
    if (!ready || width <= 0 || height <= 0 || fittedKey === key) return;
    // Measured nodes are ready: one frame, with cancellation, replaces the delayed double fit.
    const frame = requestAnimationFrame(() => untrack(() => {
      fittedKey = key;
      readable();
    }));
    return () => cancelAnimationFrame(frame);
  });
  $effect(() => {
    const point = focus;
    const ready = initialized.current && viewportInitialized.current && !pending;
    if (!ready || !point || width <= 0 || height <= 0) return;
    // A refresh becoming ready is not a new focus request. Keep requests unconsumed while
    // loading, and still reveal an existing focus when the available canvas size changes.
    if (revealed.point === point && revealed.width === width && revealed.height === height) return;
    revealed.point = point; revealed.width = width; revealed.height = height;
    untrack(() => reveal(point, reduced ? 0 : 250));
  });
</script>

<Controls showLock={false} showFitView={false} orientation="horizontal" position="bottom-left">
  <ControlButton class="readable-zoom" aria-label="Reset to readable size" title="Reset to readable size" onclick={() => readable(true)}>
    {Math.round(viewport.current.zoom * 100)}%
  </ControlButton>
  <ControlButton aria-label="Fit all services in view" title="Fit all services in view" onclick={() => void flow.fitView({ padding, minZoom: .08, maxZoom: 1.1, duration: reduced ? 0 : 250 })}>
    <Icon name="fit" size={16} />
  </ControlButton>
</Controls>

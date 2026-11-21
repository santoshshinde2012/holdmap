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
  const readableZoom = .85;
  // A picture that still fits with readable cards opens whole. Below this, sit on one card at readableZoom.
  const fitFloor = .78;
  let fittedKey: string | null = null;
  let fitToken = 0;
  const revealed = { point: null as Point | null, width: 0, height: 0 };

  function reveal(point: Point, duration: number) {
    const view = flow.getViewport();
    const x = point.x * view.zoom + view.x, y = point.y * view.zoom + view.y;
    if (view.zoom >= readableZoom && x - NODE_W * view.zoom / 2 >= 20
      && x + NODE_W * view.zoom / 2 <= width - 20
      && y - NODE_H * view.zoom / 2 >= 20 && y + NODE_H * view.zoom / 2 <= height - 64) return;
    void flow.setCenter(point.x, point.y, { zoom: Math.max(readableZoom, view.zoom), duration });
  }

  // fitView queues a second layout pass that drops the camera update. setViewport applies it directly.
  function place(zoom: number, duration: number) {
    const bounds = flow.getNodesBounds(flow.getNodes());
    if (bounds.width < 2 || bounds.height < 2) return;
    const x = 24 + ((width - 48) - bounds.width * zoom) / 2 - bounds.x * zoom;
    const y = 84 + ((height - 168) - bounds.height * zoom) / 2 - bounds.y * zoom;
    void flow.setViewport({ x, y, zoom }, { duration });
  }

  function readable(reset = false) {
    const duration = reduced ? 0 : 250;
    const point = focus ?? anchor;
    if (reset && point) {
      void flow.setCenter(point.x, point.y, { zoom: 1, duration });
      return;
    }
    const bounds = flow.getNodesBounds(flow.getNodes());
    const fitZoom = Math.min(1.1, (width - 48) / Math.max(bounds.width, 1), (height - 168) / Math.max(bounds.height, 1));
    if (fitZoom >= fitFloor) {
      place(Math.min(fitZoom, 1.1), duration);
      return;
    }
    // Too wide to read all at once: open on a real service card, centered, instead of
    // the empty corner of the bounds (that corner is blank when a group is a cloud).
    if (point) {
      void flow.setCenter(point.x, point.y, { zoom: readableZoom, duration });
      if (focus) reveal(focus, duration);
      return;
    }
    place(readableZoom, duration);
  }

  function fitAll() {
    const duration = reduced ? 0 : 250;
    const bounds = flow.getNodesBounds(flow.getNodes());
    const zoom = Math.max(.08, Math.min(1.1, (width - 48) / Math.max(bounds.width, 1), (height - 168) / Math.max(bounds.height, 1)));
    place(zoom, duration);
  }

  $effect(() => {
    const ready = initialized.current && viewportInitialized.current && !pending;
    const key = viewKey;
    if (!ready || width <= 0 || height <= 0 || fittedKey === key) return;
    // A later layout pass supersedes this one. Cancelling the frame on every width
    // change dropped the opening fit while the canvas was still settling.
    const token = ++fitToken;
    let tries = 0;
    const attempt = () => untrack(() => {
      if (token !== fitToken || fittedKey === key) return;
      const bounds = flow.getNodesBounds(flow.getNodes());
      if ((bounds.width < 2 || bounds.height < 2) && tries++ < 8) {
        requestAnimationFrame(attempt);
        return;
      }
      fittedKey = key;
      readable();
    });
    requestAnimationFrame(attempt);
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
  <ControlButton class="readable-zoom" aria-label="{Math.round(viewport.current.zoom * 100)}%, reset to readable size" title="Reset to readable size" onclick={() => readable(true)}>
    {Math.round(viewport.current.zoom * 100)}%
  </ControlButton>
  <ControlButton aria-label="Fit all services in view" title="Fit all services in view" onclick={fitAll}>
    <Icon name="fit" size={16} />
  </ControlButton>
</Controls>

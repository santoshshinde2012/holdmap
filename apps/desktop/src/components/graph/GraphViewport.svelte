<script lang="ts">
  import { untrack } from "svelte";
  import { Controls, ControlButton, useNodesInitialized, useSvelteFlow, useViewport, useViewportInitialized } from "@xyflow/svelte";
  import Icon from "../Icon.svelte";
  import { cardInside, centeredView, clampZoom, fitZoom } from "../../lib/flow-camera";
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
    // 20px keeps a card off the canvas edge. 64px keeps it above the zoom controls.
    if (cardInside(view, point, { width: NODE_W, height: NODE_H }, { width, height }, { left: 20, right: 20, top: 20, bottom: 64 }, readableZoom)) return;
    void flow.setCenter(point.x, point.y, { zoom: Math.max(readableZoom, view.zoom), duration });
  }

  // fitView queues a second layout pass that drops the camera update. setViewport applies it directly.
  // 24px and 84px clear the side edge and the toolbar; 168px is that top inset plus the controls.
  function place(zoom: number, duration: number) {
    const bounds = flow.getNodesBounds(flow.getNodes());
    if (bounds.width < 2 || bounds.height < 2) return;
    void flow.setViewport(centeredView(bounds, zoom, { left: 24, top: 84, width: width - 48, height: height - 168 }), { duration });
  }

  function readable(reset = false) {
    const duration = reduced ? 0 : 250;
    const point = focus ?? anchor;
    if (reset && point) {
      void flow.setCenter(point.x, point.y, { zoom: 1, duration });
      return;
    }
    const bounds = flow.getNodesBounds(flow.getNodes());
    const opening = Math.min(1.1, fitZoom(bounds, width - 48, height - 168));
    if (opening >= fitFloor) {
      place(Math.min(opening, 1.1), duration);
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
    place(clampZoom(fitZoom(bounds, width - 48, height - 168), 0.08, 1.1), duration);
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

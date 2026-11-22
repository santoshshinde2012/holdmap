// Pure camera math for the Svelte Flow canvases. Each canvas keeps its own insets and
// readable zoom, because its chrome eats a different edge. The decision — fit the picture,
// or sit on one card so the names stay readable — is the same everywhere.

export interface CameraBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface CameraView {
  x: number;
  y: number;
  zoom: number;
}

export interface CameraPoint {
  x: number;
  y: number;
}

export interface CameraSize {
  width: number;
  height: number;
}

export interface CameraInsets {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

/** Zoom that fits `box` in a pane of `innerWidth` × `innerHeight`. Uncapped: the caller clamps it. */
export function fitZoom(box: Pick<CameraBox, "width" | "height">, innerWidth: number, innerHeight: number): number {
  return Math.min(innerWidth / Math.max(box.width, 1), innerHeight / Math.max(box.height, 1));
}

/** Keep a zoom between two limits. */
export function clampZoom(zoom: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, zoom));
}

/**
 * Center `box` inside a frame. `left` and `top` are the frame's origin; `width` and `height`
 * are the pane minus the chrome on both sides.
 */
export function centeredView(box: CameraBox, zoom: number, frame: { left: number; top: number; width: number; height: number }): CameraView {
  return {
    x: frame.left + (frame.width - box.width * zoom) / 2 - box.x * zoom,
    y: frame.top + (frame.height - box.height * zoom) / 2 - box.y * zoom,
    zoom,
  };
}

/** Pin the box origin to `origin`. The calls graph opens on the first participants, not the middle. */
export function pinnedView(box: CameraBox, zoom: number, origin: CameraPoint): CameraView {
  return { x: origin.x - box.x * zoom, y: origin.y - box.y * zoom, zoom };
}

/** The scaled picture is larger than the pane minus slack, so fitting it would drop below `zoom`. */
export function tooWide(box: Pick<CameraBox, "width" | "height">, pane: CameraSize, zoom: number, slackX: number, slackY: number): boolean {
  return box.width * zoom > pane.width - slackX || box.height * zoom > pane.height - slackY;
}

/** A card centered on `center` is fully inside the pane, and the zoom is already readable. */
export function cardInside(view: CameraView, center: CameraPoint, card: CameraSize, pane: CameraSize, inset: CameraInsets, minZoom: number): boolean {
  if (view.zoom < minZoom) return false;
  const x = center.x * view.zoom + view.x;
  const y = center.y * view.zoom + view.y;
  const halfW = card.width * view.zoom / 2;
  const halfH = card.height * view.zoom / 2;
  return x - halfW >= inset.left
    && x + halfW <= pane.width - inset.right
    && y - halfH >= inset.top
    && y + halfH <= pane.height - inset.bottom;
}

/**
 * Zoom for a jump to one card. Never below `readable`. Never past the zoom that fits the card.
 * A card bigger than the pane stays at `readable`: one card, type still large enough to read.
 */
export function cardZoom(current: number, readable: number, cardFit: number): number {
  return Math.max(readable, Math.min(Math.max(readable, current), cardFit));
}

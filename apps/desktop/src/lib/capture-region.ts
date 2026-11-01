// A part of the holdmap window to snapshot or record, chosen by dragging over the window. It is in
// CSS pixels from the top left of the window's viewport; each capture path maps it onto its own
// pixels: the rendered view (exact), the macOS window capture (points, exact), or a shared
// window's video frames (scaled, with any title bar the frame carries above the page).

export interface CaptureRegion { x: number; y: number; width: number; height: number }
/** Smaller than this, a capture is a crumb rather than a picture. */
export const MIN_REGION = 32;

export type CaptureArea = "window" | "region";

/** Whole pixels, inside the viewport, at least MIN_REGION each way (shrinking the viewport's own
 *  size if that's smaller); `even` rounds the size down to even numbers, as video encoders need. */
export function clampRegion(r: CaptureRegion, viewport: { width: number; height: number }, even = false): CaptureRegion | null {
  const vw = Math.floor(viewport.width), vh = Math.floor(viewport.height);
  if (![r.x, r.y, r.width, r.height, vw, vh].every(Number.isFinite) || vw < 2 || vh < 2) return null;
  const min = (limit: number) => Math.min(MIN_REGION, limit);
  let x = Math.round(Math.min(Math.max(0, r.x), vw - min(vw)));
  let y = Math.round(Math.min(Math.max(0, r.y), vh - min(vh)));
  let width = Math.round(Math.max(min(vw), Math.min(r.width, vw - x)));
  let height = Math.round(Math.max(min(vh), Math.min(r.height, vh - y)));
  // A minimum-size box at the far edge pushes back inside.
  x = Math.min(x, vw - width); y = Math.min(y, vh - height);
  if (even) { width -= width % 2; height -= height % 2; }
  return { x, y, width, height };
}

/** Normalise a drag from (x0, y0) to (x1, y1), in either direction. */
export function fromDrag(x0: number, y0: number, x1: number, y1: number): CaptureRegion {
  return { x: Math.min(x0, x1), y: Math.min(y0, y1), width: Math.abs(x1 - x0), height: Math.abs(y1 - y0) };
}

/** The region in an image of the whole viewport: scaled by the image's width, and below any rows
 *  the image carries above the page (a window frame's title bar). Whole pixels, inside the image. */
export function regionInImage(r: CaptureRegion, viewport: { width: number; height: number }, image: { width: number; height: number }) {
  const scale = image.width / Math.max(1, viewport.width);
  const top = Math.max(0, Math.round(image.height - viewport.height * scale));
  const sx = Math.max(0, Math.round(r.x * scale)), sy = Math.max(0, Math.round(top + r.y * scale));
  return { sx, sy, sw: Math.max(1, Math.min(image.width - sx, Math.round(r.width * scale))), sh: Math.max(1, Math.min(image.height - sy, Math.round(r.height * scale))), scale };
}

/** Crop a PNG to `region` (in the viewport), at the image's own resolution. */
export async function cropPng(png: Blob, region: CaptureRegion, viewport: { width: number; height: number }): Promise<{ blob: Blob; width: number; height: number }> {
  const bitmap = await createImageBitmap(png);
  try {
    const { sx, sy, sw, sh } = regionInImage(region, viewport, bitmap);
    const canvas = document.createElement("canvas");
    canvas.width = sw; canvas.height = sh;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Couldn't prepare the cropped snapshot.");
    ctx.drawImage(bitmap, sx, sy, sw, sh, 0, 0, sw, sh);
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    if (!blob) throw new Error("Couldn't encode the cropped snapshot.");
    return { blob, width: sw, height: sh };
  } finally {
    bitmap.close();
  }
}

/** The area to snap to under a point: the smallest marked panel (data-capture-area) holding it. */
export function snapArea(x: number, y: number, root: ParentNode = document): CaptureRegion | null {
  let best: CaptureRegion | null = null;
  for (const el of root.querySelectorAll<HTMLElement>("[data-capture-area]")) {
    const b = el.getBoundingClientRect();
    if (!b.width || !b.height || x < b.left || x > b.right || y < b.top || y > b.bottom) continue;
    if (!best || b.width * b.height < best.width * best.height) best = { x: b.left, y: b.top, width: b.width, height: b.height };
  }
  return best;
}

/** "640 × 360", the size a region will be in CSS pixels. */
export const regionLabel = (r: CaptureRegion) => `${Math.round(r.width)} × ${Math.round(r.height)}`;

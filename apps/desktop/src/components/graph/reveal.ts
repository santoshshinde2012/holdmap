/** A screen rectangle. */
export interface Box { left: number; top: number; right: number; bottom: number }

const area = (b: Box) => Math.max(0, b.right - b.left) * Math.max(0, b.bottom - b.top);

/**
 * The part of `pane` not covered by a floating `obstacle`: the largest of the strips left of,
 * right of, above and below it. With no obstacle, or none that overlaps, the whole pane.
 */
export function freeRegion(pane: Box, obstacle: Box | null): Box {
  if (!obstacle) return pane;
  const o: Box = {
    left: Math.max(pane.left, obstacle.left), top: Math.max(pane.top, obstacle.top),
    right: Math.min(pane.right, obstacle.right), bottom: Math.min(pane.bottom, obstacle.bottom),
  };
  if (o.left >= o.right || o.top >= o.bottom) return pane;
  const strips: Box[] = [
    { ...pane, right: o.left },
    { ...pane, left: o.right },
    { ...pane, bottom: o.top },
    { ...pane, top: o.bottom },
  ];
  return strips.reduce((best, s) => (area(s) > area(best) ? s : best));
}

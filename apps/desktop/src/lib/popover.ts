/** Position a bounded popup against its trigger in viewport coordinates. */
export function placePopover(
  anchor: { left: number; top: number; bottom: number; width: number },
  viewport: { width: number; height: number },
  preferred: { width: number; height: number },
) {
  const margin = Math.min(8, Math.max(0, viewport.width / 4), Math.max(0, viewport.height / 4));
  const width = Math.max(1, Math.min(Math.max(anchor.width, preferred.width), viewport.width - margin * 2));
  const left = Math.max(margin, Math.min(anchor.left, viewport.width - width - margin));
  const below = Math.max(0, viewport.height - anchor.bottom - 4 - margin);
  const above = Math.max(0, anchor.top - 4 - margin);
  const up = below < preferred.height && above > below;
  const maxHeight = Math.max(1, Math.min(320, preferred.height, up ? above : below, viewport.height - margin * 2));
  const height = Math.min(preferred.height, maxHeight);
  const top = Math.max(margin, Math.min(up ? anchor.top - height - 4 : anchor.bottom + 4, viewport.height - height - margin));
  return { top, left, width, maxHeight, up };
}

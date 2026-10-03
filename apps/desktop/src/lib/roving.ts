// Roving-focus arithmetic for radio groups, tab lists and listboxes (WAI-ARIA APG).

export type Orientation = "horizontal" | "vertical" | "both";

/** Index to move to for `key`, or null when the key isn't a navigation key. Skips disabled items. */
export function nextIndex(
  current: number,
  key: string,
  count: number,
  { orientation = "horizontal", loop = true, disabled = [] as boolean[] }: { orientation?: Orientation; loop?: boolean; disabled?: boolean[] } = {},
): number | null {
  if (count <= 0) return null;
  const fwd = orientation === "vertical" ? ["ArrowDown"] : orientation === "horizontal" ? ["ArrowRight"] : ["ArrowRight", "ArrowDown"];
  const back = orientation === "vertical" ? ["ArrowUp"] : orientation === "horizontal" ? ["ArrowLeft"] : ["ArrowLeft", "ArrowUp"];
  const ok = (i: number) => !disabled[i];
  const scan = (start: number, step: number): number | null => {
    let i = start;
    for (let n = 0; n < count; n++) {
      if (i < 0 || i >= count) { if (!loop) return null; i = (i + count) % count; }
      if (ok(i)) return i;
      i += step;
    }
    return null;
  };
  if (key === "Home") return scan(0, 1);
  if (key === "End") return scan(count - 1, -1);
  if (fwd.includes(key)) return scan(current + 1, 1) ?? current;
  if (back.includes(key)) return scan(current - 1, -1) ?? current;
  return null;
}

/** Type-ahead: the next item (after `current`) whose label starts with `prefix`. */
export function typeahead(labels: string[], prefix: string, current: number): number | null {
  const p = prefix.toLowerCase();
  if (!p) return null;
  for (let n = 1; n <= labels.length; n++) {
    const i = (current + n) % labels.length;
    if (labels[i].toLowerCase().startsWith(p)) return i;
  }
  return null;
}

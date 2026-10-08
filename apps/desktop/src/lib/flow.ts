// Svelte Flow helpers shared by the service graph and the agents map. Kept apart from graph.ts so
// the agents map doesn't pull in the dagre layout.

/** Fit-view margins: the top clears the floating toolbar + legend so no node hides under them. */
export const FIT_PADDING = { top: "84px", bottom: "28px", x: "28px" } as const;

/**
 * Whether the minimap is worth showing: only when part of the graph is off-screen. When everything
 * fits (the default after fit-view) it adds nothing and would only sit on top of nodes.
 * `bounds` is in flow coordinates; `view` is the pan/zoom; `w`×`h` is the canvas in px.
 */
export function needsMiniMap(
  bounds: { x: number; y: number; width: number; height: number },
  view: { x: number; y: number; zoom: number },
  w: number,
  h: number,
  slack = 8,
): boolean {
  if (!w || !h || !bounds.width || !bounds.height) return false;
  const left = bounds.x * view.zoom + view.x;
  const top = bounds.y * view.zoom + view.y;
  const right = left + bounds.width * view.zoom;
  const bottom = top + bounds.height * view.zoom;
  return left < -slack || top < -slack || right > w + slack || bottom > h + slack;
}

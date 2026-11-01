// Home presentation: the same agents as cards, as a machine map, or as a 3D machine. Persisted by the app shell.
export const HOME_LAYOUTS = ["cards", "map", "3d"] as const;
export type HomeLayout = (typeof HOME_LAYOUTS)[number];
export const HOME_LAYOUT_LABEL: Record<HomeLayout, string> = { cards: "Cards", map: "Map", "3d": "3D" };

/** The next layout; the single-key shortcut cycles through them. */
export function nextLayout(l: HomeLayout): HomeLayout {
  return HOME_LAYOUTS[(HOME_LAYOUTS.indexOf(l) + 1) % HOME_LAYOUTS.length];
}

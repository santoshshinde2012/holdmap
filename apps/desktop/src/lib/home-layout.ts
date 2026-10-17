// Home presentation: the same agents as cards or as a machine map. Persisted by the app shell.
export const HOME_LAYOUTS = ["cards", "map"] as const;
export type HomeLayout = (typeof HOME_LAYOUTS)[number];
export const HOME_LAYOUT_LABEL: Record<HomeLayout, string> = { cards: "Cards", map: "Map" };

/** The other layout; the single-key shortcut flips between the two. */
export function nextLayout(l: HomeLayout): HomeLayout {
  return l === "cards" ? "map" : "cards";
}

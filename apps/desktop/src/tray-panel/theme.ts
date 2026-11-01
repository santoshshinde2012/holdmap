// The panel's theme: the app's choice (saved in localStorage, which the window shares), or the
// system's when it follows the system, as the menu bar does.
import { prefGetChoice } from "../lib/prefs";
import type { Theme } from "../lib/settings";

export const THEMES = ["system", "light", "dark"] as const satisfies readonly Theme[];

export function savedTheme(): Theme {
  return prefGetChoice("theme", THEMES, "system");
}

export function applyTheme(theme: Theme = savedTheme()): void {
  const dark = typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: dark)").matches;
  document.documentElement.dataset.theme = theme === "system" ? (dark ? "dark" : "light") : theme;
}

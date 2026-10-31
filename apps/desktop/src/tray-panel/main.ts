// The tray popup panel's page (tray.html): its own small bundle, separate from the app. In the
// app it reads `tray_panel` and runs `tray_action`, the only commands this page may call; in a
// browser it shows sample data.
import { mount } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import "../fonts.css";
import "../app.css";
import TrayPanel from "./TrayPanel.svelte";
import { prefGetChoice } from "../lib/prefs";
import { demoPanel, type PanelData } from "../lib/tray-panel";

const inApp = "__TAURI_INTERNALS__" in window;

// The app's theme choice, or the system's when it follows the system (as the menu bar does).
const media = matchMedia("(prefers-color-scheme: dark)");
function applyTheme() {
  const theme = prefGetChoice("theme", ["system", "light", "dark"], "system");
  document.documentElement.dataset.theme = theme === "system" ? (media.matches ? "dark" : "light") : theme;
}
applyTheme();
media.addEventListener("change", applyTheme);
addEventListener("storage", applyTheme);

mount(TrayPanel, {
  target: document.getElementById("app")!,
  props: inApp
    ? { load: () => invoke<PanelData>("tray_panel"), run: (action: string) => invoke<void>("tray_action", { id: action }) }
    : { load: async () => demoPanel(), run: () => {} },
});

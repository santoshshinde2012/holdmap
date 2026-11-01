// The tray popup panel's page (tray.html): its own small bundle, separate from the app. In the
// app it reads `tray_panel` and runs `tray_action`, the only commands this page may call; in a
// browser it shows sample data, and its own controls change that data.
import { mount } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import "../fonts.css";
import "../app.css";
import TrayPanel from "./TrayPanel.svelte";
import { applyTheme } from "./theme";
import { demoAction, demoPanel, type PanelData } from "../lib/tray-panel";

const inApp = "__TAURI_INTERNALS__" in window;

// Follow the system, and the window's theme when it changes there (on the next open, too).
applyTheme();
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => applyTheme());
addEventListener("storage", () => applyTheme());
addEventListener("focus", () => applyTheme());

let demo = demoPanel();
mount(TrayPanel, {
  target: document.getElementById("app")!,
  props: inApp
    ? { load: () => invoke<PanelData>("tray_panel"), run: (action: string) => invoke<void>("tray_action", { id: action }) }
    : { load: async () => demo, run: (action: string) => { demo = demoAction(demo, action); } },
});

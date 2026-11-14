// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import SettingsDialog from "./SettingsDialog.svelte";
import type { SettingsActions, SettingsModel } from "../lib/settings";

beforeAll(() => {
  window.matchMedia ??= ((media: string) => ({ matches: true, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof matchMedia;
  Element.prototype.animate ??= vi.fn(() => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} })) as unknown as typeof Element.prototype.animate;
  Element.prototype.scrollIntoView ??= vi.fn();
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const model: SettingsModel = {
  theme: "dark", density: "comfortable", singleKeyShortcuts: true,
  config: { pins: [], notify: true, notify_dev_only: true, history_limit: 100, scan_interval_secs: 4, hotkey: "off", recent_hosts: [], tray_items: ["agents", "needs"], tray_combined: false, tray_compact: false, show_in_dock: true },
  autostart: false, shortcut: null, hotkeys: [], version: "0.12.1", platform: "macos", configDir: null,
  powerAvailable: true, powerDemo: false, tray: true,
};
const actions = (over: Partial<SettingsActions> = {}): SettingsActions => ({
  setTheme() {}, setDensity() {}, setSingleKeyShortcuts() {}, async setAutostart() {}, async setHotkey() {},
  async setNotify() {}, async setScanInterval() {}, async setHistoryLimit() {}, async clearHistory() {},
  copy: vi.fn(), shutdown() {},
  setTrayItems: vi.fn(async () => {}), setTrayCombined: vi.fn(async () => {}), setTrayCompact: vi.fn(async () => {}), setShowInDock: vi.fn(async () => {}),
  ...over,
});
const mount = (override: Partial<SettingsModel> = {}, a = actions(), section: "appearance" | "general" = "appearance") =>
  render(SettingsDialog, { model: { ...model, ...override }, actions: a, section, onclose() {} });

describe("Menu bar / tray widget setting", () => {
  it("ticks the chosen items and saves a change in menu-bar order", async () => {
    const a = actions();
    const view = mount({}, a);
    expect(view.getByRole("heading", { name: "Menu bar" })).toBeTruthy();
    expect(view.getByRole("group", { name: "Menu bar shows" })).toBeTruthy();
    expect(view.getByRole("checkbox", { name: /^Agents/ })).toHaveProperty("checked", true);
    expect(view.getByRole("checkbox", { name: /^Alerts/ })).toHaveProperty("checked", true);
    expect(view.getByRole("checkbox", { name: /^CPU history/ })).toHaveProperty("checked", false);
    await fireEvent.click(view.getByRole("checkbox", { name: /^CPU history/ }));
    await waitFor(() => expect(a.setTrayItems).toHaveBeenCalledWith(["agents", "needs", "cpu_history"]));
    await fireEvent.click(view.getByRole("switch", { name: "As one item" }));
    await waitFor(() => expect(a.setTrayCombined).toHaveBeenCalledWith(true));
  });

  it("is compact by default: the icon and two readings, with every reading one switch away", async () => {
    const a = actions();
    const view = mount({ config: { ...model.config, tray_compact: undefined } }, a);
    const compact = view.getByRole("switch", { name: "Compact menu bar" });
    expect(compact.getAttribute("aria-checked")).toBe("true");
    expect(view.queryByRole("switch", { name: "As one item" })).toBeNull();
    expect(view.getByRole("checkbox", { name: /^Agent RAM/ })).toHaveProperty("checked", false);
    await fireEvent.click(compact);
    await waitFor(() => expect(a.setTrayCompact).toHaveBeenCalledWith(false));
  });

  it("restores the saved items when the write is rejected", async () => {
    const a = actions({ setTrayItems: vi.fn().mockRejectedValue(new Error("Settings are read-only")) });
    const view = mount({}, a);
    await fireEvent.click(view.getByRole("checkbox", { name: /^Agent CPU/ }));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Settings are read-only"));
    expect(view.getByRole("checkbox", { name: /^Agent CPU/ })).toHaveProperty("checked", false);
  });

  it("offers text items only on Linux, and nothing where the tray holds no text", () => {
    const linux = mount({ platform: "linux" });
    expect(linux.getByRole("heading", { name: "Panel" })).toBeTruthy();
    expect(linux.queryByRole("checkbox", { name: /^holdmap icon/ })).toBeNull();
    expect(linux.queryByRole("checkbox", { name: /^Agent bars/ })).toBeNull();
    expect(linux.getByRole("checkbox", { name: /^Agents/ })).toBeTruthy();
    expect(linux.queryByRole("switch", { name: "As one item" })).toBeNull();
    cleanup();
    for (const override of [{ platform: "windows" }, { platform: "linux", tray: false }]) {
      const view = mount(override);
      expect(view.queryByRole("group", { name: /shows$/ })).toBeNull();
      cleanup();
    }
  });

  it("names the items macOS hides and says how to make room", () => {
    const hidden = mount({ trayOnScreen: false, trayHidden: ["Agents", "Needs you"] });
    expect(hidden.getByText("macOS is hiding Agents, Needs you in the menu bar")).toBeTruthy();
    expect(hidden.getByText(/The menu bar is full/).textContent).toContain("camera notch");
    cleanup();
    for (const trayOnScreen of [true, null, undefined]) {
      const view = mount({ trayOnScreen });
      expect(view.queryByText(/macOS is hiding/)).toBeNull();
      cleanup();
    }
  });

  it("can run holdmap from the menu bar only, on macOS", async () => {
    const a = actions();
    const view = mount({}, a, "general");
    await fireEvent.click(view.getByRole("switch", { name: "Show in the Dock" }));
    await waitFor(() => expect(a.setShowInDock).toHaveBeenCalledWith(false));
    cleanup();
    expect(mount({ platform: "windows" }, actions(), "general").queryByRole("switch", { name: "Show in the Dock" })).toBeNull();
  });
});

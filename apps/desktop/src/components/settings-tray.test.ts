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
  config: { pins: [], notify: true, notify_dev_only: true, history_limit: 100, scan_interval_secs: 4, hotkey: "off", recent_hosts: [], tray_style: "count" },
  autostart: false, shortcut: null, hotkeys: [], version: "0.12.1", platform: "macos", configDir: null,
  powerAvailable: true, powerDemo: false, tray: true,
};
const actions = (setTrayStyle: SettingsActions["setTrayStyle"] = vi.fn(async () => {})): SettingsActions => ({
  setTheme() {}, setDensity() {}, setSingleKeyShortcuts() {}, async setAutostart() {}, async setHotkey() {},
  async setNotify() {}, async setScanInterval() {}, async setHistoryLimit() {}, async clearHistory() {},
  copy: vi.fn(), shutdown() {}, setTrayStyle,
});
const mount = (override: Partial<SettingsModel> = {}, a = actions()) =>
  render(SettingsDialog, { model: { ...model, ...override }, actions: a, section: "appearance", onclose() {} });

describe("Menu bar / tray widget setting", () => {
  it("saves what sits beside the icon", async () => {
    const setTrayStyle = vi.fn(async () => {});
    const view = mount({}, actions(setTrayStyle));
    expect(view.getByRole("heading", { name: "Menu bar" })).toBeTruthy();
    await fireEvent.click(view.getByRole("combobox", { name: "Beside the icon: Counts" }));
    await fireEvent.click(view.getByRole("option", { name: "Counts and agent CPU trend" }));
    await waitFor(() => expect(setTrayStyle).toHaveBeenCalledWith("trend"));
  });

  it("restores the saved style when the write is rejected", async () => {
    const setTrayStyle = vi.fn().mockRejectedValue(new Error("Settings are read-only"));
    const view = mount({}, actions(setTrayStyle));
    await fireEvent.click(view.getByRole("combobox", { name: "Beside the icon: Counts" }));
    await fireEvent.click(view.getByRole("option", { name: "Icon only" }));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Settings are read-only"));
    expect(view.getByRole("combobox", { name: "Beside the icon: Counts" })).toBeTruthy();
  });

  it("calls it the panel on Linux and leaves it out where nothing can sit beside the icon", () => {
    const linux = mount({ platform: "linux" });
    expect(linux.getByRole("heading", { name: "Panel" })).toBeTruthy();
    expect(linux.getByText(/as a panel label where your desktop shows one/)).toBeTruthy();
    cleanup();
    for (const override of [{ platform: "windows" }, { platform: "linux", tray: false }]) {
      const view = mount(override);
      expect(view.queryByRole("combobox", { name: /Beside the icon/ })).toBeNull();
      cleanup();
    }
  });
});

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
  config: { pins: [], notify: true, notify_dev_only: true, history_limit: 100, scan_interval_secs: 4, hotkey: "off", recent_hosts: [] },
  autostart: false, shortcut: null, hotkeys: [], version: "0.7.0", platform: "macos",
  updater: false, appPath: "/Volumes/holdmap 1/holdmap.app", architecture: "aarch64",
  configDir: "/Users/test/.config/holdmap", powerAvailable: true, powerDemo: false,
};
const actions = (): SettingsActions => ({
  setTheme() {}, setDensity() {}, setSingleKeyShortcuts() {}, async setAutostart() {}, async setHotkey() {},
  async setNotify() {}, async setScanInterval() {}, async setHistoryLimit() {}, async clearHistory() {},
  copy: vi.fn(), shutdown() {}, openDesktopHelp: vi.fn(async () => {}),
});
const mount = (override: Partial<SettingsModel> = {}, a = actions()) => render(SettingsDialog, { model: { ...model, ...override }, actions: a, section: "about", onclose() {} });

describe("About and desktop update recovery", () => {
  it("identifies the actual running copy and offers explicit manual installation help", async () => {
    const a = actions(); const view = mount({}, a);
    expect(view.getByText("0.7.0")).toBeTruthy();
    expect(view.getByText(model.appPath!)).toBeTruthy();
    expect(view.getByText("Apple silicon (ARM64)")).toBeTruthy();
    expect(view.getByText(/quit Holdmap completely/)).toBeTruthy();
    expect(view.getByText(/Closing the window can leave Holdmap running in the tray/)).toBeTruthy();
    expect(view.getByText(/Updating the CLI does not update this desktop app/)).toBeTruthy();
    await fireEvent.click(view.getByRole("button", { name: "Copy app location" }));
    expect(a.copy).toHaveBeenCalledWith(model.appPath, "App location");
    await fireEvent.click(view.getByRole("button", { name: "Copy architecture" }));
    expect(a.copy).toHaveBeenCalledWith("aarch64", "Architecture");
    expect(a.openDesktopHelp).not.toHaveBeenCalled();
    await fireEvent.click(view.getByRole("button", { name: "Download desktop app" }));
    await fireEvent.click(view.getByRole("button", { name: "Installation guide" }));
    expect(a.openDesktopHelp).toHaveBeenNthCalledWith(1, "downloads");
    expect(a.openDesktopHelp).toHaveBeenNthCalledWith(2, "installation");
  });

  it.each([
    ["macos", "x86_64", "Intel (x64)"],
    ["windows", "x86_64", "x64 (x86_64)"],
    ["linux", "aarch64", "ARM64 (aarch64)"],
    ["linux", "riscv64", "riscv64"],
  ])("matches %s download terminology without guessing unknown architectures", (platform, architecture, label) => {
    const view = mount({ platform, architecture });
    expect(view.getByText(label)).toBeTruthy();
  });

  it("preserves signed updater guidance without claiming the app is current", () => {
    const view = mount({ updater: true });
    expect(view.getByText(/checks for signed in-app updates after startup/)).toBeTruthy();
    expect(view.queryByText(/requires manual desktop updates/)).toBeNull();
    expect(view.queryByText(/up to date/i)).toBeNull();
  });

  it.each([undefined, null])("keeps missing updater metadata unknown (%s)", (updater) => {
    const view = mount({ updater, appPath: null, architecture: null });
    expect(view.getByText(/In-app update support is unknown/)).toBeTruthy();
    expect(view.queryByText(/checks for signed in-app updates/)).toBeNull();
    expect(view.queryByText(/requires manual desktop updates/)).toBeNull();
    expect(view.queryByText(/up to date/i)).toBeNull();
    expect(view.queryByRole("button", { name: "Copy app location" })).toBeNull();
  });

  it("labels browser mode honestly and hides desktop metadata even from a stale model", () => {
    const view = mount({ platform: "browser" });
    expect(view.getByText("Browser preview")).toBeTruthy();
    expect(view.getByText("Preview version")).toBeTruthy();
    expect(view.getByText(/separate from any installed desktop app/)).toBeTruthy();
    expect(view.queryByText(model.appPath!)).toBeNull();
    expect(view.queryByText(model.configDir!)).toBeNull();
    expect(view.queryByText("Architecture")).toBeNull();
    expect(view.queryByText(/requires manual desktop updates/)).toBeNull();
    expect(view.queryByText(/sample interface|sample data/i)).toBeNull();
    expect(view.getByRole("button", { name: "Download desktop app" })).toBeTruthy();
  });

  it("bounds duplicate help actions, displays failures literally and permits retry", async () => {
    let reject!: (cause: Error) => void;
    const openDesktopHelp = vi.fn().mockImplementationOnce(() => new Promise<void>((_resolve, fail) => { reject = fail; })).mockResolvedValue(undefined);
    const view = mount({}, { ...actions(), openDesktopHelp });
    const button = view.getByRole("button", { name: "Download desktop app" });
    await fireEvent.click(button); await fireEvent.click(button);
    expect(openDesktopHelp).toHaveBeenCalledOnce();
    expect((button as HTMLButtonElement).disabled).toBe(true);
    reject(new Error('Browser unavailable <img src=x onerror="alert(1)">'));
    await waitFor(() => expect(view.getByRole("alert").textContent).toContain("Browser unavailable"));
    expect(view.container.querySelector("img")).toBeNull();
    await fireEvent.click(view.getByRole("button", { name: "Download desktop app" }));
    expect(openDesktopHelp).toHaveBeenCalledTimes(2);
    expect(view.queryByRole("alert")).toBeNull();
  });
});

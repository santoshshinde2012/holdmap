// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import ShutdownDialog from "../components/ShutdownDialog.svelte";
import SettingsDialog from "../components/SettingsDialog.svelte";
import { shutdownPreviewLifetimeMs, type ShutdownPreview } from "./power";
import type { SettingsActions, SettingsModel } from "./settings";

beforeAll(() => {
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(() => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} }));
});
afterEach(() => { cleanup(); vi.useRealTimers(); });
const preview: ShutdownPreview = { confirmation_id: "fixture-token", platform: "macOS", hostname: "Demo computer", expires_in_secs: 60 };
const props = () => ({ preview, loading: false, error: null, busy: false, requested: false, onconfirm: vi.fn(), onclose: vi.fn(), onretry: vi.fn() });
const action = () => screen.getByRole("button", { name: "Shut down this computer" }) as HTMLButtonElement;

describe("shutdown confirmation", () => {
  it("fails closed for missing, invalid and expired previews", () => {
    expect(shutdownPreviewLifetimeMs(null)).toBe(0);
    for (const expires_in_secs of [0, -1, NaN, Infinity]) expect(shutdownPreviewLifetimeMs({ ...preview, expires_in_secs })).toBe(0);
    expect(shutdownPreviewLifetimeMs({ ...preview, confirmation_id: "" })).toBe(0);
    expect(shutdownPreviewLifetimeMs({ ...preview, expires_in_secs: 600 })).toBe(60_000);
  });

  it("focuses Cancel, requires acknowledgement and sends at most one request", async () => {
    const p = props();
    render(ShutdownDialog, p);
    await tick();
    expect(document.activeElement?.textContent?.trim()).toBe("Cancel");
    expect(action().disabled).toBe(true);
    await fireEvent.click(action());
    expect(p.onconfirm).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("checkbox"));
    expect(action().disabled).toBe(false);
    await fireEvent.keyDown(screen.getByRole("alertdialog"), { key: "Enter" });
    expect(p.onconfirm).not.toHaveBeenCalled();
    await fireEvent.click(action());
    await fireEvent.click(action());
    expect(p.onconfirm).toHaveBeenCalledOnce();
    expect(action().disabled).toBe(true);
  });

  it("Escape cancels instead of confirming, and busy state prevents another request", async () => {
    const p = props();
    const { rerender } = render(ShutdownDialog, p);
    await fireEvent.keyDown(screen.getByRole("alertdialog"), { key: "Escape" });
    expect(p.onclose).toHaveBeenCalledOnce();
    expect(p.onconfirm).not.toHaveBeenCalled();
    await rerender({ ...p, busy: true });
    await fireEvent.keyDown(screen.getByRole("alertdialog"), { key: "Escape" });
    expect(p.onclose).toHaveBeenCalledOnce();
    expect((screen.getByRole("checkbox") as HTMLInputElement).disabled).toBe(true);
  });

  it("expires without submitting and does not renew the same token", async () => {
    vi.useFakeTimers();
    const p = { ...props(), preview: { ...preview, expires_in_secs: 1 } };
    const { rerender } = render(ShutdownDialog, p);
    await tick();
    await fireEvent.click(screen.getByRole("checkbox"));
    await vi.advanceTimersByTimeAsync(600);
    await rerender({ ...p, preview: { ...p.preview } });
    expect((screen.getByRole("checkbox") as HTMLInputElement).checked).toBe(true);
    await vi.advanceTimersByTimeAsync(401);
    expect(action().disabled).toBe(true);
    expect(screen.getByText("Confirmation expired")).toBeTruthy();
    expect(p.onconfirm).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "Review new preview" }));
    expect(p.onretry).toHaveBeenCalledOnce();
  });

  it("shows native errors literally and a fresh token needs acknowledgement again", async () => {
    const p = props();
    const { rerender, container } = render(ShutdownDialog, p);
    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(action());
    await rerender({ ...p, error: 'Permission denied <img src=x onerror="alert(1)">' });
    expect(screen.getByRole("alert").textContent).toContain("Permission denied");
    expect(container.querySelector("img")).toBeNull();
    expect(action().disabled).toBe(true);
    await rerender({ ...p, preview: { ...preview, confirmation_id: "new-token" } });
    expect((screen.getByRole("checkbox") as HTMLInputElement).checked).toBe(false);
    expect(action().disabled).toBe(true);
  });

  it("labels both demo confirmation and completed simulation explicitly", async () => {
    const p = { ...props(), demo: true };
    const { rerender } = render(ShutdownDialog, p);
    expect(screen.getByRole("alertdialog", { name: "Simulate computer shutdown?" })).toBeTruthy();
    expect(screen.getByText(/No operating-system shutdown will be requested/)).toBeTruthy();
    await rerender({ ...p, requested: true });
    expect(screen.getByText("Simulation complete")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Simulate shutdown" })).toBeNull();
  });
});

describe("Power settings", () => {
  const model: SettingsModel = { theme: "light", density: "comfortable", config: { pins: [], notify: false, notify_dev_only: true, history_limit: 200, scan_interval_secs: 4, hotkey: "off", recent_hosts: [] }, autostart: false, shortcut: null, hotkeys: [], version: "0.3.0", platform: "browser", configDir: null, powerAvailable: true, powerDemo: true };
  const actions = (): SettingsActions => ({ setTheme: vi.fn(), setDensity: vi.fn(), setAutostart: vi.fn(async () => {}), setHotkey: vi.fn(async () => {}), setNotify: vi.fn(async () => {}), setScanInterval: vi.fn(async () => {}), setHistoryLimit: vi.fn(async () => {}), clearHistory: vi.fn(async () => {}), copy: vi.fn(), shutdown: vi.fn() });
  it("opens only a simulated review in the browser and explains the local scope", async () => {
    const a = actions();
    render(SettingsDialog, { model, actions: a, section: "power", onclose: vi.fn() });
    expect(screen.getByText(/including while viewing remote hosts/)).toBeTruthy();
    expect(screen.getByText(/This website cannot shut down your computer/)).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Simulate shutdown…" }));
    expect(a.shutdown).toHaveBeenCalledOnce();
  });
  it("disables unsupported desktop shutdown", () => {
    render(SettingsDialog, { model: { ...model, powerAvailable: false, powerDemo: false }, actions: actions(), section: "power", onclose: vi.fn() });
    expect((screen.getByRole("button", { name: "Shut down this computer…" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

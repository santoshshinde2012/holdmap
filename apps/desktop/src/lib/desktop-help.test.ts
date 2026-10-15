// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DesktopHelpDestination } from "./types";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  vi.restoreAllMocks();
  vi.resetAllMocks();
  vi.resetModules();
});

describe("desktop download and installation help", () => {
  it("identifies the browser without invented installation metadata or background requests", async () => {
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    const fetch = vi.spyOn(globalThis, "fetch");
    const api = await import("./api");
    const info = await api.appInfo();
    expect(info.platform).toBe("browser");
    expect(info.version).toMatch(/^\d+\.\d+\.\d+/);
    expect(info.config_dir).toBeUndefined();
    expect(info.app_path).toBeUndefined();
    expect(info.updater).toBeUndefined();
    expect(info.architecture).toBeUndefined();
    expect(open).not.toHaveBeenCalled();
    expect(fetch).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("opens only the two official browser destinations after explicit actions", async () => {
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    const fetch = vi.spyOn(globalThis, "fetch");
    const api = await import("./api");
    await api.openDesktopHelp("downloads");
    await api.openDesktopHelp("installation");
    expect(open.mock.calls).toEqual([
      ["https://santoshshinde2012.github.io/holdmap/#install", "_blank", "noopener,noreferrer"],
      ["https://santoshshinde2012.github.io/holdmap/docs/desktop/#first-open", "_blank", "noopener,noreferrer"],
    ]);
    expect(fetch).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("passes native metadata through and delegates help destinations without accepting URLs", async () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    const info = { version: "0.7.0", platform: "macos", tray: true, updater: false, app_path: "/Volumes/holdmap/holdmap.app", architecture: "aarch64" };
    invoke.mockResolvedValueOnce(info).mockResolvedValue(undefined);
    const api = await import("./api");
    expect(await api.appInfo()).toEqual(info);
    await api.openDesktopHelp("downloads");
    await api.openDesktopHelp("installation");
    expect(invoke.mock.calls).toEqual([
      ["app_info", undefined],
      ["open_desktop_help", { destination: "downloads" }],
      ["open_desktop_help", { destination: "installation" }],
    ]);
    expect(open).not.toHaveBeenCalled();
  });

  it.each([false, true])("rejects invalid destinations before any effect (native=%s)", async (native) => {
    if (native) Object.assign(window, { __TAURI_INTERNALS__: {} });
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    const api = await import("./api");
    for (const destination of ["https://example.com/", "__proto__", ""]) {
      await expect(api.openDesktopHelp(destination as DesktopHelpDestination)).rejects.toThrow("Choose desktop downloads");
    }
    expect(open).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });
});

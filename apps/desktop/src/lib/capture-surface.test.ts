// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { checkOwnSurface, ownWindowDisplayOptions, refuseWiderSurface } from "./capture-surface";
import { requestDisplayStream } from "./screen-record";

const NAMES = ["Holdmap", "holdmap"];
const track = (displaySurface: string | undefined, label = "") => ({
  kind: "video", label, readyState: "live", getSettings: () => (displaySurface === undefined ? {} : { displaySurface }), stop: vi.fn(),
}) as unknown as MediaStreamTrack & { stop: ReturnType<typeof vi.fn> };
const streamOf = (...tracks: ReturnType<typeof track>[]) =>
  ({ getTracks: () => tracks, getVideoTracks: () => tracks }) as unknown as MediaStream;

afterEach(() => vi.restoreAllMocks());

describe("own-window surface check", () => {
  it("accepts a browser tab", () => {
    expect(checkOwnSurface(track("browser", "Holdmap"), NAMES)).toEqual({ ok: true, surface: "browser" });
  });

  it("refuses a whole screen, whatever its label says", () => {
    const verdict = checkOwnSurface(track("monitor", "Holdmap"), NAMES);
    expect(verdict.ok).toBe(false);
    expect(verdict.ok ? "" : verdict.message).toMatch(/whole screen/);
  });

  it("accepts a window only when its title is Holdmap's own", () => {
    expect(checkOwnSurface(track("window", "Holdmap"), NAMES).ok).toBe(true);
    expect(checkOwnSurface(track("window", "holdmap - Ports"), NAMES).ok).toBe(true);
    for (const label of ["", "Terminal", "Slack", "Holdmapper notes", "notes about holdmap"]) {
      const verdict = checkOwnSurface(track("window", label), NAMES);
      expect(verdict.ok, label).toBe(false);
      expect(verdict.ok ? "" : verdict.message).toMatch(/different window/);
    }
  });

  it("refuses a source it cannot identify, and a missing track", () => {
    for (const unknown of [track(undefined, "Holdmap"), track("application", "Holdmap")]) {
      const verdict = checkOwnSurface(unknown, NAMES);
      expect(verdict.ok).toBe(false);
      expect(verdict.ok ? "" : verdict.message).toMatch(/can't confirm/);
    }
    expect(checkOwnSurface(undefined, NAMES).ok).toBe(false);
  });

  it("stops every track of a refused stream and keeps an accepted one running", () => {
    const screen = track("monitor");
    expect(refuseWiderSurface(streamOf(screen), NAMES)).toMatch(/whole screen/);
    expect(screen.stop).toHaveBeenCalledOnce();
    const tab = track("browser");
    expect(refuseWiderSurface(streamOf(tab), NAMES)).toBeNull();
    expect(tab.stop).not.toHaveBeenCalled();
    expect(refuseWiderSurface(streamOf(), NAMES)).toMatch(/No picture was shared/);
  });

  it("asks the browser for this tab only, without audio or screens", () => {
    expect(ownWindowDisplayOptions({ width: 1920, height: 1080 })).toEqual({
      video: { displaySurface: "browser", frameRate: 30, width: { max: 1920 }, height: { max: 1080 } },
      audio: false, preferCurrentTab: true, selfBrowserSurface: "include", surfaceSwitching: "exclude",
      monitorTypeSurfaces: "exclude", systemAudio: "exclude",
    });
  });

  it("requestDisplayStream stops and rejects a screen the user picked anyway", async () => {
    const screen = track("monitor", "Screen 1");
    const getDisplayMedia = vi.fn().mockResolvedValue(streamOf(screen));
    Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: { getDisplayMedia } });
    await expect(requestDisplayStream()).rejects.toThrow(/records only its own window/);
    expect(screen.stop).toHaveBeenCalledOnce();
    expect(getDisplayMedia).toHaveBeenCalledWith(expect.objectContaining({ preferCurrentTab: true, systemAudio: "exclude" }));
    const tab = track("browser", "Holdmap");
    getDisplayMedia.mockResolvedValue(streamOf(tab));
    await expect(requestDisplayStream()).resolves.toBeTruthy();
    expect(tab.stop).not.toHaveBeenCalled();
  });
});

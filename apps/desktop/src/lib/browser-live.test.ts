import { afterEach, describe, expect, it, vi } from "vitest";
import { liveJson, useLiveCollector, wantsSampleData } from "./browser-live";

afterEach(() => { vi.unstubAllGlobals(); });

describe("live collector gating", () => {
  it("uses the holdmap CLI during vite serve, not in Tauri, tests, or ?demo=1", () => {
    expect(useLiveCollector({ tauri: true, dev: true, vitest: false, search: "" })).toBe(false);
    expect(useLiveCollector({ tauri: false, dev: true, vitest: true, search: "" })).toBe(false);
    expect(useLiveCollector({ tauri: false, dev: false, vitest: false, search: "" })).toBe(false);
    expect(useLiveCollector({ tauri: false, dev: true, vitest: false, search: "?demo=1" })).toBe(false);
    expect(useLiveCollector({ tauri: false, dev: true, vitest: false, search: "?demo" })).toBe(false);
    expect(useLiveCollector({ tauri: false, dev: true, vitest: false, search: "" })).toBe(true);
    expect(useLiveCollector({ tauri: false, dev: true, vitest: false, search: "?demo=0" })).toBe(true);
  });

  it("treats only an explicit demo flag as sample data", () => {
    expect(wantsSampleData("")).toBe(false);
    expect(wantsSampleData("?theme=dark")).toBe(false);
    expect(wantsSampleData("?demo")).toBe(true);
    expect(wantsSampleData("?demo=false")).toBe(false);
  });
});

describe("liveJson", () => {
  it("requests an allowlisted route and returns JSON", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ agents: [] }), { status: 200, headers: { "Content-Type": "application/json" } })));
    await expect(liveJson("agents")).resolves.toEqual({ agents: [] });
    expect(fetch).toHaveBeenCalledWith("/__holdmap/agents");
  });

  it("passes query flags and surfaces collector errors", async () => {
    vi.stubGlobal("fetch", vi.fn(async (url: string) => {
      expect(url).toBe("/__holdmap/scan?all=1");
      return new Response(JSON.stringify({ error: "holdmap CLI not found" }), { status: 503, headers: { "Content-Type": "application/json" } });
    }));
    await expect(liveJson("scan", { all: true, docker: false })).rejects.toThrow("holdmap CLI not found");
  });
});

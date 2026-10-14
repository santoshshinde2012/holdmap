import { afterEach, describe, expect, it, vi } from "vitest";
import { agents, MAX_MOCK_PORTS, scan } from "./api";
import { MOCK_SNAPSHOT } from "./mock";

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("bounded browser load fixtures", () => {
  it.each(["Infinity", "NaN", "-1", "-Infinity"])("ignores invalid mockPorts=%s", async (count) => {
    vi.useFakeTimers();
    vi.stubGlobal("location", { search: `?mockPorts=${count}` });
    const pending = scan(false);
    await vi.advanceTimersByTimeAsync(250);
    expect((await pending).entries).toHaveLength(MOCK_SNAPSHOT.entries.length);
  });

  it.each(["1000000000", "1e100"])("caps mockPorts=%s before allocating entries", async (count) => {
    vi.useFakeTimers();
    vi.stubGlobal("location", { search: `?mockPorts=${count}` });
    const pending = scan(false);
    await vi.advanceTimersByTimeAsync(250);
    const snapshot = await pending;
    expect(snapshot.entries).toHaveLength(MAX_MOCK_PORTS);
    expect(new Set(snapshot.entries.map((entry) => entry.id)).size).toBe(MAX_MOCK_PORTS);
  });

  it("retains supported finite load-test sizes", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("location", { search: "?mockPorts=100.5" });
    const pending = scan(false);
    await vi.advanceTimersByTimeAsync(250);
    expect((await pending).entries).toHaveLength(100);
  });

  it("keeps sample agents in unit tests instead of calling a live collector", async () => {
    const report = await agents();
    expect(report.agents.map((agent) => agent.name)).toEqual(["Claude Code", "Cursor", "Docker Desktop"]);
    expect(report.agents.some((agent) => agent.folders.some((folder) => folder.label === "shop-web"))).toBe(true);
  });
});

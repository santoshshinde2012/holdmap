import { describe, expect, it, vi } from "vitest";
import { createAgentReportLoader } from "./agents-live";
import type { AgentsReport } from "./types";

const report = (taken_at_ms: number): AgentsReport => ({ agents: [], platform: "linux", taken_at_ms, limits: [] });

describe("agent report polling", () => {
  it("coalesces overlapping requests and publishes once", async () => {
    let finish!: (r: AgentsReport) => void;
    const source = vi.fn(() => new Promise<AgentsReport>((resolve) => { finish = resolve; }));
    const publish = vi.fn();
    const load = createAgentReportLoader(source, () => null, publish);
    const first = load(), second = load();
    expect(second).toBe(first);
    expect(source).toHaveBeenCalledTimes(1);
    finish(report(100));
    await Promise.all([first, second]);
    expect(publish).toHaveBeenCalledExactlyOnceWith(report(100));
  });

  it("keeps a newer report when a source returns an older snapshot", async () => {
    let current = report(200);
    const publish = vi.fn((r: AgentsReport) => { current = r; });
    const source = vi.fn().mockResolvedValueOnce(report(100)).mockResolvedValueOnce(report(300));
    const load = createAgentReportLoader(source, () => current, publish);
    await load();
    expect(publish).not.toHaveBeenCalled();
    await load();
    expect(current.taken_at_ms).toBe(300);
  });

  it("compares freshness at completion and allows a retry after failure", async () => {
    let current = report(100), finish!: (r: AgentsReport) => void;
    const publish = vi.fn();
    const source = vi.fn().mockImplementationOnce(() => new Promise<AgentsReport>((resolve) => { finish = resolve; })).mockRejectedValueOnce(new Error("scan failed")).mockResolvedValueOnce(report(400));
    const load = createAgentReportLoader(source, () => current, publish);
    const pending = load();
    current = report(300);
    finish(report(200));
    await pending;
    expect(publish).not.toHaveBeenCalled();
    await expect(load()).rejects.toThrow("scan failed");
    await load();
    expect(publish).toHaveBeenCalledExactlyOnceWith(report(400));
  });
});

import { describe, expect, it, vi } from "vitest";
import { ShutdownFlow, type ShutdownPort } from "./shutdown-flow.svelte";

const preview = { confirmation_id: "c1" } as never;
const port = (over: Partial<ShutdownPort> = {}): ShutdownPort => ({
  shutdownPreview: vi.fn(async () => preview),
  shutdownMachine: vi.fn(async () => ({ requested: true })),
  scheduleShutdown: vi.fn(async () => ({ id: "s1" }) as never),
  ...over,
});

describe("ShutdownFlow", () => {
  it("previews, then confirms once with the reviewed confirmation id", async () => {
    const p = port();
    const flow = new ShutdownFlow(p);
    await flow.open("now", "settings");
    expect(flow.visible && flow.preview).toBeTruthy();
    await flow.confirm();
    await flow.confirm();
    expect(p.shutdownMachine).toHaveBeenCalledTimes(1);
    expect(p.shutdownMachine).toHaveBeenCalledWith("c1");
    expect(flow.requested).toBe(true);
  });

  it("drops the consumed preview after a failure so a retry needs a fresh review", async () => {
    const flow = new ShutdownFlow(port({ shutdownMachine: vi.fn(async () => ({ requested: false })) }));
    await flow.open("now", "settings");
    await flow.confirm();
    expect(flow.error).toContain("did not accept");
    expect(flow.preview).toBeNull();
  });

  it("schedules only with the schedule intent and reports back", async () => {
    const p = port();
    const onScheduled = vi.fn(async () => {});
    const flow = new ShutdownFlow(p, onScheduled);
    await flow.open("now", "schedules");
    await flow.schedule(60);
    expect(p.scheduleShutdown).not.toHaveBeenCalled();
    await flow.open("schedule", "schedules");
    await flow.schedule(60);
    expect(p.scheduleShutdown).toHaveBeenCalledWith("c1", 60);
    expect(onScheduled).toHaveBeenCalled();
    expect(flow.close()).toBe("schedules");
    expect(flow.visible).toBe(false);
  });

  it("ignores a preview that arrives after closing", async () => {
    let resolve!: (v: never) => void;
    const flow = new ShutdownFlow(port({ shutdownPreview: () => new Promise((r) => (resolve = r)) }));
    const opening = flow.open("now", "settings");
    flow.close();
    resolve(preview);
    await opening;
    expect(flow.preview).toBeNull();
  });
});

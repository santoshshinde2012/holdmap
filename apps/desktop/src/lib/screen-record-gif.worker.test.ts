import { afterEach, describe, expect, it, vi } from "vitest";

class WorkerScope {
  onmessage: ((event: MessageEvent<unknown>) => void) | null = null;
  postMessage = vi.fn();
}

afterEach(() => { vi.unstubAllGlobals(); vi.resetModules(); });

async function worker() {
  const scope = new WorkerScope();
  vi.stubGlobal("DedicatedWorkerGlobalScope", WorkerScope);
  vi.stubGlobal("self", scope);
  await import("./screen-record-gif.worker");
  const send = (data: unknown, envelope: Partial<MessageEvent<unknown>> = {}) => scope.onmessage!({
    data, origin: "", source: null, ports: [], isTrusted: true, ...envelope,
  } as unknown as MessageEvent<unknown>);
  return { scope, send };
}

function frame() {
  return { type: "frame", at: 0, frame: { width: 1, height: 1, data: new Uint8ClampedArray([255, 0, 0, 255]) } };
}

describe("dedicated GIF worker boundary", () => {
  it("processes the owner's implicit-port frames and returns actual GIF bytes", async () => {
    const { scope, send } = await worker();
    send(frame());
    expect(scope.postMessage).toHaveBeenCalledWith({ type: "frame", intervalMs: 125, bytes: expect.any(Number), full: false });
    send({ type: "finish", durationMs: 125 });
    const [result, options] = scope.postMessage.mock.calls[1];
    expect(result.type).toBe("done");
    expect(String.fromCharCode(...result.bytes.slice(0, 6))).toBe("GIF89a");
    expect(options.transfer).toEqual([result.bytes.buffer]);
  });

  it("applies the limits it is initialised with", async () => {
    const { scope, send } = await worker();
    send({ type: "init", limits: { fps: 4, maxBytes: 5_000_000, maxDurationMs: 30_000 } });
    send(frame());
    expect(scope.postMessage).toHaveBeenCalledWith({ type: "frame", intervalMs: 250, bytes: expect.any(Number), full: false });
  });

  it("ignores window-origin, foreign-source, transferred-port and synthetic events", async () => {
    const { scope, send } = await worker();
    for (const envelope of [
      { origin: "https://example.com" }, { origin: "null" },
      { source: {} as MessagePort }, { ports: [{} as MessagePort] }, { isTrusted: false },
    ]) send(frame(), envelope);
    expect(scope.postMessage).not.toHaveBeenCalled();
    send({ type: "finish", durationMs: 125 });
    expect(scope.postMessage).toHaveBeenCalledWith({ type: "error", message: "Nothing was captured." });
  });

  it("rejects malformed or unbounded requests before encoding", async () => {
    const { scope, send } = await worker();
    for (const invalid of [
      null, "frame", {}, { type: "other" }, { type: "finish", durationMs: Infinity },
      { type: "finish", durationMs: 3_600_001 }, { ...frame(), at: -1 },
      { ...frame(), frame: { width: 961, height: 1, data: new Uint8ClampedArray(961 * 4) } },
      { ...frame(), frame: { width: 1, height: 1, data: [255, 0, 0, 255] } },
      { ...frame(), frame: { width: 2, height: 1, data: new Uint8ClampedArray(4) } },
      { type: "init", limits: { fps: 0, maxBytes: 1_000, maxDurationMs: 1_000 } },
      { type: "init", limits: { fps: 8, maxBytes: 1e12, maxDurationMs: 1_000 } },
      { type: "init", limits: null },
    ]) {
      send(invalid);
      expect(scope.postMessage).toHaveBeenLastCalledWith({ type: "error", message: "Invalid GIF recording request." });
    }
    send(frame());
    expect(scope.postMessage).toHaveBeenLastCalledWith({ type: "frame", intervalMs: 125, bytes: expect.any(Number), full: false });
  });

  it("cannot register a Window or shared-worker message handler", async () => {
    vi.stubGlobal("DedicatedWorkerGlobalScope", WorkerScope);
    const windowLike = { onmessage: null };
    vi.stubGlobal("self", windowLike);
    await expect(import("./screen-record-gif.worker")).rejects.toThrow(/dedicated worker/);
    expect(windowLike.onmessage).toBeNull();
    vi.resetModules();
    vi.stubGlobal("DedicatedWorkerGlobalScope", undefined);
    await expect(import("./screen-record-gif.worker")).rejects.toThrow(/dedicated worker/);
    expect(windowLike.onmessage).toBeNull();
  });
});

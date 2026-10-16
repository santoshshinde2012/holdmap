// @vitest-environment jsdom
import { Blob as NodeBlob } from "node:buffer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { captureNativeSnapshot } from "./screen-snapshot-native";
import { MAX_SNAPSHOT_BYTES } from "./screen-snapshot";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const bytes = () => Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg=="), (c) => c.charCodeAt(0)).buffer;
const status = (patch = {}) => ({ snapshotId: id, phase: "ready", width: 1, height: 1, pngBytes: bytes().byteLength, error: null, ...patch });
beforeEach(() => {
  vi.stubGlobal("Blob", NodeBlob);
  invoke.mockImplementation(async (command) => command === "read_screen_snapshot" ? bytes() : command === "discard_screen_snapshot" ? undefined : status());
});
afterEach(() => { invoke.mockReset(); vi.unstubAllGlobals(); vi.useRealTimers(); });

describe("native PNG snapshot ownership", () => {
  it("uses opaque snapshot handles, reads exact PNG bytes and discards only the snapshot", async () => {
    const result = await captureNativeSnapshot(new AbortController().signal, "recording-session", 800);
    expect(result).toMatchObject({ source: "recording", width: 1, height: 1, recordingElapsedMs: 800 });
    expect(invoke).toHaveBeenCalledWith("start_screen_snapshot", { sessionId: "recording-session" });
    expect(invoke).toHaveBeenCalledWith("read_screen_snapshot", { snapshotId: id });
    expect(invoke.mock.calls.at(-1)).toEqual(["discard_screen_snapshot", { snapshotId: id }]);
    expect(invoke.mock.calls.every(([name]) => !String(name).includes("screen_recording"))).toBe(true);
  });

  it("polls a standalone chooser and releases every completed snapshot", async () => {
    vi.useFakeTimers();
    invoke.mockImplementation(async (command) => command === "start_screen_snapshot" ? status({ phase: "choosing", width: 0, height: 0, pngBytes: 0 }) : command === "read_screen_snapshot" ? bytes() : command === "screen_snapshot_status" ? status() : undefined);
    const pending = captureNativeSnapshot(new AbortController().signal);
    await vi.advanceTimersByTimeAsync(150);
    expect(await pending).toMatchObject({ source: "screen" });
    expect(invoke).toHaveBeenCalledWith("start_screen_snapshot", {});
    expect(vi.getTimerCount()).toBe(0);
  });

  it("discards a late acknowledged chooser after cancellation", async () => {
    let resolve!: (value: ReturnType<typeof status>) => void;
    invoke.mockImplementation((command) => command === "start_screen_snapshot" ? new Promise((done) => { resolve = done; }) : Promise.resolve());
    const controller = new AbortController();
    const pending = captureNativeSnapshot(controller.signal);
    const rejected = expect(pending).rejects.toMatchObject({ name: "AbortError" });
    controller.abort(); await rejected;
    resolve(status({ phase: "choosing" })); await Promise.resolve(); await Promise.resolve();
    expect(invoke).toHaveBeenCalledWith("discard_screen_snapshot", { snapshotId: id });
    expect(invoke.mock.calls.filter(([name]) => name === "discard_screen_snapshot")).toHaveLength(1);
  });

  it("cancels chooser polling without leaving timer work", async () => {
    vi.useFakeTimers();
    invoke.mockImplementation(async (command) => command === "discard_screen_snapshot" ? undefined : status({ phase: "choosing", width: 0, height: 0, pngBytes: 0 }));
    const controller = new AbortController();
    const pending = captureNativeSnapshot(controller.signal);
    const rejected = expect(pending).rejects.toMatchObject({ name: "AbortError" });
    await vi.advanceTimersByTimeAsync(0); controller.abort(); await rejected;
    expect(vi.getTimerCount()).toBe(0);
    expect(invoke).toHaveBeenCalledWith("discard_screen_snapshot", { snapshotId: id });
  });

  it.each([{ width: 3841 }, { pngBytes: MAX_SNAPSHOT_BYTES + 1 }, { phase: "surprise" }, { width: -1 }])("rejects malformed status %j while retaining cleanup ownership", async (patch) => {
    invoke.mockImplementation(async (command) => command === "start_screen_snapshot" ? status(patch) : undefined);
    await expect(captureNativeSnapshot(new AbortController().signal)).rejects.toThrow(/status/);
    expect(invoke).toHaveBeenCalledWith("discard_screen_snapshot", { snapshotId: id });
    expect(invoke).not.toHaveBeenCalledWith("read_screen_snapshot", expect.anything());
  });

  it("rejects a substituted handle and truncated export", async () => {
    vi.useFakeTimers();
    invoke.mockImplementation(async (command) => command === "start_screen_snapshot" ? status({ phase: "choosing" }) : command === "screen_snapshot_status" ? status({ snapshotId: "ffffffff-bbbb-cccc-dddd-eeeeeeeeeeee" }) : undefined);
    const pending = captureNativeSnapshot(new AbortController().signal);
    const rejected = expect(pending).rejects.toThrow(/status/);
    await vi.advanceTimersByTimeAsync(150); await rejected;
    expect(invoke).toHaveBeenCalledWith("discard_screen_snapshot", { snapshotId: id });
    invoke.mockImplementation(async (command) => command === "start_screen_snapshot" ? status() : command === "read_screen_snapshot" ? new ArrayBuffer(4) : undefined);
    await expect(captureNativeSnapshot(new AbortController().signal)).rejects.toThrow(/complete native/);
  });
});

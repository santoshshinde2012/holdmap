import { afterEach, describe, expect, it, vi } from "vitest";
import { createCopy } from "./use-copy.svelte";

afterEach(() => { vi.useRealTimers(); });

describe("createCopy", () => {
  it("remembers the copied text, then forgets it after the hold time", async () => {
    vi.useFakeTimers();
    const writeText = vi.fn(async () => {});
    const state = createCopy(1000, () => ({ writeText }) as unknown as Clipboard);
    expect(await state.copy("/a")).toBe(true);
    expect(state.copied).toBe("/a");
    vi.advanceTimersByTime(999);
    expect(state.copied).toBe("/a");
    vi.advanceTimersByTime(1);
    expect(state.copied).toBeNull();
  });

  it("restarts the timer when another path is copied and clears it on dispose", async () => {
    vi.useFakeTimers();
    const state = createCopy(1000, () => ({ writeText: async () => {} }) as unknown as Clipboard);
    await state.copy("/a");
    vi.advanceTimersByTime(800);
    await state.copy("/b");
    vi.advanceTimersByTime(800);
    expect(state.copied).toBe("/b");
    state.dispose();
    vi.advanceTimersByTime(1000);
    expect(state.copied).toBe("/b");
  });

  it("reports a refused or missing clipboard without throwing", async () => {
    const refused = createCopy(1000, () => ({ writeText: async () => { throw new Error("denied"); } }) as unknown as Clipboard);
    expect(await refused.copy("/a")).toBe(false);
    expect(refused.copied).toBeNull();
    expect(await createCopy(1000, () => undefined).copy("/a")).toBe(false);
  });
});

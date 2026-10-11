import { afterEach, describe, expect, it, vi } from "vitest";
import { copyGeneratedText } from "./generated-clipboard";

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function clipboardFixture(write: (items: { data: Record<string, Promise<Blob>> }[]) => Promise<void>) {
  class Item { constructor(public data: Record<string, Promise<Blob>>) {} }
  vi.stubGlobal("ClipboardItem", Item);
  vi.stubGlobal("navigator", { clipboard: { write } });
}

describe("generated clipboard text", () => {
  it("starts writing during the click before asynchronous configuration resolves", async () => {
    let resolve!: (text: string) => void;
    const generation = new Promise<string>((done) => { resolve = done; });
    const generate = vi.fn(() => generation);
    const write = vi.fn(async (items: { data: Record<string, Promise<Blob>> }[]) => {
      const blob = await items[0].data["text/plain"];
      expect(blob.type).toBe("text/plain");
      expect(await blob.text()).toBe("private-generated-settings");
    });
    clipboardFixture(write);

    const copied = copyGeneratedText(generate);
    expect(write).toHaveBeenCalledOnce();
    expect(generate).not.toHaveBeenCalled();
    await Promise.resolve();
    expect(generate).toHaveBeenCalledOnce();
    resolve("private-generated-settings");
    await copied;
  });

  it("does not materialize configuration after the requesting view is destroyed", async () => {
    let current = true, resolve!: (text: string) => void;
    const generation = new Promise<string>((done) => { resolve = done; });
    const materialized = vi.fn();
    clipboardFixture(async (items) => { materialized(await items[0].data["text/plain"]); });
    const copied = copyGeneratedText(() => generation, () => current);
    current = false;
    resolve("private-generated-settings");
    await expect(copied).rejects.toThrow("Clipboard unavailable");
    expect(materialized).not.toHaveBeenCalled();
  });

  it("handles a source rejection even when clipboard rejects before consuming it", async () => {
    clipboardFixture(() => Promise.reject(new Error("private-clipboard-error")));
    await expect(copyGeneratedText(() => Promise.reject(new Error("private-native-token"))))
      .rejects.toThrow(/^Clipboard unavailable$/);
    await new Promise((resolve) => setTimeout(resolve, 0));
  });

  it("handles a synchronous clipboard failure without leaking a detached source rejection", async () => {
    clipboardFixture(() => { throw new Error("private-clipboard-error"); });
    await expect(copyGeneratedText(() => Promise.reject(new Error("private-native-token"))))
      .rejects.toThrow(/^Clipboard unavailable$/);
    await new Promise((resolve) => setTimeout(resolve, 0));
  });

  it("uses writeText only when promise-backed clipboard items are unavailable", async () => {
    vi.stubGlobal("ClipboardItem", undefined);
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    await copyGeneratedText(async () => "generated-settings");
    expect(writeText).toHaveBeenCalledWith("generated-settings");
  });

  it("does not request credentials when clipboard access is unavailable", async () => {
    vi.stubGlobal("navigator", {});
    const generate = vi.fn().mockResolvedValue("private-generated-settings");
    await expect(copyGeneratedText(generate)).rejects.toThrow(/^Clipboard unavailable$/);
    expect(generate).not.toHaveBeenCalled();
  });
});

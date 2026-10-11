// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import CommandPalette from "./CommandPalette.svelte";
import type { Command } from "../lib/palette";

beforeAll(() => {
  Element.prototype.scrollIntoView ??= () => {};
  (Element.prototype as unknown as { animate: unknown }).animate ??= vi.fn(() => ({ cancel() {}, finish() {}, onfinish: null, currentTime: 0, play() {}, pause() {} }));
});
afterEach(cleanup);
const command = (id: string): Command => ({ id, title: `Command ${id}`, group: "Actions", run: vi.fn() });

describe("command palette keyboard access", () => {
  it("names the input and announces results, including an empty result set", async () => {
    const { getByRole } = render(CommandPalette, { commands: [command("one")], onclose: vi.fn() });
    const input = getByRole("combobox", { name: "Search commands" });
    expect(getByRole("status").textContent).toBe("1 result");
    await fireEvent.input(input, { target: { value: "unmatched" } });
    expect(getByRole("status").textContent).toBe("0 results");
    expect(input.getAttribute("aria-activedescendant")).toBeNull();
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(input.getAttribute("aria-activedescendant")).toBeNull();
  });

  it("retains the selected command through asynchronous insertions and safely recovers after removal", async () => {
    const one = command("one"), two = command("two"), three = command("three");
    const onclose = vi.fn();
    const { getByRole, rerender } = render(CommandPalette, { commands: [one, two], onclose });
    const input = getByRole("combobox", { name: "Search commands" });
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(input.getAttribute("aria-activedescendant")).toBe("cmd-two");
    await rerender({ commands: [three, one, two], onclose });
    expect(input.getAttribute("aria-activedescendant")).toBe("cmd-two");
    await rerender({ commands: [one], onclose });
    expect(input.getAttribute("aria-activedescendant")).toBe("cmd-one");
    await rerender({ commands: [three, one, two], onclose });
    expect(input.getAttribute("aria-activedescendant")).toBe("cmd-one");
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(onclose).toHaveBeenCalledOnce();
    expect(one.run).toHaveBeenCalledOnce();
    expect(two.run).not.toHaveBeenCalled();
  });

  it("leaves composing and modified keys alone, including Escape handled by its dialog", async () => {
    const one = command("one"), two = command("two"), onclose = vi.fn();
    const { getByRole } = render(CommandPalette, { commands: [one, two], onclose });
    const input = getByRole("combobox", { name: "Search commands" });
    for (const values of [{ key: "Enter", isComposing: true }, { key: "Enter", keyCode: 229 }, { key: "Escape", isComposing: true }, { key: "Escape", keyCode: 229 }, { key: "Enter", ctrlKey: true }, { key: "ArrowDown", altKey: true }, { key: "ArrowDown", shiftKey: true }]) {
      await fireEvent.keyDown(input, values);
      expect(input.getAttribute("aria-activedescendant")).toBe("cmd-one");
    }
    expect(onclose).not.toHaveBeenCalled();
    expect(one.run).not.toHaveBeenCalled();
    await fireEvent.keyDown(input, { key: "Escape" });
    expect(onclose).toHaveBeenCalledOnce();
  });
});

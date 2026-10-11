// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { isComposingKey, shortcutContext } from "./keyboard";

afterEach(() => { document.body.replaceChildren(); });
const key = (values: KeyboardEventInit = {}) => new KeyboardEvent("keydown", { key: "d", ...values });

describe("shortcut context", () => {
  it("protects IME composition and WebKit's completion event", () => {
    expect(isComposingKey(key({ isComposing: true }))).toBe(true);
    expect(isComposingKey(key({ keyCode: 229 }))).toBe(true);
    expect(shortcutContext(key({ isComposing: true, ctrlKey: true }), null).command).toBe(false);
    expect(shortcutContext(key({ keyCode: 229 }), null).navigation).toBe(false);
  });

  it("reserves AltGraph, Alt and modified navigation for the operating system", () => {
    const altGraph = key({ ctrlKey: true, altKey: true });
    Object.defineProperty(altGraph, "getModifierState", { value: (name: string) => name === "AltGraph" });
    expect(shortcutContext(altGraph, null).blocked).toBe(true);
    for (const modifiers of [{ altKey: true }, { ctrlKey: true }, { metaKey: true }, { shiftKey: true }]) {
      expect(shortcutContext(key({ key: "ArrowDown", ...modifiers }), null).navigation).toBe(false);
    }
    expect(shortcutContext(key({ ctrlKey: true }), null).command).toBe(true);
    expect(shortcutContext(key({ metaKey: true }), null).command).toBe(true);
    expect(shortcutContext(key({ ctrlKey: true, shiftKey: true }), null).command).toBe(false);
  });

  it("recognizes inherited editing, plaintext-only and explicit false overrides", () => {
    document.body.innerHTML = '<div contenteditable><span id="inside"></span><span contenteditable="false" id="outside"></span></div><div contenteditable="plaintext-only"><span id="plain"></span></div>';
    expect(shortcutContext(key(), document.getElementById("inside")).editing).toBe(true);
    expect(shortcutContext(key(), document.getElementById("plain")).editing).toBe(true);
    expect(shortcutContext(key(), document.getElementById("outside")).editing).toBe(false);
  });

  it("protects native and ARIA controls, including their child elements", () => {
    for (const markup of ['<button><span></span></button>', '<select><option></option></select>', '<a href="#"><span></span></a>', '<div role="slider"><span></span></div>', '<div role="tablist"><span></span></div>']) {
      document.body.innerHTML = markup;
      expect(shortcutContext(key(), document.body.firstElementChild!.firstElementChild).control).toBe(true);
    }
    for (const markup of ['<input>', '<textarea></textarea>', '<div role="combobox"></div>']) {
      document.body.innerHTML = markup;
      expect(shortcutContext(key(), document.body.firstElementChild).editing).toBe(true);
    }
    document.body.innerHTML = '<div role="listbox"></div>';
    expect(shortcutContext(key(), document.body.firstElementChild).control).toBe(false);
  });

  it("honors an already-handled event and its actual target", () => {
    const handled = new KeyboardEvent("keydown", { key: "d", cancelable: true });
    handled.preventDefault();
    expect(shortcutContext(handled, null).blocked).toBe(true);
    const input = document.createElement("input");
    document.body.append(input);
    input.addEventListener("keydown", (event) => { expect(shortcutContext(event, document.body).editing).toBe(true); });
    input.dispatchEvent(key());
  });
});

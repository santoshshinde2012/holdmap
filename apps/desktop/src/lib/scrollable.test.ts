// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { needsScrollFocus, scrollFocus } from "./scrollable";

function box(scroll: number, client: number, html = "<p>text</p>") {
  const el = document.createElement("div");
  el.innerHTML = html;
  Object.defineProperty(el, "scrollHeight", { configurable: true, get: () => scroll });
  Object.defineProperty(el, "clientHeight", { configurable: true, get: () => client });
  return el;
}

describe("scroll focus", () => {
  it("only an overflowing region with nothing focusable inside needs focus", () => {
    expect(needsScrollFocus(box(400, 200))).toBe(true);
    expect(needsScrollFocus(box(200, 200))).toBe(false);
    expect(needsScrollFocus(box(400, 200, "<button>Go</button>"))).toBe(false);
    expect(needsScrollFocus(box(400, 200, "<button disabled>Go</button>"))).toBe(true);
  });

  it("the action adds a tab stop when needed and leaves an explicit tabindex alone", () => {
    const el = box(400, 200);
    scrollFocus(el).destroy();
    expect(el.tabIndex).toBe(0);
    const fits = box(100, 200);
    scrollFocus(fits).destroy();
    expect(fits.hasAttribute("tabindex")).toBe(false);
    const own = box(400, 200);
    own.setAttribute("tabindex", "-1");
    scrollFocus(own).destroy();
    expect(own.getAttribute("tabindex")).toBe("-1");
  });
});

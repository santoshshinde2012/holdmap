import { describe, expect, it } from "vitest";
import { nextIndex, typeahead } from "./roving";

describe("roving focus", () => {
  it("moves and loops along the orientation", () => {
    expect(nextIndex(0, "ArrowRight", 3)).toBe(1);
    expect(nextIndex(2, "ArrowRight", 3)).toBe(0);
    expect(nextIndex(0, "ArrowLeft", 3)).toBe(2);
    expect(nextIndex(0, "ArrowDown", 3)).toBeNull();
    expect(nextIndex(0, "ArrowDown", 3, { orientation: "vertical" })).toBe(1);
    expect(nextIndex(1, "Home", 3)).toBe(0);
    expect(nextIndex(0, "End", 3)).toBe(2);
    expect(nextIndex(0, "x", 3)).toBeNull();
  });
  it("skips disabled items and respects loop=false", () => {
    expect(nextIndex(0, "ArrowRight", 3, { disabled: [false, true, false] })).toBe(2);
    expect(nextIndex(2, "ArrowRight", 3, { loop: false })).toBe(2);
    expect(nextIndex(0, "Home", 3, { disabled: [true, false, false] })).toBe(1);
  });
  it("type-ahead finds the next match after the current item", () => {
    const l = ["Grouped", "Cluster", "Port", "Newest", "Memory"];
    expect(typeahead(l, "p", 0)).toBe(2);
    expect(typeahead(l, "M", 0)).toBe(4);
    expect(typeahead(l, "z", 0)).toBeNull();
    expect(typeahead(l, "", 0)).toBeNull();
  });
});

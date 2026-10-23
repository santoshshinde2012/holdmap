import { describe, expect, it } from "vitest";
import { freeRegion } from "./reveal";

const pane = { left: 0, top: 0, right: 1000, bottom: 600 };

describe("freeRegion", () => {
  it("is the whole pane without an obstacle or when nothing overlaps", () => {
    expect(freeRegion(pane, null)).toEqual(pane);
    expect(freeRegion(pane, { left: 2000, top: 0, right: 2400, bottom: 600 })).toEqual(pane);
  });
  it("is the strip left of a right-hand panel", () => {
    expect(freeRegion(pane, { left: 620, top: 8, right: 992, bottom: 592 })).toEqual({ left: 0, top: 0, right: 620, bottom: 600 });
  });
  it("is the strip above a bottom sheet", () => {
    expect(freeRegion({ left: 0, top: 0, right: 600, bottom: 800 }, { left: 8, top: 400, right: 592, bottom: 792 })).toEqual({ left: 0, top: 0, right: 600, bottom: 400 });
  });
});

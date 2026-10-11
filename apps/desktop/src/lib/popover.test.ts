import { describe, expect, it } from "vitest";
import { placePopover } from "./popover";

describe("popup viewport bounds", () => {
  it("keeps descriptions inside the right edge even when wider than the trigger", () => {
    const p = placePopover({ left: 1020, top: 110, bottom: 140, width: 150 }, { width: 1180, height: 760 }, { width: 280, height: 220 });
    expect(p.left + p.width).toBeLessThanOrEqual(1172);
    expect(p.width).toBe(280);
    expect(p.up).toBe(false);
  });
  it("opens above a low trigger and bounds a long menu to available space", () => {
    const p = placePopover({ left: 12, top: 260, bottom: 292, width: 170 }, { width: 375, height: 320 }, { width: 280, height: 900 });
    expect(p.up).toBe(true);
    expect(p.maxHeight).toBe(248);
    expect(p.top).toBeGreaterThanOrEqual(8);
    expect(p.left + p.width).toBeLessThanOrEqual(367);
  });
  it("bounds upward content even if wrapping changes after placement", () => {
    const p = placePopover({ left: 180, top: 280, bottom: 312, width: 150 }, { width: 360, height: 320 }, { width: 280, height: 80 });
    expect(p.up).toBe(true);
    expect(p.top + p.maxHeight).toBeLessThanOrEqual(312);
    expect(p.maxHeight).toBe(80);
  });
  it("clamps a wide trigger and offscreen coordinates in a small viewport", () => {
    const p = placePopover({ left: -40, top: 0, bottom: 44, width: 680 }, { width: 240, height: 160 }, { width: 280, height: 220 });
    expect(p.left).toBe(8);
    expect(p.width).toBe(224);
    expect(p.top + p.maxHeight).toBeLessThanOrEqual(152);
  });
});

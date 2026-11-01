// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { clampRegion, fromDrag, MIN_REGION, regionInImage, regionLabel, snapArea } from "./capture-region";

const viewport = { width: 1200, height: 800 };

describe("capture regions", () => {
  it("keeps an area in whole pixels inside the window, at least the minimum size", () => {
    expect(clampRegion({ x: 10.4, y: 20.6, width: 300.2, height: 200.7 }, viewport)).toEqual({ x: 10, y: 21, width: 300, height: 201 });
    expect(clampRegion({ x: -50, y: -5, width: 10, height: 5 }, viewport)).toEqual({ x: 0, y: 0, width: MIN_REGION, height: MIN_REGION });
    // Off the far edge, it comes back inside rather than shrinking below the minimum.
    expect(clampRegion({ x: 1190, y: 790, width: 100, height: 100 }, viewport)).toEqual({ x: 1200 - MIN_REGION, y: 800 - MIN_REGION, width: MIN_REGION, height: MIN_REGION });
    // Video needs even sizes.
    expect(clampRegion({ x: 0, y: 0, width: 301, height: 199 }, viewport, true)).toMatchObject({ width: 300, height: 198 });
    expect(clampRegion({ x: Number.NaN, y: 0, width: 50, height: 50 }, viewport)).toBeNull();
  });

  it("normalises a drag in any direction", () => {
    expect(fromDrag(300, 200, 100, 50)).toEqual({ x: 100, y: 50, width: 200, height: 150 });
  });

  it("maps an area onto an image of the window: scaled, and below a title bar the frame carries", () => {
    // A 2x rendering of the page itself.
    expect(regionInImage({ x: 100, y: 50, width: 300, height: 200 }, viewport, { width: 2400, height: 1600 })).toMatchObject({ sx: 200, sy: 100, sw: 600, sh: 400 });
    // A shared window whose frame has a 28 px title bar above the page.
    expect(regionInImage({ x: 0, y: 0, width: 1200, height: 100 }, viewport, { width: 1200, height: 828 })).toMatchObject({ sx: 0, sy: 28, sw: 1200, sh: 100 });
    // Never outside the image.
    const edge = regionInImage({ x: 1100, y: 700, width: 300, height: 300 }, viewport, { width: 1200, height: 800 });
    expect(edge.sx + edge.sw).toBeLessThanOrEqual(1200);
    expect(edge.sy + edge.sh).toBeLessThanOrEqual(800);
  });

  it("snaps to the smallest marked panel under the pointer", () => {
    document.body.innerHTML = '<div data-capture-area id="outer"><div data-capture-area id="inner"></div></div><div id="plain"></div>';
    const box = (id: string, l: number, t: number, w: number, h: number) => {
      document.getElementById(id)!.getBoundingClientRect = () => ({ left: l, top: t, right: l + w, bottom: t + h, width: w, height: h, x: l, y: t, toJSON() {} }) as DOMRect;
    };
    box("outer", 0, 0, 600, 400); box("inner", 100, 100, 200, 100); box("plain", 0, 0, 50, 50);
    expect(snapArea(150, 150)).toEqual({ x: 100, y: 100, width: 200, height: 100 });
    expect(snapArea(500, 300)).toEqual({ x: 0, y: 0, width: 600, height: 400 });
    expect(snapArea(900, 700)).toBeNull();
    expect(regionLabel({ x: 0, y: 0, width: 640.4, height: 360 })).toBe("640 × 360");
  });
});

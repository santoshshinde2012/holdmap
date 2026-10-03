import { describe, expect, it } from "vitest";
import { place } from "./tooltip";

describe("tooltip placement", () => {
  it("places tooltips above, flips below near the top and clamps to the viewport", () => {
    expect(place({ top: 100, bottom: 120, left: 100, width: 40 }, { width: 60, height: 20 }, 800, 600)).toEqual({ top: 74, left: 90 });
    expect(place({ top: 4, bottom: 24, left: 100, width: 40 }, { width: 60, height: 20 }, 800, 600).top).toBe(30);
    expect(place({ top: 100, bottom: 120, left: 790, width: 10 }, { width: 100, height: 20 }, 800, 600).left).toBe(696);
    expect(place({ top: 100, bottom: 120, left: 0, width: 10 }, { width: 100, height: 20 }, 800, 600).left).toBe(4);
  });
});

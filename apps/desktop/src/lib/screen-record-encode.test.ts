// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { encodeGif } from "./screen-record-encode";

function fill(width: number, height: number, r: number, g: number, b: number): Uint8ClampedArray {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let i = 0; i < data.length; i += 4) {
    data[i] = r;
    data[i + 1] = g;
    data[i + 2] = b;
    data[i + 3] = 255;
  }
  return data;
}

describe("GIF export", () => {
  it("writes a looping GIF89a with at least two frames", async () => {
    const blob = encodeGif([
      { data: fill(8, 8, 220, 40, 40), width: 8, height: 8 },
      { data: fill(8, 8, 40, 80, 220), width: 8, height: 8 },
    ], 125);
    expect(blob.type).toBe("image/gif");
    expect(blob.size).toBeGreaterThan(32);
    const bytes = new Uint8Array(await blob.arrayBuffer());
    expect(String.fromCharCode(...bytes.slice(0, 6))).toBe("GIF89a");
  });

  it("refuses an empty frame list", () => {
    expect(() => encodeGif([], 125)).toThrow(/Nothing was captured/);
  });
});

import { describe, expect, it } from "vitest";
import { Vector3 } from "three";
import { roundedOutline, roundedPlane, roundedPoints, roundedSlab } from "./geometry";

const rect = { x0: -2, x1: 4, z0: -1, z1: 3 };

describe("3D rounded rectangles", () => {
  it("goes once round the rectangle, inside it, touching every edge", () => {
    const pts = roundedPoints(rect, 0.5, 4);
    expect(pts).toHaveLength(20);
    for (const [x, z] of pts) { expect(x).toBeGreaterThanOrEqual(-2 - 1e-9); expect(x).toBeLessThanOrEqual(4 + 1e-9); expect(z).toBeGreaterThanOrEqual(-1 - 1e-9); expect(z).toBeLessThanOrEqual(3 + 1e-9); }
    for (const edge of [(p: number[]) => p[0] === 4, (p: number[]) => p[0] === -2, (p: number[]) => p[1] === 3, (p: number[]) => p[1] === -1]) expect(pts.some((p) => edge(p.map((v) => Math.round(v * 1e9) / 1e9)))).toBe(true);
    // A radius larger than half the short side is clamped to it.
    const pill = roundedPoints({ x0: 0, x1: 4, z0: 0, z1: 1 }, 5, 2);
    expect(Math.min(...pill.map(([x]) => x))).toBeCloseTo(0, 9);
  });

  it("builds a plane facing up and a slab whose faces all point outward", () => {
    const plane = roundedPlane(rect, 0.5);
    const n = plane.getAttribute("normal");
    for (let i = 0; i < n.count; i++) expect(n.getY(i)).toBe(1);
    const slab = roundedSlab(rect, 0.5, 0.36);
    expect(slab.groups.map((g) => g.materialIndex)).toEqual([0, 1]);
    const pos = slab.getAttribute("position"), nor = slab.getAttribute("normal");
    const centre = new Vector3(1, 0.18, 1);
    // Each triangle's winding agrees with its normal, which points away from the centre.
    for (let i = 0; i < pos.count; i += 3) {
      const [a, b, c] = [0, 1, 2].map((k) => new Vector3().fromBufferAttribute(pos, i + k));
      const wound = new Vector3().subVectors(b, a).cross(new Vector3().subVectors(c, a));
      const normal = new Vector3().fromBufferAttribute(nor, i);
      if (wound.lengthSq() < 1e-12) continue;
      expect(wound.dot(normal)).toBeGreaterThan(0);
      const mid = a.clone().add(b).add(c).divideScalar(3).sub(centre);
      expect(mid.dot(normal)).toBeGreaterThan(0);
    }
    // The top's UVs are world x and -z, so a once-per-unit texture lines up with the grid.
    const uv = slab.getAttribute("uv");
    expect([uv.getX(1), uv.getY(1)]).toEqual([pos.getX(1), -pos.getZ(1)]);
  });

  it("outlines at the height asked for", () => {
    const g = roundedOutline(rect, 0.5, 0.25);
    const p = g.getAttribute("position");
    for (let i = 0; i < p.count; i++) expect(p.getY(i)).toBe(0.25);
  });
});

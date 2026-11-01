// Rounded rectangles for the board, the district pads, the floors and the calls shelf. A rounded
// rectangle is convex, so a fan from its centre covers it: no general shape triangulation (and
// none of its code in the 3D chunk). Everything lies on the ground plane (x, z), y up.
import { BufferGeometry, Float32BufferAttribute } from "three";
import type { Rect } from "./layout";

/** The outline, going once round counter-clockwise in (x, z), each corner an arc of `segments`. */
export function roundedPoints(r: Rect, radius: number, segments = 6): [number, number][] {
  const k = Math.max(0, Math.min(radius, (r.x1 - r.x0) / 2, (r.z1 - r.z0) / 2));
  const corners: [number, number, number][] = [[r.x1 - k, r.z1 - k, 0], [r.x0 + k, r.z1 - k, 0.5], [r.x0 + k, r.z0 + k, 1], [r.x1 - k, r.z0 + k, 1.5]];
  const out: [number, number][] = [];
  for (const [cx, cz, a0] of corners) {
    for (let i = 0; i <= segments; i++) {
      const a = (a0 + (i / segments) * 0.5) * Math.PI;
      out.push([cx + Math.cos(a) * k, cz + Math.sin(a) * k]);
    }
  }
  return out;
}

type Builder = { pos: number[]; nor: number[]; uv: number[] };
const vertex = (b: Builder, x: number, y: number, z: number, n: readonly [number, number, number]) => {
  b.pos.push(x, y, z); b.nor.push(...n);
  // Caps take world x and -z as UVs: a texture repeating once per unit lines up with the grid.
  b.uv.push(x, -z);
};

/** Fan triangles from the centre at height y, facing up or down. */
function fan(b: Builder, r: Rect, pts: [number, number][], y: number, up: boolean) {
  const cx = (r.x0 + r.x1) / 2, cz = (r.z0 + r.z1) / 2, n = pts.length;
  const normal = [0, up ? 1 : -1, 0] as const;
  for (let i = 0; i < n; i++) {
    const p = pts[i], q = pts[(i + 1) % n];
    // Counter-clockwise points: (centre, p, q) faces down, so the top takes them the other way.
    const [a, c] = up ? [q, p] : [p, q];
    vertex(b, cx, y, cz, normal); vertex(b, a[0], y, a[1], normal); vertex(b, c[0], y, c[1], normal);
  }
}

function build(b: Builder, groups: [number, number, number][] = []): BufferGeometry {
  const g = new BufferGeometry();
  g.setAttribute("position", new Float32BufferAttribute(b.pos, 3));
  g.setAttribute("normal", new Float32BufferAttribute(b.nor, 3));
  g.setAttribute("uv", new Float32BufferAttribute(b.uv, 2));
  for (const [start, count, material] of groups) g.addGroup(start, count, material);
  g.computeBoundingSphere();
  return g;
}

/** A flat rounded rectangle at y = 0, facing up. */
export function roundedPlane(r: Rect, radius: number, segments = 6): BufferGeometry {
  const b: Builder = { pos: [], nor: [], uv: [] };
  fan(b, r, roundedPoints(r, radius, segments), 0, true);
  return build(b);
}

/** A rounded slab from y = 0 up to `depth`. Material group 0 is the top and bottom, group 1 the sides. */
export function roundedSlab(r: Rect, radius: number, depth: number, segments = 6): BufferGeometry {
  const pts = roundedPoints(r, radius, segments), n = pts.length;
  const b: Builder = { pos: [], nor: [], uv: [] };
  fan(b, r, pts, depth, true);
  fan(b, r, pts, 0, false);
  const caps = b.pos.length / 3;
  for (let i = 0; i < n; i++) {
    const [px, pz] = pts[i], [qx, qz] = pts[(i + 1) % n];
    const len = Math.hypot(qx - px, qz - pz) || 1;
    // Outward, square to the edge.
    const out = [(qz - pz) / len, 0, -(qx - px) / len] as const;
    vertex(b, px, 0, pz, out); vertex(b, qx, depth, qz, out); vertex(b, qx, 0, qz, out);
    vertex(b, px, 0, pz, out); vertex(b, px, depth, pz, out); vertex(b, qx, depth, qz, out);
  }
  return build(b, [[0, caps, 0], [caps, b.pos.length / 3 - caps, 1]]);
}

/** The outline at height y, for a line loop. */
export function roundedOutline(r: Rect, radius: number, y: number, segments = 6): BufferGeometry {
  const g = new BufferGeometry();
  g.setAttribute("position", new Float32BufferAttribute(roundedPoints(r, radius, segments).flatMap(([x, z]) => [x, y, z]), 3));
  return g;
}

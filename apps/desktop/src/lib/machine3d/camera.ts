// Camera arithmetic for the 3D machine: poses, fitting and eased flights. Pure, so the renderer,
// the tour and the tests share one definition of "looking at" something. The view is
// orthographic, like an architectural model: sizes and spacing read the same at the front and
// the back, and nothing is foreshortened. A pose's distance sets how much fits (see halfHeight).
import { AGENT_MAX_HEIGHT, bounds, partSize, plateY, sceneTop, type MachineLayout } from "./layout";
import type { MachineScene } from "./scene";

/** Spherical camera around a target: theta around the vertical axis (0 = from the front, +z), phi above the ground. */
export interface Pose { theta: number; phi: number; distance: number; tx: number; ty: number; tz: number }
export interface Box { x0: number; x1: number; y0: number; y1: number; z0: number; z1: number }

/** The orthographic view's half-height is distance · tan(FOV / 2): zooming scales the distance. */
export const FOV = 34;
export const PHI = { min: 0.12, max: 1.53 } as const;
export const DISTANCE = { min: 3, max: 160 } as const;
const TAN = Math.tan((FOV * Math.PI) / 360);
export const halfHeight = (distance: number) => distance * TAN;
const distanceFor = (half: number) => half / TAN;

export function clampPose(p: Pose): Pose {
  return { ...p, phi: Math.min(PHI.max, Math.max(PHI.min, p.phi)), distance: Math.min(DISTANCE.max, Math.max(DISTANCE.min, p.distance)) };
}

/** Where the camera looks from (the renderer stands farther back along the same line). */
export function cameraPosition(p: Pose): [number, number, number] {
  const c = Math.cos(p.phi);
  return [p.tx + p.distance * c * Math.sin(p.theta), p.ty + p.distance * Math.sin(p.phi), p.tz + p.distance * c * Math.cos(p.theta)];
}

/** Screen right and up, in world space, for a view from (theta, phi). */
export function viewAxes(theta: number, phi: number): { right: [number, number, number]; up: [number, number, number] } {
  return {
    right: [Math.cos(theta), 0, -Math.sin(theta)],
    up: [-Math.sin(phi) * Math.sin(theta), Math.cos(phi), -Math.sin(phi) * Math.cos(theta)],
  };
}

/** A box's half-extents on screen from (theta, phi), in world units. */
export function projectedHalf(box: Box, theta: number, phi: number): { w: number; h: number } {
  const { right, up } = viewAxes(theta, phi);
  const hx = (box.x1 - box.x0) / 2, hy = (box.y1 - box.y0) / 2, hz = (box.z1 - box.z0) / 2;
  // A box is centrally symmetric, so its projection is centred on its centre's projection.
  const extent = (a: readonly number[]) => Math.abs(a[0]) * hx + Math.abs(a[1]) * hy + Math.abs(a[2]) * hz;
  return { w: extent(right), h: extent(up) };
}

/** The pose that frames `box` exactly from (theta, phi): centred, the tighter side touching the margin. */
export function fitBox(box: Box, theta: number, phi: number, aspect: number, margin = 1.05): Pose {
  const half = projectedHalf(box, theta, phi);
  const h = Math.max(half.h, half.w / Math.max(0.2, aspect)) * margin;
  return clampPose({ theta, phi, distance: distanceFor(h), tx: (box.x0 + box.x1) / 2, ty: (box.y0 + box.y1) / 2, tz: (box.z0 + box.z1) / 2 });
}

/** The whole machine from the layout's own angle, which keeps every district clear of the next. */
export function overviewPose(layout: MachineLayout, explode: number, aspect: number, theta = layout.theta, phi = layout.phi): Pose {
  return fitBox(bounds(layout, explode), theta, phi, aspect, 1.04);
}

/** Straight down: the floor plan, every district and what stands in it. */
export function topPose(layout: MachineLayout, aspect: number): Pose {
  return fitBox({ ...bounds(layout, 0), y1: 0 }, 0, PHI.max, aspect, 1.04);
}

/** One agent's district, as high as its own floors and mast reach, from the overview's angle so nothing spins. */
export function agentPose(layout: MachineLayout, id: string, explode: number, aspect: number): Pose {
  const d = layout.districts.get(id);
  if (!d) return overviewPose(layout, explode, aspect);
  const top = d.mast ? sceneTop(explode) : d.floors.config ? plateY("config", explode) + 1.4 : d.floors.runtime ? plateY("runtime", explode) + 1.4 : AGENT_MAX_HEIGHT + 0.8;
  return fitBox({ x0: d.x0 - 0.4, x1: d.x1 + 0.4, y0: -0.36, y1: top, z0: d.z0 - 0.4, z1: d.z1 + 0.6 }, layout.theta, layout.phi, aspect, 1.12);
}

/** Close on a single part, with room around it for its neighbours. */
export function partPose(layout: MachineLayout, scene: MachineScene, id: string, explode: number, aspect: number): Pose {
  const part = scene.parts.find((p) => p.id === id);
  const at = layout.parts.get(id);
  if (!part || !at) return overviewPose(layout, explode, aspect);
  const y = plateY(at.layer, explode) + partSize(part).h / 2;
  const r = 2.4;
  return fitBox({ x0: at.x - r, x1: at.x + r, y0: y - r / 2, y1: y + r / 2, z0: at.z - r, z1: at.z + r }, layout.theta, layout.phi, aspect, 1);
}

/** Shortest-way angle difference, so a flight never spins the long way round. */
const angleDelta = (from: number, to: number) => Math.atan2(Math.sin(to - from), Math.cos(to - from));

export function lerpPose(a: Pose, b: Pose, t: number): Pose {
  const k = Math.min(1, Math.max(0, t));
  // A finished flight lands exactly on its target.
  if (k === 1) return { ...b };
  const mix = (x: number, y: number) => x + (y - x) * k;
  // Zoom eases in log space, so a long zoom doesn't rush at one end.
  const distance = Math.exp(mix(Math.log(a.distance), Math.log(b.distance)));
  return { theta: a.theta + angleDelta(a.theta, b.theta) * k, phi: mix(a.phi, b.phi), distance, tx: mix(a.tx, b.tx), ty: mix(a.ty, b.ty), tz: mix(a.tz, b.tz) };
}

/** CSS-style cubic-bezier timing as a function of progress (0–1). */
export function cubicBezier(x1: number, y1: number, x2: number, y2: number): (t: number) => number {
  const bez = (t: number, p1: number, p2: number) => 3 * (1 - t) * (1 - t) * t * p1 + 3 * (1 - t) * t * t * p2 + t * t * t;
  return (x: number) => {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    // Solve bez(t, x1, x2) = x by bisection: monotonic for x1, x2 in [0, 1], and exact enough for motion.
    let lo = 0, hi = 1, t = x;
    for (let i = 0; i < 24; i++) {
      t = (lo + hi) / 2;
      if (bez(t, x1, x2) < x) lo = t; else hi = t;
    }
    return bez(t, y1, y2);
  };
}

/** State-to-state moves (flights, explode), entrances and exits: the same curves as the rest of the motion. */
export const EASE = {
  move: cubicBezier(0.65, 0, 0.35, 1),
  enter: cubicBezier(0.16, 1, 0.3, 1),
  exit: cubicBezier(0.6, 0, 0.9, 0.35),
};

export const posesEqual = (a: Pose, b: Pose, eps = 1e-3) =>
  Math.abs(angleDelta(a.theta, b.theta)) < eps && Math.abs(a.phi - b.phi) < eps && Math.abs(a.distance - b.distance) < eps * 10
  && Math.abs(a.tx - b.tx) < eps * 10 && Math.abs(a.ty - b.ty) < eps * 10 && Math.abs(a.tz - b.tz) < eps * 10;

// Orbit camera arithmetic for the 3D machine: poses, fitting and eased flights. Pure, so the
// renderer, the tour and the tests share one definition of "looking at" something.
import { groundRadius, partSize, plateY, sceneTop, type MachineLayout } from "./layout";
import type { MachineScene } from "./scene";

/** Spherical camera around a target: theta around the vertical axis (0 = from the front, +z), phi above the ground. */
export interface Pose { theta: number; phi: number; distance: number; tx: number; ty: number; tz: number }

/** Vertical field of view, degrees. Narrow enough to read as a model, not a fisheye. */
export const FOV = 34;
export const PHI = { min: 0.12, max: 1.53 } as const;
export const DISTANCE = { min: 3, max: 140 } as const;

export function clampPose(p: Pose): Pose {
  return { ...p, phi: Math.min(PHI.max, Math.max(PHI.min, p.phi)), distance: Math.min(DISTANCE.max, Math.max(DISTANCE.min, p.distance)) };
}

export function cameraPosition(p: Pose): [number, number, number] {
  const c = Math.cos(p.phi);
  return [p.tx + p.distance * c * Math.sin(p.theta), p.ty + p.distance * Math.sin(p.phi), p.tz + p.distance * c * Math.cos(p.theta)];
}

/** Distance at which a sphere of `radius` fits the narrower field of view, with a margin. */
export function fitDistance(radius: number, aspect: number, fov = FOV, margin = 1.08): number {
  const v = (fov * Math.PI) / 180;
  const h = 2 * Math.atan(Math.tan(v / 2) * Math.max(0.2, aspect));
  return (radius / Math.sin(Math.min(v, h) / 2)) * margin;
}

/** The whole machine from a raised three-quarter angle. */
export function overviewPose(layout: MachineLayout, explode: number, aspect: number, theta = 0.32, phi = 0.56): Pose {
  const top = sceneTop(explode, layout.callsRadius > 0);
  // A sphere around a flat disc wastes most of the frame; fit the board's tilted footprint instead.
  const r = Math.max(groundRadius(layout) * (0.5 + 0.45 * Math.sin(phi)), top * 0.6);
  return clampPose({ theta, phi, distance: fitDistance(r, aspect, FOV, 0.9), tx: 0, ty: top * 0.3, tz: 0 });
}

/** Straight down: the 2D map's view of the same ring. */
export function topPose(layout: MachineLayout, aspect: number): Pose {
  // Fit the board itself; the plates above sit a little nearer and read slightly larger.
  return clampPose({ theta: 0, phi: PHI.max, distance: fitDistance(groundRadius(layout), aspect, FOV, 0.92) + plateY("config", 0), tx: 0, ty: 0, tz: 0 });
}

/** One agent's sector seen from beyond the board's edge: its ports and folders in front, the tower
 *  and its layers above, the other agents (faded) across the board behind. */
export function agentPose(layout: MachineLayout, id: string, explode: number, aspect: number): Pose {
  const angle = layout.angles.get(id);
  const at = layout.parts.get(id);
  if (angle === undefined || !at) return overviewPose(layout, explode, aspect);
  const own = [...layout.parts.entries()].filter(([pid]) => pid === id || pid.startsWith(`${id}:`)).map(([, p]) => p);
  // The camera sits outward along the agent's direction, looking back toward the centre.
  const theta = layout.ring === 0 ? 0.32 : Math.atan2(Math.cos(angle), Math.sin(angle));
  const cx = own.reduce((s, p) => s + p.x, 0) / own.length, cz = own.reduce((s, p) => s + p.z, 0) / own.length;
  const spread = Math.max(2.6, ...own.map((p) => Math.hypot(p.x - cx, p.z - cz)));
  const top = sceneTop(explode, layout.callsRadius > 0);
  return clampPose({ theta, phi: 0.5, distance: fitDistance(Math.hypot(spread + 0.4, top / 2), aspect, FOV, 0.86), tx: cx, ty: top * 0.4, tz: cz });
}

/** Close on a single part, keeping its agent's viewing direction. */
export function partPose(layout: MachineLayout, scene: MachineScene, id: string, explode: number, aspect: number): Pose {
  const part = scene.parts.find((p) => p.id === id);
  const at = layout.parts.get(id);
  if (!part || !at) return overviewPose(layout, explode, aspect);
  const base = agentPose(layout, part.owner, explode, aspect);
  const size = partSize(part);
  return clampPose({ ...base, phi: 0.5, distance: Math.max(7, fitDistance(3.2, aspect)), tx: at.x, ty: plateY(at.layer, explode) + size.h / 2, tz: at.z });
}

/** Shortest-way angle difference, so a flight never spins the long way round. */
const angleDelta = (from: number, to: number) => Math.atan2(Math.sin(to - from), Math.cos(to - from));

export function lerpPose(a: Pose, b: Pose, t: number): Pose {
  const k = Math.min(1, Math.max(0, t));
  // A finished flight lands exactly on its target.
  if (k === 1) return { ...b };
  const mix = (x: number, y: number) => x + (y - x) * k;
  return { theta: a.theta + angleDelta(a.theta, b.theta) * k, phi: mix(a.phi, b.phi), distance: mix(a.distance, b.distance), tx: mix(a.tx, b.tx), ty: mix(a.ty, b.ty), tz: mix(a.tz, b.tz) };
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

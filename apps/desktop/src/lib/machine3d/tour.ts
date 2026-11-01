// The guided tour: a script of beats over the current scene, evaluated as a pure function of
// time. The same input always gives the same frame, so it can be scrubbed, tested or filmed.
import { agentPose, EASE, lerpPose, overviewPose, topPose, type Pose } from "./camera";
import type { MachineLayout } from "./layout";
import { agentSummary, type MachineScene } from "./scene";

export interface TourBeat {
  /** Seconds to fly from the previous beat, then to hold. */
  fly: number;
  hold: number;
  /** Continuous drift (an orbit) moves linearly; state changes ease. */
  linear?: boolean;
  pose: Pose;
  explode: number;
  xray: boolean;
  focus: string | null;
  caption: string;
}
export interface TourFrame { pose: Pose; explode: number; xray: boolean; focus: string | null; caption: string; beat: number }

/** At most this many agents get their own stop, so a busy machine still loops in about a minute. */
export const TOUR_AGENTS = 4;
const plural = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;

export function tourBeats(scene: MachineScene, layout: MachineLayout, aspect: number): TourBeat[] {
  const running = scene.agents.filter((a) => a.kind === "agent");
  const ports = scene.parts.filter((p) => p.kind === "port").length;
  // A gentle sweep either side of the layout's own angle, which keeps every district clear.
  const overview = (explode: number, turn: number) => overviewPose(layout, explode, aspect, layout.theta + turn);
  const beats: TourBeat[] = [
    { fly: 1.2, hold: 2.4, pose: overview(0, -0.3), explode: 0, xray: false, focus: null, caption: `This computer: ${plural(running.length, "agent")} holding ${plural(ports, "port")}` },
    { fly: 6, hold: 0, linear: true, pose: overview(0, 0.2), explode: 0, xray: false, focus: null, caption: "Each agent has its own district: its tower, the folders it works in, and the ports it holds along the front" },
    { fly: 1.6, hold: 2.6, pose: overview(1, 0.2), explode: 1, xray: false, focus: null, caption: "Pulled apart: each district's floors lift straight up, from the board to processes running now to configuration on disk" },
  ];
  // Agents that need you first, then the rest in ring order.
  const order = [...running].sort((a, b) => Number(!!b.warn) - Number(!!a.warn));
  for (const a of order.slice(0, TOUR_AGENTS)) {
    beats.push({ fly: 1.6, hold: 3.6, pose: agentPose(layout, a.id, 1, aspect), explode: 1, xray: false, focus: a.id, caption: `${a.label}: ${agentSummary(scene, a.id)}${a.warn ? `. Needs you: ${a.warn}` : ""}` });
  }
  if (scene.calls) beats.push({ fly: 1.6, hold: 3.6, pose: overviewPose(layout, 1, aspect, layout.theta, layout.phi + 0.2), explode: 1, xray: false, focus: null,
    caption: `Recorded calls: ${plural(scene.calls, "call")} reported by agent integrations, on a shelf of their own apart from every district, because nothing ties them to a process or a configured server` });
  if (scene.inspected) beats.push({ fly: 1.6, hold: 3.6, pose: overview(1, -0.2), explode: 1, xray: true, focus: null, caption: "X-ray: blueprint parts are files on disk; a green beam means that MCP server runs now" });
  beats.push({ fly: 1.8, hold: 2.4, pose: topPose(layout, aspect), explode: 0, xray: false, focus: null, caption: "From above: the floor plan, one district per agent" });
  return beats;
}

export const tourLength = (beats: readonly TourBeat[]) => beats.reduce((s, b) => s + b.fly + b.hold, 0);

/** The frame at `t` seconds (looping). Beat 0 flies in from the last beat, so the loop is seamless. */
export function tourFrame(beats: readonly TourBeat[], t: number): TourFrame | null {
  if (!beats.length) return null;
  const total = tourLength(beats);
  let at = ((t % total) + total) % total;
  for (let i = 0; i < beats.length; i++) {
    const b = beats[i], prev = beats[(i - 1 + beats.length) % beats.length];
    if (at < b.fly + b.hold || i === beats.length - 1) {
      const k = b.fly > 0 ? Math.min(1, at / b.fly) : 1;
      const e = b.linear ? k : EASE.move(k);
      return { pose: lerpPose(prev.pose, b.pose, e), explode: prev.explode + (b.explode - prev.explode) * e, xray: k < 0.5 ? prev.xray : b.xray,
        focus: k < 0.5 ? prev.focus : b.focus, caption: b.caption, beat: i };
    }
    at -= b.fly + b.hold;
  }
  return null;
}

/** Seconds of quiet after the viewer takes control before the tour picks up again. */
export const TOUR_RESUME_S = 25;

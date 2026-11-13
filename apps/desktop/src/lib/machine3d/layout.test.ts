import { describe, expect, it } from "vitest";
import { MOCK_SNAPSHOT } from "../mock";
import { mockAgents } from "../mock-agents";
import { agentPose, cubicBezier, EASE, fitBox, halfHeight, lerpPose, overviewPose, PHI, projectedHalf, topPose, viewAxes, type Pose } from "./camera";
import { AGENT_MAX_HEIGHT, bounds, layoutScene, partSize, plateY, sceneTop, SIZE, skyY, STREET, type MachineLayout } from "./layout";
import { demoActivity } from "../activity-demo";
import { buildScene, lensView, type MachineScene, type Part } from "./scene";

const report = mockAgents(undefined, true);
const scene = buildScene({ report, entries: MOCK_SNAPSHOT.entries });
const layout = layoutScene(scene);

const inside = (p: { x: number; z: number }, r: { x0: number; x1: number; z0: number; z1: number }) => p.x >= r.x0 - 1e-9 && p.x <= r.x1 + 1e-9 && p.z >= r.z0 - 1e-9 && p.z <= r.z1 + 1e-9;

/** Smallest centre distance between two parts on the same floor. */
function closest(s: MachineScene, l: MachineLayout) {
  const placed = s.parts.map((p) => ({ p, at: l.parts.get(p.id)! }));
  let min = Infinity;
  for (let i = 0; i < placed.length; i++) for (let j = i + 1; j < placed.length; j++) {
    const a = placed[i], b = placed[j];
    if (a.at.layer !== b.at.layer) continue;
    min = Math.min(min, Math.hypot(a.at.x - b.at.x, a.at.z - b.at.z));
  }
  return min;
}

/** A report with `n` running agents, cloned from the sample ones with fresh ids. */
function many(n: number) {
  const agents = Array.from({ length: n }, (_, i) => {
    const a = report.agents[i % report.agents.length];
    const id = `${a.id}#${i}`;
    return { ...a, id, ports: a.ports.map((p) => ({ ...p, entry_id: `${p.entry_id}#${i}` })), links: [] };
  });
  return buildScene({ report: { ...report, agents, configured: [] }, entries: MOCK_SNAPSHOT.entries });
}

describe("3D machine layout", () => {
  it("gives every agent a district, running agents first in report order, a street apart", () => {
    const ids = [...layout.districts.keys()];
    const running = scene.agents.filter((a) => a.kind === "agent").map((a) => a.id);
    expect(ids.slice(0, running.length)).toEqual(running);
    expect(ids).toHaveLength(scene.agents.length);
    const ds = [...layout.districts.values()];
    for (let i = 1; i < ds.length; i++) expect(ds[i].x0 - ds[i - 1].x1).toBeCloseTo(STREET, 6);
  });

  it("places every part in its own agent's district, floors over the same footprint", () => {
    for (const p of scene.parts) {
      const at = layout.parts.get(p.id);
      expect(at, p.id).toBeDefined();
      if (p.kind === "call") continue;
      expect(inside(at!, layout.districts.get(p.owner)!), p.id).toBe(true);
    }
  });

  it("keeps parts on one floor apart, and each floor's parts on its tray", () => {
    expect(closest(scene, layout)).toBeGreaterThan(0.9);
    for (const p of scene.parts.filter((x) => x.layer === "runtime" || x.layer === "config")) {
      const tray = layout.districts.get(p.owner)!.floors[p.layer as "runtime" | "config"];
      expect(tray, p.id).not.toBeNull();
      expect(inside(layout.parts.get(p.id)!, tray!), p.id).toBe(true);
    }
    // No tray where there's nothing to stand on it.
    for (const d of layout.districts.values()) {
      expect(d.floors.runtime !== null).toBe(scene.parts.some((p) => p.owner === d.id && p.layer === "runtime"));
    }
  });

  it("stands the tower at the back, folders in front of it and ports along the front edge", () => {
    const a = report.agents[1];
    const tower = layout.parts.get(a.id)!;
    const ports = scene.parts.filter((p) => p.owner === a.id && p.kind === "port").map((p) => layout.parts.get(p.id)!);
    const folders = scene.parts.filter((p) => p.owner === a.id && p.kind === "project").map((p) => layout.parts.get(p.id)!);
    expect(ports.length).toBeGreaterThan(0);
    for (const f of folders) expect(f.z).toBeGreaterThan(tower.z);
    for (const p of ports) for (const f of folders) expect(p.z).toBeGreaterThan(f.z);
  });

  it("lines every front edge up and turns the view only as far as the streets allow", () => {
    const fronts = new Set([...layout.districts.values()].map((d) => d.z1));
    expect(fronts.size).toBe(1);
    const depth = [...layout.districts.values()][0].z1 - [...layout.districts.values()][0].z0;
    // A district's far edge, seen from the default angle, stays clear of its neighbour.
    expect(Math.tan(layout.theta) * depth).toBeLessThan(STREET);
    expect(layout.theta).toBeGreaterThan(0);
  });

  it("snaps every position to the quarter-unit grid", () => {
    for (const [id, at] of layout.parts) for (const v of [at.x, at.z]) expect(Math.abs(v * 4 - Math.round(v * 4)), id).toBeLessThan(1e-9);
  });

  it("is deterministic and ignores resource samples", () => {
    const again = layoutScene(buildScene({ report: { ...report, agents: report.agents.map((a) => ({ ...a, cpu_percent: 90, memory_bytes: 9e9 })) }, entries: MOCK_SNAPSHOT.entries }));
    expect([...again.parts]).toEqual([...layout.parts]);
    expect([...again.districts]).toEqual([...layout.districts]);
  });

  it("keeps eight districts in a row, and starts the next row well behind", () => {
    const eight = layoutScene(many(8));
    expect(new Set([...eight.districts.values()].map((d) => d.z0)).size).toBe(1);
    const twelve = many(12), l = layoutScene(twelve);
    const rows = [...new Set([...l.districts.values()].map((d) => d.z0))].sort((a, b) => b - a);
    expect(rows).toHaveLength(2);
    const ds = [...l.districts.values()];
    const depth = ds[0].z1 - ds[0].z0;
    // Far enough that the front row's floors, pulled apart, stay clear of the row behind on screen.
    expect(rows[0] - rows[1] - depth).toBeGreaterThan(plateY("config", 1) / Math.tan(l.phi) - depth);
    expect(l.phi).toBeGreaterThan(eight.phi);
    for (const p of twelve.parts) expect(l.parts.has(p.id)).toBe(true);
  });

  it("puts the chip at the left end and keeps everything on the board", () => {
    const first = [...layout.districts.values()][0];
    expect(layout.hub.x).toBeLessThan(first.x0);
    for (const d of layout.districts.values()) expect(d.x0 >= layout.board.x0 && d.x1 <= layout.board.x1 && d.z0 >= layout.board.z0 && d.z1 <= layout.board.z1).toBe(true);
    expect(inside(layout.hub, layout.board)).toBe(true);
  });

  it("shelves recorded calls at the right end, apart from every district, newest first", () => {
    const s = buildScene({ report, entries: MOCK_SNAPSHOT.entries, activity: demoActivity() });
    const l = layoutScene(s);
    const calls = s.parts.filter((p) => p.kind === "call");
    expect(calls.length).toBeGreaterThan(0);
    expect(l.calls).not.toBeNull();
    const right = Math.max(...[...l.districts.values()].map((d) => d.x1));
    expect(l.calls!.x0).toBeGreaterThanOrEqual(right + STREET - 1e-9);
    for (const c of calls) expect(inside(l.parts.get(c.id)!, l.calls!)).toBe(true);
    // Newest first, from the back left.
    const [a, b] = calls.map((c) => l.parts.get(c.id)!);
    expect(a.z < b.z || a.z === b.z && a.x < b.x).toBe(true);
    expect(layout.calls).toBeNull();
    // Calls never move a district.
    for (const [id, d] of l.districts) expect(d).toEqual(layout.districts.get(id));
  });

  it("gives every agent with remote hosts a mast at its district's back corner", () => {
    for (const p of scene.parts.filter((x) => x.kind === "remote")) {
      const d = layout.districts.get(p.owner)!;
      expect(d.mast).not.toBeNull();
      expect(layout.parts.get(p.id)).toMatchObject({ x: d.mast!.x, z: d.mast!.z });
    }
  });

  it("grows towers with memory on a log scale, within bounds", () => {
    expect(partSize({ kind: "agent", memory: 0 }).h).toBeCloseTo(0.6);
    expect(partSize({ kind: "agent", memory: 520e6 }).h).toBeLessThan(partSize({ kind: "agent", memory: 2.4e9 }).h);
    expect(partSize({ kind: "agent", memory: 1e12 }).h).toBe(AGENT_MAX_HEIGHT);
    expect(partSize({ kind: "port", memory: 0 })).toEqual(SIZE.port);
  });

  it("stacks the floors above the tallest tower and pulls them apart when exploded; calls stay on the board", () => {
    expect(plateY("board", 1)).toBe(0);
    expect(plateY("runtime", 0)).toBeGreaterThan(AGENT_MAX_HEIGHT);
    expect(plateY("config", 0)).toBeGreaterThan(plateY("runtime", 0));
    expect(plateY("config", 1) - plateY("runtime", 1)).toBeGreaterThan(plateY("config", 0) - plateY("runtime", 0));
    expect(plateY("calls", 1)).toBe(0);
    expect(skyY(1)).toBeGreaterThan(plateY("config", 1));
    expect(sceneTop(1)).toBeGreaterThan(sceneTop(0));
  });
});

describe("3D machine lenses", () => {
  it("shows everything with All, and one concern at a time otherwise", () => {
    expect(lensView(scene, "all")).toEqual({ lit: null, hidden: new Set() });
    const net = lensView(scene, "network");
    expect([...net.hidden].sort()).toEqual(["calls", "config"]);
    const kinds = (lit: Set<string> | null) => new Set(scene.parts.filter((p) => lit?.has(p.id)).map((p) => p.kind));
    expect(kinds(net.lit).has("port")).toBe(true);
    expect(kinds(net.lit).has("skill")).toBe(false);
    // A process holding a port is part of the network picture.
    for (const w of scene.wires.filter((x) => x.kind === "listener")) expect(net.lit!.has(w.from)).toBe(true);
    const rt = lensView(scene, "runtime");
    expect([...kinds(rt.lit)].every((k) => k === "tool" || k === "port" || k === "agent" || k === "more")).toBe(true);
    const cfg = lensView(scene, "config");
    expect(cfg.hidden.has("config")).toBe(false);
    // Running servers bring the processes that run them.
    for (const w of scene.wires.filter((x) => x.kind === "running")) expect(cfg.lit!.has(w.to)).toBe(true);
  });

  it("lights only what needs you, and the agents it belongs to", () => {
    const needs = lensView(scene, "needs");
    const flagged = scene.parts.filter((p: Part) => p.warn && p.kind !== "call");
    expect(flagged.length).toBeGreaterThan(0);
    for (const p of flagged) { expect(needs.lit!.has(p.id)).toBe(true); expect(needs.lit!.has(p.owner)).toBe(true); }
    for (const id of needs.lit!) {
      const p = scene.parts.find((x) => x.id === id)!;
      expect(!!p.warn || flagged.some((f) => f.owner === id)).toBe(true);
    }
  });
});

describe("3D machine camera", () => {
  it("frames a box exactly: centred, the tighter side touching the margin", () => {
    const box = { x0: -10, x1: 10, y0: 0, y1: 4, z0: -3, z1: 3 };
    for (const aspect of [0.8, 1.6, 3]) {
      const pose = fitBox(box, 0.2, 0.6, aspect, 1);
      const half = projectedHalf(box, 0.2, 0.6);
      const h = halfHeight(pose.distance);
      expect(Math.max(half.h / h, half.w / (h * aspect))).toBeCloseTo(1, 6);
      expect([pose.tx, pose.ty, pose.tz]).toEqual([0, 2, 0]);
    }
  });

  it("projects with screen axes that are orthonormal and level", () => {
    const { right, up } = viewAxes(0.4, 0.7);
    const dot = right[0] * up[0] + right[1] * up[1] + right[2] * up[2];
    expect(dot).toBeCloseTo(0, 9);
    expect(Math.hypot(...right)).toBeCloseTo(1, 9);
    expect(Math.hypot(...up)).toBeCloseTo(1, 9);
    expect(right[1]).toBe(0);
  });

  it("fits the whole machine in the overview, from the layout's own angle", () => {
    const pose = overviewPose(layout, 1, 1.6);
    expect(pose.theta).toBe(layout.theta);
    expect(pose.phi).toBe(layout.phi);
    const half = projectedHalf(bounds(layout, 1), pose.theta, pose.phi);
    const h = halfHeight(pose.distance);
    expect(half.h).toBeLessThanOrEqual(h);
    expect(half.w).toBeLessThanOrEqual(h * 1.6);
  });

  it("closes on one agent's district without turning, and looks straight down for the plan", () => {
    for (const a of scene.agents) {
      const pose = agentPose(layout, a.id, 1, 1.6);
      const d = layout.districts.get(a.id)!;
      expect(pose.theta).toBe(layout.theta);
      expect(pose.tx).toBeCloseTo((d.x0 + d.x1) / 2, 6);
      expect(pose.distance).toBeLessThan(overviewPose(layout, 1, 1.6).distance);
    }
    expect(topPose(layout, 1.6).phi).toBe(PHI.max);
  });

  it("interpolates angles the short way round and zoom evenly", () => {
    const a: Pose = { theta: Math.PI - 0.1, phi: 0.5, distance: 10, tx: 0, ty: 0, tz: 0 };
    const b: Pose = { ...a, theta: -Math.PI + 0.1, distance: 40 };
    expect(lerpPose(a, b, 0.5).theta).toBeCloseTo(Math.PI, 6);
    // Halfway in log space: the geometric mean.
    expect(lerpPose(a, b, 0.5).distance).toBeCloseTo(20, 6);
    expect(lerpPose(a, b, 1)).toEqual(b);
  });

  it("matches CSS cubic-bezier timing", () => {
    expect(EASE.move(0)).toBe(0);
    expect(EASE.move(1)).toBe(1);
    expect(EASE.move(0.5)).toBeCloseTo(0.5, 3);
    expect(cubicBezier(0, 0, 1, 1)(0.3)).toBeCloseTo(0.3, 3);
    expect(EASE.enter(0.2)).toBeGreaterThan(0.5);
    expect(EASE.exit(0.2)).toBeLessThan(0.2);
  });
});

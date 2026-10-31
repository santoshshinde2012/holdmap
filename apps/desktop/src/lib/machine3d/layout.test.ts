import { describe, expect, it } from "vitest";
import { MOCK_SNAPSHOT, mockAgents } from "../mock";
import { radialLayout } from "../machine-map";
import { agentPose, cameraPosition, cubicBezier, EASE, fitDistance, lerpPose, overviewPose, PHI, topPose, type Pose } from "./camera";
import { AGENT_MAX_HEIGHT, fan, layoutScene, partSize, plateY, sceneTop, SIZE } from "./layout";
import { demoActivity } from "../activity-demo";
import { buildScene, type MachineScene } from "./scene";

const report = mockAgents(undefined, true);
const scene = buildScene({ report, entries: MOCK_SNAPSHOT.entries });
const layout = layoutScene(scene);

/** Smallest centre distance between two parts on the same layer (remote hosts float apart). */
function closest(s: MachineScene) {
  const placed = s.parts.map((p) => ({ p, at: layout.parts.get(p.id)! }));
  let min = Infinity;
  for (let i = 0; i < placed.length; i++) for (let j = i + 1; j < placed.length; j++) {
    const a = placed[i], b = placed[j];
    if (a.at.layer !== b.at.layer) continue;
    min = Math.min(min, Math.hypot(a.at.x - b.at.x, a.at.z - b.at.z));
  }
  return min;
}

describe("3D machine layout", () => {
  it("places agents at the 2D map's angles, so the top view is the map", () => {
    const map = radialLayout(report.agents);
    for (const g of map) {
      const at = layout.parts.get(g.agent.id)!;
      expect(Math.atan2(at.z, at.x)).toBeCloseTo(Math.atan2(g.y / 0.75, g.x / 1.45), 6);
    }
  });

  it("places every part, and keeps parts on a layer apart", () => {
    for (const p of scene.parts) expect(layout.parts.has(p.id)).toBe(true);
    expect(closest(scene)).toBeGreaterThan(0.9);
  });

  it("keeps each agent's parts inside its own sector, sharing configuration with the agent beside it that isn't running", () => {
    const sector = (Math.PI * 2) / report.agents.length;
    expect(scene.agents.some((a) => a.kind === "dormant")).toBe(true);
    for (const p of scene.parts) {
      const owner = layout.angles.get(p.owner)!;
      const at = layout.parts.get(p.id)!;
      const d = Math.atan2(Math.sin(at.angle - owner), Math.cos(at.angle - owner));
      expect(Math.abs(d)).toBeLessThanOrEqual((p.layer === "config" ? sector / 4 : sector / 2) + 1e-9);
    }
    // A dormant agent stands halfway between two running ones.
    const dormant = scene.agents.find((a) => a.kind === "dormant")!;
    expect(layout.angles.get(dormant.id)).toBeCloseTo(-Math.PI / 2 + sector / 2);
  });

  it("is deterministic and ignores resource samples", () => {
    const again = layoutScene(buildScene({ report: { ...report, agents: report.agents.map((a) => ({ ...a, cpu_percent: 90, memory_bytes: 9e9 })) }, entries: MOCK_SNAPSHOT.entries }));
    expect([...again.parts]).toEqual([...layout.parts]);
  });

  it("places every agent that isn't running, however many there are next to running ones", () => {
    const codex = report.configured![0];
    const configured = Array.from({ length: 4 }, (_, i) => ({ ...codex, product: `codex-${i}`, name: `Codex ${i}` }));
    for (const agents of [[], [report.agents[0]], report.agents.slice(0, 2), report.agents]) {
      const s = buildScene({ report: { ...report, agents, configured }, entries: MOCK_SNAPSHOT.entries });
      const l = layoutScene(s);
      expect(s.agents.filter((a) => a.kind === "dormant")).toHaveLength(4);
      for (const p of s.parts) expect(l.parts.has(p.id), `${agents.length} running: ${p.id}`).toBe(true);
      // Running agents keep the map's angles; the others take distinct angles in the gaps.
      agents.forEach((a, i) => expect(l.angles.get(a.id)).toBeCloseTo(-Math.PI / 2 + (i * Math.PI * 2) / agents.length));
      expect(new Set([...l.angles.values()].map((a) => a.toFixed(6))).size).toBe(s.agents.length);
      // Configuration fans never overlap between neighbours.
      const fans = s.agents.map((a) => s.parts.filter((p) => p.owner === a.id && p.layer === "config").map((p) => l.parts.get(p.id)!.angle));
      const spans = fans.filter((f) => f.length).map((f) => [Math.min(...f), Math.max(...f)]).sort((a, b) => a[0] - b[0]);
      for (let i = 1; i < spans.length; i++) expect(spans[i][0]).toBeGreaterThan(spans[i - 1][1]);
    }
  });

  it("stands a single agent in the middle with its parts all around", () => {
    const one = buildScene({ report: { ...report, agents: [report.agents[1]], configured: [] }, entries: MOCK_SNAPSHOT.entries });
    const l = layoutScene(one);
    expect(l.ring).toBe(0);
    expect(l.parts.get(report.agents[1].id)).toMatchObject({ x: 0, z: 0 });
  });

  it("grows towers with memory on a log scale, within bounds", () => {
    expect(partSize({ kind: "agent", memory: 0 }).h).toBeCloseTo(0.6);
    expect(partSize({ kind: "agent", memory: 520e6 }).h).toBeLessThan(partSize({ kind: "agent", memory: 2.4e9 }).h);
    expect(partSize({ kind: "agent", memory: 1e12 }).h).toBe(AGENT_MAX_HEIGHT);
    expect(partSize({ kind: "port", memory: 0 })).toEqual(SIZE.port);
  });

  it("stacks the plates above the tallest tower and pulls them apart when exploded", () => {
    expect(plateY("board", 1)).toBe(0);
    expect(plateY("runtime", 0)).toBeGreaterThan(AGENT_MAX_HEIGHT);
    expect(plateY("config", 0)).toBeGreaterThan(plateY("runtime", 0));
    expect(plateY("config", 1) - plateY("runtime", 1)).toBeGreaterThan(plateY("config", 0) - plateY("runtime", 0));
  });

  it("fans slots in rows within the sector", () => {
    const slots = fan(9, 0, Math.PI / 2, 3, 1.25);
    expect(slots).toHaveLength(9);
    expect(new Set(slots.map((s) => s.r)).size).toBeGreaterThan(1);
    for (const s of slots) expect(Math.abs(s.angle)).toBeLessThanOrEqual(Math.PI / 4);
    expect(fan(4, 0, Math.PI * 2, 2, 1).map((s) => s.angle)).toEqual([0, Math.PI / 2, Math.PI, Math.PI * 1.5]);
  });

  it("rings recorded calls above the plates, newest at the front, apart from every agent", () => {
    const s = buildScene({ report, entries: MOCK_SNAPSHOT.entries, activity: demoActivity() });
    const l = layoutScene(s);
    expect(l.callsRadius).toBeGreaterThan(0);
    const calls = s.parts.filter((p) => p.kind === "call");
    for (const c of calls) {
      const at = l.parts.get(c.id)!;
      expect(at.layer).toBe("calls");
      expect(Math.hypot(at.x, at.z)).toBeCloseTo(l.callsRadius, 6);
    }
    expect(l.parts.get(calls[0].id)!.angle).toBeCloseTo(Math.PI / 2);
    expect(layout.callsRadius).toBe(0);
    expect(plateY("calls", 1)).toBeGreaterThan(plateY("config", 1));
    expect(sceneTop(1, true)).toBeGreaterThan(sceneTop(1));
    // Agents keep their places when calls arrive.
    for (const a of s.agents) expect(l.parts.get(a.id)).toEqual(layout.parts.get(a.id));
  });
});

describe("3D machine camera", () => {
  it("fits a sphere in the narrower field of view", () => {
    expect(fitDistance(10, 0.5)).toBeGreaterThan(fitDistance(10, 2));
    expect(fitDistance(20, 1.5)).toBeCloseTo(fitDistance(10, 1.5) * 2);
  });

  it("looks at one agent from beyond the board's edge, back toward the centre", () => {
    for (const a of scene.agents) {
      const pose = agentPose(layout, a.id, 1, 1.6);
      const at = layout.parts.get(a.id)!;
      const [cx, , cz] = cameraPosition(pose);
      expect(Math.hypot(cx, cz)).toBeGreaterThan(layout.radius);
      // On the agent's side of the board.
      expect(cx * at.x + cz * at.z).toBeGreaterThan(0);
    }
  });

  it("looks straight down for the top view and keeps poses in bounds", () => {
    expect(topPose(layout, 1.6).phi).toBe(PHI.max);
    expect(overviewPose(layout, 0, 1.6).phi).toBeGreaterThan(PHI.min);
  });

  it("interpolates angles the short way round", () => {
    const a: Pose = { theta: Math.PI - 0.1, phi: 0.5, distance: 10, tx: 0, ty: 0, tz: 0 };
    const b: Pose = { ...a, theta: -Math.PI + 0.1 };
    expect(lerpPose(a, b, 0.5).theta).toBeCloseTo(Math.PI, 6);
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

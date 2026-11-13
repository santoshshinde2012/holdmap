import { describe, expect, it } from "vitest";
import { MOCK_SNAPSHOT } from "../mock";
import { mockAgents } from "../mock-agents";
import { attention, hotAgents, needsByAgent } from "../home";
import { layoutScene } from "./layout";
import { labelPriority, labelSize, pickLabels, placeLabels } from "./labels";
import { buildScene } from "./scene";
import type { AgentsReport } from "../types";
import { parseColor, toHex } from "./theme";
import { TOUR_AGENTS, tourBeats, tourFrame, tourLength } from "./tour";
import { demoActivity } from "../activity-demo";

const report = mockAgents(undefined, true);
/** What Home passes in: agents that need you themselves, and every "Needs you" title by agent. */
const attended = (r: AgentsReport | null) => { const items = attention(MOCK_SNAPSHOT.entries, r); return { hot: hotAgents(items), needs: needsByAgent(items, r) }; };
const scene = buildScene({ report, entries: MOCK_SNAPSHOT.entries, ...attended(report) });
const layout = layoutScene(scene);
const beats = tourBeats(scene, layout, 1.6);

describe("3D machine tour", () => {
  it("visits the overview, the exploded machine, agents that need you first, X-ray and the top view", () => {
    const focus = beats.map((b) => b.focus).filter(Boolean);
    expect(focus.length).toBeLessThanOrEqual(TOUR_AGENTS);
    const warned = scene.agents.filter((a) => a.kind === "agent" && a.warn).map((a) => a.id);
    expect(focus.slice(0, warned.length)).toEqual(warned);
    expect(beats.some((b) => b.xray)).toBe(true);
    expect(beats.at(-1)!.caption).toMatch(/floor plan/);
    expect(beats.every((b) => b.caption.length > 0)).toBe(true);
  });

  it("is a pure, looping function of time", () => {
    const total = tourLength(beats);
    expect(tourFrame(beats, 3.3)).toEqual(tourFrame(beats, 3.3));
    expect(tourFrame(beats, 3.3)).toEqual(tourFrame(beats, 3.3 + total));
    expect(tourFrame(beats, -1)!.beat).toBe(beats.length - 1);
    expect(tourFrame([], 1)).toBeNull();
  });

  it("holds each beat's pose once its flight ends", () => {
    let t = 0;
    for (const [i, b] of beats.entries()) {
      // Mid-hold, or late in a hold-less drift (its end is the next beat's start).
      const f = tourFrame(beats, t + (b.hold > 0 ? b.fly + b.hold * 0.5 : b.fly * 0.9))!;
      expect(f.beat).toBe(i);
      if (b.hold > 0) { expect(f.pose).toEqual(b.pose); expect(f.explode).toBe(b.explode); }
      t += b.fly + b.hold;
    }
  });

  it("stops at the recorded calls when there are some, and says why they stand apart", () => {
    const withCalls = buildScene({ report, entries: MOCK_SNAPSHOT.entries, activity: demoActivity() });
    const b = tourBeats(withCalls, layoutScene(withCalls), 1.6);
    expect(b.find((x) => x.caption.startsWith("Recorded calls"))!.caption).toMatch(/nothing ties them to a process/);
    expect(beats.some((x) => x.caption.startsWith("Recorded calls"))).toBe(false);
  });

  it("skips X-ray when no configuration was read", () => {
    const plain = buildScene({ report: mockAgents(), entries: MOCK_SNAPSHOT.entries });
    expect(tourBeats(plain, layoutScene(plain), 1.6).some((b) => b.xray)).toBe(false);
  });
});

describe("3D machine labels", () => {
  const state = { focus: null, selected: null, hovered: null, hiddenLayers: new Set<string>() };

  it("keeps the highest priority label where two would overlap", () => {
    const shown = pickLabels([
      { id: "low", x: 100, y: 100, w: 80, h: 20, priority: 10 },
      { id: "high", x: 110, y: 104, w: 80, h: 20, priority: 90 },
      { id: "apart", x: 300, y: 100, w: 80, h: 20, priority: 10 },
      { id: "off", x: 2, y: 100, w: 80, h: 20, priority: 99 },
      { id: "behind", x: 200, y: 300, w: 80, h: 20, priority: 99, behind: true },
    ], { width: 400, height: 400 });
    expect([...shown].sort()).toEqual(["apart", "high"]);
    // Labels under an overlay along the top don't count as shown.
    expect(pickLabels([{ id: "under", x: 200, y: 40, w: 80, h: 20, priority: 50 }], { width: 400, height: 400, top: 60 }).size).toBe(0);
  });

  it("steps a nameplate down a line rather than hide it, and keeps out from under the inspector", () => {
    const placed = placeLabels([
      { id: "a", x: 100, y: 100, w: 80, h: 20, priority: 90, alts: [26, 52] },
      { id: "b", x: 140, y: 100, w: 80, h: 20, priority: 90, alts: [26, 52] },
      { id: "c", x: 180, y: 100, w: 80, h: 20, priority: 90, alts: [26, 52] },
    ], { width: 400, height: 400 });
    expect([...placed]).toEqual([["a", 0], ["b", 26], ["c", 52]]);
    expect(pickLabels([{ id: "right", x: 350, y: 100, w: 80, h: 20, priority: 50 }], { width: 400, height: 400, right: 60 }).size).toBe(0);
  });

  it("shows agents always, a focused agent's parts, and warnings in the overview", () => {
    const agent = scene.agents[0];
    const own = scene.parts.find((p) => p.owner === agent.id && p.kind === "tool")!;
    const other = scene.parts.find((p) => p.owner !== agent.id && p.kind === "tool")!;
    expect(labelPriority(agent, state)).toBe(90);
    expect(labelPriority(scene.agents[1], { ...state, focus: agent.id })).toBeNull();
    expect(labelPriority(own, state)).toBeNull();
    expect(labelPriority(own, { ...state, focus: agent.id })).toBe(70);
    expect(labelPriority(other, { ...state, focus: agent.id })).toBeNull();
    expect(labelPriority(own, { ...state, selected: own.id })).toBe(100);
    expect(labelPriority(own, { ...state, focus: agent.id, hiddenLayers: new Set(["runtime"]) })).toBeNull();
    const warned = scene.parts.find((p) => p.warn && p.kind !== "agent")!;
    expect(labelPriority(warned, state)).toBe(60);
  });

  it("names only the parts a pressed reading lights up, keeping towers as landmarks", () => {
    const port = scene.parts.find((p) => p.kind === "port")!;
    const tool = scene.parts.find((p) => p.kind === "tool")!;
    const lit = { ...state, highlight: new Set([port.id]) };
    expect(labelPriority(port, lit)).toBe(75);
    expect(labelPriority(tool, lit)).toBeNull();
    expect(labelPriority(scene.agents[0], lit)).toBe(40);
    expect(labelPriority(port, { ...lit, selected: port.id })).toBe(100);
  });

  it("names a recorded call only when it failed, or on hover or selection", () => {
    const withCalls = buildScene({ report, entries: MOCK_SNAPSHOT.entries, activity: demoActivity() });
    const ok = withCalls.parts.find((p) => p.kind === "call" && !p.warn)!;
    const failed = withCalls.parts.find((p) => p.kind === "call" && p.warn)!;
    expect(labelPriority(ok, state)).toBeNull();
    expect(labelPriority(failed, state)).toBe(60);
    expect(labelPriority(ok, { ...state, hovered: ok.id })).toBe(95);
  });

  it("sizes labels from their text", () => {
    expect(labelSize("Cursor", null).h).toBeLessThan(labelSize("Cursor", "IDE").h);
    expect(labelSize("A much longer label", null).w).toBeGreaterThan(labelSize("Short", null).w);
  });
});

describe("3D machine colours", () => {
  it("parses the colour forms getComputedStyle returns", () => {
    expect(parseColor("rgb(80, 72, 229)")).toEqual({ r: 80, g: 72, b: 229 });
    expect(parseColor("rgba(17, 24, 39, 0.08)")).toEqual({ r: 17, g: 24, b: 39 });
    expect(parseColor("rgb(17 24 39 / 0.08)")).toEqual({ r: 17, g: 24, b: 39 });
    expect(parseColor("#5048e5")).toEqual({ r: 80, g: 72, b: 229 });
    expect(parseColor("#fff")).toEqual({ r: 255, g: 255, b: 255 });
    expect(parseColor("color(srgb 1 0.5 0)")).toEqual({ r: 255, g: 127.5, b: 0 });
    expect(parseColor("transparent")).toBeNull();
    expect(toHex({ r: 80, g: 72, b: 229 })).toBe(0x5048e5);
  });
});

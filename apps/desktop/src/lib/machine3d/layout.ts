// Deterministic 3D placement. Agents stand on a ring on the board at the same angles as the 2D
// map, so the top-down view is the map. Each agent's parts fan out in its own sector, on their
// layer: ports and folders on the board, processes on the runtime plate, configuration on top.
// Positions depend only on the scene's identity, never on resource samples.
import type { Layer, MachineScene, Part, PartKind } from "./scene";

export interface Size { w: number; h: number; d: number }
export interface Placed { x: number; z: number; layer: Layer; angle: number }
export interface MachineLayout {
  parts: Map<string, Placed>;
  /** Board radius; the runtime and configuration plates share it. */
  radius: number;
  /** Agent ring radius (0 for a single agent, which stands in the middle). */
  ring: number;
  /** Agent angle on the ring, by agent id. */
  angles: Map<string, number>;
  /** Radius of the recorded-calls lane, or 0 when no calls are drawn. */
  callsRadius: number;
  signature: string;
}

export const SIZE: Record<Exclude<PartKind, "agent">, Size> = {
  dormant: { w: 1.5, h: 0.9, d: 1.5 },
  project: { w: 1.1, h: 0.08, d: 0.8 },
  port: { w: 0.6, h: 0.34, d: 0.44 },
  remote: { w: 0.56, h: 0.56, d: 0.56 },
  tool: { w: 0.8, h: 0.5, d: 0.8 },
  mcp: { w: 0.9, h: 0.16, d: 0.9 },
  skill: { w: 0.7, h: 0.1, d: 0.9 },
  instructions: { w: 0.9, h: 0.3, d: 0.7 },
  more: { w: 0.6, h: 0.2, d: 0.6 },
  call: { w: 0.58, h: 0.22, d: 0.22 },
};
export const AGENT_MAX_HEIGHT = 2.2;

/** Agent tower size: height grows with memory on a log scale, so one large IDE doesn't dwarf the rest. */
export function partSize(p: Pick<Part, "kind" | "memory">): Size {
  if (p.kind !== "agent") return SIZE[p.kind];
  const h = 0.6 + 0.28 * Math.log2(1 + Math.max(0, p.memory) / (64 * 1024 * 1024));
  return { w: 1.5, h: Math.min(AGENT_MAX_HEIGHT, h), d: 1.5 };
}

/** Plate heights (the base parts stand on), assembled → exploded. They don't move with samples. */
const PLATE: Record<Layer, [number, number]> = {
  board: [0, 0], runtime: [AGENT_MAX_HEIGHT + 0.55, AGENT_MAX_HEIGHT + 2.9], config: [AGENT_MAX_HEIGHT + 1.8, AGENT_MAX_HEIGHT + 6.4],
  calls: [AGENT_MAX_HEIGHT + 3.1, AGENT_MAX_HEIGHT + 8.4],
};
export function plateY(layer: Layer, explode: number): number {
  const [a, b] = PLATE[layer];
  return a + (b - a) * Math.min(1, Math.max(0, explode));
}
/** Highest point drawn (labels included) at an explode amount, with or without the calls lane. */
export const sceneTop = (explode: number, calls = false) => plateY(calls ? "calls" : "config", explode) + 1.4;

/** Item spacing along a row, and the distance between rows. */
const SPACING: Record<Layer, number> = { board: 1.25, runtime: 1.3, config: 1.3, calls: 0.9 };
const ROW = 1.25;

interface Slot { r: number; angle: number }
/** `count` slots fanned around `angle` within `sector`, filling rows outward from `r0`. */
export function fan(count: number, angle: number, sector: number, r0: number, spacing: number, row = ROW): Slot[] {
  const full = sector >= Math.PI * 2 - 1e-6;
  const out: Slot[] = [];
  let r = Math.max(r0, 0.8);
  while (out.length < count) {
    const usable = full ? Math.PI * 2 * r : sector * 0.84 * r;
    const cap = Math.max(1, Math.floor(usable / spacing) + (full ? 0 : 1));
    const k = Math.min(cap, count - out.length);
    const step = full ? (Math.PI * 2) / k : k > 1 ? Math.min(spacing / r, (sector * 0.84) / (k - 1)) : 0;
    const start = full ? angle : angle - (step * (k - 1)) / 2;
    for (let i = 0; i < k; i++) out.push({ r, angle: start + i * step });
    r += row;
  }
  return out;
}

const outer = (slots: Slot[]) => slots.reduce((m, s) => Math.max(m, s.r), 0);

export function layoutScene(scene: MachineScene): MachineLayout {
  // Running agents take the 2D map's angles; agents that aren't running share out the gaps between
  // them, so they never move a running agent. Configuration then splits each sector between the
  // agents that stand in it. With nothing running, the agents that aren't take the ring evenly.
  const running = scene.agents.filter((a) => a.kind === "agent");
  const dormant = scene.agents.filter((a) => a.kind !== "agent");
  const hosts = running.length ? running.length : dormant.length;
  const sector = hosts > 0 ? (Math.PI * 2) / hosts : Math.PI * 2;
  // Agents that aren't running, by the gap (after running agent g) they stand in.
  const perGap = running.length ? running.map((_, g) => Math.floor(dormant.length / running.length) + (g < dormant.length % running.length ? 1 : 0)) : [];
  const share = running.length ? 1 + Math.max(0, ...perGap) : 1;
  const configSector = sector / share;
  const ring = scene.agents.length <= 1 ? 0 : Math.max(3.2, (scene.agents.length * 3.6) / (Math.PI * 2));
  const parts = new Map<string, Placed>();
  const angles = new Map<string, number>();
  const byOwner = new Map<string, Part[]>();
  for (const p of scene.parts) if (p.kind !== "agent" && p.kind !== "dormant") byOwner.set(p.owner, [...(byOwner.get(p.owner) ?? []), p]);
  let extent = ring + 2.6;
  const remotes: [Part, number][] = [];
  const slots: (readonly [Part, number])[] = running.map((a, i) => [a, -Math.PI / 2 + i * sector] as const);
  if (running.length) {
    let next = 0;
    perGap.forEach((count, g) => {
      for (let k = 0; k < count; k++) slots.push([dormant[next++], -Math.PI / 2 + (g + (k + 1) / (count + 1)) * sector] as const);
    });
  } else dormant.forEach((a, j) => slots.push([a, -Math.PI / 2 + j * sector] as const));

  for (const [agent, a] of slots) {
    // The same angle convention as the 2D map: the first agent at the top (the back, in 3D).
    angles.set(agent.id, a);
    // `+ 0` keeps a centred agent at 0, not -0.
    parts.set(agent.id, { x: Math.cos(a) * ring + 0, z: Math.sin(a) * ring + 0, layer: "board", angle: a });
    const own = byOwner.get(agent.id) ?? [];
    const place = (items: Part[], slots: Slot[]) => items.forEach((p, j) => parts.set(p.id, { x: Math.cos(slots[j].angle) * slots[j].r, z: Math.sin(slots[j].angle) * slots[j].r, layer: p.layer, angle: slots[j].angle }));
    const start = ring === 0 ? 2.2 : ring;

    const folders = own.filter((p) => p.kind === "project" || (p.kind === "more" && p.variant === "folders"));
    const ports = own.filter((p) => p.kind === "port" || (p.kind === "more" && p.variant === "ports"));
    const folderSlots = fan(folders.length, a, sector, start + 1.8, SPACING.board);
    const portSlots = fan(ports.length, a, sector, Math.max(start + 1.8, outer(folderSlots)) + (folders.length ? ROW : 0), SPACING.board);
    place(folders, folderSlots); place(ports, portSlots);

    const runtime = own.filter((p) => p.layer === "runtime");
    const runtimeSlots = fan(runtime.length, a, sector, start, SPACING.runtime);
    place(runtime, runtimeSlots);

    const config = own.filter((p) => p.layer === "config");
    const configSlots = fan(config.length, a, configSector, Math.max(start - 0.6, 1.6), SPACING.config);
    place(config, configSlots);

    for (const p of own) if (p.kind === "remote") remotes.push([p, a]);
    extent = Math.max(extent, outer(folderSlots), outer(portSlots), outer(runtimeSlots), outer(configSlots));
  }

  const radius = extent + 1.0;
  // Recorded calls: a lane of their own above the plates, newest at the front, older ones round
  // the ring in order. Their place says when they were reported, never which agent made them.
  const calls = scene.parts.filter((p) => p.layer === "calls");
  const callsRadius = calls.length ? Math.max(2.4, radius * 0.55) : 0;
  const step = calls.length ? Math.min((Math.PI * 2) / calls.length, 0.9 / callsRadius) : 0;
  calls.forEach((p, i) => {
    const a = Math.PI / 2 - i * step;
    parts.set(p.id, { x: Math.cos(a) * callsRadius, z: Math.sin(a) * callsRadius, layer: "calls", angle: a });
  });
  // Remote hosts float just beyond the board's edge, in their agent's direction.
  for (const [p, a] of remotes) parts.set(p.id, { x: Math.cos(a) * (radius + 1.5), z: Math.sin(a) * (radius + 1.5), layer: "board", angle: a });
  return { parts, radius, ring, angles, callsRadius, signature: scene.signature };
}

/** Bounding radius in the ground plane, remote hosts included. */
export const groundRadius = (layout: MachineLayout) => layout.radius + ([...layout.parts.values()].some((p) => Math.hypot(p.x, p.z) > layout.radius) ? 2.1 : 0);

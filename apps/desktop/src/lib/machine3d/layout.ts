// Deterministic 3D placement. Each agent owns a district on the board: its tower at the back,
// the folders it works in, then the ports it holds along the front edge. Its processes and its
// configuration sit on floors stacked over the same footprint, so everything above a district
// belongs to that agent and no wire is needed to say so. Districts stand side by side, a street
// apart; the default view turns only as far as the streets allow, so no district hides another.
// Positions are on a quarter-unit grid and depend only on the scene's identity, never on
// resource samples.
import type { Layer, MachineScene, Part, PartKind } from "./scene";

export interface Size { w: number; h: number; d: number }
export interface Placed { x: number; z: number; layer: Layer }
export interface Rect { x0: number; x1: number; z0: number; z1: number }
export interface District extends Rect {
  id: string;
  /** Each floor's tray, sized to its parts; null where the agent has none. */
  floors: { runtime: Rect | null; config: Rect | null };
  /** The antenna at the back-right corner, for an agent with remote hosts. */
  mast: { x: number; z: number } | null;
}
export interface MachineLayout {
  parts: Map<string, Placed>;
  /** By agent id, in drawing order: running agents in report order, then agents that aren't. */
  districts: Map<string, District>;
  board: Rect;
  /** The "This computer" chip at the left end of the board. */
  hub: { x: number; z: number };
  /** The recorded-calls shelf at the right end, apart from every district, or null. */
  calls: Rect | null;
  /** Default view: turned as far as the streets allow, raised a little more for each row. */
  theta: number;
  phi: number;
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
/** The tower's footprint, and the CPU gauge ring drawn around its base. */
export const TOWER = 1.5;
export const GAUGE_RADIUS = 1.12;

/** Agent tower size: height grows with memory on a log scale, so one large IDE doesn't dwarf the rest. */
export function partSize(p: Pick<Part, "kind" | "memory">): Size {
  if (p.kind !== "agent") return SIZE[p.kind];
  const h = 0.6 + 0.28 * Math.log2(1 + Math.max(0, p.memory) / (64 * 1024 * 1024));
  return { w: TOWER, h: Math.min(AGENT_MAX_HEIGHT, h), d: TOWER };
}

/** Floor heights (the base parts stand on), assembled → exploded. They don't move with samples. */
const PLATE: Record<Layer, [number, number]> = {
  board: [0, 0], runtime: [AGENT_MAX_HEIGHT + 0.55, AGENT_MAX_HEIGHT + 2.9], config: [AGENT_MAX_HEIGHT + 1.8, AGENT_MAX_HEIGHT + 6.4],
  // Recorded calls aren't one of the machine's floors: their shelf stays on the board.
  calls: [0, 0],
};
export function plateY(layer: Layer, explode: number): number {
  const [a, b] = PLATE[layer];
  return a + (b - a) * Math.min(1, Math.max(0, explode));
}
/** Where remote hosts float: above the top floor, at the head of their agent's mast. */
export const skyY = (explode: number) => plateY("config", explode) + 1.7;
/** Highest point drawn (labels included) at an explode amount. */
export const sceneTop = (explode: number) => skyY(explode) + 0.9;

/** Grid snap, district padding, the street between districts and the gap between rows. */
const GRID = 0.25;
const PAD = 0.5;
export const STREET = 1.5;
/** Rows stand far enough apart that a row's exploded floors stay clear of the row behind: on
 *  screen, each row is its own band. One row holds up to eight districts. */
const ROW_GAP = 11;
const PER_ROW = 8;
/** Spacing and columns per kind of row. */
const STEP = { port: 1.05, folder: 1.35, runtime: 1.3, config: 1.25, calls: 0.9 } as const;
const COLS = { port: 4, folder: 3, runtime: 3, config: 4, calls: 3 } as const;

const snap = (v: number) => Math.round(v / GRID) * GRID + 0;

interface Grid { cols: number; rows: number; w: number; d: number }
/** Columns and rows for `n` items: square-ish, at most `max` wide. */
function grid(n: number, step: number, max: number, square = true): Grid {
  if (!n) return { cols: 0, rows: 0, w: 0, d: 0 };
  const cols = Math.min(max, n, square ? Math.max(1, Math.ceil(Math.sqrt(n * 1.4))) : max);
  const rows = Math.ceil(n / cols);
  return { cols, rows, w: cols * step, d: rows * step };
}

/** Local positions (centred on x = 0) for `items` in a grid whose first row's centre is at `z`. */
function place(items: Part[], g: Grid, step: number, z: number): [Part, number, number][] {
  return items.map((p, i) => {
    const row = Math.floor(i / g.cols), col = i % g.cols;
    const inRow = Math.min(g.cols, items.length - row * g.cols);
    return [p, (col - (inRow - 1) / 2) * step, z + row * step] as [Part, number, number];
  });
}

const isKind = (...kinds: string[]) => (p: Part) => kinds.includes(p.kind);
const moreOf = (variant: string) => (p: Part) => p.kind === "more" && p.variant === variant;

interface Plan {
  agent: Part;
  w: number;
  d: number;
  /** Ground placements: x from the district's centre, z from its back edge. */
  local: [Part, number, number][];
  remote: Part | undefined;
  /** Floor contents; their grids are centred once the row's depth is known. */
  runtime: Part[];
  config: Part[];
  gR: Grid;
  gC: Grid;
}

function plan(agent: Part, own: Part[]): Plan {
  const folders = own.filter((p) => p.kind === "project" || moreOf("folders")(p));
  const ports = own.filter((p) => p.kind === "port" || moreOf("ports")(p));
  const runtime = own.filter((p) => p.layer === "runtime");
  // Configuration reads as rows of a kind: MCP servers, then skills, then rules.
  const order: PartKind[] = ["mcp", "skill", "instructions"];
  const config = own.filter((p) => p.layer === "config").sort((a, b) => {
    const rank = (p: Part) => p.kind === "more" ? order.indexOf(p.variant === "skills" ? "skill" : "mcp") + 0.5 : order.indexOf(p.kind);
    return rank(a) - rank(b);
  });
  const gF = grid(folders.length, STEP.folder, COLS.folder, false);
  const gP = grid(ports.length, STEP.port, COLS.port, false);
  const gR = grid(runtime.length, STEP.runtime, COLS.runtime);
  const gC = grid(config.length, STEP.config, COLS.config);
  // Ground, back to front: the tower in its gauge ring, folders, then ports on the front edge.
  const towerRow = GAUGE_RADIUS * 2 + 0.2;
  const folderZ = towerRow + (gF.rows ? 0.35 + SIZE.project.d / 2 : 0);
  const portZ = folderZ + (gF.rows ? (gF.rows - 1) * STEP.folder + SIZE.project.d / 2 : 0) + (gP.rows ? 0.45 + SIZE.port.d / 2 : 0);
  const groundD = gP.rows ? portZ + (gP.rows - 1) * STEP.port + SIZE.port.d / 2 + 0.35 : gF.rows ? folderZ + (gF.rows - 1) * STEP.folder + SIZE.project.d / 2 : towerRow;
  const w = Math.max(towerRow, gF.w, gP.w, gR.w, gC.w, 2.6) + PAD * 2;
  const d = Math.max(groundD, gR.d, gC.d) + PAD * 2;
  const local: [Part, number, number][] = [
    ...place(folders, gF, STEP.folder, PAD + folderZ),
    ...place(ports, gP, STEP.port, PAD + portZ),
  ];
  return { agent, w, d, local, remote: own.find(isKind("remote")), runtime, config, gR, gC };
}

export function layoutScene(scene: MachineScene): MachineLayout {
  const byOwner = new Map<string, Part[]>();
  for (const p of scene.parts) if (p.kind !== "agent" && p.kind !== "dormant" && p.layer !== "calls") byOwner.set(p.owner, [...(byOwner.get(p.owner) ?? []), p]);
  // Running agents first (their colours follow the report order), then agents that aren't running.
  const order = [...scene.agents.filter((a) => a.kind === "agent"), ...scene.agents.filter((a) => a.kind !== "agent")];
  const plans = order.map((a) => plan(a, byOwner.get(a.id) ?? []));
  const rows = Math.max(1, Math.ceil(plans.length / PER_ROW));
  // One depth per row keeps every front edge, and every nameplate on it, on one line.
  const depth = snap(Math.max(PAD * 2 + 2.6, ...plans.map((p) => p.d)));
  const parts = new Map<string, Placed>();
  const districts = new Map<string, District>();
  const rowWidth = (r: number) => plans.slice(r * PER_ROW, (r + 1) * PER_ROW).reduce((s, p, i) => s + snap(p.w) + (i ? STREET : 0), 0);
  const widest = Math.max(0, ...Array.from({ length: rows }, (_, r) => rowWidth(r)));

  plans.forEach((pl, i) => {
    const row = Math.floor(i / PER_ROW);
    const first = row * PER_ROW;
    // Each row centred on the widest.
    let x0 = -widest / 2 + (rowWidth(row) < widest ? (widest - rowWidth(row)) / 2 : 0);
    for (let k = first; k < i; k++) x0 += snap(plans[k].w) + STREET;
    x0 = snap(x0);
    const w = snap(pl.w);
    const z0 = snap(-depth / 2 - row * (depth + ROW_GAP));
    const cx = x0 + w / 2;
    const d: District = { id: pl.agent.id, x0, x1: x0 + w, z0, z1: z0 + depth, floors: { runtime: null, config: null }, mast: null };
    parts.set(pl.agent.id, { x: snap(cx), z: snap(z0 + PAD + GAUGE_RADIUS + 0.1), layer: "board" });
    for (const [p, x, z] of pl.local) parts.set(p.id, { x: snap(cx + x), z: snap(z0 + z), layer: p.layer });
    // Floors: their grids centred in the district.
    for (const [layer, items, g, step] of [["runtime", pl.runtime, pl.gR, STEP.runtime], ["config", pl.config, pl.gC, STEP.config]] as const) {
      if (!items.length) continue;
      const top = z0 + (depth - g.d) / 2;
      for (const [p, x, z] of place(items, g, step, top + step / 2)) parts.set(p.id, { x: snap(cx + x), z: snap(z), layer: p.layer });
      // A tray a little larger than its grid, never narrower than the tower below it.
      const tw = Math.max(g.w, TOWER + 0.5) + 0.3, td = g.d + 0.3;
      d.floors[layer] = { x0: snap(cx - tw / 2), x1: snap(cx + tw / 2), z0: snap(top - 0.15), z1: snap(top - 0.15 + td) };
    }
    if (pl.remote) {
      d.mast = { x: x0 + w - 0.5, z: z0 + 0.5 };
      parts.set(pl.remote.id, { x: d.mast.x, z: d.mast.z, layer: "board" });
    }
    districts.set(pl.agent.id, d);
  });

  // The hub stands at the left end of the front row; the recorded-calls shelf at the right end,
  // so a call's place never suggests which agent made it.
  const front = [...districts.values()].slice(0, PER_ROW);
  const left = front.length ? Math.min(...front.map((d) => d.x0)) : 0;
  const right = districts.size ? Math.max(...[...districts.values()].map((d) => d.x1)) : 0;
  const hub = { x: snap(left - STREET - 0.8), z: snap(depth / 2 - 1.2) };
  const callParts = scene.parts.filter((p) => p.layer === "calls");
  let calls: Rect | null = null;
  if (callParts.length) {
    const g = grid(callParts.length, STEP.calls, COLS.calls);
    const x0 = snap(right + STREET), w = snap(g.w + PAD * 2), z0 = snap(-depth / 2);
    calls = { x0, x1: x0 + w, z0, z1: z0 + Math.max(depth, snap(g.d + PAD * 2)) };
    const cx = x0 + w / 2;
    // Newest first, from the back left.
    for (const [p, x, z] of place(callParts, g, STEP.calls, z0 + PAD + STEP.calls / 2)) parts.set(p.id, { x: snap(cx + x), z: snap(z), layer: "calls" });
  }
  const back = districts.size ? Math.min(...[...districts.values()].map((d) => d.z0)) : -depth / 2;
  const board: Rect = { x0: snap(hub.x - 1.4), x1: snap((calls?.x1 ?? right) + 0.8), z0: snap(back - 0.8), z1: snap(depth / 2 + 0.8) };
  // Turned so a district's far edge stays clear of its neighbour: tan θ · depth < street.
  // Nearly square on to the row, so it fills the width; just enough turn to show the sides.
  const theta = Math.min(0.14, Math.atan((STREET - 0.6) / depth));
  const phi = Math.min(0.9, 0.62 + 0.08 * (rows - 1));
  return { parts, districts, board, hub, calls, theta, phi, signature: scene.signature };
}

/** Room past the board's ends for the labels that hang over them (the chip's, the shelf's). */
const LABEL_ROOM = 1.4;

/** The model's bounds at an explode amount: the board, the floors and the sky above them. */
export function bounds(layout: MachineLayout, explode: number): { x0: number; x1: number; y0: number; y1: number; z0: number; z1: number } {
  const { board } = layout;
  return { x0: board.x0 - LABEL_ROOM, x1: board.x1 + LABEL_ROOM, y0: -0.36, y1: sceneTop(explode), z0: board.z0, z1: board.z1 };
}

// The WebGL renderer for the 3D machine (three.js). It only projects the pure scene and layout:
// what is drawn and how sure it is are decided in scene.ts, where it stands in layout.ts. It
// renders on demand through an orthographic camera; the continuous effects (flow along
// connections, ripples round exposed ports, a running server's pulse) run at most at 30 fps,
// pause while the window is hidden, and stand still with reduced motion.
//
// Visual grammar, one meaning per form:
// - shapes say what a part is, and the icon on its top face says it again;
// - colour says whose it is (the agent's colour) or that it needs you (amber, red);
// - solid means seen now, faded means inferred, blueprint means a file on disk;
// - rings measure or warn: the CPU gauge round a tower, ripples round a port open to the
//   network, a red ring round an agent that needs you, the accent ring round a selection;
// - lines carry a direction: an arrowhead at the end that receives, dots moving the same way.
//   Ownership needs no line (everything over a district is that agent's); those lines appear
//   only for the part under the pointer or selected.
import {
  AmbientLight, BoxGeometry, BufferGeometry, CanvasTexture, CapsuleGeometry, ConeGeometry, CylinderGeometry, DirectionalLight, EdgesGeometry,
  Float32BufferAttribute, Group, HemisphereLight, LineBasicMaterial, LineDashedMaterial, LineLoop, LineSegments, Mesh, MeshBasicMaterial,
  MeshStandardMaterial, OrthographicCamera, PCFShadowMap, PlaneGeometry, QuadraticBezierCurve3, Raycaster, RepeatWrapping, RingGeometry, Scene,
  SphereGeometry, Sprite, SpriteMaterial, SRGBColorSpace, TubeGeometry, Vector2, Vector3, WebGLRenderer, type Material,
} from "three";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";
import { roundedOutline as outline, roundedPlane, roundedSlab } from "./geometry";
import { clampPose, EASE, halfHeight, lerpPose, posesEqual, type Pose } from "./camera";
import { GAUGE_RADIUS, partSize, plateY, skyY, type District, type MachineLayout, type Placed, type Rect, type Size } from "./layout";
import { monogram } from "../agents";
import type { Layer, MachineScene, Part, Wire } from "./scene";
import { mixRgb, toHex, type Rgb, type StageColors } from "./theme";

export interface StageView {
  /** Target explode amount, 0 (assembled) to 1 (pulled apart). */
  explode: number;
  xray: boolean;
  /** Agent whose parts stay lit; the rest fade. */
  focus: string | null;
  selected: string | null;
  hovered: string | null;
  hidden: ReadonlySet<Layer>;
  reduced: boolean;
  /** Parts a lens or a clicked reading lights up; everything else steps back. */
  highlight?: ReadonlySet<string> | null;
}

export type Support = "ok" | "slow" | "none";
/** WebGL 2 availability, and whether the browser would fall back to software rendering. */
export function webglSupport(): Support {
  if (typeof document === "undefined" || typeof WebGL2RenderingContext === "undefined") return "none";
  const probe = (strict: boolean) => {
    const gl = document.createElement("canvas").getContext("webgl2", { failIfMajorPerformanceCaveat: strict });
    gl?.getExtension("WEBGL_lose_context")?.loseContext();
    return !!gl;
  };
  try {
    if (probe(true)) return "ok";
    return probe(false) ? "slow" : "none";
  } catch {
    return "none";
  }
}

/** Where a label sits relative to its anchor: above a part, or below a front edge (nameplates). */
export interface Anchor { x: number; y: number; behind: boolean; below?: boolean; end?: boolean }

type Vec = { x: number; y: number; z: number };
type Tinted = MeshStandardMaterial | MeshBasicMaterial | LineBasicMaterial | SpriteMaterial;
interface Tint { mat: Tinted; base: number; glass?: boolean; blueprint?: boolean }
interface Gauge { track: Mesh; arc: Mesh; value: number; key: number }
interface Item {
  part: Part;
  group: Group;
  body: Mesh;
  size: Size;
  height: number;
  tints: Tint[];
  solids: MeshStandardMaterial[];
  edgeMat: LineBasicMaterial;
  edgeBase: number;
  cap?: Mesh;
  /** Agent initials on the tower cap, and what they were drawn from. */
  mono?: Mesh;
  monoKey?: string;
  /** The icon on a part's top face, and what it was drawn from. */
  glyph?: Mesh;
  glyphKey?: string;
  gauge?: Gauge;
  halo?: Mesh;
  ripples?: Mesh[];
  led?: Mesh;
  mast?: Mesh;
  pin?: Sprite;
  pinKey?: string;
  slices?: Group;
  /** What the slices were built from, so hovers and scans with the same samples don't rebuild them. */
  slicesKey?: string;
  /** Part of the opening build-up rather than a change: it rises in without the "new" glow. */
  intro: boolean;
  sel: Mesh;
  from: Vec;
  to: Placed;
  moveAt: number;
  pos: Vec;
  opacity: number;
  scale: number;
  /** Raised a little while under the pointer. */
  lift: number;
  born: number;
  dying: number | null;
  fresh: number;
}
type WireMode = "trace" | "rod" | "arc" | "beam" | "uplink";
interface WireItem {
  wire: Wire;
  mode: WireMode;
  segs: Mesh[];
  tube: Mesh | null;
  mat: MeshBasicMaterial;
  arrow: Mesh | null;
  packets: Mesh[];
  base: number;
  /** Ownership lines only appear for the part under the pointer or selected; this eases them in and out. */
  demand: boolean;
  shown: number;
  key: string;
  ends: string;
}

const UP = new Vector3(0, 1, 0);
/** Materials don't release their textures; the board, icons, pins and initials are canvas textures. */
function disposeMaterial(mat: Material) {
  (mat as MeshBasicMaterial).map?.dispose();
  mat.dispose();
}
const MOVE_MS = 700, EXPLODE_MS = 900, FRESH_MS = 4000, ENTER_MS = 520, EXIT_MS = 260, INTRO_MS = 760;
/** Opening build-up: each floor starts after the one below, districts a little apart. */
const INTRO_DELAY: Record<Layer, number> = { board: 0, runtime: 260, config: 520, calls: 400 };
/** Continuous effects: smooth while the window has focus, a slow tick while it doesn't. */
const PULSE_FPS = 30, IDLE_FPS = 8;
/** The orthographic camera stands this far back along its line of sight. */
const STANDOFF = 220;
/** Flow speed along a connection, in path lengths per second; ripple period. */
const FLOW = 0.32, RIPPLE_S = 2.6;
/** Floor slab thickness. */
const SLAB = 0.05;

export class MachineStage {
  readonly renderer: WebGLRenderer;
  private readonly scene3 = new Scene();
  private readonly camera = new OrthographicCamera(-1, 1, 1, -1, 1, STANDOFF * 3);
  private readonly root = new Group();
  private readonly decor = new Group();
  private readonly sun = new DirectionalLight(0xffffff, 1.5);
  private readonly hemi = new HemisphereLight(0xffffff, 0xdddddd, 1.1);
  private readonly ambient = new AmbientLight(0xffffff, 0.25);
  private readonly ray = new Raycaster();
  private readonly geo = {
    tower: new RoundedBoxGeometry(1.5, 1, 1.5, 3, 0.12).translate(0, 0.5, 0),
    cap: new RoundedBoxGeometry(1.2, 0.06, 1.2, 2, 0.03),
    slice: new CylinderGeometry(0.5, 0.5, 0.05, 24),
    port: new RoundedBoxGeometry(0.6, 0.34, 0.44, 2, 0.04).translate(0, 0.17, 0),
    socket: new BoxGeometry(0.36, 0.12, 0.02),
    tile: new RoundedBoxGeometry(1.1, 0.08, 0.8, 2, 0.03).translate(0, 0.04, 0),
    tab: new BoxGeometry(0.42, 0.05, 0.16),
    sat: new SphereGeometry(0.3, 10, 7),
    mast: new CylinderGeometry(0.03, 0.03, 1, 8).translate(0, -0.5, 0),
    hex: new CylinderGeometry(0.44, 0.44, 0.5, 6).translate(0, 0.25, 0),
    devbox: new RoundedBoxGeometry(0.8, 0.5, 0.62, 2, 0.08).translate(0, 0.25, 0),
    puck: new CylinderGeometry(0.34, 0.34, 0.36, 32).translate(0, 0.18, 0),
    cube: new RoundedBoxGeometry(0.56, 0.5, 0.56, 2, 0.05).translate(0, 0.25, 0),
    chip: new CylinderGeometry(0.46, 0.46, 0.16, 6).translate(0, 0.08, 0),
    card: new RoundedBoxGeometry(0.7, 0.1, 0.9, 2, 0.03).translate(0, 0.05, 0),
    sheet: new BoxGeometry(0.9, 0.07, 0.7).translate(0, 0.035, 0),
    more: new CylinderGeometry(0.3, 0.3, 0.2, 32).translate(0, 0.1, 0),
    led: new SphereGeometry(0.075, 16, 10),
    rod: new CylinderGeometry(1, 1, 1, 10, 1, true),
    trace: new BoxGeometry(1, 1, 1),
    arrow: new ConeGeometry(0.09, 0.24, 14),
    packet: new SphereGeometry(0.065, 12, 8),
    call: new CapsuleGeometry(0.1, 0.38, 4, 10).rotateZ(Math.PI / 2).translate(0, 0.11, 0),
    mono: new PlaneGeometry(0.98, 0.98).rotateX(-Math.PI / 2),
    glyph: new PlaneGeometry(1, 1).rotateX(-Math.PI / 2),
  };
  private readonly edgeCache = new Map<BufferGeometry, EdgesGeometry>();
  private readonly ringCache = new Map<string, RingGeometry>();
  private readonly textures = new Map<string, CanvasTexture>();
  private readonly items = new Map<string, Item>();
  private readonly wires = new Map<string, WireItem>();
  /** Parts each part connects to, and every part over an agent's district, for the pointer's neighbourhood. */
  private neighbours = new Map<string, Set<string>>();
  private readonly pickables: Mesh[] = [];
  private colors: StageColors;
  private view: StageView = { explode: 0, xray: false, focus: null, selected: null, hovered: null, hidden: new Set(), reduced: false };
  private layout: MachineLayout | null = null;
  private explode = { value: 0, from: 0, to: 0, at: 0 };
  private pose: Pose = { theta: 0.3, phi: 0.52, distance: 40, tx: 0, ty: 1.5, tz: 0 };
  private flight: { from: Pose; to: Pose; at: number; ms: number } | null = null;
  private floors: { layer: "runtime" | "config"; district: District; slab: Mesh; rim: LineLoop }[] = [];
  private guides: LineSegments | null = null;
  private guidesAt = -1;
  private shelf: { slab: Mesh; rim: LineLoop } | null = null;
  private width = 1;
  private height = 1;
  private insets = { top: 0, bottom: 0, right: 0 };
  /** Whether the last step left a transition running. */
  private busy = false;
  private decorKey = "";
  private raf = 0;
  private lastRender = 0;
  private lastFrame = 0;
  private dirty = true;
  private first = true;
  private disposed = false;
  /** Called after every rendered frame, so labels can follow the camera. */
  onrender: (() => void) | null = null;
  onlost: (() => void) | null = null;

  constructor(readonly canvas: HTMLCanvasElement, colors: StageColors) {
    this.renderer = new WebGLRenderer({ canvas, antialias: true, alpha: true, preserveDrawingBuffer: true, powerPreference: "default" });
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = PCFShadowMap;
    this.renderer.shadowMap.autoUpdate = false;
    this.renderer.setPixelRatio(Math.min(2, globalThis.devicePixelRatio || 1));
    this.colors = colors;
    this.sun.castShadow = true;
    this.sun.shadow.mapSize.set(2048, 2048);
    this.sun.shadow.bias = -0.0004;
    this.sun.shadow.normalBias = 0.02;
    this.scene3.add(this.hemi, this.ambient, this.sun, this.sun.target, this.decor, this.root);
    canvas.addEventListener("webglcontextlost", this.lost);
    this.applyTheme();
  }

  private lost = (e: Event) => { e.preventDefault(); this.stop(); this.onlost?.(); };

  // ---------- inputs ----------

  setColors(colors: StageColors) {
    this.colors = colors;
    for (const t of this.textures.values()) t.dispose();
    this.textures.clear();
    this.applyTheme();
    for (const item of this.items.values()) { item.glyphKey = undefined; item.pinKey = undefined; item.monoKey = undefined; this.restyle(item); }
    for (const w of this.wires.values()) this.restyleWire(w);
    this.wake();
  }

  setScene(scene: MachineScene, layout: MachineLayout) {
    const now = performance.now();
    // The board, districts and floors depend only on where districts stand, not on the parts in
    // them, so new samples or calls don't rebuild them.
    const decorKey = JSON.stringify([layout.board, layout.hub, layout.calls, [...layout.districts.values()]]);
    this.layout = layout;
    if (decorKey !== this.decorKey) { this.decorKey = decorKey; this.buildDecor(layout); }
    const seen = new Set<string>();
    const order = [...layout.districts.keys()];
    for (const p of scene.parts) {
      const to = layout.parts.get(p.id);
      if (!to) continue;
      seen.add(p.id);
      let item = this.items.get(p.id);
      if (!item || item.dying !== null && item.part.kind !== p.kind) {
        if (item) this.remove(item);
        item = this.build(p, to, now + (this.first ? INTRO_DELAY[p.layer] + Math.max(0, order.indexOf(p.owner)) * 70 : 0));
        this.items.set(p.id, item);
      } else {
        item.dying = null;
        const shapeChanged = item.part.kind !== p.kind || item.part.variant !== p.variant && p.kind === "tool";
        item.part = p;
        if (shapeChanged) { this.remove(item); item = this.build(p, to, now); this.items.set(p.id, item); }
        if (Math.abs(item.to.x - to.x) > 1e-3 || Math.abs(item.to.z - to.z) > 1e-3 || item.to.layer !== to.layer) {
          item.from = { ...item.pos }; item.to = to; item.moveAt = now;
        }
      }
      item.size = partSize(p);
      this.restyle(item);
    }
    for (const item of this.items.values()) if (!seen.has(item.part.id) && item.dying === null) item.dying = now;
    // The pointer's neighbourhood: what a part is wired to, and for an agent everything it owns.
    const near = new Map<string, Set<string>>();
    const link = (a: string, b: string) => { if (!near.has(a)) near.set(a, new Set()); near.get(a)!.add(b); };
    for (const w of scene.wires) { link(w.from, w.to); link(w.to, w.from); }
    for (const p of scene.parts) if (p.owner !== p.id && p.kind !== "call") { link(p.owner, p.id); link(p.id, p.owner); }
    this.neighbours = near;
    this.syncWires(scene);
    // Parts in the first scene with agents are the starting picture, not changes. Recorded calls
    // can arrive before the first agent scan; they join that opening build-up.
    if (scene.agents.length) this.first = false;
    this.dirty = true;
    this.wake();
  }

  /** `immediate` sets the explode amount without easing: the tour already eases it every frame. */
  setView(view: StageView, { immediate = false } = {}) {
    const now = performance.now();
    if (view.explode !== this.explode.to || immediate && view.explode !== this.explode.value) {
      this.explode = view.reduced || immediate ? { value: view.explode, from: view.explode, to: view.explode, at: now } : { value: this.explode.value, from: this.explode.value, to: view.explode, at: now };
    }
    const restyle = view.xray !== this.view.xray || view.selected !== this.view.selected || view.hovered !== this.view.hovered;
    this.view = view;
    if (restyle) for (const item of this.items.values()) this.restyle(item);
    this.dirty = true;
    this.wake();
  }

  /** Fly to a pose (instantly with reduced motion or ms = 0). */
  flyTo(to: Pose, ms = 1100) {
    const target = clampPose(to);
    if (posesEqual(this.pose, target)) return;
    if (ms <= 0 || this.view.reduced) { this.pose = target; this.flight = null; }
    else this.flight = { from: { ...this.pose }, to: target, at: performance.now(), ms };
    this.dirty = true;
    this.wake();
  }

  /** Set the pose directly (the tour drives it every frame). */
  setPose(pose: Pose) { this.pose = clampPose(pose); this.flight = null; this.dirty = true; this.wake(); }

  orbit(dTheta: number, dPhi: number) { this.setPose({ ...this.pose, theta: this.pose.theta + dTheta, phi: this.pose.phi + dPhi }); }

  /** Zoom by `factor`, keeping the point under (x, y) in CSS pixels where it is, as maps do. */
  zoom(factor: number, at?: { x: number; y: number }) {
    const before = this.pose.distance;
    const next = clampPose({ ...this.pose, distance: before * factor });
    if (at) {
      // Orthographic: the point under the cursor moves by the change in scale times its offset from the centre.
      const unit = (2 * halfHeight(before)) / this.height, after = (2 * halfHeight(next.distance)) / this.height;
      const shift = this.viewShift();
      const dx = (at.x - this.width / 2 - shift.x) * (unit - after), dy = (at.y - this.height / 2 - shift.y) * (unit - after);
      const { right, up } = this.axes();
      next.tx += right.x * dx - up.x * dy; next.ty += right.y * dx - up.y * dy; next.tz += right.z * dx - up.z * dy;
    }
    this.setPose(next);
  }

  /** Pan by screen pixels: the model follows the pointer exactly. */
  pan(dx: number, dy: number) {
    const unit = (2 * halfHeight(this.pose.distance)) / this.height;
    const { right, up } = this.axes();
    this.setPose({ ...this.pose, tx: this.pose.tx - (right.x * dx - up.x * dy) * unit, ty: this.pose.ty - (right.y * dx - up.y * dy) * unit, tz: this.pose.tz - (right.z * dx - up.z * dy) * unit });
  }

  private axes() {
    const { theta, phi } = this.pose;
    return { right: new Vector3(Math.cos(theta), 0, -Math.sin(theta)), up: new Vector3(-Math.sin(phi) * Math.sin(theta), Math.cos(phi), -Math.sin(phi) * Math.cos(theta)) };
  }

  resize(width: number, height: number) {
    this.width = Math.max(1, width); this.height = Math.max(1, height);
    this.renderer.setPixelRatio(Math.min(2, globalThis.devicePixelRatio || 1));
    this.renderer.setSize(this.width, this.height, false);
    this.applyInsets();
  }

  /** Overlays along the edges (and the inspector on the right): the picture centres in the space between them. */
  setInsets(top: number, bottom: number, right = 0) {
    if (top === this.insets.top && bottom === this.insets.bottom && right === this.insets.right) return;
    this.insets = { top, bottom, right };
    this.applyInsets();
  }
  /** How far the centre of the free area sits from the canvas centre, in CSS pixels. */
  private viewShift() { return { x: -Math.round(this.insets.right / 2), y: Math.round((this.insets.top - this.insets.bottom) / 2) }; }
  private applyInsets() {
    const shift = this.viewShift();
    if (shift.x || shift.y) this.camera.setViewOffset(this.width, this.height, -shift.x, -shift.y, this.width, this.height);
    else this.camera.clearViewOffset();
    this.dirty = true;
    this.wake();
  }

  /** The part under a point in CSS pixels relative to the canvas, if any. A district's pad is its agent. */
  pick(x: number, y: number): string | null {
    const ndc = new Vector2((x / this.width) * 2 - 1, -(y / this.height) * 2 + 1);
    this.ray.setFromCamera(ndc, this.camera);
    for (const hit of this.ray.intersectObjects(this.pickables, false)) {
      const id = hit.object.userData.partId as string | undefined;
      const item = id ? this.items.get(id) : undefined;
      // Faded parts (another agent in focus) stay clickable; hidden layers and exits don't.
      if (item && item.dying === null && item.opacity > 0.05 && !this.view.hidden.has(item.part.layer)) return id!;
    }
    return null;
  }

  /** Screen anchors (CSS pixels) for labels: above each part, below the front edge for a
   *  district's nameplate and a port's number, and at the floors', shelf's and chip's corners. */
  anchors(ids: Iterable<string>): Map<string, Anchor> {
    const out = new Map<string, Anchor>();
    const v = new Vector3();
    const put = (id: string, x: number, y: number, z: number, below = false, end = false) => {
      v.set(x, y, z).project(this.camera);
      out.set(id, { x: ((v.x + 1) / 2) * this.width, y: ((1 - v.y) / 2) * this.height, behind: v.z > 1 || v.z < -1, below, end });
    };
    const layout = this.layout;
    for (const id of ids) {
      const item = this.items.get(id);
      if (item) {
        const d = layout?.districts.get(id);
        if (d) { put(id, (d.x0 + d.x1) / 2, 0, d.z1 + 0.12, true); continue; }
        if (item.part.kind === "remote") { put(id, item.pos.x, item.pos.y + 0.4, item.pos.z); continue; }
        put(id, item.pos.x, item.pos.y + item.height * item.scale + (item.pin ? 0.62 : 0.2), item.pos.z);
        continue;
      }
      if (!layout) continue;
      if (id === "plate:runtime" || id === "plate:config") {
        const layer = id === "plate:runtime" ? "runtime" : "config";
        // Left of the first tray on that floor, in the clear space over the chip.
        const r = [...layout.districts.values()].map((x) => x.floors[layer]).find((x) => x);
        if (r) put(id, r.x0 - 0.5, plateY(layer, this.explode.value) + SLAB, r.z1, false, true);
      } else if (id === "plate:calls" && layout.calls) put(id, (layout.calls.x0 + layout.calls.x1) / 2, plateY("calls", this.explode.value) + SLAB, layout.calls.z0);
      else if (id === "hub") put(id, layout.hub.x, 0, layout.hub.z + 1.05, true);
    }
    return out;
  }

  get explodeValue() { return this.explode.value; }

  // ---------- loop ----------

  wake() {
    if (this.disposed || this.raf) return;
    this.raf = requestAnimationFrame(this.tick);
  }

  stop() { if (this.raf) cancelAnimationFrame(this.raf); this.raf = 0; }

  private tick = (now: number) => {
    this.raf = 0;
    if (this.disposed) return;
    if (document.hidden || this.width < 2 || this.height < 2) return;
    const continuous = !this.view.reduced && this.hasContinuous();
    const due = now - this.lastRender >= 1000 / (document.hasFocus() ? PULSE_FPS : IDLE_FPS) - 2;
    // Between effect frames, with nothing else moving, there is nothing to advance.
    if (!this.busy && !this.dirty && continuous && !due) { this.raf = requestAnimationFrame(this.tick); return; }
    this.busy = this.step(now);
    if (this.busy || this.dirty || continuous && due) this.render(now);
    if (this.busy || continuous) this.raf = requestAnimationFrame(this.tick);
  };

  /** Advances transitions; true while any is still running. */
  private step(now: number): boolean {
    const dt = Math.min(0.1, this.lastFrame ? (now - this.lastFrame) / 1000 : 0.016);
    this.lastFrame = now;
    let busy = false;
    if (this.flight) {
      const k = Math.min(1, (now - this.flight.at) / this.flight.ms);
      this.pose = lerpPose(this.flight.from, this.flight.to, EASE.move(k));
      if (k >= 1) this.flight = null; else busy = true;
      this.dirty = true;
    }
    if (this.explode.value !== this.explode.to) {
      const k = Math.min(1, (now - this.explode.at) / EXPLODE_MS);
      this.explode.value = this.explode.from + (this.explode.to - this.explode.from) * EASE.move(k);
      if (k >= 1) this.explode.value = this.explode.to; else busy = true;
      this.dirty = true;
    }
    const reduced = this.view.reduced;
    const smooth = reduced ? 1 : 1 - Math.exp(-dt * 11);
    for (const item of [...this.items.values()]) {
      // Position: eased move to the new layout slot, at the current floor height.
      const k = reduced ? 1 : Math.min(1, (now - item.moveAt) / MOVE_MS);
      const e = EASE.move(k);
      const y = this.baseY(item);
      const pos = { x: item.from.x + (item.to.x - item.from.x) * e, y: item.from.y + (y - item.from.y) * e, z: item.from.z + (item.to.z - item.from.z) * e };
      if (k >= 1) { pos.x = item.to.x; pos.y = y; pos.z = item.to.z; item.from = pos; } else busy = true;
      if (Math.abs(pos.x - item.pos.x) + Math.abs(pos.y - item.pos.y) + Math.abs(pos.z - item.pos.z) > 1e-5) this.dirty = true;
      item.pos = pos;
      // Entrance and exit.
      let scale = 1;
      if (item.dying !== null) {
        const d = reduced ? 1 : Math.min(1, (now - item.dying) / EXIT_MS);
        scale = 1 - EASE.exit(d);
        if (d >= 1) { this.remove(item); this.dirty = true; continue; }
        busy = true;
      } else if (!reduced && item.intro && now < item.born + INTRO_MS) {
        // Opening build-up: rise from the board (or floor), each floor after the one below.
        scale = Math.max(0.001, EASE.enter((now - item.born) / INTRO_MS));
        busy = true;
      } else if (!reduced && now - item.born < ENTER_MS) {
        scale = 0.6 + 0.4 * EASE.enter((now - item.born) / ENTER_MS);
        busy = true;
      }
      // Height follows memory samples, the gauge follows CPU, a hovered part lifts: all eased.
      const ease = (from: number, to: number, set: (v: number) => void) => {
        if (Math.abs(from - to) > 1e-3) { set(from + (to - from) * smooth); busy = true; this.dirty = true; } else if (from !== to) { set(to); this.dirty = true; }
      };
      ease(item.height, item.size.h, (v) => (item.height = v));
      ease(item.lift, this.view.hovered === item.part.id && item.part.kind !== "agent" && item.part.kind !== "dormant" ? 0.08 : 0, (v) => (item.lift = v));
      if (item.gauge) ease(item.gauge.value, Math.min(1, Math.max(0, item.part.cpu / 100)), (v) => (item.gauge!.value = v));
      // Opacity: hidden layers vanish, other agents fade while one is in focus.
      ease(item.opacity, this.targetOpacity(item), (v) => (item.opacity = v));
      if (scale !== item.scale) { item.scale = scale; this.dirty = true; }
      if (item.fresh > now) busy = true;
      this.place(item, now);
    }
    for (const w of this.wires.values()) {
      const want = this.wireWanted(w) ? 1 : 0;
      if (w.shown !== want) {
        w.shown = reduced ? want : Math.abs(w.shown - want) < 0.01 ? want : w.shown + (want - w.shown) * smooth;
        busy = busy || w.shown !== want;
        this.dirty = true;
      }
    }
    return busy;
  }

  private hasContinuous(): boolean {
    for (const item of this.items.values()) if ((item.ripples?.[0]?.visible || item.part.kind === "call" && item.part.live) && item.opacity > 0.2) return true;
    for (const w of this.wires.values()) if (w.packets.some((p) => p.visible)) return true;
    return false;
  }

  private render(now: number) {
    const { theta, phi, tx, ty, tz } = this.pose;
    const c = Math.cos(phi);
    this.camera.position.set(tx + STANDOFF * c * Math.sin(theta), ty + STANDOFF * Math.sin(phi), tz + STANDOFF * c * Math.cos(theta));
    this.camera.lookAt(tx, ty, tz);
    const half = halfHeight(this.pose.distance), aspect = this.width / this.height;
    Object.assign(this.camera, { left: -half * aspect, right: half * aspect, top: half, bottom: -half });
    this.camera.updateProjectionMatrix();
    // Floors follow the explode animation and layer visibility.
    this.placeFloors();
    for (const w of this.wires.values()) this.placeWire(w, now);
    if (this.dirty) this.renderer.shadowMap.needsUpdate = true;
    this.renderer.render(this.scene3, this.camera);
    this.lastRender = now;
    this.dirty = false;
    this.onrender?.();
  }

  dispose() {
    this.disposed = true;
    this.stop();
    this.canvas.removeEventListener("webglcontextlost", this.lost);
    const materials = new Set<Material>();
    const geometries = new Set<BufferGeometry>([...Object.values(this.geo), ...this.edgeCache.values(), ...this.ringCache.values()]);
    this.scene3.traverse((o) => {
      const m = o as Mesh;
      if (m.material) for (const mat of Array.isArray(m.material) ? m.material : [m.material]) materials.add(mat);
      if (m.geometry) geometries.add(m.geometry);
    });
    for (const m of materials) disposeMaterial(m);
    for (const g of geometries) g.dispose();
    for (const t of this.textures.values()) t.dispose();
    this.renderer.dispose();
    this.renderer.forceContextLoss();
  }

  // ---------- items ----------

  /** Base height: the board, a floor at the current explode amount, or the sky for remote hosts. */
  private baseY(item: Pick<Item, "part" | "to">): number {
    if (item.part.kind === "remote") return skyY(this.explode.value);
    return item.to.layer !== "board" ? plateY(item.to.layer, this.explode.value) + SLAB : 0;
  }

  /** Whether a part is in the pointer's neighbourhood (itself, what it's wired to, what an agent owns). */
  private near(id: string): boolean {
    const h = this.view.hovered;
    return !h || h === id || !!this.neighbours.get(h)?.has(id);
  }

  private targetOpacity(item: Item): number {
    const v = this.view;
    const tower = item.part.kind === "agent" || item.part.kind === "dormant";
    if (v.hidden.has(item.part.layer) && !tower) return 0;
    // A lens or a clicked reading: its parts stay lit, towers stay as landmarks, the rest steps back.
    if (v.highlight) return v.highlight.has(item.part.id) ? 1 : tower ? 0.35 : 0.1;
    if (v.focus && item.part.owner !== v.focus) return 0.12;
    // Under the pointer: its neighbourhood stays, the rest dims a little.
    return this.near(item.part.id) ? 1 : 0.45;
  }

  private std(color = 0xffffff): MeshStandardMaterial {
    return new MeshStandardMaterial({ color, roughness: 0.55, metalness: 0.05, transparent: true });
  }
  private flat(color = 0xffffff): MeshBasicMaterial {
    return new MeshBasicMaterial({ color, transparent: true, depthWrite: false });
  }
  private edgesOf(geo: BufferGeometry): EdgesGeometry {
    let e = this.edgeCache.get(geo);
    if (!e) { e = new EdgesGeometry(geo, 28); this.edgeCache.set(geo, e); }
    return e;
  }
  private ring(inner: number, outer: number, start = 0, length = Math.PI * 2): RingGeometry {
    const key = `${inner}:${outer}:${start.toFixed(3)}:${length.toFixed(3)}`;
    let g = this.ringCache.get(key);
    if (!g) { g = new RingGeometry(inner, outer, Math.max(8, Math.round(96 * (length / (Math.PI * 2)))), 1, start, length).rotateX(-Math.PI / 2); this.ringCache.set(key, g); }
    return g;
  }

  private bodyGeometry(p: Part): BufferGeometry {
    switch (p.kind) {
      case "agent": case "dormant": return this.geo.tower;
      case "port": return this.geo.port;
      case "project": return this.geo.tile;
      case "remote": return this.geo.sat;
      case "tool": return p.variant === "mcp_server" ? this.geo.hex : p.variant === "dev_server" ? this.geo.devbox : p.variant === "shell" ? this.geo.puck : this.geo.cube;
      case "mcp": return this.geo.chip;
      case "skill": return this.geo.card;
      case "instructions": return this.geo.sheet;
      case "more": return this.geo.more;
      case "call": return this.geo.call;
    }
  }

  private build(p: Part, to: Placed, now: number): Item {
    const group = new Group();
    const bodyMat = this.std();
    const body = new Mesh(this.bodyGeometry(p), bodyMat);
    body.userData.partId = p.id;
    body.castShadow = p.layer === "board" && p.kind !== "remote" || p.layer === "runtime";
    body.receiveShadow = p.kind === "agent";
    const edgeMat = new LineBasicMaterial({ transparent: true, depthWrite: false });
    body.add(new LineSegments(this.edgesOf(body.geometry), edgeMat));
    group.add(body);
    this.pickables.push(body);
    const size = partSize(p);
    const r = Math.max(size.w, size.d) * 0.62;
    const sel = new Mesh(p.kind === "agent" || p.kind === "dormant" ? this.ring(GAUGE_RADIUS + 0.2, GAUGE_RADIUS + 0.28) : this.ring(r, r + 0.06), this.flat());
    sel.position.y = 0.014;
    group.add(sel);
    const y = this.baseY({ part: p, to });
    const item: Item = { part: p, group, body, size, height: size.h, tints: [], solids: [bodyMat], edgeMat, edgeBase: 0, sel, lift: 0,
      from: { x: to.x, y, z: to.z }, to, moveAt: now - MOVE_MS, pos: { x: to.x, y, z: to.z },
      opacity: this.first ? 1 : 0, scale: this.first && !this.view.reduced ? 0.001 : 1, born: now, dying: null, fresh: this.first ? 0 : now + FRESH_MS, intro: this.first };

    if (p.kind === "agent") {
      const capMat = this.std();
      item.cap = new Mesh(this.geo.cap, capMat);
      item.cap.userData.partId = p.id;
      item.solids.push(capMat);
      group.add(item.cap);
      this.pickables.push(item.cap);
      // The CPU gauge: a track and an arc round the tower's base, filling clockwise from the front.
      const track = new Mesh(this.ring(GAUGE_RADIUS - 0.05, GAUGE_RADIUS + 0.05), this.flat());
      const arc = new Mesh(this.ring(GAUGE_RADIUS - 0.05, GAUGE_RADIUS + 0.05, 0, 0.01), this.flat());
      track.position.y = arc.position.y = 0.012;
      track.renderOrder = arc.renderOrder = 1;
      group.add(track, arc);
      item.gauge = { track, arc, value: Math.min(1, Math.max(0, p.cpu / 100)), key: -1 };
      item.slices = new Group();
      group.add(item.slices);
      item.mono = new Mesh(this.geo.mono, new MeshBasicMaterial({ transparent: true, depthWrite: false }));
      group.add(item.mono);
    }
    if (p.kind === "agent" || p.kind === "dormant") {
      item.halo = new Mesh(this.ring(GAUGE_RADIUS + 0.1, GAUGE_RADIUS + 0.16), this.flat());
      item.halo.position.y = 0.012;
      group.add(item.halo);
    }
    if (p.kind === "port") {
      const socket = new Mesh(this.geo.socket, this.std());
      socket.position.set(0, 0.17, 0.225);
      item.solids.push(socket.material as MeshStandardMaterial);
      group.add(socket);
      // Ripples round a port open to the network: it can be reached from beyond this computer.
      item.ripples = [0, 1].map(() => {
        const m = new Mesh(this.ring(0.5, 0.55), this.flat());
        m.position.y = 0.012;
        group.add(m);
        return m;
      });
    }
    if (p.kind === "project") {
      const tab = new Mesh(this.geo.tab, this.std());
      tab.position.set(-0.3, 0.1, -0.32);
      item.solids.push(tab.material as MeshStandardMaterial);
      group.add(tab);
    }
    if (p.kind === "instructions") {
      for (const dy of [0.1, 0.2]) {
        const sheet = new Mesh(this.geo.sheet, bodyMat);
        sheet.position.set(dy * 0.3, dy, -dy * 0.25);
        sheet.userData.partId = p.id;
        this.pickables.push(sheet);
        group.add(sheet);
      }
    }
    if (p.kind === "mcp") {
      item.led = new Mesh(this.geo.led, this.std());
      item.led.position.set(0.3, 0.2, 0.2);
      item.solids.push(item.led.material as MeshStandardMaterial);
      group.add(item.led);
    }
    if (p.kind === "remote") {
      // A faceted satellite.
      bodyMat.flatShading = true;
      item.mast = new Mesh(this.geo.mast, this.std());
      item.solids.push(item.mast.material as MeshStandardMaterial);
      group.add(item.mast);
    }
    if (p.kind !== "agent" && p.kind !== "dormant" && p.kind !== "remote" && p.kind !== "call") {
      item.glyph = new Mesh(this.geo.glyph, new MeshBasicMaterial({ transparent: true, depthWrite: false }));
      item.glyph.renderOrder = 2;
      group.add(item.glyph);
    }
    item.pin = new Sprite(new SpriteMaterial({ transparent: true, depthWrite: false, depthTest: false, sizeAttenuation: true }));
    item.pin.renderOrder = 5;
    item.pin.center.set(0.5, 0);
    group.add(item.pin);
    this.root.add(group);
    return item;
  }

  private remove(item: Item) {
    this.root.remove(item.group);
    item.group.traverse((o) => {
      const m = o as Mesh;
      const i = this.pickables.indexOf(m);
      if (i >= 0) this.pickables.splice(i, 1);
      // Shared textures (icons, pins) stay cached; only the materials go.
      if (m.material) for (const mat of Array.isArray(m.material) ? m.material : [m.material]) { if (mat !== item.mono?.material) (mat as MeshBasicMaterial).map = null; mat.dispose(); }
    });
    if (item.mono) (item.mono.material as MeshBasicMaterial).map?.dispose();
    if (this.items.get(item.part.id) === item) this.items.delete(item.part.id);
  }

  /** Colours and base opacities from the part's kind, evidence and state. Called when any of them changes. */
  private restyle(item: Item) {
    const c = this.colors, p = item.part;
    const agent = c.agents.get(p.owner) ?? c.gray;
    const surface = c.dark ? c.surface3 : c.surface;
    const tint = (a: Rgb, t: number) => toHex(mixRgb(a, surface, t));
    const body = item.body.material as MeshStandardMaterial;
    item.tints = [];
    const evidenceAlpha = p.evidence === "inferred" ? 0.62 : p.evidence === "unknown" ? 0.28 : 1;
    let bodyBase = evidenceAlpha, edge = c.text2, edgeBase = p.evidence === "inferred" || p.evidence === "unknown" ? 0.55 : 0.18, glass = true, blueprint = false;
    let ink = c.text2;
    switch (p.kind) {
      case "agent":
        body.color.setHex(tint(agent, c.dark ? 0.6 : 0.72));
        (item.cap!.material as MeshStandardMaterial).color.setHex(toHex(agent));
        edge = agent; edgeBase = 0.5;
        break;
      case "dormant":
        body.color.setHex(toHex(c.gray)); bodyBase = 0.22; edge = c.text2; edgeBase = 0.9; blueprint = true;
        break;
      case "port":
        body.color.setHex(p.warn ? tint(c.warn, c.dark ? 0.15 : 0.1) : tint(agent, c.dark ? 0.35 : 0.45));
        (item.solids[1]).color.setHex(toHex(mixRgb(c.text, surface, c.dark ? 0.75 : 0.25)));
        ink = p.warn ? (c.dark ? c.text : { r: 255, g: 255, b: 255 }) : c.dark ? c.text : { r: 255, g: 255, b: 255 };
        break;
      case "project":
        body.color.setHex(toHex(c.dark ? c.surface3 : c.surface)); edge = c.muted; edgeBase = 0.45;
        item.solids[1].color.setHex(toHex(agent));
        ink = agent;
        break;
      case "remote":
        body.color.setHex(tint(c.blue, c.dark ? 0.35 : 0.45)); edge = c.blue; edgeBase = 0.6;
        item.solids[1].color.setHex(toHex(mixRgb(c.muted, surface, 0.2)));
        break;
      case "tool":
        body.color.setHex(tint(agent, c.dark ? 0.5 : 0.62));
        if (p.variant === "mcp_server") { edge = c.ok; edgeBase = 0.7; }
        ink = c.dark ? c.text : mixRgb(agent, c.text, 0.55);
        break;
      case "mcp": {
        glass = false; blueprint = true;
        const tone = p.variant === "ok" ? c.ok : p.variant === "off" ? c.gray : c.blue;
        body.color.setHex(toHex(tone)); edge = tone;
        bodyBase = p.variant === "ok" ? 0.95 : p.variant === "off" ? 0.14 : p.variant === "unknown" ? 0.3 : 0.45;
        edgeBase = p.variant === "ok" ? 0.6 : 0.9;
        const led = item.led!.material as MeshStandardMaterial;
        led.color.setHex(toHex(p.variant === "ok" ? c.ok : c.gray));
        led.emissive.setHex(toHex(c.ok)); led.emissiveIntensity = p.variant === "ok" ? 0.9 : 0;
        ink = p.variant === "ok" ? { r: 255, g: 255, b: 255 } : tone;
        break;
      }
      case "skill":
        glass = false; blueprint = true;
        body.color.setHex(toHex(c.violet)); edge = p.variant === "changed" ? c.danger : p.warn ? c.warn : c.violet;
        bodyBase = p.variant === "trusted" ? 0.7 : 0.45; edgeBase = 0.9;
        ink = c.dark ? c.text : { r: 255, g: 255, b: 255 };
        break;
      case "instructions":
        glass = false; blueprint = true;
        body.color.setHex(toHex(c.gray)); edge = c.gray; bodyBase = 0.4; edgeBase = 0.85;
        ink = c.text2;
        break;
      case "more":
        body.color.setHex(toHex(c.gray)); edge = c.muted; bodyBase = 0.35; edgeBase = 0.7;
        ink = c.text;
        break;
      case "call": {
        // Reported, not observed: translucent, coloured by what was reported.
        glass = false;
        const tone = p.variant === "error" ? c.danger : p.variant === "tool" ? c.blue : p.variant === "model" ? c.violet : c.accent;
        body.color.setHex(toHex(tone)); edge = tone; bodyBase = 0.78; edgeBase = 0.5;
        break;
      }
    }
    item.tints.push({ mat: body, base: bodyBase, glass, blueprint });
    for (const m of item.solids.slice(1)) item.tints.push({ mat: m, base: 1, glass });
    if (item.glyph) {
      const mat = item.glyph.material as MeshBasicMaterial;
      const key = `${this.glyphName(p)}:${toHex(ink)}`;
      if (item.glyphKey !== key) { item.glyphKey = key; mat.map = this.glyphTexture(this.glyphName(p), ink); mat.needsUpdate = true; }
      item.tints.push({ mat, base: p.kind === "mcp" && p.variant !== "ok" ? 0.9 : 0.92 });
    }
    // Needs you: a pin over the part, red for an agent's own risk or a changed skill, amber otherwise.
    if (item.pin) {
      const tone = p.kind === "agent" || p.variant === "changed" ? c.danger : c.warn;
      const key = p.warn ? toHex(tone).toString() : "";
      if (item.pinKey !== key) { item.pinKey = key; const m = item.pin.material; m.map = key ? this.pinTexture(tone) : null; m.needsUpdate = true; }
      item.pin.visible = !!p.warn;
      if (p.warn) item.tints.push({ mat: item.pin.material, base: 1 });
    }
    if (item.ripples) for (const m of item.ripples) {
      (m.material as MeshBasicMaterial).color.setHex(toHex(c.warn));
      m.visible = !!p.warn;
    }
    if (item.gauge) {
      const t = item.gauge.track.material as MeshBasicMaterial, a = item.gauge.arc.material as MeshBasicMaterial;
      t.color.setHex(toHex(mixRgb(agent, surface, c.dark ? 0.7 : 0.75)));
      // Colour only for state: the agent's colour, orange from 60% of a core, red from 80%.
      a.color.setHex(toHex(p.cpu >= 80 ? c.danger : p.cpu >= 60 ? c.warn : agent));
      item.tints.push({ mat: t, base: 0.9 }, { mat: a, base: 1 });
    }
    if (item.halo) { (item.halo.material as MeshBasicMaterial).color.setHex(toHex(c.danger)); item.halo.visible = !!p.warn; item.tints.push({ mat: item.halo.material as MeshBasicMaterial, base: 0.75 }); }
    const selected = this.view.selected === p.id, hovered = this.view.hovered === p.id;
    (item.sel.material as MeshBasicMaterial).color.setHex(toHex(c.accent));
    item.sel.visible = selected || hovered;
    item.tints.push({ mat: item.sel.material as MeshBasicMaterial, base: selected ? 1 : 0.55 });
    item.edgeMat.color.setHex(toHex(selected ? c.accent : edge));
    item.edgeBase = selected ? 1 : hovered ? Math.max(edgeBase, 0.75) : edgeBase;
    if (item.slices) this.buildSlices(item, agent);
    if (item.mono) this.paintMonogram(item, agent);
    this.dirty = true;
  }

  private glyphName(p: Part): string {
    switch (p.kind) {
      case "port": return p.warn ? "port-net" : "port";
      case "project": return "folder";
      case "tool": return p.variant === "mcp_server" ? "plug" : p.variant === "dev_server" ? "code" : p.variant === "shell" ? "shell" : "gear";
      case "mcp": return "plug";
      case "skill": return "spark";
      case "instructions": return "rules";
      case "more": return `text:${p.label}`;
      default: return "";
    }
  }

  /** An icon for a part's top face, drawn once per name and colour. */
  private glyphTexture(name: string, ink: Rgb): CanvasTexture {
    const key = `glyph:${name}:${toHex(ink)}`;
    const cached = this.textures.get(key);
    if (cached) return cached;
    const size = 128, canvas = document.createElement("canvas");
    canvas.width = canvas.height = size;
    const g = canvas.getContext("2d")!;
    const color = `rgb(${Math.round(ink.r)}, ${Math.round(ink.g)}, ${Math.round(ink.b)})`;
    g.strokeStyle = g.fillStyle = color;
    g.lineWidth = 9; g.lineCap = "round"; g.lineJoin = "round";
    const text = (t: string, px: number) => { g.font = `700 ${px}px ui-monospace, "SF Mono", Menlo, Consolas, monospace`; g.textAlign = "center"; g.textBaseline = "middle"; g.fillText(t, 64, 68); };
    switch (name) {
      case "port": case "port-net":
        // A socket: a rounded outlet with two pins.
        g.beginPath(); g.roundRect(30, 38, 68, 52, 12); g.stroke();
        g.fillRect(46, 54, 9, 20); g.fillRect(73, 54, 9, 20);
        if (name === "port-net") { g.beginPath(); g.arc(98, 30, 14, -Math.PI / 2, 0); g.stroke(); g.beginPath(); g.arc(98, 30, 28, -Math.PI / 2, 0); g.stroke(); }
        break;
      case "folder":
        g.beginPath(); g.moveTo(22, 40); g.lineTo(52, 40); g.lineTo(60, 50); g.lineTo(106, 50); g.lineTo(106, 96); g.lineTo(22, 96); g.closePath(); g.stroke();
        break;
      case "plug":
        g.beginPath(); g.roundRect(38, 48, 52, 34, 10); g.stroke();
        g.beginPath(); g.moveTo(52, 48); g.lineTo(52, 28); g.moveTo(76, 48); g.lineTo(76, 28); g.moveTo(64, 82); g.lineTo(64, 104); g.stroke();
        break;
      case "code": text("</>", 50); break;
      case "shell": text(">_", 58); break;
      case "gear": {
        g.beginPath(); g.arc(64, 64, 20, 0, Math.PI * 2); g.stroke();
        for (let i = 0; i < 8; i++) { const a = (i / 8) * Math.PI * 2; g.beginPath(); g.moveTo(64 + Math.cos(a) * 30, 64 + Math.sin(a) * 30); g.lineTo(64 + Math.cos(a) * 42, 64 + Math.sin(a) * 42); g.stroke(); }
        break;
      }
      case "spark":
        g.beginPath(); g.moveTo(64, 18); g.quadraticCurveTo(70, 58, 110, 64); g.quadraticCurveTo(70, 70, 64, 110); g.quadraticCurveTo(58, 70, 18, 64); g.quadraticCurveTo(58, 58, 64, 18); g.fill();
        break;
      case "rules":
        for (const y of [42, 64, 86]) { g.beginPath(); g.moveTo(32, y); g.lineTo(y === 86 ? 72 : 96, y); g.stroke(); }
        break;
      default:
        if (name.startsWith("text:")) text(name.slice(5), 52);
    }
    const t = new CanvasTexture(canvas);
    t.colorSpace = SRGBColorSpace;
    t.anisotropy = Math.min(8, this.renderer.capabilities.getMaxAnisotropy());
    this.textures.set(key, t);
    return t;
  }

  /** A map pin with "!", for a part that needs you. */
  private pinTexture(tone: Rgb): CanvasTexture {
    const key = `pin:${toHex(tone)}`;
    const cached = this.textures.get(key);
    if (cached) return cached;
    const canvas = document.createElement("canvas");
    canvas.width = 96; canvas.height = 128;
    const g = canvas.getContext("2d")!;
    g.fillStyle = `rgb(${Math.round(tone.r)}, ${Math.round(tone.g)}, ${Math.round(tone.b)})`;
    g.beginPath(); g.arc(48, 44, 38, Math.PI * 0.82, Math.PI * 0.18); g.lineTo(48, 124); g.closePath(); g.fill();
    g.strokeStyle = "rgba(255, 255, 255, 0.95)"; g.lineWidth = 5; g.stroke();
    g.fillStyle = "#fff";
    g.font = '800 52px "Inter Variable", Inter, system-ui, sans-serif';
    g.textAlign = "center"; g.textBaseline = "middle";
    g.fillText("!", 48, 46);
    const t = new CanvasTexture(canvas);
    t.colorSpace = SRGBColorSpace;
    this.textures.set(key, t);
    return t;
  }

  /** Agent initials on the tower cap, in the agent's colour's readable contrast. */
  private paintMonogram(item: Item, agent: Rgb) {
    const text = monogram(item.part.label);
    const light = (0.2126 * agent.r + 0.7152 * agent.g + 0.0722 * agent.b) / 255 > 0.55;
    const key = `${text}:${toHex(agent)}:${light}`;
    if (item.monoKey === key) return;
    item.monoKey = key;
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 128;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.fillStyle = light ? "rgba(11, 15, 25, 0.86)" : "rgba(255, 255, 255, 0.94)";
    ctx.font = '600 58px "Inter Variable", Inter, system-ui, sans-serif';
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(text, 64, 68);
    const mat = item.mono!.material as MeshBasicMaterial;
    mat.map?.dispose();
    const texture = new CanvasTexture(canvas);
    texture.colorSpace = SRGBColorSpace;
    mat.map = texture;
    mat.needsUpdate = true;
  }

  /** X-ray reveals an agent's processes as slices inside its tower; busy ones glow. */
  private buildSlices(item: Item, agent: Rgb) {
    const group = item.slices!;
    const key = JSON.stringify([item.part.slices?.map((v) => Math.round(v * 10)), item.part.count, toHex(agent), toHex(this.colors.warn)]);
    if (item.slicesKey === key) return;
    item.slicesKey = key;
    for (const child of [...group.children]) { group.remove(child); ((child as Mesh).material as Material).dispose(); }
    const cpu = item.part.slices ?? [];
    const n = Math.min(16, Math.max(cpu.length, Math.min(16, item.part.count)));
    for (let i = 0; i < n; i++) {
      const load = cpu[i] ?? 0;
      const mat = this.std(toHex(agent));
      mat.emissive.setHex(toHex(this.colors.warn));
      mat.emissiveIntensity = Math.min(0.9, load / 6);
      const s = new Mesh(this.geo.slice, mat);
      s.userData.slice = i;
      group.add(s);
    }
  }

  /** Per-frame placement and material values. */
  private place(item: Item, now: number) {
    const { group, part } = item;
    group.position.set(item.pos.x, item.pos.y + item.lift, item.pos.z);
    group.scale.setScalar(item.scale);
    const h = item.height;
    if (part.kind === "agent" || part.kind === "dormant") item.body.scale.y = h;
    if (item.cap) item.cap.position.y = h + 0.03;
    if (item.mono) {
      item.mono.position.y = h + 0.063;
      const m = item.mono.material as MeshBasicMaterial;
      m.opacity = item.opacity * (this.view.xray ? 0.3 : 1);
      m.visible = m.opacity > 0.02;
    }
    if (item.glyph) {
      const top = part.kind === "instructions" ? 0.27 : h;
      const s = Math.min(item.size.w, item.size.d) * (part.kind === "port" ? 0.8 : part.kind === "project" ? 0.62 : 0.66);
      item.glyph.position.y = top + 0.004;
      item.glyph.scale.set(s, 1, s);
    }
    if (item.mast) {
      // The mast runs from the board up to the satellite.
      item.mast.scale.y = Math.max(0.01, item.pos.y - 0.02);
    }
    if (item.pin) {
      item.pin.position.y = (part.kind === "remote" ? 0.32 : part.kind === "instructions" ? 0.32 : h) + 0.1;
      item.pin.scale.set(0.36, 0.48, 1);
    }
    const xray = this.view.xray;
    for (const t of item.tints) {
      let o = t.base * item.opacity;
      if (xray && t.glass) o *= 0.2;
      if (xray && t.blueprint) o = Math.min(1, o * 1.6);
      t.mat.opacity = o;
      t.mat.visible = o > 0.004;
      if (t.mat instanceof MeshStandardMaterial) t.mat.depthWrite = o > 0.9;
    }
    const edgeO = Math.max(item.edgeBase, xray ? 0.5 : 0) * item.opacity;
    item.edgeMat.opacity = edgeO;
    item.edgeMat.visible = edgeO > 0.01;
    // Emphasis: selection, hover, a part that just appeared, a running MCP server.
    const c = this.colors;
    const fresh = item.fresh > now ? (item.fresh - now) / FRESH_MS : 0;
    const body = item.body.material as MeshStandardMaterial;
    const tower = part.kind === "agent" || part.kind === "dormant";
    // Towers keep their own colour when picked (a glow would tint a dark body); the ring and edges mark it.
    const sel = this.view.selected === part.id ? (tower ? 0 : 0.3) : this.view.hovered === part.id ? (tower ? 0.03 : 0.16) : 0;
    const lit = this.view.highlight?.has(part.id) && !tower ? 0.2 : 0;
    // A running MCP server glows steadily; a call still running breathes.
    const live = part.kind === "mcp" && part.variant === "ok" ? 0.28
      : part.kind === "call" && part.live ? (this.view.reduced ? 0.3 : 0.18 + 0.22 * (0.5 + 0.5 * Math.sin((now / 1000) * Math.PI * 2 * 0.8))) : 0;
    const glow = Math.max(sel, lit, fresh * 0.55, live);
    const tone = part.kind === "call" ? (part.variant === "error" ? c.danger : c.accent) : live > sel && live > fresh * 0.55 ? c.ok : c.accent;
    body.emissive.setHex(toHex(tone));
    body.emissiveIntensity = glow;
    if (item.gauge) {
      // The arc's length is the CPU share, quantised to 1% so it rebuilds only when it moves.
      const key = Math.round(item.gauge.value * 100);
      if (key !== item.gauge.key) {
        item.gauge.key = key;
        item.gauge.arc.geometry = this.ring(GAUGE_RADIUS - 0.05, GAUGE_RADIUS + 0.05, Math.PI / 2, -Math.max(0.0001, (key / 100) * Math.PI * 2));
      }
      item.gauge.arc.visible = key > 0 && (item.gauge.arc.material as MeshBasicMaterial).visible;
    }
    if (item.ripples) {
      item.ripples.forEach((m, i) => {
        const mat = m.material as MeshBasicMaterial;
        if (!part.warn) { m.visible = false; return; }
        // Two rings travel outward; with reduced motion they stand at fixed radii.
        const k = this.view.reduced ? (i + 1) / 3 : ((now / 1000) / RIPPLE_S + i / 2) % 1;
        m.scale.setScalar(1 + k * 1.3);
        mat.opacity = (1 - k) * 0.65 * item.opacity;
        m.visible = mat.opacity > 0.01;
      });
    }
    if (item.slices) {
      item.slices.visible = xray && item.opacity > 0.05;
      const n = item.slices.children.length;
      item.slices.children.forEach((s, i) => {
        s.position.y = 0.12 + ((h - 0.24) * (i + 0.5)) / Math.max(1, n);
        const m = (s as Mesh).material as MeshStandardMaterial;
        m.opacity = 0.85 * item.opacity;
      });
    }
  }

  // ---------- wires ----------

  private wireMode(w: Wire): WireMode {
    const to = this.items.get(w.to)?.part;
    if (w.kind === "socket") return to?.kind === "remote" ? "uplink" : "arc";
    if (w.kind === "running") return "beam";
    if (w.kind === "config" || w.kind === "listener" && this.items.get(w.from)?.part.kind === "tool") return "rod";
    return "trace";
  }

  /** Ownership (folders, configuration, ports the agent itself holds) is said by the district;
   *  those lines only appear for the part under the pointer or selected. */
  private isDemand(w: Wire): boolean {
    if (w.kind === "folder" || w.kind === "config") return true;
    return w.kind === "listener" && this.items.get(w.from)?.part.kind !== "tool";
  }

  private wireWanted(w: WireItem): boolean {
    if (!w.demand) return true;
    const { hovered, selected } = this.view;
    return [hovered, selected].some((id) => id === w.wire.from || id === w.wire.to);
  }

  private syncWires(scene: MachineScene) {
    const seen = new Set<string>();
    for (const w of scene.wires) {
      if (!this.items.has(w.from) || !this.items.has(w.to)) continue;
      seen.add(w.id);
      const mode = this.wireMode(w);
      const key = `${mode}:${w.from}:${w.to}`;
      let item = this.wires.get(w.id);
      if (item && item.key !== key) { this.removeWire(item); item = undefined; }
      if (!item) {
        const demand = this.isDemand(w);
        item = { wire: w, mode, segs: [], tube: null, mat: this.flat(), arrow: null, packets: [], base: 1, demand, shown: demand ? 0 : 1, key, ends: "" };
        this.wires.set(w.id, item);
      }
      item.wire = w;
      this.restyleWire(item);
    }
    for (const w of [...this.wires.values()]) if (!seen.has(w.wire.id)) this.removeWire(w);
  }

  private removeWire(w: WireItem) {
    for (const s of w.segs) this.root.remove(s);
    if (w.tube) { this.root.remove(w.tube); w.tube.geometry.dispose(); }
    if (w.arrow) this.root.remove(w.arrow);
    w.mat.dispose();
    for (const p of w.packets) { this.root.remove(p); (p.material as Material).dispose(); }
    this.wires.delete(w.wire.id);
  }

  private restyleWire(w: WireItem) {
    const c = this.colors, wire = w.wire;
    const owner = this.items.get(wire.from)?.part.owner ?? "";
    const agent = c.agents.get(owner) ?? c.gray;
    const surface = c.dark ? c.surface3 : c.surface;
    const set = (rgb: Rgb, base: number) => { w.mat.color.setHex(toHex(rgb)); w.base = base; };
    switch (wire.kind) {
      case "process": set(mixRgb(agent, c.text2, c.dark ? 0.2 : 0.35), wire.evidence === "observed" ? 0.85 : 0.5); break;
      case "listener": set(mixRgb(agent, c.text2, 0.3), 0.8); break;
      case "folder": set(mixRgb(c.muted, surface, 0.1), 0.7); break;
      case "socket": set(mixRgb(c.blue, c.text2, 0.25), 0.7); break;
      case "config": set(wire.to.includes(":skill:") ? c.violet : wire.to.includes(":mcp:") ? c.blue : c.gray, 0.6); break;
      case "running": set(c.ok, 0.9); break;
    }
    const arrow = w.mode === "trace" || w.mode === "rod" || w.mode === "arc";
    if (arrow && !w.arrow) { w.arrow = new Mesh(this.geo.arrow, w.mat); this.root.add(w.arrow); }
    // Moving dots: open connections and a running server; one to three, by connection count.
    const want = w.mode === "arc" || w.mode === "uplink" ? Math.min(3, Math.max(1, Math.ceil((wire.weight ?? 1) / 2))) : w.mode === "beam" ? 2 : 0;
    while (w.packets.length > want) { const p = w.packets.pop()!; this.root.remove(p); (p.material as Material).dispose(); }
    while (w.packets.length < want) { const p = new Mesh(this.geo.packet, this.flat()); p.renderOrder = 3; this.root.add(p); w.packets.push(p); }
    for (const p of w.packets) (p.material as MeshBasicMaterial).color.setHex(toHex(w.mode === "beam" ? c.ok : w.mode === "uplink" ? c.blue : agent));
    this.dirty = true;
  }

  private top(item: Item): number { return item.pos.y + item.lift + item.height * item.scale; }

  /** The points a wire runs through this frame, from the end that starts it to the end that receives. */
  private path(w: WireItem): Vector3[] {
    const a = this.items.get(w.wire.from)!, b = this.items.get(w.wire.to)!;
    const at = (i: Item, y: number) => new Vector3(i.pos.x, y, i.pos.z);
    const floor = (layer: Layer) => plateY(layer, this.explode.value) + SLAB + 0.012;
    switch (w.mode) {
      case "trace": {
        // Right angles along a floor, as on a circuit board. From the agent's tower, a riser up to
        // the floor first when the part stands on one.
        const y = b.part.layer === "board" ? 0.012 : floor(b.part.layer);
        const start = a.part.layer === b.part.layer && a.part.kind !== "agent" ? [at(a, y)] : a.part.kind === "agent" && b.part.layer !== "board" ? [at(a, this.top(a)), at(a, y)] : [at(a, y)];
        const s = start[start.length - 1];
        return [...start, new Vector3(s.x, y, b.pos.z), new Vector3(b.pos.x, y, b.pos.z)];
      }
      case "rod": return [at(a, a.part.kind === "agent" || a.part.kind === "dormant" ? this.top(a) : a.pos.y), at(b, b.part.kind === "port" ? b.pos.y + b.size.h : b.pos.y)];
      case "beam": return [at(a, a.pos.y), at(b, this.top(b))];
      case "uplink": {
        // Up through the district's corner to the satellite at the head of the mast.
        const h = this.top(a);
        return [at(a, h), new Vector3(b.pos.x, h, b.pos.z), at(b, b.pos.y - 0.3)];
      }
      case "arc": return [at(a, this.top(a)), at(b, this.top(b))];
    }
  }

  private placeWire(w: WireItem, now: number) {
    const a = this.items.get(w.wire.from), b = this.items.get(w.wire.to);
    if (!a || !b) return;
    const hidden = this.view.hidden.has(a.part.layer) || this.view.hidden.has(b.part.layer);
    let opacity = hidden ? 0 : w.base * w.shown * Math.min(a.opacity, b.opacity) * Math.min(a.scale, b.scale);
    if (this.view.xray && w.wire.kind !== "running" && w.wire.kind !== "config") opacity *= 0.5;
    w.mat.opacity = opacity;
    const visible = opacity > 0.01;
    w.mat.visible = visible;
    let pts = this.path(w);
    // Stop short of the receiving part, where the arrowhead sits.
    const end = pts[pts.length - 1];
    const gap = w.mode === "trace" ? Math.max(b.size.w, b.size.d) * 0.55 + 0.12 : w.mode === "rod" ? 0.14 : 0;
    if (gap) {
      const prev = pts[pts.length - 2];
      const len = prev.distanceTo(end);
      if (len > gap) pts = [...pts.slice(0, -1), end.clone().addScaledVector(new Vector3().subVectors(prev, end).normalize(), gap)];
    }
    if (w.mode === "arc") {
      const from = pts[0], to = pts[1];
      const mid = from.clone().add(to).multiplyScalar(0.5);
      mid.y += 1 + from.distanceTo(to) * 0.22;
      const curve = new QuadraticBezierCurve3(from, mid, to);
      const ends = `${from.toArray().map((v) => v.toFixed(2))}>${to.toArray().map((v) => v.toFixed(2))}`;
      if (!w.tube) { w.tube = new Mesh(new BufferGeometry(), w.mat); this.root.add(w.tube); }
      if (w.ends !== ends) { w.tube.geometry.dispose(); w.tube.geometry = new TubeGeometry(curve, 40, 0.026, 6); w.ends = ends; }
      w.tube.visible = visible;
      pts = curve.getPoints(40);
    } else {
      const radius = w.mode === "beam" ? 0.045 : w.mode === "trace" ? 0.05 : 0.022;
      while (w.segs.length < pts.length - 1) { const s = new Mesh(w.mode === "trace" ? this.geo.trace : this.geo.rod, w.mat); this.root.add(s); w.segs.push(s); }
      w.segs.forEach((s, i) => {
        s.visible = visible && i < pts.length - 1;
        if (!s.visible) return;
        const p = pts[i], q = pts[i + 1];
        const dir = new Vector3().subVectors(q, p);
        const len = dir.length();
        if (w.mode === "trace" && Math.abs(dir.y) < 1e-6) {
          // A flat strip along the floor, square at the corners.
          s.position.copy(p).addScaledVector(dir, 0.5);
          s.quaternion.identity();
          s.rotation.set(0, -Math.atan2(dir.z, dir.x), 0);
          s.scale.set(len + radius, 0.014, radius);
        } else {
          s.position.copy(p).addScaledVector(dir, 0.5);
          if (len > 1e-6) s.quaternion.setFromUnitVectors(UP, dir.normalize());
          s.scale.set(radius, Math.max(1e-3, len), radius);
        }
      });
    }
    if (w.arrow) {
      const tip = pts[pts.length - 1], prev = pts[pts.length - 2] ?? pts[0];
      const dir = new Vector3().subVectors(tip, prev).normalize();
      w.arrow.visible = visible && dir.lengthSq() > 0;
      w.arrow.position.copy(tip).addScaledVector(dir, -0.12);
      w.arrow.quaternion.setFromUnitVectors(UP, dir);
      w.arrow.scale.setScalar(w.mode === "trace" ? 1.15 : 0.95);
    }
    // Dots travel from the start to the receiving end; with reduced motion they stand still, evenly apart.
    if (w.packets.length) {
      const lengths = [0];
      for (let i = 1; i < pts.length; i++) lengths.push(lengths[i - 1] + pts[i].distanceTo(pts[i - 1]));
      const total = lengths[lengths.length - 1];
      w.packets.forEach((p, i) => {
        p.visible = visible && total > 0.01;
        if (!p.visible) return;
        const t = this.view.reduced ? (i + 1) / (w.packets.length + 1) : (((now / 1000) * FLOW + i / w.packets.length) % 1);
        const s = t * total;
        let k = 1;
        while (k < lengths.length - 1 && lengths[k] < s) k++;
        const span = lengths[k] - lengths[k - 1] || 1;
        p.position.lerpVectors(pts[k - 1], pts[k], (s - lengths[k - 1]) / span);
        (p.material as MeshBasicMaterial).opacity = Math.min(1, opacity * 1.6);
      });
    }
  }

  // ---------- board, districts, floors and light ----------

  private applyTheme() {
    const c = this.colors;
    this.hemi.color.setHex(0xffffff);
    this.hemi.groundColor.setHex(toHex(c.dark ? c.surface : c.surface3));
    this.hemi.intensity = c.dark ? 0.85 : 1.15;
    this.sun.intensity = c.dark ? 1.2 : 1.45;
    this.ambient.intensity = c.dark ? 0.3 : 0.2;
    if (this.layout) this.buildDecor(this.layout);
  }

  private buildDecor(layout: MachineLayout) {
    for (const child of [...this.decor.children]) {
      this.decor.remove(child);
      child.traverse((o) => {
        const m = o as Mesh;
        const i = this.pickables.indexOf(m);
        if (i >= 0) this.pickables.splice(i, 1);
        m.geometry?.dispose();
        for (const mat of Array.isArray(m.material) ? m.material : [m.material]) if (mat) disposeMaterial(mat);
      });
    }
    this.floors = [];
    this.guides = null; this.guidesAt = -1;
    this.shelf = null;
    const c = this.colors;
    const surface = c.dark ? c.surface3 : c.surface;
    const boardTone = c.dark ? c.surface2 : c.surface;
    const line = (g: BufferGeometry, color: Rgb, opacity: number) => {
      const l = new LineLoop(g, new LineBasicMaterial({ color: toHex(color), transparent: true, opacity, depthWrite: false }));
      this.decor.add(l);
      return l;
    };
    const slab = (r: Rect, depth: number, radius: number, materials: Material | Material[]) => {
      return new Mesh(roundedSlab(r, radius, depth), materials);
    };
    // The board: a slab with a dot on every whole unit, so spacing reads at a glance.
    const top = new MeshStandardMaterial({ color: 0xffffff, map: this.dotTexture(boardTone), roughness: 0.82, metalness: 0.02 });
    const side = new MeshStandardMaterial({ color: toHex(mixRgb(boardTone, c.text2, c.dark ? 0.08 : 0.12)), roughness: 0.82 });
    const board = slab(layout.board, 0.36, 0.9, [top, side]);
    board.position.y = -0.36;
    board.receiveShadow = true;
    this.decor.add(board);
    line(outline(layout.board, 0.9, 0.002), c.border, c.dark ? 0.9 : 0.8);
    // This computer: a chip with pins on every side, wired to each district along a bus.
    const hubRect: Rect = { x0: layout.hub.x - 0.8, x1: layout.hub.x + 0.8, z0: layout.hub.z - 0.8, z1: layout.hub.z + 0.8 };
    const hub = slab(hubRect, 0.22, 0.14, new MeshStandardMaterial({ color: toHex(c.dark ? c.surface3 : mixRgb(c.text, c.surface, 0.8)), roughness: 0.6 }));
    hub.castShadow = hub.receiveShadow = true;
    this.decor.add(hub);
    const pin = new BoxGeometry(0.14, 0.05, 0.22);
    const pinMat = new MeshStandardMaterial({ color: toHex(mixRgb(c.text2, surface, 0.3)), roughness: 0.4, metalness: 0.4 });
    for (let i = 0; i < 5; i++) {
      const off = -0.52 + i * 0.26;
      for (const [x, z, turn] of [[off, -0.92, 0], [off, 0.92, 0], [-0.92, off, 1], [0.92, off, 1]] as const) {
        const m = new Mesh(pin, pinMat);
        m.position.set(layout.hub.x + x, 0.03, layout.hub.z + z);
        if (turn) m.rotation.y = Math.PI / 2;
        this.decor.add(m);
      }
    }
    const hubGlyph = new Mesh(this.geo.glyph.clone(), new MeshBasicMaterial({ map: this.chipTexture(), transparent: true, depthWrite: false }));
    hubGlyph.position.set(layout.hub.x, 0.224, layout.hub.z);
    hubGlyph.scale.set(1.25, 1, 1.25);
    this.decor.add(hubGlyph);
    const bus = new MeshBasicMaterial({ color: toHex(mixRgb(c.text2, boardTone, c.dark ? 0.55 : 0.6)), transparent: true, opacity: 0.9, depthWrite: false });
    const districts = [...layout.districts.values()];
    if (districts.length) {
      // The bus runs along the back of the front row, out of the way of everything on the pads;
      // it drops into each front-row district's back edge, and a back row's front edge.
      const frontZ0 = Math.max(...districts.map((d) => d.z0));
      const busZ = frontZ0 - 0.4;
      const strip = (x0: number, z0: number, x1: number, z1: number) => {
        const m = new Mesh(new BoxGeometry(Math.abs(x1 - x0) + 0.06, 0.01, Math.abs(z1 - z0) + 0.06), bus);
        m.position.set((x0 + x1) / 2, 0.006, (z0 + z1) / 2);
        this.decor.add(m);
      };
      strip(layout.hub.x, layout.hub.z - 0.95, layout.hub.x, busZ);
      strip(layout.hub.x, busZ, Math.max(...districts.map((d) => d.x0 + 0.6)), busZ);
      for (const d of districts) strip(d.x0 + 0.6, busZ, d.x0 + 0.6, d.z0 === frontZ0 ? d.z0 : d.z1);
    }
    // Districts: a tinted pad with the agent's outline. The pad is the agent's: clicking it picks the agent.
    for (const d of layout.districts.values()) {
      const tone = c.agents.get(d.id) ?? c.muted;
      const dormant = !c.agents.has(d.id);
      const pad = new Mesh(roundedPlane(d, 0.45), new MeshBasicMaterial({ color: toHex(mixRgb(tone, boardTone, c.dark ? 0.86 : 0.9)), polygonOffset: true, polygonOffsetFactor: -1, polygonOffsetUnits: -1 }));
      pad.position.y = 0.004;
      pad.userData.partId = d.id;
      pad.receiveShadow = true;
      this.decor.add(pad);
      this.pickables.push(pad);
      const rim = line(outline(d, 0.45, 0.008), tone, dormant ? 0.5 : 0.85);
      if (dormant) { rim.material = new LineDashedMaterial({ color: toHex(c.muted), dashSize: 0.22, gapSize: 0.16, transparent: true, opacity: 0.7, depthWrite: false }); rim.computeLineDistances(); }
      // Floors over the district: glass trays with the agent's rim, only where there are parts.
      for (const layer of ["runtime", "config"] as const) {
        const r = d.floors[layer];
        if (!r) continue;
        const glass = new MeshStandardMaterial({ color: toHex(mixRgb(layer === "config" ? c.blue : tone, surface, c.dark ? 0.8 : 0.84)), transparent: true, opacity: 0.2, roughness: 0.25, depthWrite: false });
        const s = slab(r, SLAB, 0.3, glass);
        s.renderOrder = -1;
        this.decor.add(s);
        const rimLine = line(outline(r, 0.3, 0), layer === "config" ? mixRgb(c.blue, tone, 0.35) : tone, 0.75);
        this.floors.push({ layer, district: d, slab: s, rim: rimLine });
      }
    }
    if (layout.calls) {
      const glass = new MeshStandardMaterial({ color: toHex(mixRgb(c.accent, surface, 0.85)), transparent: true, opacity: 0.22, roughness: 0.25, depthWrite: false });
      const s = slab(layout.calls, SLAB, 0.4, glass);
      this.decor.add(s);
      const dashed = new LineLoop(outline(layout.calls, 0.4, 0), new LineDashedMaterial({ color: toHex(c.accent), dashSize: 0.2, gapSize: 0.14, transparent: true, opacity: 0.7, depthWrite: false }));
      dashed.computeLineDistances();
      this.decor.add(dashed);
      this.shelf = { slab: s, rim: dashed };
    }
    // Shadow frustum fits the board; light from the upper left front.
    const span = Math.max(layout.board.x1 - layout.board.x0, layout.board.z1 - layout.board.z0) / 2 + 3;
    const cx = (layout.board.x0 + layout.board.x1) / 2, cz = (layout.board.z0 + layout.board.z1) / 2;
    Object.assign(this.sun.shadow.camera, { left: -span, right: span, top: span, bottom: -span, near: 1, far: span * 8 });
    this.sun.shadow.camera.updateProjectionMatrix();
    this.sun.position.set(cx - span * 0.6, span * 2.4, cz + span * 1.1);
    this.sun.target.position.set(cx, 0, cz);
    this.dirty = true;
    this.placeFloors();
  }

  /** One board tile: a dot at its centre. Repeated once per unit, so the dots sit on whole units. */
  private dotTexture(tone: Rgb): CanvasTexture {
    const c = this.colors, size = 64;
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = size;
    const g = canvas.getContext("2d")!;
    g.fillStyle = `rgb(${Math.round(tone.r)}, ${Math.round(tone.g)}, ${Math.round(tone.b)})`;
    g.fillRect(0, 0, size, size);
    const dot = mixRgb(tone, c.text2, c.dark ? 0.28 : 0.22);
    g.fillStyle = `rgb(${Math.round(dot.r)}, ${Math.round(dot.g)}, ${Math.round(dot.b)})`;
    g.beginPath(); g.arc(size / 2, size / 2, 2.6, 0, Math.PI * 2); g.fill();
    const t = new CanvasTexture(canvas);
    t.colorSpace = SRGBColorSpace;
    t.wrapS = t.wrapT = RepeatWrapping;
    // Slab tops take world x and -z as UVs (geometry.ts): one tile per unit, offset half a tile so dots land on whole units.
    t.offset.set(0.5, 0.5);
    t.anisotropy = Math.min(8, this.renderer.capabilities.getMaxAnisotropy());
    return t;
  }

  /** The chip's face: a die outline and a smaller square, in the board's ink. */
  private chipTexture(): CanvasTexture {
    const c = this.colors, canvas = document.createElement("canvas");
    canvas.width = canvas.height = 128;
    const g = canvas.getContext("2d")!;
    const ink = c.dark ? c.text2 : { r: 255, g: 255, b: 255 };
    g.strokeStyle = `rgba(${Math.round(ink.r)}, ${Math.round(ink.g)}, ${Math.round(ink.b)}, 0.85)`;
    g.lineWidth = 6;
    g.beginPath(); g.roundRect(18, 18, 92, 92, 12); g.stroke();
    g.beginPath(); g.roundRect(44, 44, 40, 40, 6); g.stroke();
    const t = new CanvasTexture(canvas);
    t.colorSpace = SRGBColorSpace;
    return t;
  }

  /** Floors follow the explode amount; dashed guides at each district's corners show which way they moved. */
  private placeFloors() {
    const ex = this.explode.value;
    for (const f of this.floors) {
      const y = plateY(f.layer, ex);
      const show = !this.view.hidden.has(f.layer);
      f.slab.position.y = y;
      f.rim.position.y = y + SLAB + 0.002;
      f.slab.visible = f.rim.visible = show;
      // Assembled floors sit close: keep them faint so the parts read first.
      (f.slab.material as MeshStandardMaterial).opacity = (this.colors.dark ? 0.16 : 0.2) * (0.6 + 0.4 * ex) * (this.view.xray ? 0.6 : 1);
      const focus = this.view.focus;
      (f.rim.material as LineBasicMaterial).opacity = (focus && focus !== f.district.id ? 0.15 : 0.75);
    }
    if (this.shelf) {
      const y = plateY("calls", ex);
      this.shelf.slab.position.y = y; this.shelf.rim.position.y = y + SLAB + 0.002;
      this.shelf.slab.visible = this.shelf.rim.visible = !this.view.hidden.has("calls");
    }
    const key = Math.round(ex * 100);
    if (key === this.guidesAt || !this.layout) return;
    this.guidesAt = key;
    if (this.guides) { this.decor.remove(this.guides); this.guides.geometry.dispose(); (this.guides.material as Material).dispose(); this.guides = null; }
    if (ex < 0.05) return;
    const pts: number[] = [];
    // From each tray's front corners down to the board: where it lifted from.
    for (const d of this.layout.districts.values()) {
      for (const layer of ["runtime", "config"] as const) {
        const r = d.floors[layer];
        if (!r) continue;
        const top = plateY(layer, ex);
        for (const x of [r.x0, r.x1]) pts.push(x, 0.01, r.z1, x, top, r.z1);
      }
    }
    if (!pts.length) return;
    const g = new BufferGeometry();
    g.setAttribute("position", new Float32BufferAttribute(pts, 3));
    const c = this.colors;
    this.guides = new LineSegments(g, new LineDashedMaterial({ color: toHex(c.muted), dashSize: 0.16, gapSize: 0.14, transparent: true, opacity: 0.55 * Math.min(1, ex * 1.5), depthWrite: false }));
    this.guides.computeLineDistances();
    this.decor.add(this.guides);
  }
}

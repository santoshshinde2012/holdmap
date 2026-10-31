// The WebGL renderer for the 3D machine (three.js). It only projects the pure scene and layout:
// what is drawn and how sure it is are decided in scene.ts. It renders on demand; the continuous
// effects (CPU pulse, connection dots) run at most at 30 fps, pause while the window is hidden,
// and are off with reduced motion.
import {
  AmbientLight, BoxGeometry, BufferGeometry, CanvasTexture, CapsuleGeometry, CylinderGeometry, DirectionalLight, EdgesGeometry, Float32BufferAttribute,
  Group, HemisphereLight, LineBasicMaterial, LineDashedMaterial, LineLoop, LineSegments, Mesh, MeshBasicMaterial, MeshStandardMaterial, PCFShadowMap,
  PerspectiveCamera, PlaneGeometry, QuadraticBezierCurve3, Raycaster, RingGeometry, Scene, SphereGeometry, SRGBColorSpace, TubeGeometry, Vector2, Vector3,
  WebGLRenderer, type Material,
} from "three";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";
import { cameraPosition, clampPose, EASE, FOV, lerpPose, posesEqual, type Pose } from "./camera";
import { partSize, plateY, type MachineLayout, type Placed, type Size } from "./layout";
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
  /** Parts a clicked reading lights up; everything else steps back. */
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

type Vec = { x: number; y: number; z: number };
interface Tint { mat: MeshStandardMaterial | MeshBasicMaterial | LineBasicMaterial; base: number; glass?: boolean; blueprint?: boolean }
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
  pulse?: Mesh;
  halo?: Mesh;
  beam?: Mesh;
  marker?: Mesh;
  slices?: Group;
  /** What the slices were built from, so hovers and scans with the same samples don't rebuild them. */
  slicesKey?: string;
  /** Agent initials on the tower cap, and what they were drawn from. */
  mono?: Mesh;
  monoKey?: string;
  /** Part of the opening build-up rather than a change: it rises in without the "new" glow. */
  intro: boolean;
  sel: Mesh;
  from: Vec;
  to: Placed;
  moveAt: number;
  pos: Vec;
  opacity: number;
  scale: number;
  born: number;
  dying: number | null;
  fresh: number;
}
interface WireItem { wire: Wire; obj: Mesh; mat: MeshStandardMaterial; base: number; radius: number; arc: boolean; key: string; packets: Mesh[] }

const UP = new Vector3(0, 1, 0);
/** Materials don't release their textures; the board pattern and tower initials are canvas textures. */
function disposeMaterial(mat: Material) {
  (mat as MeshBasicMaterial).map?.dispose();
  mat.dispose();
}
const MOVE_MS = 700, EXPLODE_MS = 900, FRESH_MS = 4000, ENTER_MS = 520, EXIT_MS = 260, INTRO_MS = 760;
/** Opening build-up: each layer starts after the one below, agents a little apart. */
const INTRO_DELAY: Record<Layer, number> = { board: 0, runtime: 260, config: 520, calls: 760 };
/** Continuous effects: smooth while the window has focus, a slow tick while it doesn't. */
const PULSE_FPS = 30, IDLE_FPS = 8;

export class MachineStage {
  readonly renderer: WebGLRenderer;
  private readonly scene3 = new Scene();
  private readonly camera = new PerspectiveCamera(FOV, 1, 0.1, 600);
  private readonly root = new Group();
  private readonly decor = new Group();
  private readonly sun = new DirectionalLight(0xffffff, 1.5);
  private readonly hemi = new HemisphereLight(0xffffff, 0xdddddd, 1.1);
  private readonly ambient = new AmbientLight(0xffffff, 0.25);
  private readonly ray = new Raycaster();
  private readonly geo = {
    tower: new RoundedBoxGeometry(1.5, 1, 1.5, 3, 0.1).translate(0, 0.5, 0),
    cap: new RoundedBoxGeometry(1.18, 0.07, 1.18, 2, 0.03),
    slice: new CylinderGeometry(0.5, 0.5, 0.05, 24),
    port: new BoxGeometry(0.6, 0.34, 0.44).translate(0, 0.17, 0),
    socket: new BoxGeometry(0.34, 0.12, 0.03),
    tile: new BoxGeometry(1.1, 0.08, 0.8).translate(0, 0.04, 0),
    tab: new BoxGeometry(0.42, 0.05, 0.16),
    sphere: new SphereGeometry(0.28, 24, 16).translate(0, 0.28, 0),
    hex: new CylinderGeometry(0.42, 0.42, 0.5, 6).translate(0, 0.25, 0),
    devbox: new RoundedBoxGeometry(0.8, 0.5, 0.6, 2, 0.08).translate(0, 0.25, 0),
    puck: new CylinderGeometry(0.32, 0.32, 0.36, 28).translate(0, 0.18, 0),
    cube: new BoxGeometry(0.46, 0.46, 0.46).translate(0, 0.23, 0),
    chip: new CylinderGeometry(0.46, 0.46, 0.16, 6).translate(0, 0.08, 0),
    card: new BoxGeometry(0.7, 0.1, 0.9).translate(0, 0.05, 0),
    sheet: new BoxGeometry(0.9, 0.07, 0.7).translate(0, 0.035, 0),
    more: new CylinderGeometry(0.3, 0.3, 0.2, 24).translate(0, 0.1, 0),
    marker: new SphereGeometry(0.11, 16, 10),
    beam: new CylinderGeometry(0.06, 0.34, 2.2, 20, 1, true).translate(0, 1.1, 0),
    rod: new CylinderGeometry(1, 1, 1, 10, 1, true),
    trace: new BoxGeometry(1, 1, 1),
    packet: new SphereGeometry(0.07, 12, 8),
    call: new CapsuleGeometry(0.1, 0.38, 4, 10).rotateZ(Math.PI / 2).translate(0, 0.11, 0),
    mono: new PlaneGeometry(0.98, 0.98).rotateX(-Math.PI / 2),
  };
  private readonly edgeCache = new Map<BufferGeometry, EdgesGeometry>();
  private readonly ringCache = new Map<string, RingGeometry>();
  private readonly items = new Map<string, Item>();
  private readonly wires = new Map<string, WireItem>();
  private readonly pickables: Mesh[] = [];
  private colors: StageColors;
  private view: StageView = { explode: 0, xray: false, focus: null, selected: null, hovered: null, hidden: new Set(), reduced: false };
  private layout: MachineLayout | null = null;
  private explode = { value: 0, from: 0, to: 0, at: 0 };
  private pose: Pose = { theta: 0.32, phi: 0.5, distance: 30, tx: 0, ty: 1.5, tz: 0 };
  private flight: { from: Pose; to: Pose; at: number; ms: number } | null = null;
  private plates: { board?: Mesh; boardRim?: LineLoop; runtime?: Mesh; runtimeRim?: LineLoop; config?: Mesh; configRim?: LineLoop; ring?: LineLoop; hub?: Mesh; calls?: LineLoop } = {};
  private width = 1;
  private height = 1;
  private insets = { top: 0, bottom: 0 };
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
    this.applyTheme();
    for (const item of this.items.values()) this.restyle(item);
    for (const w of this.wires.values()) this.restyleWire(w);
    this.wake();
  }

  setScene(scene: MachineScene, layout: MachineLayout) {
    const now = performance.now();
    // The board, plates and lane depend only on the ring and radius, not on which parts or calls
    // are drawn, so new recorded calls don't rebuild them.
    const decorKey = JSON.stringify([layout.radius, layout.ring, [...layout.angles], layout.callsRadius]);
    this.layout = layout;
    if (decorKey !== this.decorKey) { this.decorKey = decorKey; this.buildDecor(layout); }
    const seen = new Set<string>();
    const agentOrder = new Map(scene.agents.map((a, i) => [a.id, i]));
    for (const p of scene.parts) {
      const to = layout.parts.get(p.id);
      if (!to) continue;
      seen.add(p.id);
      let item = this.items.get(p.id);
      if (!item || item.dying !== null && item.part.kind !== p.kind) {
        if (item) this.remove(item);
        item = this.build(p, to, now + (this.first ? INTRO_DELAY[p.layer] + (agentOrder.get(p.owner) ?? 0) * 70 : 0));
        this.items.set(p.id, item);
      } else {
        item.dying = null;
        const shapeChanged = item.part.kind !== p.kind || item.part.variant !== p.variant && p.kind === "tool";
        item.part = p;
        if (shapeChanged) { this.remove(item); item = this.build(p, to, now); this.items.set(p.id, item); }
        if (Math.abs(item.to.x - to.x) > 1e-3 || Math.abs(item.to.z - to.z) > 1e-3 || item.to.layer !== to.layer) {
          item.from = { ...item.pos }; item.to = to; item.moveAt = now;
        }
        item.group.rotation.y = Math.atan2(Math.cos(to.angle), Math.sin(to.angle));
      }
      item.size = partSize(p);
      this.restyle(item);
    }
    for (const item of this.items.values()) if (!seen.has(item.part.id) && item.dying === null) item.dying = now;
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

  zoom(factor: number) { this.setPose({ ...this.pose, distance: this.pose.distance * factor }); }

  /** Pan by screen pixels, along the camera's right and the ground's forward axis. */
  pan(dx: number, dy: number) {
    const scale = (2 * this.pose.distance * Math.tan((FOV * Math.PI) / 360)) / this.height;
    const right = new Vector3(Math.cos(this.pose.theta), 0, -Math.sin(this.pose.theta));
    const forward = new Vector3(Math.sin(this.pose.theta), 0, Math.cos(this.pose.theta));
    const move = right.multiplyScalar(-dx * scale).add(forward.multiplyScalar(-dy * scale));
    this.setPose({ ...this.pose, tx: this.pose.tx + move.x, tz: this.pose.tz + move.z });
  }

  resize(width: number, height: number) {
    this.width = Math.max(1, width); this.height = Math.max(1, height);
    this.renderer.setPixelRatio(Math.min(2, globalThis.devicePixelRatio || 1));
    this.renderer.setSize(this.width, this.height, false);
    this.camera.aspect = this.width / this.height;
    this.applyInsets();
  }

  /** Overlays along the top and bottom edges: the picture centres in the space between them. */
  setInsets(top: number, bottom: number) {
    if (top === this.insets.top && bottom === this.insets.bottom) return;
    this.insets = { top, bottom };
    this.applyInsets();
  }
  private applyInsets() {
    const shift = Math.round((this.insets.top - this.insets.bottom) / 2);
    if (shift) this.camera.setViewOffset(this.width, this.height, 0, -shift, this.width, this.height);
    else this.camera.clearViewOffset();
    this.camera.updateProjectionMatrix();
    this.dirty = true;
    this.wake();
  }

  /** The part under a point in CSS pixels relative to the canvas, if any. */
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

  /** Screen anchors (CSS pixels) above each part, plus the plates' left edges and the hub. */
  anchors(ids: Iterable<string>): Map<string, { x: number; y: number; behind: boolean }> {
    const out = new Map<string, { x: number; y: number; behind: boolean }>();
    const v = new Vector3();
    const put = (id: string, x: number, y: number, z: number) => {
      v.set(x, y, z).project(this.camera);
      out.set(id, { x: ((v.x + 1) / 2) * this.width, y: ((1 - v.y) / 2) * this.height, behind: v.z > 1 || v.z < -1 });
    };
    for (const id of ids) {
      const item = this.items.get(id);
      if (item) { put(id, item.pos.x, item.pos.y + item.height * item.scale + (item.marker ? 0.36 : 0.22), item.pos.z); continue; }
      // Plate names sit on the rim at the viewer's left, clear of the parts in the middle.
      const r = this.layout?.radius ?? 0;
      const sx = -Math.cos(this.pose.theta) * r, sz = Math.sin(this.pose.theta) * r;
      if (id === "plate:runtime") put(id, sx, plateY("runtime", this.explode.value) + 0.1, sz);
      else if (id === "plate:config") put(id, sx, plateY("config", this.explode.value) + 0.1, sz);
      else if (id === "plate:calls" && this.layout) put(id, -Math.cos(this.pose.theta) * this.layout.callsRadius, plateY("calls", this.explode.value) + 0.1, Math.sin(this.pose.theta) * this.layout.callsRadius);
      else if (id === "hub") put(id, 0, 0.5, 0);
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
    // Between pulse frames, with nothing else moving, there is nothing to advance.
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
      // Position: eased move to the new layout slot, at the current plate height.
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
        // Opening build-up: rise from the board (or plate), each layer after the one below.
        scale = Math.max(0.001, EASE.enter((now - item.born) / INTRO_MS));
        busy = true;
      } else if (!reduced && now - item.born < ENTER_MS) {
        scale = 0.6 + 0.4 * EASE.enter((now - item.born) / ENTER_MS);
        busy = true;
      }
      // Height follows memory samples smoothly.
      const h = item.size.h;
      if (Math.abs(item.height - h) > 1e-3) { item.height += (h - item.height) * smooth; busy = true; this.dirty = true; } else item.height = h;
      // Opacity: hidden layers vanish, other agents fade while one is in focus.
      const target = this.targetOpacity(item);
      if (Math.abs(item.opacity - target) > 0.004) { item.opacity += (target - item.opacity) * smooth; busy = true; this.dirty = true; } else item.opacity = target;
      if (scale !== item.scale) { item.scale = scale; this.dirty = true; }
      if (item.fresh > now) busy = true;
      this.place(item, now);
    }
    return busy;
  }

  private hasContinuous(): boolean {
    for (const item of this.items.values()) if ((item.pulse || item.part.kind === "call" && item.part.live) && item.opacity > 0.2) return true;
    for (const w of this.wires.values()) if (w.packets.length) return true;
    return false;
  }

  private render(now: number) {
    const [x, y, z] = cameraPosition(this.pose);
    this.camera.position.set(x, y, z);
    this.camera.lookAt(this.pose.tx, this.pose.ty, this.pose.tz);
    // Plates follow the explode animation and layer visibility.
    this.placePlates();
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
    this.renderer.dispose();
    this.renderer.forceContextLoss();
  }

  // ---------- items ----------

  /** Base height: the board, a plate at the current explode amount, or just above the board for remote hosts beyond its edge. */
  private baseY(item: Pick<Item, "part" | "to">): number {
    return item.to.layer !== "board" ? plateY(item.to.layer, this.explode.value) + 0.03 : item.part.kind === "remote" ? 0.7 : 0;
  }

  private targetOpacity(item: Item): number {
    const v = this.view;
    const tower = item.part.kind === "agent" || item.part.kind === "dormant";
    if (v.hidden.has(item.part.layer) && !tower) return 0;
    // A clicked reading: its parts stay lit, towers stay as landmarks, everything else steps back.
    if (v.highlight) return v.highlight.has(item.part.id) ? 1 : tower ? 0.35 : 0.08;
    if (v.focus && item.part.owner !== v.focus) return 0.1;
    return 1;
  }

  private std(color = 0xffffff): MeshStandardMaterial {
    return new MeshStandardMaterial({ color, roughness: 0.6, metalness: 0.04, transparent: true });
  }
  private flat(color = 0xffffff): MeshBasicMaterial {
    return new MeshBasicMaterial({ color, transparent: true, depthWrite: false });
  }
  private edgesOf(geo: BufferGeometry): EdgesGeometry {
    let e = this.edgeCache.get(geo);
    if (!e) { e = new EdgesGeometry(geo, 28); this.edgeCache.set(geo, e); }
    return e;
  }
  private ring(inner: number, outer: number): RingGeometry {
    const key = `${inner}:${outer}`;
    let g = this.ringCache.get(key);
    if (!g) { g = new RingGeometry(inner, outer, 64).rotateX(-Math.PI / 2); this.ringCache.set(key, g); }
    return g;
  }

  private bodyGeometry(p: Part): BufferGeometry {
    switch (p.kind) {
      case "agent": case "dormant": return this.geo.tower;
      case "port": return this.geo.port;
      case "project": return this.geo.tile;
      case "remote": return this.geo.sphere;
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
    body.castShadow = p.layer === "board" || p.layer === "runtime";
    body.receiveShadow = p.kind === "agent";
    const edgeMat = new LineBasicMaterial({ transparent: true, depthWrite: false });
    body.add(new LineSegments(this.edgesOf(body.geometry), edgeMat));
    group.add(body);
    this.pickables.push(body);
    const size = partSize(p);
    const sel = new Mesh(this.ring(Math.max(size.w, size.d) * 0.62, Math.max(size.w, size.d) * 0.62 + 0.07), this.flat());
    sel.position.y = 0.02;
    group.add(sel);
    const item: Item = { part: p, group, body, size, height: size.h, tints: [], solids: [bodyMat], edgeMat, edgeBase: 0, sel,
      from: { x: to.x, y: this.baseY({ part: p, to }), z: to.z }, to, moveAt: now - MOVE_MS, pos: { x: to.x, y: this.baseY({ part: p, to }), z: to.z },
      opacity: this.first ? 1 : 0, scale: this.first && !this.view.reduced ? 0.001 : 1, born: now, dying: null, fresh: this.first ? 0 : now + FRESH_MS, intro: this.first };

    if (p.kind === "agent") {
      const capMat = this.std();
      item.cap = new Mesh(this.geo.cap, capMat);
      item.cap.userData.partId = p.id;
      item.solids.push(capMat);
      group.add(item.cap);
      this.pickables.push(item.cap);
      item.pulse = new Mesh(this.ring(1.02, 1.2), this.flat());
      item.pulse.position.y = 0.015;
      group.add(item.pulse);
      item.slices = new Group();
      group.add(item.slices);
      item.mono = new Mesh(this.geo.mono, new MeshBasicMaterial({ transparent: true, depthWrite: false }));
      group.add(item.mono);
    }
    if (p.kind === "agent" || p.kind === "dormant") {
      item.halo = new Mesh(this.ring(1.28, 1.5), this.flat());
      item.halo.position.y = 0.012;
      group.add(item.halo);
    }
    if (p.kind === "port") {
      const socket = new Mesh(this.geo.socket, this.std());
      socket.position.set(0, 0.17, 0.225);
      item.solids.push(socket.material as MeshStandardMaterial);
      group.add(socket);
      item.beam = new Mesh(this.geo.beam, this.flat());
      item.beam.position.y = 0.34;
      group.add(item.beam);
    }
    if (p.kind === "project") {
      const tab = new Mesh(this.geo.tab, this.std());
      tab.position.set(-0.3, 0.1, -0.32);
      item.solids.push(tab.material as MeshStandardMaterial);
      group.add(tab);
    }
    if (p.kind === "instructions") {
      for (const dy of [0.11, 0.22]) {
        const sheet = new Mesh(this.geo.sheet, bodyMat);
        sheet.position.set(dy * 0.4, dy, -dy * 0.3);
        sheet.userData.partId = p.id;
        this.pickables.push(sheet);
        group.add(sheet);
      }
    }
    if (p.kind === "mcp" || p.kind === "skill" || p.kind === "tool" || p.kind === "instructions" || p.kind === "project") {
      item.marker = new Mesh(this.geo.marker, this.std());
      item.solids.push(item.marker.material as MeshStandardMaterial);
      group.add(item.marker);
    }
    group.rotation.y = Math.atan2(Math.cos(to.angle), Math.sin(to.angle));
    this.root.add(group);
    return item;
  }

  private remove(item: Item) {
    this.root.remove(item.group);
    item.group.traverse((o) => {
      const m = o as Mesh;
      const i = this.pickables.indexOf(m);
      if (i >= 0) this.pickables.splice(i, 1);
      if (m.material) for (const mat of Array.isArray(m.material) ? m.material : [m.material]) disposeMaterial(mat);
    });
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
    let bodyBase = evidenceAlpha, edge = c.text2, edgeBase = p.evidence === "inferred" || p.evidence === "unknown" ? 0.55 : 0, glass = true, blueprint = false;
    switch (p.kind) {
      case "agent":
        body.color.setHex(tint(agent, c.dark ? 0.62 : 0.74));
        (item.cap!.material as MeshStandardMaterial).color.setHex(toHex(agent));
        edge = agent;
        break;
      case "dormant":
        body.color.setHex(toHex(c.gray)); bodyBase = 0.12; edge = c.muted; edgeBase = 0.8; blueprint = true;
        break;
      case "port":
        body.color.setHex(p.warn ? toHex(c.warn) : tint(agent, c.dark ? 0.3 : 0.38));
        (item.solids[1]).color.setHex(toHex(mixRgb(c.text, surface, c.dark ? 0.75 : 0.2)));
        break;
      case "project":
        body.color.setHex(toHex(c.dark ? c.surface3 : c.surface)); edge = c.muted; edgeBase = Math.max(edgeBase, 0.35);
        item.solids[1].color.setHex(toHex(agent));
        break;
      case "remote":
        body.color.setHex(tint(c.muted, 0.35));
        break;
      case "tool":
        body.color.setHex(tint(agent, c.dark ? 0.55 : 0.7));
        if (p.variant === "mcp_server") edge = c.ok;
        break;
      case "mcp": {
        glass = false; blueprint = true;
        const tone = p.variant === "ok" ? c.ok : p.variant === "off" ? c.gray : c.blue;
        body.color.setHex(toHex(tone)); edge = tone;
        bodyBase = p.variant === "ok" ? 0.95 : p.variant === "off" ? 0.12 : p.variant === "unknown" ? 0.3 : 0.45;
        edgeBase = p.variant === "ok" ? 0.6 : 0.9;
        break;
      }
      case "skill":
        glass = false; blueprint = true;
        body.color.setHex(toHex(c.violet)); edge = p.variant === "changed" ? c.danger : p.warn ? c.warn : c.violet;
        bodyBase = p.variant === "trusted" ? 0.7 : 0.45; edgeBase = 0.9;
        break;
      case "instructions":
        glass = false; blueprint = true;
        body.color.setHex(toHex(c.gray)); edge = c.gray; bodyBase = 0.4; edgeBase = 0.85;
        break;
      case "more":
        body.color.setHex(toHex(c.gray)); edge = c.muted; bodyBase = 0.35; edgeBase = 0.7;
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
    if (item.marker) {
      const m = item.marker.material as MeshStandardMaterial;
      m.color.setHex(toHex(p.warn ? (p.variant === "changed" ? c.danger : c.warn) : c.ok));
      m.emissive.setHex(m.color.getHex()); m.emissiveIntensity = 0.35;
      item.marker.visible = !!p.warn;
    }
    if (item.beam) { (item.beam.material as MeshBasicMaterial).color.setHex(toHex(c.warn)); item.beam.visible = !!p.warn; item.tints.push({ mat: item.beam.material as MeshBasicMaterial, base: c.dark ? 0.16 : 0.22 }); }
    if (item.pulse) { (item.pulse.material as MeshBasicMaterial).color.setHex(toHex(agent)); }
    if (item.halo) { (item.halo.material as MeshBasicMaterial).color.setHex(toHex(c.danger)); item.halo.visible = !!p.warn; item.tints.push({ mat: item.halo.material as MeshBasicMaterial, base: 0.3 }); }
    const selected = this.view.selected === p.id, hovered = this.view.hovered === p.id;
    (item.sel.material as MeshBasicMaterial).color.setHex(toHex(c.accent));
    item.sel.visible = selected || hovered;
    item.tints.push({ mat: item.sel.material as MeshBasicMaterial, base: selected ? 0.95 : 0.5 });
    item.edgeMat.color.setHex(toHex(selected ? c.accent : edge));
    item.edgeBase = selected ? 1 : hovered ? Math.max(edgeBase, 0.7) : edgeBase;
    if (item.slices) this.buildSlices(item, agent);
    if (item.mono) this.paintMonogram(item, agent);
    this.dirty = true;
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
    group.position.set(item.pos.x, item.pos.y, item.pos.z);
    group.scale.setScalar(item.scale);
    const h = item.height;
    if (part.kind === "agent" || part.kind === "dormant") item.body.scale.y = h;
    if (item.cap) item.cap.position.y = h + 0.035;
    if (item.mono) {
      // Upright from the default view whichever way the tower faces.
      item.mono.position.y = h + 0.073;
      item.mono.rotation.y = -group.rotation.y;
      const m = item.mono.material as MeshBasicMaterial;
      m.opacity = item.opacity * (this.view.xray ? 0.3 : 1);
      m.visible = m.opacity > 0.02;
    }
    if (item.marker) item.marker.position.y = (part.kind === "instructions" ? 0.35 : h) + 0.16;
    const xray = this.view.xray;
    for (const t of item.tints) {
      let o = t.base * item.opacity;
      if (xray && t.glass) o *= 0.2;
      if (xray && t.blueprint) o = Math.min(1, o * 1.6);
      t.mat.opacity = o;
      t.mat.visible = o > 0.004;
      if (!(t.mat instanceof LineBasicMaterial) && !(t.mat instanceof MeshBasicMaterial)) t.mat.depthWrite = o > 0.9;
    }
    const edgeO = Math.max(item.edgeBase, xray ? 0.5 : 0) * item.opacity;
    item.edgeMat.opacity = edgeO;
    item.edgeMat.visible = edgeO > 0.01;
    // Emphasis: selection, hover, a part that just appeared, a running MCP server.
    const c = this.colors;
    const fresh = item.fresh > now ? (item.fresh - now) / FRESH_MS : 0;
    const body = item.body.material as MeshStandardMaterial;
    // Towers are large: a soft glow keeps their agent colour; the ring and edges mark the selection.
    const tower = part.kind === "agent" || part.kind === "dormant";
    const sel = this.view.selected === part.id ? (tower ? 0.1 : 0.3) : this.view.hovered === part.id ? (tower ? 0.06 : 0.16) : 0;
    const lit = this.view.highlight?.has(part.id) && !tower ? 0.24 : 0;
    // A running MCP server glows steadily; a call still running breathes.
    const live = part.kind === "mcp" && part.variant === "ok" ? 0.28
      : part.kind === "call" && part.live ? (this.view.reduced ? 0.3 : 0.18 + 0.22 * (0.5 + 0.5 * Math.sin((now / 1000) * Math.PI * 2 * 0.8))) : 0;
    const glow = Math.max(sel, lit, fresh * 0.55, live);
    const tone = part.kind === "call" ? (part.variant === "error" ? c.danger : c.accent) : live > sel && live > fresh * 0.55 ? c.ok : c.accent;
    body.emissive.setHex(toHex(tone));
    body.emissiveIntensity = glow;
    // CPU pulse: frequency and depth follow the agent's observed CPU.
    if (item.pulse) {
      const cpu = Math.min(100, part.cpu);
      const amp = 0.3 + Math.min(1, cpu / 40) * 0.7;
      const wave = this.view.reduced ? 0.5 : 0.5 + 0.5 * Math.sin((now / 1000) * Math.PI * 2 * (0.3 + (cpu / 100) * 1.6));
      const mat = item.pulse.material as MeshBasicMaterial;
      mat.opacity = (0.14 + 0.5 * wave * amp) * item.opacity;
      item.pulse.scale.setScalar(1 + (this.view.reduced ? 0 : 0.06 * wave * amp));
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

  private syncWires(scene: MachineScene) {
    const seen = new Set<string>();
    for (const w of scene.wires) {
      if (!this.items.has(w.from) || !this.items.has(w.to)) continue;
      seen.add(w.id);
      const arc = w.kind === "socket";
      const key = `${w.kind}:${w.from}:${w.to}`;
      let item = this.wires.get(w.id);
      if (item && item.key !== key) { this.removeWire(item); item = undefined; }
      if (!item) {
        const mat = this.std();
        const obj = new Mesh(arc ? new TubeGeometry(new QuadraticBezierCurve3(new Vector3(), new Vector3(0, 1, 0), new Vector3(1, 0, 0)), 24, 0.022, 6) : w.kind === "folder" || w.kind === "listener" ? this.geo.trace : this.geo.rod, mat);
        obj.userData.ends = "";
        this.root.add(obj);
        item = { wire: w, obj, mat, base: 1, radius: 0.02, arc, key, packets: [] };
        this.wires.set(w.id, item);
      }
      item.wire = w;
      this.restyleWire(item);
    }
    for (const w of [...this.wires.values()]) if (!seen.has(w.wire.id)) this.removeWire(w);
  }

  private removeWire(w: WireItem) {
    this.root.remove(w.obj);
    if (w.arc) w.obj.geometry.dispose();
    w.mat.dispose();
    for (const p of w.packets) { this.root.remove(p); (p.material as Material).dispose(); }
    this.wires.delete(w.wire.id);
  }

  private restyleWire(w: WireItem) {
    const c = this.colors, wire = w.wire;
    const owner = this.items.get(wire.from)?.part.owner ?? "";
    const agent = c.agents.get(owner) ?? c.gray;
    const surface = c.dark ? c.surface3 : c.surface;
    const set = (rgb: Rgb, base: number, radius: number) => { w.mat.color.setHex(toHex(rgb)); w.base = base; w.radius = radius; };
    w.mat.emissiveIntensity = 0;
    switch (wire.kind) {
      case "process": set(mixRgb(agent, c.muted, 0.35), wire.evidence === "observed" ? 0.6 : 0.38, 0.026); break;
      case "listener": set(mixRgb(agent, surface, 0.25), 0.75, 0.05); break;
      case "folder": set(mixRgb(c.muted, surface, 0.2), 0.45, 0.045); break;
      case "socket": set(mixRgb(c.muted, surface, 0.15), 0.42, 0.02); break;
      case "config": set(wire.to.includes(":skill:") ? c.violet : wire.to.includes(":mcp:") ? c.blue : c.gray, 0.3, 0.016); break;
      case "running": set(c.ok, 0.92, 0.055); w.mat.emissive.setHex(toHex(c.ok)); w.mat.emissiveIntensity = 0.45; break;
    }
    // Moving dots on connection arcs: open sockets, not traffic volume.
    const want = wire.kind === "socket" ? Math.min(3, Math.max(1, Math.ceil((wire.weight ?? 1) / 2))) : 0;
    while (w.packets.length > want) { const p = w.packets.pop()!; this.root.remove(p); (p.material as Material).dispose(); }
    while (w.packets.length < want) { const p = new Mesh(this.geo.packet, this.flat()); this.root.add(p); w.packets.push(p); }
    for (const p of w.packets) (p.material as MeshBasicMaterial).color.setHex(toHex(agent));
    this.dirty = true;
  }

  private anchor(item: Item, end: "top" | "base" | "mid"): Vector3 {
    const h = item.height * item.scale;
    return new Vector3(item.pos.x, item.pos.y + (end === "top" ? h : end === "mid" ? h * 0.6 : 0.02), item.pos.z);
  }

  private placeWire(w: WireItem, now: number) {
    const a = this.items.get(w.wire.from), b = this.items.get(w.wire.to);
    if (!a || !b) return;
    const kind = w.wire.kind;
    const hidden = this.view.hidden.has(a.part.layer) || this.view.hidden.has(b.part.layer);
    let opacity = hidden ? 0 : w.base * Math.min(a.opacity, b.opacity) * Math.min(a.scale, b.scale);
    if (this.view.xray && kind !== "running" && kind !== "config") opacity *= 0.45;
    if (this.view.xray && kind === "config") opacity = Math.min(1, opacity * 1.8);
    w.mat.opacity = opacity;
    w.mat.visible = w.obj.visible = opacity > 0.01;
    w.mat.depthWrite = false;
    if (!w.obj.visible) { for (const p of w.packets) p.visible = false; return; }
    let from: Vector3, to: Vector3;
    if (kind === "folder" || kind === "listener" && a.part.layer === "board") {
      // A flat trace on the board, from the agent's foot to the part.
      from = this.anchor(a, "base"); to = this.anchor(b, "base");
      const dx = to.x - from.x, dz = to.z - from.z, len = Math.hypot(dx, dz);
      const trim = Math.min(0.75, len / 2);
      w.obj.position.set(from.x + (dx * (trim + (len - trim * 2) / 2)) / len, 0.012, from.z + (dz * (trim + (len - trim * 2) / 2)) / len);
      w.obj.rotation.set(0, -Math.atan2(dz, dx), 0);
      w.obj.scale.set(Math.max(0.01, len - trim * 2), 0.012, w.radius * 1.4);
      return;
    }
    if (kind === "socket") {
      from = this.anchor(a, "mid"); to = b.part.kind === "remote" ? this.anchor(b, "mid") : this.anchor(b, "top");
      const ends = `${from.x.toFixed(2)},${from.y.toFixed(2)},${from.z.toFixed(2)}>${to.x.toFixed(2)},${to.y.toFixed(2)},${to.z.toFixed(2)}`;
      const mid = from.clone().add(to).multiplyScalar(0.5);
      mid.y += 1 + from.distanceTo(to) * 0.18;
      const curve = new QuadraticBezierCurve3(from, mid, to);
      if (w.obj.userData.ends !== ends) {
        w.obj.geometry.dispose();
        w.obj.geometry = new TubeGeometry(curve, 32, w.radius, 6);
        w.obj.userData.ends = ends;
      }
      w.packets.forEach((p, i) => {
        p.visible = !this.view.reduced;
        const t = ((now / 1000) * 0.22 + i / w.packets.length) % 1;
        p.position.copy(curve.getPoint(t));
        (p.material as MeshBasicMaterial).opacity = Math.min(1, opacity * 2.2);
      });
      return;
    }
    // Vertical-ish rods: process (agent top → process base), config (agent top → file base),
    // listener from a process (process base → port top), running (MCP file base → process top).
    from = kind === "running" || kind === "listener" ? this.anchor(a, "base") : this.anchor(a, "top");
    to = kind === "running" || kind === "listener" ? this.anchor(b, "top") : this.anchor(b, "base");
    const dir = new Vector3().subVectors(to, from);
    const len = dir.length();
    w.obj.position.copy(from).addScaledVector(dir, 0.5);
    if (len > 1e-6) w.obj.quaternion.setFromUnitVectors(UP, dir.normalize());
    w.obj.scale.set(w.radius, Math.max(1e-3, len), w.radius);
  }

  // ---------- board, plates and light ----------

  private applyTheme() {
    const c = this.colors;
    this.hemi.color.setHex(0xffffff);
    this.hemi.groundColor.setHex(toHex(c.dark ? c.surface : c.surface3));
    this.hemi.intensity = c.dark ? 0.85 : 1.15;
    this.sun.intensity = c.dark ? 1.25 : 1.55;
    this.ambient.intensity = c.dark ? 0.3 : 0.2;
    if (this.layout) this.buildDecor(this.layout);
  }

  private buildDecor(layout: MachineLayout) {
    for (const child of [...this.decor.children]) {
      this.decor.remove(child);
      const m = child as Mesh;
      m.geometry?.dispose();
      for (const mat of Array.isArray(m.material) ? m.material : [m.material]) if (mat) disposeMaterial(mat);
    }
    const c = this.colors, r = layout.radius;
    const circle = (radius: number, y: number, color: Rgb, opacity: number) => {
      const pts: number[] = [];
      for (let i = 0; i < 128; i++) { const a = (i / 128) * Math.PI * 2; pts.push(Math.cos(a) * radius, y, Math.sin(a) * radius); }
      const g = new BufferGeometry();
      g.setAttribute("position", new Float32BufferAttribute(pts, 3));
      const loop = new LineLoop(g, new LineBasicMaterial({ color: toHex(color), transparent: true, opacity, depthWrite: false }));
      this.decor.add(loop);
      return loop;
    };
    // Cylinder groups: side, top, bottom. The top carries a faint circuit-board pattern.
    const boardTone = c.dark ? c.surface2 : c.surface;
    const side = new MeshStandardMaterial({ color: toHex(boardTone), roughness: 0.82, metalness: 0.02 });
    const top = new MeshStandardMaterial({ color: 0xffffff, map: this.circuit(layout, boardTone), roughness: 0.8, metalness: 0.02 });
    const board = new Mesh(new CylinderGeometry(r, r, 0.36, 128), [side, top, side]);
    board.position.y = -0.18;
    board.receiveShadow = true;
    this.decor.add(board);
    this.plates.board = board;
    this.plates.boardRim = circle(r, 0.002, c.border, 0.9);
    if (layout.ring > 0) {
      this.plates.ring = circle(layout.ring, 0.004, c.muted, 0.35);
      const hub = new Mesh(new CylinderGeometry(1.05, 1.15, 0.2, 48), new MeshStandardMaterial({ color: toHex(c.dark ? c.surface3 : c.surface2), roughness: 0.7 }));
      hub.position.y = 0.1;
      hub.castShadow = hub.receiveShadow = true;
      this.decor.add(hub);
      this.plates.hub = hub;
      // Spokes from the hub to each agent, as on the map.
      for (const [id, a] of layout.angles) {
        const tone = c.agents.get(id) ?? c.muted;
        const len = layout.ring - 2.0;
        const spoke = new Mesh(new BoxGeometry(len, 0.014, 0.07), new MeshStandardMaterial({ color: toHex(mixRgb(tone, c.dark ? c.surface2 : c.surface, 0.45)), roughness: 0.7 }));
        spoke.position.set(Math.cos(a) * (1.15 + len / 2), 0.008, Math.sin(a) * (1.15 + len / 2));
        spoke.rotation.y = -a;
        spoke.receiveShadow = true;
        this.decor.add(spoke);
      }
    }
    const glass = (tone: Rgb) => new Mesh(new CylinderGeometry(r, r, 0.05, 128), new MeshStandardMaterial({ color: toHex(tone), transparent: true, opacity: c.dark ? 0.1 : 0.14, roughness: 0.25, depthWrite: false }));
    this.plates.runtime = glass(c.dark ? c.surface3 : c.surface);
    this.plates.config = glass(mixRgb(c.blue, c.dark ? c.surface3 : c.surface, 0.75));
    this.plates.runtimeRim = circle(r, 0, c.text2, 0.3);
    this.plates.configRim = circle(r, 0, c.blue, 0.4);
    this.decor.add(this.plates.runtime, this.plates.config);
    this.plates.calls = undefined;
    if (layout.callsRadius > 0) {
      const pts: number[] = [];
      for (let i = 0; i < 160; i++) { const a = (i / 160) * Math.PI * 2; pts.push(Math.cos(a) * layout.callsRadius, 0, Math.sin(a) * layout.callsRadius); }
      const g = new BufferGeometry();
      g.setAttribute("position", new Float32BufferAttribute(pts, 3));
      const lane = new LineLoop(g, new LineDashedMaterial({ color: toHex(c.accent), dashSize: 0.18, gapSize: 0.14, transparent: true, opacity: 0.5, depthWrite: false }));
      lane.computeLineDistances();
      this.decor.add(lane);
      this.plates.calls = lane;
    }
    // Shadow frustum fits the board; light from the upper left front.
    const s = r + 3;
    Object.assign(this.sun.shadow.camera, { left: -s, right: s, top: s, bottom: -s, near: 1, far: s * 6 });
    this.sun.shadow.camera.updateProjectionMatrix();
    this.sun.position.set(-s * 0.7, s * 2.2, s * 0.9);
    this.dirty = true;
    this.placePlates();
  }

  /** A faint printed-circuit pattern for the board's top: a dot grid, the agent ring, traces
   *  between sectors and solder pads. Decoration only; nothing in it is data. */
  private circuit(layout: MachineLayout, tone: Rgb): CanvasTexture {
    const size = 1024, c = this.colors;
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = size;
    const ctx = canvas.getContext("2d")!;
    const ink = (alpha: number) => { const m = mixRgb(tone, c.text2, 1); return `rgba(${Math.round(m.r)}, ${Math.round(m.g)}, ${Math.round(m.b)}, ${alpha})`; };
    ctx.fillStyle = `rgb(${Math.round(tone.r)}, ${Math.round(tone.g)}, ${Math.round(tone.b)})`;
    ctx.fillRect(0, 0, size, size);
    const k = size / 2 / layout.radius, mid = size / 2;
    const at = (r: number, a: number) => [mid + Math.cos(a) * r * k, mid + Math.sin(a) * r * k] as const;
    // Dot grid, clipped to the disc.
    ctx.save();
    ctx.beginPath(); ctx.arc(mid, mid, size / 2 - 2, 0, Math.PI * 2); ctx.clip();
    ctx.fillStyle = ink(c.dark ? 0.12 : 0.1);
    for (let x = 12; x < size; x += 24) for (let y = 12; y < size; y += 24) ctx.fillRect(x - 1, y - 1, 2, 2);
    ctx.restore();
    ctx.strokeStyle = ink(c.dark ? 0.2 : 0.16);
    ctx.lineWidth = 2;
    // Concentric rails just inside the rim and around the ring.
    for (const r of [layout.radius - 0.35, layout.ring + 1.1, layout.ring - 1.1]) if (r > 0.6) { ctx.beginPath(); ctx.arc(mid, mid, r * k, 0, Math.PI * 2); ctx.stroke(); }
    // Traces between agent sectors, each with a jog and a pad.
    const angles = [...layout.angles.values()].sort((a, b) => a - b);
    angles.forEach((a, i) => {
      const next = angles[(i + 1) % angles.length] + (i === angles.length - 1 ? Math.PI * 2 : 0);
      const gap = angles.length > 1 ? (a + next) / 2 : a + Math.PI;
      const r0 = Math.max(1.4, layout.ring - 1.1), r1 = layout.radius - 0.35;
      const [x0, y0] = at(r0, gap), [x1, y1] = at((r0 + r1) / 2, gap), [x2, y2] = at((r0 + r1) / 2, gap + 0.08), [x3, y3] = at(r1, gap + 0.08);
      ctx.beginPath(); ctx.moveTo(x0, y0); ctx.lineTo(x1, y1); ctx.lineTo(x2, y2); ctx.lineTo(x3, y3); ctx.stroke();
      ctx.fillStyle = ink(c.dark ? 0.28 : 0.22);
      for (const [x, y] of [[x0, y0], [x3, y3]]) { ctx.beginPath(); ctx.arc(x, y, 5, 0, Math.PI * 2); ctx.fill(); }
    });
    const texture = new CanvasTexture(canvas);
    texture.colorSpace = SRGBColorSpace;
    texture.anisotropy = Math.min(8, this.renderer.capabilities.getMaxAnisotropy());
    return texture;
  }

  private placePlates() {
    const p = this.plates;
    if (p.calls) {
      p.calls.position.y = plateY("calls", this.explode.value) + 0.02;
      p.calls.visible = !this.view.hidden.has("calls");
    }
    for (const [mesh, rim, layer] of [[p.runtime, p.runtimeRim, "runtime"], [p.config, p.configRim, "config"]] as const) {
      if (!mesh || !rim) continue;
      const y = plateY(layer, this.explode.value) - 0.026;
      const show = !this.view.hidden.has(layer);
      mesh.position.y = y; rim.position.y = y + 0.026;
      mesh.visible = rim.visible = show;
      // Assembled plates sit close together: keep them faint so the parts read first.
      (mesh.material as MeshStandardMaterial).opacity = (this.colors.dark ? 0.08 : 0.12) * (0.55 + 0.45 * this.explode.value) * (this.view.xray ? 0.6 : 1);
    }
  }

}

// Design tokens resolved to plain sRGB for the WebGL renderer, so the 3D machine uses the same
// colours as the rest of the app in both themes. Read from a probe element under the stage.

export interface Rgb { r: number; g: number; b: number }
export interface StageColors {
  dark: boolean;
  surface: Rgb; surface2: Rgb; surface3: Rgb; text: Rgb; text2: Rgb; muted: Rgb; border: Rgb;
  accent: Rgb; ok: Rgb; warn: Rgb; danger: Rgb; blue: Rgb; violet: Rgb; gray: Rgb;
  /** Agent colour by agent id, shared with cards, map and timeline. */
  agents: Map<string, Rgb>;
}

const FALLBACK: Rgb = { r: 128, g: 128, b: 128 };

/** Parses computed colours: rgb()/rgba() with commas or spaces, #hex and color(srgb …). Alpha is dropped. */
export function parseColor(value: string): Rgb | null {
  const v = value.trim();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(v);
  if (hex) {
    const h = hex[1].length === 3 ? hex[1].split("").map((c) => c + c).join("") : hex[1];
    return { r: parseInt(h.slice(0, 2), 16), g: parseInt(h.slice(2, 4), 16), b: parseInt(h.slice(4, 6), 16) };
  }
  const rgb = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/i.exec(v);
  if (rgb) return { r: Number(rgb[1]), g: Number(rgb[2]), b: Number(rgb[3]) };
  const srgb = /^color\(srgb\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)/i.exec(v);
  if (srgb) return { r: Number(srgb[1]) * 255, g: Number(srgb[2]) * 255, b: Number(srgb[3]) * 255 };
  return null;
}

export const toHex = (c: Rgb) => (Math.round(c.r) << 16) | (Math.round(c.g) << 8) | Math.round(c.b);
export const mixRgb = (a: Rgb, b: Rgb, t: number): Rgb => ({ r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t });

const TOKENS = { surface: "--surface", surface2: "--surface-2", surface3: "--surface-3", text: "--text", text2: "--text-2", muted: "--muted", border: "--border-strong",
  accent: "--accent", ok: "--ok", warn: "--warn", danger: "--danger", blue: "--tone-blue", violet: "--tone-violet", gray: "--tone-gray" } as const;

/** Reads every token under `host`; `agents` maps ids to CSS colours such as "var(--accent)". */
export function readColors(host: HTMLElement, agents: ReadonlyMap<string, string>): StageColors {
  const probe = document.createElement("span");
  probe.style.display = "none";
  host.appendChild(probe);
  const read = (css: string) => { probe.style.color = ""; probe.style.color = css; return parseColor(getComputedStyle(probe).color) ?? FALLBACK; };
  try {
    const out = Object.fromEntries(Object.entries(TOKENS).map(([k, token]) => [k, read(`var(${token})`)])) as Omit<StageColors, "dark" | "agents">;
    const luminance = (c: Rgb) => (0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b) / 255;
    return { ...out, dark: luminance(out.surface) < 0.4, agents: new Map([...agents].map(([id, css]) => [id, read(css)])) };
  } finally {
    probe.remove();
  }
}

// Brand-ish monograms for frameworks/runtimes so rows are recognisable at a glance without
// shipping logo assets. Colours are tuned so white text on them meets WCAG AA (≥ 4.5:1).
import type { GraphNode, PortEntry } from "./types";

export interface Brand {
  glyph: string;
  bg: string;
  fg: string;
}

const B = (glyph: string, bg: string, fg = "#ffffff"): Brand => ({ glyph, bg, fg });

const BRANDS: [RegExp, Brand][] = [
  [/^next/i, B("N", "#111111")],
  [/^vite/i, B("V", "#6b46e5")],
  [/^svelte/i, B("S", "#c2410c")],
  [/^nuxt/i, B("N", "#047857")],
  [/^astro/i, B("A", "#c2410c")],
  [/^remix/i, B("R", "#1d4ed8")],
  [/^angular/i, B("A", "#b91c1c")],
  [/react|create react/i, B("R", "#0e7490")],
  [/^express/i, B("ex", "#3f3f46")],
  [/^nest/i, B("N", "#be123c")],
  [/^node/i, B("JS", "#3f7d2f")],
  [/^bun/i, B("B", "#3f3f46")],
  [/^deno/i, B("D", "#18181b")],
  [/^django/i, B("dj", "#0c4b33")],
  [/^fastapi/i, B("F", "#00796b")],
  [/^flask/i, B("Fl", "#27272a")],
  [/^uvicorn|^gunicorn/i, B("U", "#166534")],
  [/python/i, B("Py", "#2b5b84")],
  [/^rails|ruby/i, B("Rb", "#b91c1c")],
  [/^laravel|php/i, B("L", "#c2410c")],
  [/^go\b|golang/i, B("Go", "#0e7490")],
  [/^axum|^actix|^rocket|rust/i, B("Rs", "#9a3412")],
  [/spring|java/i, B("J", "#3f6212")],
  [/postgres/i, B("Pg", "#2f5e8e")],
  [/mysql|mariadb/i, B("My", "#00618a")],
  [/mongo/i, B("Mg", "#15803d")],
  [/redis|valkey/i, B("Rd", "#b91c1c")],
  [/kafka/i, B("K", "#27272a")],
  [/rabbit/i, B("Rq", "#c2410c")],
  [/elastic|opensearch/i, B("Es", "#0f766e")],
  [/nginx/i, B("Nx", "#15803d")],
  [/caddy/i, B("Cd", "#166534")],
  [/airplay|controlcenter/i, B("", "#52525b")],
  [/vs ?code|code helper/i, B("VS", "#1d63b8")],
];

const HUES = ["#4338ca", "#0f766e", "#b45309", "#be185d", "#1d4ed8", "#7e22ce", "#15803d", "#9f1239"];

function hash(s: string): number {
  let h = 0;
  for (const c of s) h = (h * 31 + c.charCodeAt(0)) | 0;
  return Math.abs(h);
}

export function initials(name: string): string {
  const words = name.replace(/[^A-Za-z0-9 ]+/g, " ").trim().split(/\s+/).filter(Boolean);
  if (!words.length) return "?";
  if (words.length === 1) return words[0].slice(0, 2).replace(/^./, (c) => c.toUpperCase());
  return (words[0][0] + words[1][0]).toUpperCase();
}

/** The monogram for a row: container → runtime tile, framework → brand, else a stable hash colour. */
export function brandFor(e: PortEntry): Brand & { kind: "container" | "brand" | "generic" | "hidden" } {
  if (!e.process && !e.container) return { ...B("?", "#52525b"), kind: "hidden" };
  if (e.container) {
    const fw = e.framework?.name ?? "";
    const hit = BRANDS.find(([re]) => re.test(fw));
    return { ...(hit ? hit[1] : B("◫", "#1d63b8")), kind: "container" };
  }
  const name = e.framework?.name ?? e.process?.name ?? "";
  const hit = BRANDS.find(([re]) => re.test(name));
  if (hit) return { ...hit[1], kind: "brand" };
  return { ...B(initials(name), HUES[hash(name) % HUES.length]), kind: "generic" };
}

export type BrandKind = "container" | "brand" | "generic" | "hidden" | "external" | "client";

/** The monogram for a topology node (same palette as rows). */
export function brandForNode(n: GraphNode): Brand & { kind: BrandKind } {
  if (n.kind === "external") return { ...B("◍", "#475467"), kind: "external" };
  if (n.kind === "hidden") return { ...B("?", "#52525b"), kind: "hidden" };
  const name = n.framework?.name ?? n.label;
  const hit = BRANDS.find(([re]) => re.test(name));
  const kind: BrandKind = n.kind === "container" ? "container" : n.kind === "client" ? "client" : hit ? "brand" : "generic";
  if (hit) return { ...hit[1], kind };
  return { ...B(initials(name), HUES[hash(name) % HUES.length]), kind };
}

/** WCAG relative luminance contrast ratio between two #rrggbb colours. */
export function contrast(a: string, b: string): number {
  const lum = (hex: string) => {
    const n = parseInt(hex.slice(1), 16);
    const ch = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => {
      const s = v / 255;
      return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
  };
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

export const ALL_BRANDS = [...BRANDS.map(([, b]) => b), ...HUES.map((h) => B("x", h))];

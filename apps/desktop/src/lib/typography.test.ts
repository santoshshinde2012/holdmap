// Typography lint: every font size, weight, line height, tracking and family in the desktop UI must
// come from the semantic tokens in app.css (display, title, heading, body, body-sm, caption, label,
// mono, mono-sm). Raw values only live in the :root token block. Exempt a line with
// `/* type-exempt: <reason> */` (e.g. a glyph that scales with its icon tile).
import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const ROLES = ["display", "title", "heading", "body", "body-sm", "caption", "label", "mono", "mono-sm"];
const role = `(?:${ROLES.join("|")})`;
const ALLOWED: Record<string, RegExp> = {
  "font-size": new RegExp(`^(?:var\\(--fs-${role}\\)|inherit)$`),
  "line-height": new RegExp(`^(?:var\\(--lh-${role}\\)|1|0|inherit|normal)$`),
  "letter-spacing": new RegExp(`^(?:var\\(--ls-${role}\\)|0|normal|inherit)$`),
  "font-weight": /^(?:var\(--fw-(?:regular|medium|semibold)\)|inherit)$/,
  "font-family": /^(?:var\(--font\)|var\(--mono\)|inherit)$/,
  font: /^(?:inherit)$/,
};

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) return n === "node_modules" || n === "assets" ? [] : files(p);
    return /\.(svelte|css)$/.test(n) && n !== "fonts.css" ? [p] : [];
  });
}

/** CSS of a file with :root token blocks removed (they define the raw scale). */
function css(path: string): string {
  let s = readFileSync(path, "utf8");
  if (path.endsWith(".svelte")) {
    const style = [...s.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]).join("\n");
    const inline = [...s.matchAll(/style="([^"]*)"/g)].map((m) => `x{${m[1]}}`).join("\n");
    s = style + "\n" + inline;
  }
  return s.replace(/:root(?:\[[^\]]*\])?\s*\{[^}]*\}/g, "");
}

export function violations(text: string): string[] {
  const out: string[] = [];
  text.split("\n").forEach((line) => {
    if (line.includes("type-exempt:")) return;
    for (const m of line.matchAll(/(?:^|[;{\s])(font-size|line-height|letter-spacing|font-weight|font-family|font)\s*:\s*([^;}]+)/g)) {
      const [, prop, raw] = m;
      const v = raw.trim().replace(/\s*!important$/, "");
      if (!ALLOWED[prop].test(v)) out.push(`${prop}: ${v}`);
    }
  });
  return out;
}

describe("typography tokens", () => {
  it("the lint itself catches raw values", () => {
    expect(violations(".a { font-size: 12px; font-weight: 700 }")).toEqual(["font-size: 12px", "font-weight: 700"]);
    expect(violations(".a { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium) }")).toEqual([]);
    expect(violations(".a { font-size: var(--fs-13) }")).toEqual(["font-size: var(--fs-13)"]);
    expect(violations(".a { font-size: 9px } /* type-exempt: test */")).toEqual([]);
  });

  it("no component uses a raw font size, weight, line height, tracking or family", () => {
    const bad = files(ROOT).flatMap((f) => violations(css(f)).map((v) => `${relative(ROOT, f)} → ${v}`));
    expect(bad).toEqual([]);
  });

  it("weights are limited to 400 / 500 / 600 and the scale is the agreed one", () => {
    const root = readFileSync(join(ROOT, "app.css"), "utf8");
    expect(root).toMatch(/--fw-regular: 400; --fw-medium: 500; --fw-semibold: 600;/);
    const scale = [...root.matchAll(/--fs-(\d+): (\d+)px/g)].map((m) => Number(m[2]));
    expect(scale).toEqual([11, 12, 13, 14, 16, 20, 24, 32]);
    for (const r of ROLES) expect(root, r).toMatch(new RegExp(`--fs-${r}: var\\(--fs-\\d+\\);\\s+--lh-${r}: \\d+px;\\s+--ls-${r}: -?[\\d.]+em;`));
  });

  it("fonts are bundled locally with font-display: swap", () => {
    const f = readFileSync(join(ROOT, "fonts.css"), "utf8");
    expect(f).not.toMatch(/https?:/);
    expect(f.match(/font-display: swap/g)?.length).toBe(2);
    for (const file of ["InterVariable.woff2", "JetBrainsMonoVariable.woff2"]) expect(statSync(join(ROOT, "assets/fonts", file)).size).toBeGreaterThan(20_000);
  });
});

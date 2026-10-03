// Contrast lint: the colour tokens in app.css must meet WCAG AA in both themes.
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { contrast } from "./lib/frameworks";

describe("design tokens meet WCAG AA", () => {
  const css = readFileSync(fileURLToPath(new URL("./app.css", import.meta.url)), "utf8");
  const block = (sel: string) => css.slice(css.indexOf(sel), css.indexOf("}", css.indexOf(sel)));
  const tok = (b: string, n: string) => b.match(new RegExp(`--${n}:\\s*(#[0-9a-fA-F]{6})`))![1];
  const raw = (b: string, n: string) => b.match(new RegExp(`--${n}:\\s*([^;]+);`))![1].trim();
  /** `rgb(r g b / a)` composited over a solid `#rrggbb`, as `#rrggbb`. */
  const over = (tint: string, base: string) => {
    const [r, g, bl, a] = tint.match(/rgb\((\d+) (\d+) (\d+) \/ ([\d.]+)\)/)!.slice(1).map(Number);
    const bs = [1, 3, 5].map((i) => parseInt(base.slice(i, i + 2), 16));
    return "#" + [r, g, bl].map((c, i) => Math.round(c * a + bs[i] * (1 - a)).toString(16).padStart(2, "0")).join("");
  };
  for (const [theme, sel] of [["light", ":root {"], ["dark", ':root[data-theme="dark"] {']] as const) {
    const b = block(sel);
    it(`${theme}: placeholder, hint and button text contrast`, () => {
      for (const bg of ["input-bg", "surface", "bg"]) {
        expect(contrast(tok(b, "faint"), tok(b, bg)), `faint on ${bg}`).toBeGreaterThanOrEqual(4.5);
        expect(contrast(tok(b, "muted"), tok(b, bg)), `muted on ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
      expect(contrast(tok(b, "accent-fg"), tok(b, "accent"))).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tok(b, "danger-fg"), tok(b, "danger"))).toBeGreaterThanOrEqual(4.5);
      expect(contrast(tok(b, "danger"), tok(b, "surface")), "danger text").toBeGreaterThanOrEqual(4.5);
    });
    it(`${theme}: text on tinted rows, chips and callouts`, () => {
      // Translucent tints are composited over the surface they sit on, then checked like solid colours.
      const pairs: [string, string, string][] = [
        ["muted", "row-selected", "bg"], ["faint", "row-selected", "bg"], ["faint", "row-hover", "bg"],
        ["accent", "accent-soft", "surface"], ["danger", "danger-soft", "surface"], ["warn", "warn-soft", "surface"], ["ok", "ok-soft", "surface"],
        ["tone-green", "tone-green-bg", "surface"], ["tone-blue", "tone-blue-bg", "surface"], ["tone-violet", "tone-violet-bg", "surface"],
        ["tone-amber", "tone-amber-bg", "surface"], ["tone-gray", "tone-gray-bg", "surface"],
      ];
      for (const [fg, tint, base] of pairs) {
        expect(contrast(tok(b, fg), over(raw(b, tint), tok(b, base))), `${fg} on ${tint}`).toBeGreaterThanOrEqual(4.5);
      }
    });
  }
});

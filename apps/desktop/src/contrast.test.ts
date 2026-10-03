// Contrast lint: the colour tokens in app.css must meet WCAG AA in both themes.
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { contrast } from "./lib/frameworks";

describe("design tokens meet WCAG AA", () => {
  const css = readFileSync(fileURLToPath(new URL("./app.css", import.meta.url)), "utf8");
  const block = (sel: string) => css.slice(css.indexOf(sel), css.indexOf("}", css.indexOf(sel)));
  const tok = (b: string, n: string) => b.match(new RegExp(`--${n}:\\s*(#[0-9a-fA-F]{6})`))![1];
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
  }
});

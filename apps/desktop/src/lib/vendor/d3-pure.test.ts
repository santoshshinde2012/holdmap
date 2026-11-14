import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { easeCubicInOut } from "./d3-ease";

// vite.config.ts marks d3's Lab, HCL and cubehelix setup pure so the bundle can drop it. That
// holds only while nothing in the app reaches those colour spaces.
const files = (dir: string): string[] => readdirSync(dir).flatMap((name) => {
  const path = join(dir, name);
  return statSync(path).isDirectory() ? files(path) : /\.(ts|svelte)$/.test(name) && !name.endsWith(".test.ts") ? [path] : [];
});

describe("d3 bundle trims", () => {
  it("never imports the colour spaces the build drops", () => {
    const offenders = files(join(__dirname, "../..")).filter((file) =>
      /from\s+["']d3-(color|interpolate)["']/.test(readFileSync(file, "utf8")));
    expect(offenders).toEqual([]);
  });

  it("keeps d3's default cubic in-out easing", () => {
    expect([0, 0.25, 0.5, 0.75, 1].map(easeCubicInOut)).toEqual([0, 0.0625, 0.5, 0.9375, 1]);
  });
});

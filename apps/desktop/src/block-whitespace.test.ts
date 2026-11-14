// Svelte drops the whitespace that starts a block's content, so `{#if x} · y{/if}` renders
// "a· y" with no space before the dot. Inline text that follows a block opener must start with
// an explicit `{" "}`. Blocks that open on their own line are not affected.
import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL(".", import.meta.url));
const OPENER = /\{(?:#if|:else if|:else|#each)[^{}\n]*\}( +)(?=[^\s<{])/g;

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) return n === "node_modules" || n === "assets" ? [] : files(p);
    return n.endsWith(".svelte") ? [p] : [];
  });
}

describe("inline block whitespace", () => {
  it("no inline block starts its text with a space Svelte would drop", () => {
    const bad: string[] = [];
    for (const file of files(ROOT)) {
      readFileSync(file, "utf8").split("\n").forEach((line, i) => {
        for (const m of line.matchAll(OPENER)) {
          if (line.slice(0, m.index).trim() !== "") bad.push(`${relative(ROOT, file)}:${i + 1}: ${m[0]}`);
        }
      });
    }
    expect(bad).toEqual([]);
  });
});

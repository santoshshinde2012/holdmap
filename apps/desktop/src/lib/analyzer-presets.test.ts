import { readFileSync, readdirSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { presetSource, presetViews, PRESET_SOURCES } from "./analyzer-presets";
import { readManifest } from "./analyzer-manifest";

const dir = new URL("../../../../crates/holdmap-analyzers/presets/", import.meta.url);

describe("analyzer presets in the browser demo", () => {
  it("are exactly the manifests the Rust crate ships", () => {
    const files = readdirSync(dir).filter((f) => f.endsWith(".toml")).map((f) => readFileSync(new URL(f, dir), "utf8"));
    expect([...PRESET_SOURCES].sort()).toEqual(files.sort());
  });

  it("pass the demo's manifest checks and never start MCP servers", () => {
    for (const src of PRESET_SOURCES) {
      const m = readManifest(src, ["nvidia.skillspector", "holdmap.osv"]);
      expect(m.kind).toBe("cli");
      expect(m.network.length > 0).toBe(m.sends.length > 0);
    }
  });

  it("list publisher, licence, install and network, and mark added ones", () => {
    const views = presetViews((id) => id === "pycqa.bandit");
    expect(views.map((v) => v.id)).toEqual(["cisco.skill-scanner", "google.osv-scanner", "aquasecurity.trivy-secrets", "pycqa.bandit"]);
    expect(views.find((v) => v.id === "google.osv-scanner")?.network).toEqual(["api.osv.dev"]);
    expect(views.every((v) => v.install && v.homepage.startsWith("https://github.com/"))).toBe(true);
    expect(views.filter((v) => v.added).map((v) => v.id)).toEqual(["pycqa.bandit"]);
    expect(() => presetSource("nope")).toThrow(/no preset/);
  });
});

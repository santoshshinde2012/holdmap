// Quick-select presets for the browser demo. The manifests are the same files the Rust crate
// compiles in (crates/holdmap-analyzers/presets), so the demo can't drift from the app.
import cisco from "../../../../crates/holdmap-analyzers/presets/cisco.skill-scanner.toml?raw";
import osv from "../../../../crates/holdmap-analyzers/presets/google.osv-scanner.toml?raw";
import trivy from "../../../../crates/holdmap-analyzers/presets/aquasecurity.trivy-secrets.toml?raw";
import bandit from "../../../../crates/holdmap-analyzers/presets/pycqa.bandit.toml?raw";
import { parseToml } from "./analyzer-manifest";

/** Mirrors `holdmap_analyzers::presets::PresetView`. */
export interface PresetView {
  id: string; name: string; publisher: string; summary: string; homepage: string; license: string;
  install: string | null; network: string[]; added: boolean;
}

export const PRESET_SOURCES: readonly string[] = [cisco, osv, trivy, bandit];

const text = (v: unknown) => (typeof v === "string" ? v : "");

/** The presets in UI order, marking those `added` reports as registered. */
export function presetViews(added: (id: string) => boolean): PresetView[] {
  return PRESET_SOURCES.map((src) => {
    const t = parseToml(src);
    const top = t[""], perms = t.permissions ?? {};
    const id = text(top.id);
    return {
      id, name: text(top.name), publisher: text(top.publisher), summary: text(top.summary),
      homepage: text(top.homepage), license: text(top.license), install: text(top.install) || null,
      network: Array.isArray(perms.network) ? perms.network : [], added: added(id),
    };
  });
}

/** One preset's manifest text. */
export function presetSource(id: string): string {
  const src = PRESET_SOURCES.find((s) => text(parseToml(s)[""].id) === id);
  if (!src) throw new Error(`no preset named "${id}"`);
  return src;
}

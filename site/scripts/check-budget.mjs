// Size budget for the built site (gzip sizes, what a visitor actually downloads).
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
const gz = (f) => gzipSync(readFileSync(f), { level: 9 }).length;
const kb = (n) => (n >= 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(2)} MB` : `${(n / 1024).toFixed(1)} KB`);
const files = (d) => readdirSync(d).map((f) => join(d, f)).filter((f) => statSync(f).isFile());

const index = readFileSync(join(dist, "index.html"), "utf8");
const scripts = [...index.matchAll(/<script[^>]+src="\/holdmap\/(_astro\/[^"]+\.js)"/g)].map((m) => join(dist, m[1]));
const astro = files(join(dist, "_astro"));
const images = astro.filter((f) => /\.(avif|webp|png|jpe?g)$/.test(f));
const fonts = astro.filter((f) => f.endsWith(".woff2"));
const demoJs = files(join(dist, "demo", "assets")).filter((f) => f.endsWith(".js"));
// Follow Vite's static import graph: capture renderers and encoders load on demand.
// Budget optional PNG code separately without changing existing feature limits.
const demoManifest = JSON.parse(readFileSync(join(dist, "demo", "manifest.json"), "utf8"));
function staticDemoFiles(roots) {
  const visited = new Set(), output = new Set();
  function visit(key) {
    if (visited.has(key)) return;
    visited.add(key);
    const entry = demoManifest[key];
    if (!entry) throw new Error(`Missing demo manifest entry: ${key}`);
    if (entry.file.endsWith(".js")) output.add(join(dist, "demo", entry.file));
    for (const imported of entry.imports ?? []) visit(imported);
  }
  for (const root of roots) visit(root);
  return output;
}
const initialDemoFiles = staticDemoFiles(Object.entries(demoManifest).filter(([, entry]) => entry.isEntry).map(([key]) => key));
if (!initialDemoFiles.size) throw new Error("Demo has no entry JavaScript in its manifest.");
const snapshotDemoFiles = new Set();
const visitedSnapshotEntries = new Set();
function visitSnapshotEntry(key) {
  if (visitedSnapshotEntries.has(key)) return;
  visitedSnapshotEntries.add(key);
  const entry = demoManifest[key];
  if (!entry) throw new Error(`Missing snapshot manifest entry: ${key}`);
  const file = join(dist, "demo", entry.file);
  // A lazy chunk may import shared helpers from the entry. Do not follow the
  // entry's unrelated dynamic imports back into every optional feature.
  if (initialDemoFiles.has(file)) return;
  if (entry.file.endsWith(".js")) snapshotDemoFiles.add(file);
  for (const imported of [...entry.imports ?? [], ...entry.dynamicImports ?? []]) visitSnapshotEntry(imported);
}
// Both paths must remain deferred. Missing manifest entries fail the budget check.
for (const key of ["src/lib/app-snapshot.ts", "src/lib/screen-snapshot-native.ts"]) {
  if (!demoManifest[key] || initialDemoFiles.has(join(dist, "demo", demoManifest[key].file))) {
    throw new Error(`Snapshot renderer must remain deferred: ${key}`);
  }
  visitSnapshotEntry(key);
}
// Opening Agents Overview must not load the capability explorer. Count only its
// unique static chunks here; dependencies shared with other entry points retain
// their existing initial, snapshot or optional-feature budget.
const capabilityKey = "src/components/agents/AgentCapabilitiesView.svelte";
const agentsKey = "src/components/AgentsView.svelte";
const capabilityEntry = demoManifest[capabilityKey];
if (!capabilityEntry?.isDynamicEntry || !demoManifest[agentsKey]) {
  throw new Error("Capability workspace and Agents must have separate manifest entries.");
}
const capabilityFile = join(dist, "demo", capabilityEntry.file);
const agentsStaticFiles = staticDemoFiles([agentsKey]);
if (initialDemoFiles.has(capabilityFile) || agentsStaticFiles.has(capabilityFile)) {
  throw new Error("Capability workspace must remain deferred until explicit inspection.");
}
const otherEntryFiles = staticDemoFiles(Object.entries(demoManifest)
  .filter(([key, entry]) => key !== capabilityKey && (entry.isEntry || entry.isDynamicEntry))
  .map(([key]) => key));
const capabilityDemoFiles = new Set([...staticDemoFiles([capabilityKey])].filter((file) => !otherEntryFiles.has(file)));
if (!capabilityDemoFiles.has(capabilityFile)) {
  throw new Error("Capability workspace must not be a static dependency of another entry point.");
}
const deferredDemoJs = demoJs.filter((file) => !initialDemoFiles.has(file) && !snapshotDemoFiles.has(file) && !capabilityDemoFiles.has(file));
const videos = files(join(dist, "hero")).filter((f) => /\.(mp4|webm)$/.test(f));

const rows = [
  ["Home page HTML (inline CSS)", gz(join(dist, "index.html")), 45 * 1024],
  ["Home page JS", scripts.reduce((n, f) => n + gz(f), 0), 20 * 1024],
  ["Site fonts (woff2)", fonts.reduce((n, f) => n + statSync(f).size, 0), 100 * 1024],
  ["Largest image", Math.max(...images.map((f) => statSync(f).size)), 260 * 1024],
  ["Open Graph image", statSync(join(dist, "og.png")).size, 300 * 1024],
  ["Live demo JS (initial imports)", [...initialDemoFiles].reduce((n, f) => n + gz(f), 0), 220 * 1024],
  ["Snapshot PNG JS (on capture)", [...snapshotDemoFiles].reduce((n, f) => n + gz(f), 0), 40 * 1024],
  ["Capability workspace JS (on inspection)", [...capabilityDemoFiles].reduce((n, f) => n + gz(f), 0), 20 * 1024],
  ["Demo JS (optional features/workers)", deferredDemoJs.reduce((n, f) => n + gz(f), 0), 160 * 1024],
  ["Largest hero video (one is loaded)", Math.max(...videos.map((f) => statSync(f).size)), 2.5 * 1024 * 1024],
];

let failed = false;
for (const [name, size, limit] of rows) {
  const ok = size <= limit;
  failed ||= !ok;
  console.log(`${ok ? "✔" : "✖"} ${name.padEnd(34)} ${kb(size).padStart(9)}  (budget ${kb(limit)})`);
}
process.exit(failed ? 1 : 0);

// Size budget for the built site (gzip sizes, what a visitor actually downloads).
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
const gz = (f) => gzipSync(readFileSync(f), { level: 9 }).length;
const kb = (n) => `${(n / 1024).toFixed(1)} KB`;
const files = (d) => readdirSync(d).map((f) => join(d, f)).filter((f) => statSync(f).isFile());

const index = readFileSync(join(dist, "index.html"), "utf8");
const scripts = [...index.matchAll(/<script[^>]+src="\/portwise\/(_astro\/[^"]+\.js)"/g)].map((m) => join(dist, m[1]));
const astro = files(join(dist, "_astro"));
const images = astro.filter((f) => /\.(avif|webp|png|jpe?g)$/.test(f));
const fonts = astro.filter((f) => f.endsWith(".woff2"));
const demoJs = files(join(dist, "demo", "assets")).filter((f) => f.endsWith(".js"));

const rows = [
  ["Home page HTML (inline CSS)", gz(join(dist, "index.html")), 45 * 1024],
  ["Home page JS", scripts.reduce((n, f) => n + gz(f), 0), 20 * 1024],
  ["Site fonts (woff2)", fonts.reduce((n, f) => n + statSync(f).size, 0), 100 * 1024],
  ["Largest image", Math.max(...images.map((f) => statSync(f).size)), 260 * 1024],
  ["Open Graph image", statSync(join(dist, "og.png")).size, 300 * 1024],
  ["Live demo JS (loaded on demand)", demoJs.reduce((n, f) => n + gz(f), 0), 220 * 1024],
];

let failed = false;
for (const [name, size, limit] of rows) {
  const ok = size <= limit;
  failed ||= !ok;
  console.log(`${ok ? "✔" : "✖"} ${name.padEnd(34)} ${kb(size).padStart(9)}  (budget ${kb(limit)})`);
}
process.exit(failed ? 1 : 0);

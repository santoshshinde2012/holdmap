// Check every internal link, asset and #anchor in the built site (dist/). External links are
// listed but only fetched with --external (they can be flaky in CI).
import { readFileSync, existsSync, readdirSync, statSync } from "node:fs";
import { join, dirname, relative } from "node:path";
import { fileURLToPath } from "node:url";

const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
const BASE = "/portwise/";
const ORIGIN = "https://santoshshinde2012.github.io";

const pages = [];
(function walk(d) {
  for (const f of readdirSync(d)) {
    const p = join(d, f);
    if (statSync(p).isDirectory()) walk(p);
    else if (f.endsWith(".html")) pages.push(p);
  }
})(dist);

const ids = new Map();
const idsOf = (file) => {
  if (!ids.has(file)) ids.set(file, new Set([...readFileSync(file, "utf8").matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])));
  return ids.get(file);
};

/** dist file for a site path like /portwise/docs/ or /portwise/_astro/x.css */
function fileFor(pathname) {
  let p = decodeURIComponent(pathname.slice(BASE.length));
  if (p === "" || p.endsWith("/")) p += "index.html";
  const f = join(dist, p);
  if (existsSync(f) && statSync(f).isFile()) return f;
  if (existsSync(join(f, "index.html"))) return join(f, "index.html");
  return null;
}

const problems = [];
const external = new Set();
let checked = 0;
for (const page of pages) {
  const html = readFileSync(page, "utf8");
  const rel = "/" + relative(dist, page).replace(/index\.html$/, "");
  const pageUrl = new URL(BASE.slice(0, -1) + rel, ORIGIN);
  const refs = [];
  for (const m of html.matchAll(/\s(?:href|src|data-src)="([^"]+)"/g)) refs.push(m[1]);
  for (const m of html.matchAll(/\ssrcset="([^"]+)"/g)) refs.push(...m[1].split(",").map((s) => s.trim().split(/\s+/)[0]));
  for (const ref of refs) {
    if (/^(mailto:|data:|javascript:)/.test(ref)) continue;
    let u;
    try { u = new URL(ref.replace(/&amp;/g, "&"), pageUrl); } catch { problems.push(`${rel}: invalid URL ${ref}`); continue; }
    if (u.origin !== ORIGIN) { external.add(u.href.split("#")[0]); continue; }
    checked++;
    if (!u.pathname.startsWith(BASE)) { problems.push(`${rel}: ${ref} is outside ${BASE}`); continue; }
    const target = fileFor(u.pathname);
    if (!target) { problems.push(`${rel}: broken link ${ref}`); continue; }
    if (u.hash && target.endsWith(".html") && !idsOf(target).has(decodeURIComponent(u.hash.slice(1)))) {
      problems.push(`${rel}: missing anchor ${ref}`);
    }
  }
}

if (process.argv.includes("--external")) {
  const skip = /releases\/(latest\/)?download\//; // release assets are checked by the release job
  for (const href of external) {
    if (skip.test(href)) continue;
    try {
      const r = await fetch(href, { method: "GET", redirect: "follow", signal: AbortSignal.timeout(15000), headers: { "user-agent": "portwise-site-linkcheck" } });
      if (r.status >= 400 && r.status !== 429) problems.push(`external ${r.status}: ${href}`);
    } catch (e) {
      problems.push(`external error: ${href} (${e.message})`);
    }
  }
}

console.log(`${pages.length} pages, ${checked} internal links checked, ${external.size} external links${process.argv.includes("--external") ? " fetched" : " (not fetched; pass --external)"}`);
if (problems.length) {
  console.error(problems.map((p) => `  ✖ ${p}`).join("\n"));
  process.exit(1);
}
console.log("✔ no broken links");

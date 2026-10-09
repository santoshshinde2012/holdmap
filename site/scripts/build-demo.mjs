// Build the real desktop UI (apps/desktop) for the browser into public/demo/. Outside Tauri the
// app runs on its mock data provider (src/lib/mock.ts), so the demo is the actual interface with
// a sample machine: nothing is scanned or stopped for real.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const site = join(dirname(fileURLToPath(import.meta.url)), "..");
const desktop = join(site, "..", "apps", "desktop");
const out = join(site, "public", "demo");
const base = "/holdmap/demo/";

if (!existsSync(join(desktop, "node_modules"))) {
  console.error("apps/desktop has no node_modules: run `npm ci` in apps/desktop first.");
  process.exit(1);
}

execFileSync("npx", ["vite", "build", "--base", base, "--outDir", out, "--emptyOutDir", "--logLevel", "warn"], {
  cwd: desktop,
  stdio: "inherit",
  env: { ...process.env, TAURI_ENV_PLATFORM: "" },
});

// Demo defaults before the app mounts: skip onboarding, start on the list, follow the site's
// theme (?theme=dark|light), keep "Open in browser" from opening localhost, and offer a way
// back to the site when the demo is opened on its own.
const boot = `<script>
(function () {
  var q = new URLSearchParams(location.search), t = q.get("theme");
  try {
    localStorage.setItem("pw.onboarded", "1");
    localStorage.setItem("pw.view", "list");
    localStorage.setItem("pw.sort", "group");
    if (t === "light" || t === "dark") localStorage.setItem("pw.theme", t);
  } catch (e) {}
  var open = window.open;
  window.open = function (u) { return /^https?:\\/\\/localhost/.test(String(u)) ? null : open.apply(window, arguments); };
  if (window.top === window) addEventListener("DOMContentLoaded", function () {
    var a = document.createElement("a");
    a.href = "../";
    a.textContent = "← holdmap · sample data";
    a.setAttribute("style", "position:fixed;left:12px;bottom:12px;z-index:9999;padding:6px 10px;border-radius:8px;font:500 12px/1.2 system-ui,sans-serif;background:#5048e5;color:#fff;text-decoration:none;box-shadow:0 4px 16px rgb(0 0 0/.25)");
    document.body.append(a);
  });
})();
</script>`;

const indexPath = join(out, "index.html");
let html = readFileSync(indexPath, "utf8");
html = html
  .replace("<title>holdmap</title>", "<title>holdmap live demo (sample data)</title>")
  .replace('<meta charset="UTF-8" />', `<meta charset="UTF-8" />\n    <meta name="robots" content="noindex" />\n    <meta name="description" content="The holdmap desktop app running in your browser with sample data." />`)
  .replace("</head>", `${boot}\n  </head>`);
writeFileSync(indexPath, html);
console.log(`demo → ${out}`);

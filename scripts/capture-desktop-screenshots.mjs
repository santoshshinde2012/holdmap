#!/usr/bin/env node
// Captures the desktop screenshots in docs/screenshots/ from the browser preview (mock data).
//
//   cd apps/desktop && npm run dev            # serves the UI on http://localhost:1420
//   npm install --no-save puppeteer-core      # once, anywhere on the module path
//   CHROME=/path/to/chrome node scripts/capture-desktop-screenshots.mjs docs/screenshots [name,name...]
//
// Each scenario is [width, height, theme, steps, hash, extra localStorage]. Names follow the
// <surface>-<view>-<theme>.png convention (see CONTRIBUTING.md). SETTLE (ms) waits for fonts and
// the first scan before each capture.
import puppeteer from "puppeteer-core";
const [,, out = "docs/screenshots", only = ""] = process.argv;
const URL = "http://localhost:1420/";
const K = async (p, mod, key) => { await p.keyboard.down(mod); await p.keyboard.press(key); await p.keyboard.up(mod); await w(450); };
const S = {
  "desktop-overview-light": [1440, 900, "light", async (p) => { await row(p, 3000); }],
  "desktop-overview-dark": [1440, 900, "dark", async (p) => { await row(p, 3000); }],
  "desktop-command-palette-light": [1440, 900, "light", async (p) => { await K(p, "Control", "k"); await p.keyboard.type("sto", { delay: 30 }); await w(300); }],
  "desktop-stop-confirm-light": [1440, 900, "light", async (p) => { await row(p, 3000); await p.keyboard.press("Backspace"); await w(500); }],
  "desktop-stop-progress-dark": [1440, 900, "dark", async (p) => { await row(p, 3001); await p.keyboard.press("Backspace"); await w(500); await p.keyboard.press("Enter"); await w(350); }],
  "desktop-stop-success-light": [1440, 900, "light", async (p) => { await row(p, 3001); await p.keyboard.press("Backspace"); await w(500); await p.keyboard.press("Enter"); await w(1800); }],
  "desktop-stopped-toast-dark": [1440, 900, "dark", async (p) => { await row(p, 5173); await p.keyboard.press("Backspace"); await w(500); await p.keyboard.press("Enter"); await w(1500); await p.keyboard.press("Escape"); await w(500); }],
  "desktop-stop-anyway-dark": [1440, 900, "dark", async (p) => { await row(p, 49152); await p.keyboard.press("Backspace"); await w(500); }],
  "desktop-onboarding-light": [1440, 900, "light", async (p) => {}, "", { "pw.onboarded": "" }],
  "desktop-explain-blocked-light": [1440, 900, "light", async (p) => { await row(p, 631); }],
  "desktop-free-port-light": [1440, 900, "light", async (p) => { await p.click("input[type=search]"); await p.keyboard.type("4321", { delay: 20 }); await w(900); }],
  "desktop-shortcuts-dark": [1440, 900, "dark", async (p) => { await p.keyboard.press("?"); await w(400); }],
  "desktop-settings-light": [1440, 900, "light", async (p) => { await K(p, "Control", ","); }],
  "desktop-settings-appearance-dark": [1440, 900, "dark", async (p) => { await K(p, "Control", ","); await clickText(p, '[role="tab"]', "Appearance"); await w(300); }],
  "desktop-pin-light": [1440, 900, "light", async (p) => { await row(p, 5173); await p.keyboard.down("Shift"); await p.keyboard.press("P"); await p.keyboard.up("Shift"); await w(400); }],
  "desktop-history-light": [1440, 900, "light", async (p) => { await row(p, 3001); await p.keyboard.press("Backspace"); await w(500); await p.keyboard.press("Enter"); await w(2500); await p.keyboard.press("Escape"); await w(300); await p.keyboard.press("h"); await w(500); }],
  "desktop-remote-dark": [1440, 900, "dark", async (p) => { await clickText(p, "button", "Remote"); await w(400); await p.keyboard.type("devbox", { delay: 15 }); await p.keyboard.press("Enter"); await w(1600); }],
  "desktop-graph-light": [1440, 900, "light", async (p) => { await p.keyboard.press("g"); await w(1500); }],
  "desktop-graph-dark": [1440, 900, "dark", async (p) => { await p.keyboard.press("g"); await w(1500); }],
  "desktop-cluster-stop-light": [1440, 900, "light", async (p) => { await p.keyboard.press("g"); await w(1200); await p.keyboard.press("ArrowDown"); await w(300); await p.keyboard.press("s"); await w(700); }],
  "desktop-list-by-cluster-light": [1440, 900, "light", async (p) => {}, "", { "pw.sort": "cluster" }],
  "desktop-narrow-light": [820, 760, "light", async (p) => { await row(p, 3000); }],
  "desktop-no-match-light": [1440, 900, "light", async (p) => { await p.click("input[type=search]"); await p.keyboard.type("zzzz", { delay: 20 }); await w(600); }],
  "desktop-compact-dark": [1440, 900, "dark", async (p) => {}, "", { "pw.density": "compact" }],
};
const w = (ms) => new Promise((r) => setTimeout(r, ms));
async function row(p, port) {
  const box = await p.evaluate((port) => { const r = [...document.querySelectorAll('[id^="row-"]')].find((el) => el.id.split(":")[1] == port); if (!r) return null; r.scrollIntoView({ block: "center" }); const b = r.getBoundingClientRect(); return { x: b.left + 60, y: b.top + b.height / 2 }; }, port);
  if (!box) throw new Error("no row " + port);
  await w(100); await p.mouse.click(box.x, box.y);
  await w(600); await p.mouse.move(2, 600); await w(150);
}
async function hover(p, port) {
  const b = await p.evaluate((port) => { const r = [...document.querySelectorAll('[id^="row-"]')].find((el) => el.id.split(":")[1] == port); const b = r.getBoundingClientRect(); return { x: b.left + 200, y: b.top + b.height / 2 }; }, port);
  await p.mouse.move(b.x, b.y); await w(400);
}
async function tab(p, name) { await clickText(p, '[role="tab"]', name); await w(300); }
async function clickText(p, sel, text) {
  const b = await p.evaluate((sel, text) => { const b = [...document.querySelectorAll(sel)].find((x) => x.textContent.trim().startsWith(text) || x.getAttribute("aria-label")?.startsWith(text)); if (!b) return null; const r = b.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; }, sel, text);
  if (!b) throw new Error(`no ${sel} "${text}"`);
  await p.mouse.click(b.x, b.y);
}
const fs = await import("fs"); fs.mkdirSync(out, { recursive: true });
const browser = await puppeteer.launch({ executablePath: process.env.CHROME || "/usr/bin/google-chrome", headless: "new", args: ["--no-sandbox", "--font-render-hinting=none", "--force-device-scale-factor=1"] });
for (const [name, [wd, ht, theme, fn, hash = "", extra = {}]] of Object.entries(S)) {
  if (only && !only.split(",").includes(name)) continue;
  const page = await browser.newPage();
  await page.setViewport({ width: wd, height: ht, deviceScaleFactor: 1 });
  await page.emulateMediaFeatures([{ name: "prefers-reduced-motion", value: "reduce" }]);
  await page.evaluateOnNewDocument((theme, extra) => { if (!sessionStorage.getItem("seeded")) { localStorage.clear(); sessionStorage.setItem("seeded", "1"); } localStorage.setItem("pw.theme", theme); localStorage.setItem("pw.onboarded", "1"); localStorage.setItem("pw.view", "list"); for (const [k, v] of Object.entries(extra)) localStorage.setItem(k, v); }, theme, extra);
  const errs = []; page.on("pageerror", (e) => errs.push(String(e))); page.on("console", (m) => m.type() === "error" && errs.push(m.text()));
  await page.goto(URL + hash, { waitUntil: "networkidle0" }); await w(+process.env.SETTLE || 3000);
  try { await fn(page); await w(250); await page.screenshot({ path: `${out}/${name}.png` }); console.log("ok", name); }
  catch (e) { console.log("FAIL", name, e.message); }
  if (errs.length) console.log("  errors:", errs.slice(0, 3).join(" | "));
  await page.close();
}
await browser.close();

// Regenerate public/og.png (1200×630) for Open Graph and Twitter cards. Not part of the build;
// run it when the brand or screenshots change, with any Playwright install and a Chromium:
//   PLAYWRIGHT=/path/to/node_modules/playwright/index.mjs CHROME=/path/to/chrome node scripts/og.mjs
// The card is plain HTML below, rendered with the site's fonts and a real app screenshot.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const { chromium } = await import(process.env.PLAYWRIGHT ?? "playwright");
const site = join(dirname(fileURLToPath(import.meta.url)), "..");
const b64 = (p) => readFileSync(join(site, p)).toString("base64");
const inter = b64("src/assets/fonts/inter-site.woff2");
const mono = b64("src/assets/fonts/jetbrains-mono-site.woff2");
const shot = b64("../docs/screenshots/desktop-overview-dark.png");
const icon = readFileSync(join(site, "public/favicon.svg"), "utf8");

const html = `<!doctype html><html><head><style>
@font-face { font-family: I; src: url(data:font/woff2;base64,${inter}) format("woff2"); font-weight: 400 800; }
@font-face { font-family: M; src: url(data:font/woff2;base64,${mono}) format("woff2"); font-weight: 400 700; }
* { margin: 0; box-sizing: border-box; }
body { width: 1200px; height: 630px; overflow: hidden; background: #07070b; color: #f1f2f6; font-family: I; position: relative; }
.grid { position: absolute; inset: 0; background-image: radial-gradient(rgb(255 255 255 / .07) 1.2px, transparent 1.4px); background-size: 28px 28px; mask-image: radial-gradient(ellipse 80% 80% at 30% 40%, #000, transparent 75%); }
.glow { position: absolute; width: 900px; height: 600px; left: 380px; top: 160px; background: radial-gradient(closest-side, rgb(99 102 241 / .45), rgb(34 211 238 / .12) 60%, transparent); }
.copy { position: absolute; left: 72px; top: 76px; width: 600px; }
.brand { display: flex; align-items: center; gap: 14px; font-size: 30px; font-weight: 700; letter-spacing: -.02em; }
.brand svg { width: 52px; height: 52px; }
h1 { margin-top: 56px; font-size: 84px; line-height: .98; letter-spacing: -.05em; font-weight: 800; }
h1 span { font-family: M; font-weight: 700; letter-spacing: -.06em; background: linear-gradient(100deg,#22d3ee,#6366f1 52%,#a855f7); -webkit-background-clip: text; color: transparent; }
p { margin-top: 26px; font-size: 26px; line-height: 1.4; color: #b4bac8; }
.tags { position: absolute; left: 72px; bottom: 64px; display: flex; gap: 10px; font-family: M; font-size: 18px; color: #c9cdd8; }
.tags b { font-weight: 500; padding: 8px 14px; border-radius: 10px; border: 1px solid rgb(255 255 255 / .14); background: rgb(255 255 255 / .04); }
.shot { position: absolute; left: 700px; top: 120px; width: 760px; border-radius: 16px; overflow: hidden; box-shadow: 0 0 0 1px rgb(255 255 255 / .1), 0 40px 100px rgb(0 0 0 / .6); transform: perspective(1600px) rotateY(-14deg) rotateX(4deg); transform-origin: 0 50%; }
.shot img { width: 100%; display: block; }
</style></head><body>
<div class="grid"></div><div class="glow"></div>
<div class="shot"><img src="data:image/png;base64,${shot}"></div>
<div class="copy">
  <div class="brand">${icon.replace(/ width="1024" height="1024"/, "")}portwise</div>
  <h1>Who's on port <span>3000</span>?</h1>
  <p>See which ports are in use, why they're busy, and stop the right thing safely.</p>
</div>
<div class="tags"><b>CLI</b><b>TUI</b><b>Desktop</b><b>MCP</b></div>
</body></html>`;

const browser = await chromium.launch({ executablePath: process.env.CHROME || undefined });
const page = await browser.newPage({ viewport: { width: 1200, height: 630 } });
await page.setContent(html, { waitUntil: "load" });
await page.evaluate(() => document.fonts.ready);
await page.screenshot({ path: join(site, "public/og.png") });
await browser.close();
console.log("wrote public/og.png");

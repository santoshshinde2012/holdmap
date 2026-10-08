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
body { width: 1200px; height: 630px; overflow: hidden; background: #0a0a0b; color: #ededef; font-family: I; position: relative; }
.copy { position: absolute; left: 72px; top: 72px; width: 960px; }
.brand { display: flex; align-items: center; gap: 12px; font-size: 26px; font-weight: 600; letter-spacing: -.01em; }
.brand svg { width: 40px; height: 40px; }
h1 { margin-top: 48px; font-size: 64px; line-height: 1.05; letter-spacing: -.035em; font-weight: 560; }
h1 span { color: #8b8b93; }
.shot { position: absolute; left: 72px; top: 352px; width: 1056px; border-radius: 12px; overflow: hidden; box-shadow: 0 0 0 1px rgb(255 255 255 / .1), 0 24px 64px -16px rgb(0 0 0 / .7); }
.shot img { width: 100%; display: block; }
</style></head><body>
<div class="shot"><img src="data:image/png;base64,${shot}"></div>
<div class="copy">
  <div class="brand">${icon.replace(/ width="1024" height="1024"/, "")}portwise</div>
  <h1>Know what's on every port.<br><span>Free it safely.</span></h1>
</div>
</body></html>`;

const browser = await chromium.launch({ executablePath: process.env.CHROME || undefined });
const page = await browser.newPage({ viewport: { width: 1200, height: 630 } });
await page.setContent(html, { waitUntil: "load" });
await page.evaluate(() => document.fonts.ready);
await page.screenshot({ path: join(site, "public/og.png") });
await browser.close();
console.log("wrote public/og.png");

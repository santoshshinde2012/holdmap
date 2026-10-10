// Capture the current built public website. Build it first with HOLDMAP_SITE_OFFLINE=1
// for stable release/star data. Uses the desktop's managed Chromium and a private preview.
import assert from "node:assert/strict";
import { mkdir, stat } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { chromium } from "../../apps/desktop/node_modules/playwright/index.mjs";
import { previewSite } from "./preview.mjs";

const output = new URL("../../docs/screenshots/", import.meta.url);
await mkdir(output, { recursive: true });
const preview = await previewSite();
let browser;
const captures = [
  { name: "site-home-dark.png", route: "", theme: "dark", width: 1440 },
  { name: "site-home-light.png", route: "", theme: "light", width: 1440 },
  { name: "site-guide-dark.png", route: "docs/agents/", theme: "dark", width: 1440 },
  { name: "site-guide-light.png", route: "docs/agents/", theme: "light", width: 1440 },
  { name: "site-guide-mobile-light.png", route: "docs/agents/", theme: "light", width: 390 },
];
try {
  browser = await chromium.launch({ executablePath: process.env.CHROME || undefined });
  for (const capture of captures) {
    const context = await browser.newContext({
      viewport: { width: capture.width, height: 900 }, deviceScaleFactor: 1,
      reducedMotion: "reduce", locale: "en-US", timezoneId: "UTC",
    });
    try {
      await context.addInitScript((theme) => {
        localStorage.setItem("holdmap-site-theme", theme);
        // Keep the install label identical across macOS/Linux capture machines.
        Object.defineProperty(navigator, "platform", { get: () => "MacIntel" });
        Object.defineProperty(navigator, "userAgentData", { value: undefined });
      }, capture.theme);
      const page = await context.newPage();
      const errors = [];
      page.on("pageerror", (error) => errors.push(String(error)));
      await page.goto(new URL(capture.route, preview.url).href, { waitUntil: "load" });
      await page.getByRole("heading", { level: 1 }).waitFor();
      if (capture.route) {
        await page.getByRole("navigation", { name: "Breadcrumb", exact: true }).waitFor();
      } else {
        // The recording uses sample app data. Pause its first frame for stable captures,
        // leaving the poster/video controls in their real public-page state.
        await page.getByRole("button", { name: "Play the tour", exact: true }).click();
        await page.locator("[data-hero-video][data-playing]").waitFor();
        await page.getByRole("button", { name: "Pause the video", exact: true }).click();
        await page.locator("[data-hero-video] video").evaluate((video) => { video.currentTime = 0; });
        await page.waitForFunction(() => {
          const video = document.querySelector("[data-hero-video] video");
          return video.paused && !video.seeking && video.readyState >= 2;
        });
      }
      await page.evaluate(async () => {
        await document.fonts.ready;
        await Promise.all([...document.images].filter((image) => image.getBoundingClientRect().height && image.getBoundingClientRect().top < innerHeight).map((image) => image.decode()));
      });
      assert.equal(await page.locator("html").getAttribute("data-theme"), capture.theme);
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false, `${capture.name}: horizontal overflow`);
      assert.deepEqual(errors, [], `${capture.name}: page errors`);
      const file = fileURLToPath(new URL(capture.name, output));
      await page.screenshot({ path: file, animations: "disabled" });
      const bytes = (await stat(file)).size;
      assert.ok(bytes < 600 * 1024, `${capture.name}: exceeds the 600 KB snapshot budget`);
      console.log(`${capture.name}: ${capture.width}×900, ${(bytes / 1024).toFixed(1)} KB`);
    } finally {
      await context.close();
    }
  }
} finally {
  try { await browser?.close(); } finally { await preview.close(); }
}

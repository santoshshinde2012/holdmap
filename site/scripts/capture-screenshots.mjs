// Capture the built public website. Use HOLDMAP_SITE_REQUIRE_RELEASE=1 for actual
// published-release availability, or HOLDMAP_SITE_OFFLINE=1 for a source preview.
// Uses the desktop's managed Chromium and a private preview; never builds the site.
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
  { name: "site-release-dark.png", route: "", theme: "dark", width: 1440, height: 1100, focusSelector: "#whats-new" },
  { name: "site-release-light.png", route: "", theme: "light", width: 1440, height: 1100, focusSelector: "#whats-new" },
  { name: "site-release-mobile-light.png", route: "", theme: "light", width: 390, height: 1800, focusSelector: "#whats-new" },
  { name: "site-guide-dark.png", route: "docs/agents/", theme: "dark", width: 1440 },
  { name: "site-guide-light.png", route: "docs/agents/", theme: "light", width: 1440 },
  { name: "site-guide-mobile-light.png", route: "docs/agents/", theme: "light", width: 390 },
  { name: "site-guide-images-light.png", route: "docs/agents/", theme: "light", width: 1440, focusImage: 2 },
  { name: "site-guide-images-mobile-light.png", route: "docs/agents/", theme: "light", width: 390, focusImage: 2 },
];
try {
  browser = await chromium.launch({ executablePath: process.env.CHROME || undefined });
  for (const capture of captures) {
    const context = await browser.newContext({
      viewport: { width: capture.width, height: capture.height ?? 900 }, deviceScaleFactor: 1,
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
      } else if (!capture.focusSelector) {
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
      const focusTarget = capture.focusSelector ? page.locator(capture.focusSelector)
        : capture.focusImage !== undefined ? page.locator(".prose img").nth(capture.focusImage) : null;
      if (focusTarget) {
        await focusTarget.scrollIntoViewIfNeeded();
        await focusTarget.evaluate((element) => {
          window.scrollTo({ top: window.scrollY + element.getBoundingClientRect().top - 110, behavior: "instant" });
        });
      }
      await page.evaluate(async () => {
        await document.fonts.ready;
        await Promise.all([...document.images].filter((image) => {
          const bounds = image.getBoundingClientRect();
          // Offscreen lazy images may never load; only await images in this capture.
          return bounds.height > 0 && bounds.bottom > 0 && bounds.top < innerHeight;
        }).map((image) => image.decode()));
      });
      assert.equal(await page.locator("html").getAttribute("data-theme"), capture.theme);
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false, `${capture.name}: horizontal overflow`);
      if (capture.focusSelector) assert.equal(await focusTarget.evaluate((element) => {
        const bounds = element.getBoundingClientRect();
        return bounds.top >= 0 && bounds.bottom <= innerHeight && bounds.left >= 0 && bounds.right <= innerWidth;
      }), true, `${capture.name}: focused section must fit completely in the capture`);
      assert.deepEqual(errors, [], `${capture.name}: page errors`);
      const file = fileURLToPath(new URL(capture.name, output));
      await page.screenshot({ path: file, animations: "disabled" });
      const bytes = (await stat(file)).size;
      assert.ok(bytes < 600 * 1024, `${capture.name}: exceeds the 600 KB snapshot budget`);
      console.log(`${capture.name}: ${capture.width}×${capture.height ?? 900}, ${(bytes / 1024).toFixed(1)} KB`);
    } finally {
      await context.close();
    }
  }
} finally {
  try { await browser?.close(); } finally { await preview.close(); }
}

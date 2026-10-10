// End-to-end check of the built website, or an explicitly supplied public/preview URL:
// the iframe loads, the "Try" buttons drive the real app, and a simulated stop removes :3000.
//   [CHROME=/path/to/chrome] node scripts/check-demo.mjs [base-url]
// Reuses the locked desktop browser-test dependency; install both projects before running.
// Diagnostics: HOLDMAP_DEMO_PLATFORM=Linux HOLDMAP_DEMO_CPU_RATE=6 emulates a Linux
// navigator and slower CPU locally. HOLDMAP_DEMO_MEASURE_VIEWPORT=1 logs outer clipping;
// HOLDMAP_DEMO_TRACE=0 measures without tracing. Failures save PNG/JSON/trace.zip by default.
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const playwright = process.env.PLAYWRIGHT ?? new URL("../../apps/desktop/node_modules/playwright/index.mjs", import.meta.url).href;
const { chromium } = await import(playwright);
const { previewSite } = await import("./preview.mjs");
const preview = process.argv[2] ? null : await previewSite();
const base = process.argv[2] ?? preview.url;
const artifacts = fileURLToPath(new URL("../test-results/demo/", import.meta.url));
const traceEnabled = process.env.HOLDMAP_DEMO_TRACE !== "0";

let browser;
const results = [];
try {
  browser = await chromium.launch({ executablePath: process.env.CHROME || undefined });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  if (process.env.HOLDMAP_DEMO_PLATFORM) await page.addInitScript((platform) => {
    Object.defineProperty(navigator, "platform", { value: platform });
    Object.defineProperty(navigator, "userAgentData", { value: undefined });
  }, process.env.HOLDMAP_DEMO_PLATFORM);
  if (process.env.HOLDMAP_DEMO_CPU_RATE) {
    const session = await page.context().newCDPSession(page);
    await session.send("Emulation.setCPUThrottlingRate", { rate: Number(process.env.HOLDMAP_DEMO_CPU_RATE) });
  }
  if (traceEnabled) await page.context().tracing.start({ screenshots: true, snapshots: true });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  const check = async (name, fn) => {
    try { await fn(); results.push(["PASS", name]); } catch (e) {
      results.push(["FAIL", `${name}: ${e.stack ?? e.message ?? String(e)}`]);
      const stem = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/-$/, "");
      await mkdir(artifacts, { recursive: true });
      try {
        await page.screenshot({ path: `${artifacts}${stem}.png` });
        await writeFile(`${artifacts}${stem}.json`, JSON.stringify(await page.evaluate(() => {
          const frame = document.querySelector("#demo iframe");
          const viewport = frame?.parentElement;
          const doc = frame?.contentDocument;
          const target = doc?.querySelector("[role=alertdialog] [data-primary]");
          const bounds = (element) => element ? { ...element.getBoundingClientRect().toJSON() } : null;
          const rect = target?.getBoundingClientRect();
          const frameRect = frame?.getBoundingClientRect();
          const parentTarget = rect && frameRect ? {
            x: frameRect.x + (rect.x + rect.width / 2) * frameRect.width / frame.clientWidth,
            y: frameRect.y + (rect.y + rect.height / 2) * frameRect.height / frame.clientHeight,
          } : null;
          return {
            platform: navigator.platform, url: location.href, scrollY, browserViewport: { width: innerWidth, height: innerHeight },
            viewport: bounds(viewport), viewportScroll: viewport ? { top: viewport.scrollTop, left: viewport.scrollLeft, height: viewport.clientHeight, scrollHeight: viewport.scrollHeight, width: viewport.clientWidth, scrollWidth: viewport.scrollWidth } : null,
            frame: bounds(frame), dialog: bounds(doc?.querySelector("[role=alertdialog]")), target: bounds(target),
            targetText: target?.textContent, disabled: target?.disabled,
            hit: rect ? doc.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2)?.outerHTML : null,
            parentTarget, outsideBrowserViewport: parentTarget ? parentTarget.x < 0 || parentTarget.x > innerWidth || parentTarget.y < 0 || parentTarget.y > innerHeight : null,
            parentHit: parentTarget ? document.elementFromPoint(parentTarget.x, parentTarget.y)?.outerHTML.slice(0, 2000) ?? null : null,
            activeElement: doc?.activeElement?.outerHTML,
          };
        }), null, 2));
      } catch (diagnosticError) {
        console.error(`Could not capture failure diagnostics: ${diagnosticError.message}`);
      }
    }
  };

  await page.goto(base, { waitUntil: "domcontentloaded" });
  await page.locator("#demo").scrollIntoViewIfNeeded();
  const frame = page.frameLocator("#demo iframe");
  const try_ = (label) => page.locator(".try", { hasText: label }).click();
  const measureViewport = async (label) => {
    if (!process.env.HOLDMAP_DEMO_MEASURE_VIEWPORT) return;
    const state = await page.locator("#demo [data-viewport]").evaluate((viewport) => {
      const iframe = viewport.querySelector("iframe"), target = iframe?.contentDocument?.querySelector("[role=alertdialog] [data-primary]");
      const clip = viewport.getBoundingClientRect(), frame = iframe?.getBoundingClientRect(), button = target?.getBoundingClientRect();
      const center = frame && button ? { x: frame.x + (button.x + button.width / 2) * frame.width / iframe.clientWidth, y: frame.y + (button.y + button.height / 2) * frame.height / iframe.clientHeight } : null;
      return { top: viewport.scrollTop, left: viewport.scrollLeft, browserViewport: { width: innerWidth, height: innerHeight }, clip: clip.toJSON(), frame: frame?.toJSON(), center,
        clippedByContainer: center ? center.x < clip.left || center.x > clip.right || center.y < clip.top || center.y > clip.bottom : null,
        outsideBrowserViewport: center ? center.x < 0 || center.x > innerWidth || center.y < 0 || center.y > innerHeight : null };
    });
    console.log(`VIEWPORT ${label}: ${JSON.stringify(state)}`);
  };

  await check("iframe loads the app with sample ports", async () => {
    await frame.locator('[id^="row-tcp:3000:"]').waitFor({ timeout: 15000 });
    await page.locator(".try:not([disabled])").first().waitFor({ timeout: 5000 });
    await measureViewport("loaded");
  });
  await check("Inspect :3000 selects the port and shows its details", async () => {
    await try_("Inspect :3000");
    await frame.locator('[id^="row-tcp:3000:"][aria-selected="true"]').waitFor({ timeout: 5000 });
    await frame.getByText("shop-web").first().waitFor();
  });
  await check("Command palette opens", async () => {
    await try_("Command palette");
    await frame.locator("#palette-list").waitFor({ timeout: 5000 });
    await page.keyboard.press("Escape");
    await frame.locator("#palette-list").waitFor({ state: "detached", timeout: 5000 });
  });
  await check("Real keystrokes reach the app (Ctrl/⌘+K inside the frame)", async () => {
    await frame.locator("#port-list").click({ position: { x: 5, y: 5 } });
    await page.keyboard.press("Control+k");
    await frame.locator("#palette-list").waitFor({ timeout: 5000 });
    await page.keyboard.press("Escape");
  });
  await check("Service graph renders", async () => {
    await try_("Service graph");
    await frame.locator(".svelte-flow").waitFor({ timeout: 8000 });
    await frame.getByText("shop-api").first().waitFor();
  });
  await check("Agents shows searchable tool metadata without a listening port", async () => {
    await try_("Agents");
    await frame.getByRole("article", { name: "Cursor", exact: true }).waitFor({ timeout: 8000 });
    const search = frame.getByLabel("Search agents", { exact: true });
    await search.fill("filesystem");
    await frame.getByRole("article", { name: "Cursor", exact: true }).waitFor();
    await frame.getByRole("button", { name: /^MCP server Filesystem MCP,.*Opens its agent card$/ }).click();
    await frame.getByText("MCP server · inferred · no listening port", { exact: true }).waitFor();
    await search.press("Escape");
    await measureViewport("after-agents");
  });
  await check("Stop :3000 asks first, then frees the port (simulated)", async () => {
    await try_("Stop :3000");
    const dialog = frame.locator('[role="alertdialog"]');
    await dialog.waitFor({ timeout: 5000 });
    await measureViewport("stop-dialog");
    // The iframe is scaled: scrolling a child already inside its own viewport can leave the
    // modal below the parent browser window. Bring the actual demo canvas into view first.
    await page.locator("#demo [data-viewport]").scrollIntoViewIfNeeded();
    await measureViewport("stop-dialog-in-view");
    await dialog.locator("[data-primary]").click();
    await frame.locator('[id^="row-tcp:3000:"]').waitFor({ state: "detached", timeout: 10000 });
    await measureViewport("after-stop");
  });
  await check("Reset brings :3000 back", async () => {
    await try_("Reset");
    await frame.locator('[id^="row-tcp:3000:"]').waitFor({ timeout: 15000 });
  });
  await check("No console errors", async () => { if (errors.length) throw new Error(errors.join(" | ")); });

  if (traceEnabled) {
    if (results.some(([status]) => status === "FAIL")) {
      await mkdir(artifacts, { recursive: true });
      await page.context().tracing.stop({ path: `${artifacts}trace.zip` });
    } else await page.context().tracing.stop();
  }

} finally {
  try { await browser?.close(); } finally { await preview?.close(); }
}
for (const [s, n] of results) console.log(`${s}  ${n}`);
process.exit(results.some(([s]) => s === "FAIL") ? 1 : 0);

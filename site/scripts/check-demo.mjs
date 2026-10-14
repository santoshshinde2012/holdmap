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
  const check = async (name, fn, diagnosticPage = page) => {
    try { await fn(); results.push(["PASS", name]); } catch (e) {
      results.push(["FAIL", `${name}: ${e.stack ?? e.message ?? String(e)}`]);
      const stem = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/-$/, "");
      await mkdir(artifacts, { recursive: true });
      try {
        await diagnosticPage.screenshot({ path: `${artifacts}${stem}.png` });
        await writeFile(`${artifacts}${stem}.json`, JSON.stringify(await diagnosticPage.evaluate(() => {
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
    await frame.getByRole("radio", { name: "Footprint", exact: true }).click();
    await frame.getByRole("article", { name: "Cursor", exact: true }).waitFor({ timeout: 8000 });
    const search = frame.getByLabel("Search agents", { exact: true });
    await search.fill("filesystem");
    await frame.getByRole("article", { name: "Cursor", exact: true }).waitFor();
    await frame.getByRole("button", { name: /^MCP server Filesystem MCP,.*Opens its agent card$/ }).click();
    await frame.getByText("MCP server · inferred · no listening port", { exact: true }).waitFor();
    await search.press("Escape");
    await measureViewport("after-agents");
  });
  await check("Agent folders offer keyboard-accessible choices with honest browser previews", async () => {
    await frame.getByRole("button", { name: /^Folder shop-web,.*Zooms into this project/ }).click();
    const folder = frame.getByRole("button", { name: /^Folder shop-web,.*\. Opens folder actions$/ });
    const mapNode = folder.locator("xpath=ancestor::div[contains(@class, 'svelte-flow__node')][1]");
    const dialog = frame.getByRole("dialog", { name: "Folder actions", exact: true });
    await mapNode.focus();
    await mapNode.press("Enter");
    await dialog.getByText("/Users/dev/code/shop-web", { exact: true }).waitFor();
    await dialog.getByText(/Browser demo: opening folders and editors is simulated/).waitFor();
    if (!(await dialog.getByRole("button", { name: "Close", exact: true }).evaluate((button) => button === document.activeElement))) throw new Error("Folder choices must start focused on Close");
    if (await dialog.evaluate(() => "__TAURI_INTERNALS__" in window)) throw new Error("Public folder choices must not have a native OS bridge");
    await dialog.press("Escape");
    await dialog.waitFor({ state: "detached" });
    if (!(await mapNode.evaluate((node) => node === document.activeElement))) throw new Error("Closing folder choices must restore map keyboard focus");
    await folder.click();
    await dialog.getByRole("button", { name: "Open in editor", exact: true }).click();
    await frame.getByText("Editor preview", { exact: true }).waitFor();
    await dialog.waitFor({ state: "detached" });
    await folder.click();
    await dialog.getByRole("button", { name: "Open in file manager", exact: true }).click();
    await frame.getByText("File manager preview", { exact: true }).waitFor();
    await dialog.waitFor({ state: "detached" });
    await try_("Inspect :3000");
    await frame.locator('[id^="row-tcp:3000:"][aria-selected="true"]').waitFor();
    await frame.locator('[id^="row-tcp:5173:"]').waitFor();
    await try_("Agents");
    await frame.getByRole("article", { name: "Cursor", exact: true }).waitFor();
    await measureViewport("after-folder-choices");
  });
  await check("Agent Flow separates example connections from recorded calls without OS actions", async () => {
    await frame.getByLabel("Search agents", { exact: true }).fill("");
    await frame.getByRole("radio", { name: "Flow", exact: true }).click();
    const flow = frame.getByRole("region", { name: "Agent flow", exact: true });
    await flow.getByText("Example data", { exact: true }).waitFor();
    await flow.getByRole("region", { name: "Flow participants", exact: true }).waitFor();
    await flow.locator(".limits summary").click();
    await flow.getByText(/Sockets do not reveal messages or data direction/).waitFor();
    await frame.getByRole("radio", { name: "Calls", exact: true }).click();
    const run = flow.locator("#agent-flow-run");
    await run.waitFor();
    const trace = await run.locator("option").filter({ hasText: "Planning agent" }).getAttribute("value");
    if (!trace) throw new Error("The example recorded trace is unavailable");
    await run.selectOption(trace);
    await flow.getByRole("button", { name: "Fit all relationships in view", exact: true }).click();
    await flow.locator(".flow-node").nth(9).waitFor();
    if (await flow.locator(".flow-node").count() !== 10 || await flow.locator(".svelte-flow__edge").count() !== 9) throw new Error("The example trace must contain 10 spans and 9 supplied parent links");
    await flow.getByText("Event order is not proof of communication.", { exact: true }).waitFor();
    await flow.getByText("1 failed span", { exact: true }).waitFor();
    if (!(await flow.getByRole("button", { name: "Start recording", exact: true }).isDisabled())) throw new Error("Browser example must not start a local recorder");
    const nativeBridge = await flow.evaluate(() => "__TAURI_INTERNALS__" in window);
    if (nativeBridge) throw new Error("Public example must not have a native OS bridge");
    if (await flow.locator(".svelte-flow__edge.animated").count()) throw new Error("Parent links must not simulate message traffic");
    await measureViewport("after-agent-flow");
  });
  await check("Calls diagnosis filters failures with reported parents and resets the view", async () => {
    const flow = frame.getByRole("region", { name: "Agent flow", exact: true });
    const timeline = flow.getByRole("region", { name: "Recorded activity timeline", exact: true });
    await flow.getByRole("radio", { name: "Failed", exact: true }).click();
    await flow.getByText("1 of 10 records shown", { exact: true }).waitFor();
    if (await timeline.getByRole("button").count() !== 1 || await flow.locator(".flow-node").count() !== 5 || await flow.locator(".svelte-flow__edge").count() !== 4) throw new Error("Failure diagnosis must retain only the failed record and its reported parents");
    await timeline.getByRole("button").click();
    const inspector = flow.getByRole("region", { name: "Selected relationship details", exact: true });
    await inspector.getByText("Failed", { exact: true }).waitFor();
    const search = frame.getByLabel("Search agents", { exact: true });
    await search.fill("no-such-reported-tool");
    await flow.getByText("0 of 10 records shown", { exact: true }).waitFor();
    if (await timeline.getByRole("button").count() !== 0) throw new Error("Metadata search must narrow individual records");
    await flow.getByLabel("Order", { exact: true }).selectOption("duration");
    await flow.getByRole("button", { name: "Reset activity view", exact: true }).click();
    await flow.getByText("10 of 10 records shown", { exact: true }).waitFor();
    if (await search.inputValue() !== "" || await flow.getByLabel("Order", { exact: true }).inputValue() !== "time") throw new Error("Reset must clear search, filter and ordering");
    await inspector.getByText("Failed", { exact: true }).waitFor();
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

  const guideBase = new URL("./", page.url());
  const checkGuideImages = async (viewport) => {
    await page.setViewportSize(viewport);
    for (const [route, count] of [["agents", 4], ["desktop", 5]]) {
      await page.goto(new URL(`docs/${route}/`, guideBase).href, { waitUntil: "domcontentloaded" });
      const images = page.locator(".prose img");
      if (await images.count() !== count) throw new Error(`${route}: expected ${count} guide screenshots`);
      for (let i = 0; i < count; i++) {
        const image = images.nth(i);
        const alt = await image.getAttribute("alt");
        if (!alt?.trim()) throw new Error(`${route}: screenshot ${i + 1} needs descriptive alt text`);
        if (await image.getAttribute("__astro_image_") !== null) throw new Error(`${route}: unresolved image placeholder`);
        await image.scrollIntoViewIfNeeded();
        await page.waitForFunction((index) => {
          const image = document.querySelectorAll(".prose img")[index];
          return image.complete && image.naturalWidth > 0 && image.naturalHeight > 0;
        }, i, { timeout: 10000 });
        const fits = await image.evaluate((element) => {
          const bounds = element.getBoundingClientRect();
          return bounds.width > 0 && bounds.width <= element.closest(".prose").getBoundingClientRect().width + 1;
        });
        if (!fits) throw new Error(`${route}: screenshot ${i + 1} exceeds its content column`);
      }
      const overflows = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth + 1);
      if (overflows) throw new Error(`${route}: guide overflows the ${viewport.width}px viewport`);
    }
  };
  await check("Public guide screenshots load with descriptive alt text", () => checkGuideImages({ width: 1440, height: 900 }));
  await check("Public guide screenshots fit the mobile viewport", () => checkGuideImages({ width: 390, height: 900 }));
  await check("Public hero headline remains fully readable at narrow mobile widths", async () => {
    for (const width of [320, 390]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(guideBase.href, { waitUntil: "domcontentloaded" });
      const fits = await page.locator("#hero-title .line").evaluateAll((lines) => lines.length === 2 && lines.every((line) => {
        const bounds = line.getBoundingClientRect();
        return line.scrollWidth <= line.clientWidth + 1 && bounds.left >= -1 && bounds.right <= innerWidth + 1;
      }));
      if (!fits) throw new Error(`Hero headline is clipped at ${width}px`);
    }
  });

  // Both Mac browser profiles advertise Intel. That string cannot select CPU architecture.
  const downloadProfiles = [
    { name: "Apple silicon-style Safari offers both native macOS downloads at narrow width", os: "macos", platform: "MacIntel", userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Safari/605.1.15", viewport: { width: 375, height: 900 } },
    { name: "Intel-style Chrome offers both native macOS downloads", os: "macos", platform: "MacIntel", userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36" },
    { name: "Windows keeps its direct installer download", os: "windows", platform: "Win32", userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36", suffix: "_x64-setup.exe" },
    { name: "Linux keeps its direct AppImage download", os: "linux", platform: "Linux x86_64", userAgent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36", suffix: "_amd64.AppImage" },
  ];
  for (const profile of downloadProfiles) {
    const context = await browser.newContext({ userAgent: profile.userAgent, viewport: profile.viewport ?? { width: 1440, height: 900 } });
    const downloadPage = await context.newPage();
    const profileErrors = [];
    downloadPage.on("pageerror", (error) => profileErrors.push(String(error)));
    downloadPage.on("console", (message) => message.type() === "error" && profileErrors.push(message.text()));
    await downloadPage.addInitScript((platform) => {
      Object.defineProperty(navigator, "platform", { value: platform });
      Object.defineProperty(navigator, "userAgentData", { value: undefined });
    }, profile.platform);
    try {
      await check(profile.name, async () => {
        await downloadPage.goto(guideBase.href, { waitUntil: "domcontentloaded" });
        await downloadPage.waitForFunction((os) => document.documentElement.dataset.os === os, profile.os);
        const download = downloadPage.locator("[data-download]");
        const panel = downloadPage.locator(`#in-${profile.os}`);
        await panel.waitFor({ state: "visible", timeout: 5000 });
        if (profile.os === "macos") {
          await downloadPage.getByRole("link", { name: "Choose a macOS download", exact: true }).waitFor();
          if (await download.getAttribute("href") !== "#install") throw new Error("macOS must offer an architecture choice instead of guessing its CPU");
          await downloadPage.getByRole("tab", { name: "Linux", exact: true }).click();
          await panel.waitFor({ state: "hidden" });
          await download.click();
          if (new URL(downloadPage.url()).hash !== "#install") throw new Error("macOS download must reach the install choices");
          await panel.waitFor({ state: "visible", timeout: 5000 });
          const silicon = panel.getByRole("link", { name: /^Apple silicon/ });
          const intel = panel.getByRole("link", { name: /^Intel/ });
          const [siliconUrl, intelUrl] = await Promise.all([silicon.getAttribute("href"), intel.getAttribute("href")]);
          if (!siliconUrl?.endsWith("_aarch64.dmg") || !intelUrl?.endsWith("_x64.dmg") || siliconUrl === intelUrl) throw new Error("macOS choices must link to distinct ARM64 and Intel installers");
          for (const choice of [silicon, intel]) {
            await choice.scrollIntoViewIfNeeded();
            const fits = await choice.evaluate((link) => {
              const bounds = link.getBoundingClientRect();
              return bounds.width > 0 && bounds.left >= -1 && bounds.right <= innerWidth + 1;
            });
            if (!fits) throw new Error("Both macOS architecture choices must fit the browser viewport");
          }
          await panel.getByText(/Apple silicon.*runs natively without Rosetta/).waitFor();
          await panel.getByText(/Choose Intel.*for Intel Macs/).waitFor();
        } else {
          await downloadPage.waitForFunction((suffix) => document.querySelector("[data-download]")?.getAttribute("href")?.endsWith(suffix), profile.suffix, { timeout: 5000 });
          if (await download.getAttribute("href") !== await panel.locator(".dl-btn.primary").getAttribute("href")) throw new Error(`${profile.os} must retain its direct primary download`);
        }
        if (profileErrors.length) throw new Error(profileErrors.join(" | "));
      }, downloadPage);
    } finally {
      await context.close();
    }
  }
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

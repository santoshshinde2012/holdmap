// End-to-end check of the built website, or an explicitly supplied public/preview URL:
// the iframe loads, the "Try" buttons drive the real app, and a simulated stop removes :3000.
//   [CHROME=/path/to/chrome] node scripts/check-demo.mjs [base-url]
// Reuses the locked desktop browser-test dependency; install both projects before running.
// Diagnostics: HOLDMAP_DEMO_PLATFORM=Linux HOLDMAP_DEMO_CPU_RATE=6 emulates a Linux
// navigator and slower CPU locally. HOLDMAP_DEMO_MEASURE_VIEWPORT=1 logs outer clipping;
// HOLDMAP_DEMO_TRACE=0 measures without tracing. Failures save PNG/JSON/trace.zip by default.
import { mkdir, readFile, writeFile } from "node:fs/promises";
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

  await check("iframe loads the app on Home with sample agents and ports", async () => {
    await frame.locator("#home-overview").waitFor({ timeout: 15000 });
    await frame.getByRole("button", { name: "Port 3000 Next.js", exact: true }).waitFor({ timeout: 5000 });
    await page.locator(".try:not([disabled])").first().waitFor({ timeout: 5000 });
    await measureViewport("loaded");
  });
  await check("Home switches Cards and Map and folds the Map timeline", async () => {
    const layout = frame.getByRole("radiogroup", { name: "Home layout", exact: true });
    await layout.getByRole("radio", { name: "Map", exact: true }).click();
    await frame.getByRole("article", { name: "Cursor", exact: true }).waitFor({ state: "detached" });
    const timeline = frame.locator("#lanes-title");
    await timeline.click();
    if (await timeline.getAttribute("aria-expanded") !== "false") throw new Error("Timeline must fold");
    await frame.locator("#agent-lanes").waitFor({ state: "detached" });
    await timeline.click();
    if (await timeline.getAttribute("aria-expanded") !== "true") throw new Error("Timeline must reopen");
    await frame.locator("#agent-lanes").waitFor();
    await layout.getByRole("radio", { name: "Cards", exact: true }).click();
    await frame.getByRole("article", { name: "Cursor", exact: true }).waitFor();
  });
  await check("Home port menu offers Details and a simulated stop plan that asks first", async () => {
    const chip = frame.getByRole("button", { name: "Port 3000 Next.js", exact: true });
    await chip.click();
    await frame.getByRole("menuitem", { name: "Details" }).click();
    await frame.getByText("shop-web").first().waitFor({ timeout: 5000 });
    await page.keyboard.press("Escape");
    await frame.getByRole("radio", { name: "Home", exact: true }).click();
    await chip.click();
    await frame.getByRole("menuitem", { name: /^Stop/ }).click();
    const plan = frame.locator('[role="alertdialog"]');
    await plan.waitFor({ timeout: 5000 });
    await page.keyboard.press("Escape");
    await plan.waitFor({ state: "detached" });
    await frame.getByRole("button", { name: "Review stop plan: Cursor", exact: true }).click();
    await frame.getByRole("alertdialog", { name: /^Stop Cursor/ }).waitFor();
    await page.keyboard.press("Escape");
    await plan.waitFor({ state: "detached" });
    await frame.getByRole("button", { name: "Port 3000 Next.js", exact: true }).waitFor();
  });
  await check("Every header control opens its surface, closes with Escape and restores focus", async () => {
    for (const [button, dialog] of [["Commands", "Command palette"], ["Recently stopped", "Recently stopped"], [/^Scheduled actions/, "Scheduled actions"], ["Remote host…", "Remote host"], ["Settings", "Settings"]]) {
      const control = frame.getByRole("button", { name: button, exact: true });
      await control.click();
      const surface = frame.getByRole("dialog", { name: dialog, exact: true });
      await surface.waitFor({ timeout: 5000 });
      await page.keyboard.press("Escape");
      await surface.waitFor({ state: "detached", timeout: 5000 });
      if (!(await control.evaluate((element) => element === document.activeElement))) throw new Error(`${dialog} must return focus to its header button`);
    }
    const refresh = frame.getByRole("button", { name: "Refresh", exact: true });
    await refresh.click();
    await frame.getByRole("button", { name: "Refresh", exact: true, disabled: false }).waitFor({ timeout: 5000 });
    const html = frame.locator("html");
    const start = await html.getAttribute("data-theme");
    const themeButton = frame.getByRole("button", { name: /^Theme/ });
    await themeButton.click();
    if (await html.getAttribute("data-theme") === start) throw new Error("Theme must change");
    for (let i = 0; i < 3 && await html.getAttribute("data-theme") !== start; i++) await themeButton.click();
    if (await html.getAttribute("data-theme") !== start) throw new Error("Theme must cycle back");
    // Single-key shortcuts leave focused buttons alone, so press ? from the page itself.
    await frame.locator("body").evaluate(() => (document.activeElement)?.blur());
    await page.keyboard.press("?");
    const shortcuts = frame.getByRole("dialog", { name: "Keyboard shortcuts", exact: true });
    await shortcuts.waitFor({ timeout: 5000 });
    await page.keyboard.press("Escape");
    await shortcuts.waitFor({ state: "detached" });
  });
  await check("Scheduling a simulated port stop shows a countdown and cancels without changing ports", async () => {
    await frame.getByRole("radio", { name: "List", exact: true }).click();
    await frame.getByRole("option", { name: /^Port 3000 tcp,/ }).click();
    const details = frame.getByRole("complementary", { name: "Port details", exact: true });
    await details.getByRole("button", { name: "Schedule stop…", exact: true }).click({ timeout: 5000 });
    const review = frame.getByRole("alertdialog");
    await frame.getByRole("alertdialog", { name: "Schedule stop :3000?", exact: true }).waitFor();
    await review.getByRole("button", { name: /^Schedule simulation/ }).click();
    await review.getByText("Simulation scheduled", { exact: true }).waitFor();
    await review.getByRole("button", { name: "Done", exact: true }).click();
    const banner = frame.getByRole("region", { name: "Active scheduled actions", exact: true });
    await banner.getByText("Stop :3000/tcp").waitFor({ timeout: 5000 });
    await frame.getByRole("button", { name: /^Scheduled actions \(1 active\)$/ }).waitFor();
    await banner.getByRole("button", { name: "View / cancel", exact: true }).click();
    const manager = frame.getByRole("dialog", { name: "Scheduled actions", exact: true });
    await manager.getByRole("button", { name: "Cancel Stop :3000/tcp", exact: true }).click();
    await manager.getByText("Cancellation confirmed for Stop :3000/tcp.").waitFor();
    await page.keyboard.press("Escape");
    await manager.waitFor({ state: "detached" });
    await banner.waitFor({ state: "detached", timeout: 5000 });
    await frame.getByRole("option", { name: /^Port 3000 tcp,/ }).waitFor();
  });
  await check("List, Graph, Agents and MCP & skills each render their view", async () => {
    const views = frame.getByRole("radiogroup", { name: "View", exact: true });
    await views.getByRole("radio", { name: "List", exact: true }).click();
    await frame.locator("#port-list").waitFor();
    await views.getByRole("radio", { name: /^Graph/ }).click();
    await frame.locator("#service-graph").waitFor();
    await views.getByRole("radio", { name: /^Agents/ }).click();
    await frame.locator("#agents-overview").waitFor();
    await frame.getByRole("radio", { name: "MCP & skills", exact: true }).click();
    await frame.locator("#agent-capabilities").waitFor();
    await frame.getByRole("radio", { name: "Overview", exact: true }).click();
    await views.getByRole("radio", { name: "Home", exact: true }).click();
    await frame.getByRole("button", { name: /^Listening \d+$/ }).click();
    await frame.locator("#port-list").waitFor();
  });
  await check("Agents → Security → Pipeline loads the analyzer graphs on demand", async () => {
    await frame.getByRole("radiogroup", { name: "View", exact: true }).getByRole("radio", { name: /^Agents/ }).click();
    await frame.getByRole("radio", { name: /^Security\b/ }).click();
    const insights = frame.getByRole("region", { name: "Security workspace", exact: true });
    await insights.getByRole("radio", { name: /^Pipeline/ }).click();
    const overview = insights.getByRole("group", { name: "Analyzers and the agents they cover", exact: true });
    await overview.locator(".svelte-flow").waitFor({ timeout: 8000 });
    await overview.locator('[data-graph-node="analyzer:nvidia.skillspector"]').click();
    await overview.getByRole("button", { name: "How SkillSpector works", exact: true }).click();
    await insights.getByRole("group", { name: "How SkillSpector works", exact: true }).locator('[data-graph-node="findings"]').waitFor();
    await insights.getByRole("button", { name: "All analyzers", exact: true }).click();
    await insights.getByRole("radio", { name: /^Findings/ }).click();
    await frame.getByRole("radio", { name: "Overview", exact: true }).click();
    await try_("Home");
    await frame.locator("#home-overview").waitFor();
  });
  await check("Status bar recording asks before capturing and cancels cleanly", async () => {
    await frame.getByRole("button", { name: "Record screen", exact: true }).click();
    const recorder = frame.getByRole("dialog", { name: "Record screen", exact: true });
    await recorder.waitFor();
    await page.keyboard.press("Escape");
    await recorder.waitFor({ state: "detached" });
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
    // The selected agent's details are docked and the map is zoomed to that agent; close them to see the whole map again.
    await frame.getByRole("button", { name: "Close agent details", exact: true }).click();
    await frame.getByRole("button", { name: /^Folder shop-web,.*Zooms into this project/ }).click();
    const folder = frame.getByRole("button", { name: /^Folder shop-web,.*\. Opens folder actions$/ });
    const mapNode = folder; // the focusable svelte-flow wrapper is the named control
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
    // The embedded demo must reveal its own modal actions in the parent viewport.
    // Use a normal click without a test-only outer scroll or forced pointer event.
    await dialog.locator("[data-primary]").click();
    await frame.locator('[id^="row-tcp:3000:"]').waitFor({ state: "detached", timeout: 10000 });
    await measureViewport("after-stop");
  });
  await check("Reset reloads Home with :3000 back", async () => {
    await try_("Reset");
    await frame.locator("#home-overview").waitFor({ timeout: 15000 });
    await frame.getByRole("button", { name: "Port 3000 Next.js", exact: true }).waitFor({ timeout: 15000 });
  });
  await check("Every Try button drives the app", async () => {
    await page.locator(".try:not([disabled])").first().waitFor({ timeout: 15000 });
    await try_("Shortcuts");
    const shortcuts = frame.getByRole("dialog", { name: "Keyboard shortcuts", exact: true });
    await shortcuts.waitFor({ timeout: 5000 });
    await try_("Service graph");
    await shortcuts.waitFor({ state: "detached" });
    await frame.locator("#service-graph").waitFor({ timeout: 8000 });
    await try_("Home");
    await frame.locator("#home-overview").waitFor({ timeout: 5000 });
    const labels = await page.locator(".try").allInnerTexts();
    const covered = ["Home", "Inspect :3000", "Command palette", "Service graph", "Agents map", "Stop :3000", "Shortcuts", "Reset"];
    const missing = labels.map((label) => label.trim()).filter((label) => !covered.some((name) => label.startsWith(name)));
    if (missing.length) throw new Error(`Untested Try buttons: ${missing.join(", ")}`);
  });

  await check("Holdmap PNG snapshots export from the embedded demo without a display picker", async () => {
    const body = frame.locator("body");
    await body.evaluate(() => {
      window.__holdmapDemoDisplayRequests = 0;
      const media = navigator.mediaDevices;
      if (media) {
        window.__holdmapDemoDisplayDescriptor = Object.getOwnPropertyDescriptor(media, "getDisplayMedia");
        Object.defineProperty(media, "getDisplayMedia", { configurable: true, value: () => {
          window.__holdmapDemoDisplayRequests++;
          throw new Error("The public-demo snapshot check must never request a display.");
        } });
      }
    });
    try {
      await frame.getByRole("button", { name: "Take snapshot", exact: true }).click();
      const chooser = frame.getByRole("dialog", { name: "Take snapshot", exact: true });
      if (await chooser.getByRole("radio", { name: "Holdmap view", exact: true }).getAttribute("aria-checked") !== "true") throw new Error("App capture must be the default source");
      await chooser.getByRole("button", { name: "Capture PNG", exact: true }).click();
      const preview = frame.getByRole("dialog", { name: "Snapshot preview", exact: true });
      await preview.getByRole("img", { name: "PNG snapshot preview", exact: true }).waitFor({ timeout: 20_000 });
      const valid = await preview.getByRole("img").evaluate(async (image) => {
        await image.decode();
        return image.naturalWidth > 0 && image.naturalWidth <= 3840 && image.naturalHeight > 0 && image.naturalHeight <= 2160;
      });
      if (!valid) throw new Error("Embedded demo PNG did not decode within its size limits");
      await measureViewport("snapshot-preview");
      const [download] = await Promise.all([page.waitForEvent("download"), preview.getByRole("button", { name: "Download PNG", exact: true }).click()]);
      const bytes = await readFile(await download.path());
      if (!download.suggestedFilename().endsWith(".png") || bytes.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") throw new Error("Embedded demo did not download a PNG");
      if (await body.evaluate(() => window.__holdmapDemoDisplayRequests) !== 0) throw new Error("App snapshot requested a display picker");
      await preview.getByRole("button", { name: "Discard snapshot", exact: true }).click();
    } finally {
      await body.evaluate(() => {
        if (navigator.mediaDevices) {
          if (window.__holdmapDemoDisplayDescriptor) Object.defineProperty(navigator.mediaDevices, "getDisplayMedia", window.__holdmapDemoDisplayDescriptor);
          else delete navigator.mediaDevices.getDisplayMedia;
        }
        delete window.__holdmapDemoDisplayRequests;
        delete window.__holdmapDemoDisplayDescriptor;
      });
    }
  });

  await check("Demo Try buttons never overlap Full screen at any desktop width", async () => {
    for (const width of [1440, 1201, 1200, 1100, 1000, 800]) {
      await page.setViewportSize({ width, height: 900 });
      const overlap = await page.evaluate(() => {
        const full = document.querySelector("#demo .full")?.getBoundingClientRect();
        return [...document.querySelectorAll("#demo .try")].some((button) => {
          const box = button.getBoundingClientRect();
          return full && box.width && box.right > full.left + 1 && box.left < full.right - 1 && box.bottom > full.top + 1 && box.top < full.bottom - 1;
        });
      });
      if (overlap) throw new Error(`Try buttons overlap Full screen at ${width}px`);
    }
    await page.setViewportSize({ width: 1440, height: 900 });
  });

  const guideBase = new URL("./", page.url());
  const checkGuideImages = async (viewport) => {
    await page.setViewportSize(viewport);
    for (const [route, count] of [["agents", 16], ["desktop", 11], ["whats-new", 1]]) {
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

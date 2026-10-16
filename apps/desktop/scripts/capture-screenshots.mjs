// Refresh README/website screenshots from the production browser build and its sample data.
// Run from any directory: node apps/desktop/scripts/capture-screenshots.mjs
// Install Chromium first with `npx playwright install chromium` in apps/desktop.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdir, mkdtemp, rm, stat } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { chromium, expect } from "@playwright/test";

const desktop = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const output = resolve(desktop, "../../docs/screenshots");
const siteAssets = resolve(desktop, "../../site/src/assets");
const vite = join(desktop, "node_modules/vite/bin/vite.js");
const build = await mkdtemp(join(tmpdir(), "holdmap-screenshots-"));
const executablePath = process.env.HOLDMAP_E2E_CHROMIUM_EXECUTABLE;
const viewport = { width: 1440, height: 1000 };
let server;
let browser;

async function availablePort() {
  const probe = createServer();
  probe.listen(0, "127.0.0.1");
  await once(probe, "listening");
  const address = probe.address();
  assert(address && typeof address === "object");
  await new Promise((resolveClose, reject) => probe.close((error) => error ? reject(error) : resolveClose()));
  return address.port;
}

async function buildPreview() {
  const child = spawn(process.execPath, [vite, "build", "--outDir", build, "--emptyOutDir", "--logLevel", "warn"], {
    cwd: desktop,
    env: { ...process.env, TAURI_ENV_PLATFORM: "", TAURI_ENV_DEBUG: "" },
    stdio: "inherit",
  });
  const [code] = await once(child, "exit");
  assert.equal(code, 0, "Production screenshot build failed");
}

async function startPreview() {
  const port = await availablePort();
  const url = `http://127.0.0.1:${port}`;
  let diagnostics = "";
  server = spawn(process.execPath, [vite, "preview", "--outDir", build, "--host", "127.0.0.1", "--port", String(port), "--strictPort", "--logLevel", "warn"], {
    cwd: desktop,
    stdio: ["ignore", "ignore", "pipe"],
  });
  let launchError;
  server.on("error", (error) => { launchError = error; });
  server.stderr.on("data", (chunk) => { diagnostics = (diagnostics + chunk).slice(-4000); });
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (launchError) throw launchError;
    assert.equal(server.exitCode, null, `Screenshot preview exited: ${diagnostics}`);
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(1000) });
      if (response.ok) return url;
    } catch { /* Preview is still starting. */ }
    await delay(100);
  }
  throw new Error(`Screenshot preview did not become ready: ${diagnostics}`);
}

async function stopPreview() {
  if (!server?.pid || server.exitCode !== null || server.signalCode !== null) return;
  const finished = once(server, "exit");
  server.kill("SIGTERM");
  await Promise.race([finished, delay(2000)]);
  if (server.exitCode === null && server.signalCode === null) {
    server.kill("SIGKILL");
    await finished;
  }
}

const card = (page, name) => page.getByRole("article", { name, exact: true });
const heading = (page, name) => card(page, name).getByRole("button", { name: new RegExp(`^${name} `) });

async function selectPort(page, port = 3000) {
  await page.getByRole("option", { name: new RegExp(`^Port ${port} tcp,`) }).click();
  const details = page.getByRole("complementary", { name: "Port details", exact: true });
  await expect(details.getByRole("heading", { name: port === 3000 ? /^shop-web/ : /^shop-api/ })).toBeVisible();
  await expect(details.getByText("Recommended", { exact: true })).toBeVisible();
  await expect(details.getByText("4", { exact: true })).toBeVisible();
  return details;
}

async function openPowerSettings(page) {
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("tab", { name: "Power", exact: true }).click();
  const settings = page.getByRole("dialog", { name: "Settings", exact: true });
  await expect(settings.getByRole("button", { name: "Simulate shutdown now…", exact: true })).toBeVisible();
  await expect(settings.getByRole("button", { name: "Schedule simulation…", exact: true })).toBeVisible();
  return settings;
}

async function reviewShutdown(page, scheduled = false) {
  const settings = await openPowerSettings(page);
  await settings.getByRole("button", { name: scheduled ? "Schedule simulation…" : "Simulate shutdown now…", exact: true }).click();
  const dialog = page.getByRole("alertdialog", { name: scheduled ? "Schedule shutdown simulation?" : "Simulate computer shutdown?", exact: true });
  await expect(dialog).toContainText("Demo computer");
  if (scheduled) {
    await expect(dialog.getByRole("combobox", { name: "When", exact: true })).toContainText("In 1 hour");
    await dialog.getByRole("combobox", { name: "When", exact: true }).click();
    await expect(page.getByRole("option", { name: "Now", exact: true })).toHaveCount(0);
    await page.getByRole("option", { name: "In 1 hour", exact: true }).click();
  } else {
    await expect(dialog.getByRole("combobox", { name: "When", exact: true })).toHaveCount(0);
  }
  return dialog;
}

async function openAgents(page, picture = "Footprint") {
  await page.getByRole("radio", { name: /^Agents/ }).click();
  await expect(page.getByRole("article")).toHaveCount(3);
  await expect(heading(page, "Cursor")).toContainText("15 processes");
  const choice = page.getByRole("radio", { name: picture, exact: true });
  await choice.click();
  await expect(choice).toHaveAttribute("aria-checked", "true");
  if (picture === "Footprint") await expect(page.getByRole("button", { name: /^MCP server Filesystem MCP,/ })).toBeVisible();
  if (picture === "Computer") await expect(page.getByRole("group", { name: /^This computer,/ })).toBeVisible();
}

async function openFolderActions(page) {
  await openAgents(page);
  await page.getByRole("button", { name: /^Folder shop-web,.*Zooms into this project/ }).click();
  await expect(page.getByRole("radio", { name: "Project", exact: true })).toHaveAttribute("aria-checked", "true");
  await page.getByRole("button", { name: /^Folder shop-web,.*\. Opens folder actions$/ }).click();
  const dialog = page.getByRole("dialog", { name: "Folder actions", exact: true });
  await expect(dialog).toContainText("/Users/dev/code/shop-web");
  await expect(dialog.getByRole("button", { name: "Close", exact: true })).toBeFocused();
  await expect(dialog).toContainText("opening folders and editors is simulated");
}

async function openGraph(page) {
  await page.getByRole("radio", { name: /^Graph/ }).click();
  await expect(page.getByRole("button", { name: "shop-web on ports 3000", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Stop cluster shop", exact: true })).toBeVisible();
}

async function openSnapshot(page, preview = false) {
  await openGraph(page);
  await page.getByRole("button", { name: "Take snapshot", exact: true }).click();
  const chooser = page.getByRole("dialog", { name: "Take snapshot", exact: true });
  await expect(chooser.getByRole("radio", { name: "Holdmap view", exact: true })).toHaveAttribute("aria-checked", "true");
  await expect(chooser.getByRole("radio", { name: "Screen or window", exact: true })).toBeVisible();
  if (!preview) return chooser;
  await chooser.getByRole("button", { name: "Capture PNG", exact: true }).click();
  const result = page.getByRole("dialog", { name: "Snapshot preview", exact: true });
  await expect(result.getByRole("img", { name: "PNG snapshot preview", exact: true })).toBeVisible();
  await expect(result.getByRole("button", { name: "Download PNG", exact: true })).toBeVisible();
  return result;
}

async function openAgentFlow(page, calls = false) {
  await openAgents(page);
  await page.getByRole("radio", { name: "Flow", exact: true }).click();
  const flow = page.getByRole("region", { name: "Agent flow", exact: true });
  await expect(flow.getByText("Example data", { exact: true })).toBeVisible();
  if (calls) {
    await page.getByRole("radio", { name: "Calls", exact: true }).click();
    const run = flow.locator("#agent-flow-run");
    const trace = await run.locator("option").filter({ hasText: "Planning agent" }).getAttribute("value");
    assert(trace, "Recorded trace fixture is available");
    await run.selectOption(trace);
    await expect(flow.locator(".flow-node")).toHaveCount(10);
    await expect(flow.locator(".svelte-flow__edge")).toHaveCount(9);
    await flow.getByRole("region", { name: "Recorded activity timeline" }).getByRole("button").filter({ hasText: "Tool execution" }).click();
    await expect(flow.getByRole("region", { name: "Selected relationship details" })).toContainText("Failed");
    await flow.getByRole("button", { name: "Fit all relationships in view", exact: true }).click();
    await page.evaluate(() => {
      for (const element of document.querySelectorAll(".workspace, .companion, main")) element.scrollTop = 0;
      window.scrollTo(0, 0);
    });
  } else {
    await expect(flow.getByRole("region", { name: "Flow participants", exact: true })).toBeVisible();
  }
}

async function openCapabilities(page, mode = "map") {
  await openAgents(page);
  await page.getByRole("radio", { name: "MCP & skills", exact: true }).click();
  const workspace = page.getByRole("region", { name: "MCP and skills workspace", exact: true });
  await expect(workspace.getByText("Example inventory", { exact: true })).toBeVisible();
  const assertFits = async () => {
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false, "Capability workspace must fit the viewport");
    assert.equal(await workspace.evaluate((element) => element.scrollWidth > element.clientWidth), false, "Capability workspace must not overflow horizontally");
  };
  const views = workspace.getByRole("radiogroup", { name: "Capability view", exact: true });
  if (mode === "explore") {
    await views.getByRole("radio", { name: "Explore", exact: true }).click();
    await workspace.getByRole("radio", { name: /^Skills/ }).click();
    const catalog = workspace.getByRole("region", { name: "Capability catalog", exact: true });
    await expect(catalog.getByRole("button")).toHaveCount(3);
    await expect(catalog).toContainText("Check UI behavior, accessibility and responsive layouts");
    await expect(page.getByRole("complementary", { name: "Capability details", exact: true })).toHaveCount(0);
    await assertFits();
    return;
  }
  if (mode === "compare") {
    await views.getByRole("radio", { name: "Compare", exact: true }).click();
    const comparison = workspace.getByRole("region", { name: "Capability comparison", exact: true });
    await expect(comparison.getByRole("heading", { name: "Compare observed sources", exact: true })).toBeVisible();
    await expect(comparison.getByRole("button", { name: /^Inspect compared / })).toHaveCount(7);
    await expect(comparison.getByText("Not reported here", { exact: true }).first()).toBeVisible();
    await expect(page.getByRole("complementary", { name: "Capability details", exact: true })).toHaveCount(0);
    await assertFits();
    return;
  }
  await views.getByRole("radio", { name: "Map", exact: true }).click();
  const graph = workspace.getByRole("region", { name: "Agent capability graph", exact: true });
  await expect(graph.getByRole("button", { name: "Inspect capabilities for Cursor, PID 52000", exact: true })).toBeVisible();
  if (mode === "map") {
    await expect(page.getByRole("complementary", { name: "Capability details", exact: true })).toHaveCount(0);
    await expect(graph.locator(".svelte-flow__edge")).toHaveCount(7);
    await graph.getByRole("button", { name: "Fit all capabilities in view", exact: true }).click();
    await assertFits();
    return;
  }
  await page.getByRole("button", { name: "Inspect capabilities for Cursor, PID 52000", exact: true }).click();
  await expect(page.getByRole("complementary", { name: "Capability details", exact: true }).getByRole("heading", { name: "Cursor", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Close capability details", exact: true }).click();
  await page.getByRole("radio", { name: /^Skills/ }).click();
  await page.getByRole("button", { name: "Inspect skill frontend-checks, Project scope. Agents: Cursor (PID 52000). Project: /Users/dev/code/shop-web. Source: /Users/dev/code/shop-web/.agents/skills/frontend-checks/SKILL.md", exact: true }).press("Enter");
  const detail = page.getByRole("complementary", { name: "Capability details", exact: true });
  await expect(detail.getByRole("tab", { name: "Purpose", exact: true })).toHaveAttribute("aria-selected", "true");
  await expect(detail).toContainText("Check UI behavior, accessibility and responsive layouts");
  await assertFits();
}

// Clip actual DOM bounds; retain all four services when presenting the Compose cluster.
async function boundedClip(page, locator, { height, padding = 0 } = {}) {
  const box = await locator.boundingBox();
  assert(box, "Screenshot region must have layout bounds");
  const viewport = page.viewportSize();
  const x = Math.max(0, box.x - padding), y = Math.max(0, box.y - padding);
  return { clip: { x, y, width: Math.min(viewport.width - x, box.width + padding * 2), height: Math.min(viewport.height - y, (height ?? box.height) + padding * 2) } };
}

const captures = [
  { name: "desktop-snapshot-dark.png", theme: "dark", prepare: (page) => openSnapshot(page, true) },
  { name: "desktop-snapshot-light.png", theme: "light", prepare: (page) => openSnapshot(page) },
  ...["dark", "light"].map((theme) => ({ name: `desktop-capabilities-${theme}.png`, theme, prepare: (page) => openCapabilities(page) })),
  ...["dark", "light"].flatMap((theme) => [
    { name: `desktop-capability-explore-${theme}.png`, theme, prepare: (page) => openCapabilities(page, "explore") },
    { name: `desktop-capability-compare-${theme}.png`, theme, prepare: (page) => openCapabilities(page, "compare") },
  ]),
  { name: "desktop-skills-dark.png", theme: "dark", prepare: (page) => openCapabilities(page, "skill") },
  { name: "desktop-capabilities-narrow-light.png", theme: "light", viewport: { width: 560, height: 1000 }, prepare: (page) => openCapabilities(page, "skill") },
  ...["dark", "light"].map((theme) => ({ name: `desktop-about-${theme}.png`, theme, async prepare(page) {
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByRole("tab", { name: "About", exact: true }).click();
    const settings = page.getByRole("dialog", { name: "Settings", exact: true });
    await expect(settings).toContainText("Browser preview");
    await expect(settings.getByRole("button", { name: "Download desktop app", exact: true })).toBeVisible();
    await expect(settings.getByRole("button", { name: "Installation guide", exact: true })).toBeVisible();
    await expect(settings.getByText("Running app location", { exact: true })).toHaveCount(0);
    await expect(settings.getByText("Settings folder", { exact: true })).toHaveCount(0);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false, "About must fit the viewport");
    return settings;
  } })),
  ...["dark", "light"].flatMap((theme) => [
    { name: `desktop-detail-compact-${theme}.png`, theme, viewport: { width: 1100, height: 900 }, pane: "340", async prepare(page) {
      const details = await selectPort(page);
      const box = await details.boundingBox();
      assert(box && box.width >= 339 && box.width <= 341, "Compact details capture must use the 340 px pane");
      await expect(details.getByRole("button", { name: "Stop…", exact: true })).toBeVisible();
      await expect(details.getByRole("button", { name: "Schedule stop…", exact: true })).toBeVisible();
    } },
    { name: `desktop-settings-power-${theme}.png`, theme, prepare: openPowerSettings },
    { name: `desktop-sort-right-edge-${theme}.png`, theme, viewport: { width: 1100, height: 900 }, pane: "340", async prepare(page) {
      await selectPort(page);
      await page.getByRole("combobox", { name: "Sort: Grouped", exact: true }).click();
      const list = page.getByRole("listbox", { name: "Sort by", exact: true });
      await expect(list.getByRole("option", { name: /^Cluster/ })).toBeVisible();
      const box = await list.boundingBox();
      assert(box && box.x >= 8 && box.x + box.width <= page.viewportSize().width - 7, "Sort menu must remain within the right viewport edge");
    } },
  ]),
  ...["dark", "light"].map((theme) => ({ name: `desktop-folder-actions-${theme}.png`, theme, prepare: openFolderActions })),
  { name: "desktop-folder-actions-narrow-light.png", theme: "light", viewport: { width: 560, height: 820 }, prepare: openFolderActions },
  ...["dark", "light"].flatMap((theme) => [
    { name: `desktop-agent-connections-${theme}.png`, theme, prepare: (page) => openAgentFlow(page) },
    { name: `desktop-agent-flow-${theme}.png`, theme, prepare: (page) => openAgentFlow(page, true) },
    { name: `desktop-agent-flow-narrow-${theme}.png`, theme, viewport: { width: 820, height: 1200 }, prepare: (page) => openAgentFlow(page, true) },
    { name: `desktop-agent-triage-${theme}.png`, theme, async prepare(page) {
      await openAgentFlow(page, true);
      const flow = page.getByRole("region", { name: "Agent flow", exact: true });
      await flow.getByRole("radio", { name: "Failed", exact: true }).click();
      await expect(flow.getByText("1 of 10 records shown", { exact: true })).toBeVisible();
      await expect(flow.getByRole("region", { name: "Recorded activity timeline" }).getByRole("button")).toHaveCount(1);
      await expect(flow.locator(".flow-node")).toHaveCount(5);
      await flow.getByRole("button", { name: "Fit all relationships in view", exact: true }).click();
    } },
  ]),
  ...["dark", "light"].map((theme) => ({ name: `desktop-shutdown-schedule-${theme}.png`, theme, async prepare(page) {
    const dialog = await reviewShutdown(page, true);
    await dialog.getByRole("checkbox").check();
    return dialog;
  } })),
  ...["dark", "light"].map((theme) => ({ name: `desktop-schedules-${theme}.png`, theme, async prepare(page) {
    const dialog = await reviewShutdown(page, true);
    await dialog.getByRole("checkbox").check();
    await dialog.getByRole("button", { name: "Schedule simulation", exact: true }).click();
    await dialog.getByRole("button", { name: /View scheduled actions/ }).click();
    return page.getByRole("dialog", { name: "Scheduled actions", exact: true });
  } })),
  ...["dark", "light"].map((theme) => ({ name: `desktop-agent-schedule-${theme}.png`, theme, async prepare(page) {
    await openAgents(page);
    await heading(page, "Cursor").click();
    await card(page, "Cursor").getByRole("button", { name: "Stop all (2)", exact: true }).click();
    const dialog = page.getByRole("alertdialog");
    await dialog.getByRole("combobox", { name: "When", exact: true }).click();
    await page.getByRole("option", { name: "In 1 hour", exact: true }).click();
    return dialog;
  } })),
  ...["dark", "light"].map((theme) => ({ name: `desktop-shutdown-${theme}.png`, theme, async prepare(page) {
    const dialog = await reviewShutdown(page);
    await expect(dialog.getByRole("button", { name: "Simulate shutdown", exact: true })).toBeDisabled();
    return dialog;
  } })),
  { name: "desktop-overview-dark.png", theme: "dark", async prepare(page) { await selectPort(page); } },
  { name: "desktop-overview-light.png", theme: "light", async prepare(page) { await selectPort(page); } },
  { name: "desktop-graph-dark.png", theme: "dark", prepare: openGraph },
  { name: "desktop-agents-dark.png", theme: "dark", prepare: openAgents },
  { name: "desktop-agents-light.png", theme: "light", prepare: openAgents },
  { name: "desktop-agents-computer-dark.png", theme: "dark", prepare: (page) => openAgents(page, "Computer") },
  { name: "desktop-agents-project-dark.png", theme: "dark", async prepare(page) {
    await openAgents(page);
    await page.getByRole("button", { name: /^Folder shop-web,.*Zooms into this project/ }).click();
    await expect(page.getByRole("radio", { name: "Project", exact: true })).toHaveAttribute("aria-checked", "true");
    await expect(page.getByRole("button", { name: /^Folder shop-web,.*Opens folder actions/ })).toBeVisible();
    const canvas = page.locator(".svelte-flow__pane");
    const box = await canvas.boundingBox();
    assert(box, "Project canvas must be visible");
    await canvas.click({ position: { x: box.width - 20, y: box.height - 20 } });
    await expect(page.getByRole("button", { name: /^Port shop-web, :3000/ })).not.toHaveClass(/\bdim\b/);
  } },
  ...["dark", "light"].map((theme) => ({ name: `desktop-screen-record-${theme}.png`, theme, async prepare(page) {
    await page.getByRole("button", { name: "Record screen", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Record screen", exact: true });
    await expect(dialog.getByRole("button", { name: "Choose screen…", exact: true })).toBeVisible();
    await expect(dialog).toContainText("Microphone and system audio are not captured");
    await expect(dialog).toContainText("Maximum 2 minutes");
    return dialog;
  } })),
  { name: "desktop-agent-tools-dark.png", theme: "dark", async prepare(page) {
    await openAgents(page);
    await page.getByLabel("Search agents", { exact: true }).fill("filesystem");
    await expect(page.getByRole("article")).toHaveCount(1);
    await heading(page, "Cursor").click();
    const tools = card(page, "Cursor").getByRole("region", { name: "Agent tools" });
    const mcp = tools.getByRole("listitem").filter({ hasText: "Filesystem MCP" });
    await expect(mcp).toContainText("MCP server · inferred · no listening port");
    await mcp.getByText("Command", { exact: true }).click();
    await expect(mcp.locator("code")).toBeVisible();
    return tools;
  } },
  { name: "desktop-stop-confirm-light.png", theme: "light", async prepare(page) {
    const details = await selectPort(page);
    await details.getByRole("button", { name: /^Stop/ }).click();
    const plan = page.getByRole("alertdialog");
    await expect(plan.getByRole("list", { name: "Plan", exact: true })).toBeVisible();
    await expect(plan.getByRole("button", { name: "Stop :3000", exact: true })).toBeVisible();
    await expect(plan.getByRole("combobox", { name: "When", exact: true })).toHaveCount(0);
  } },
  ...["dark", "light"].map((theme) => ({ name: `desktop-agent-stop-confirm-${theme}.png`, theme, async prepare(page) {
    await openAgents(page);
    await heading(page, "Cursor").click();
    await card(page, "Cursor").getByRole("button", { name: "Stop all (2)", exact: true }).click();
    const plan = page.getByRole("alertdialog");
    await expect(plan.getByRole("list", { name: "Plan", exact: true }).getByRole("listitem")).toHaveCount(4);
    await expect(plan).toContainText("43000");
    await expect(plan).toContainText("45173");
  } })),
  { name: "desktop-command-palette-light.png", theme: "light", async prepare(page) {
    await selectPort(page);
    await page.getByRole("button", { name: /^Commands/ }).click();
    const palette = page.getByRole("dialog", { name: "Command palette", exact: true });
    await palette.getByRole("combobox").fill("sto");
    await expect(palette.getByRole("option").first()).toBeVisible();
  } },
  { name: "desktop-settings-dark.png", theme: "dark", async prepare(page) {
    await selectPort(page);
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    const settings = page.getByRole("dialog", { name: "Settings", exact: true });
    await expect(settings.getByRole("switch", { name: "Launch at login", exact: true })).toBeVisible();
    await expect(settings.getByRole("tabpanel", { name: "General", exact: true })).toBeVisible();
  } },
];

// Keep the landing page's real UI crops and static video posters current too. The recording
// workflow remains separate because updating a poster does not require re-recording the tour.
for (const theme of ["dark", "light"]) {
  const ui = { theme, output: join(siteAssets, "ui"), viewport: { width: 1440, height: 900 }, pane: "460" };
  captures.push(
    { ...ui, name: `app-graph-${theme}.png`, prepare: openGraph },
    { ...ui, name: `ui-detail-${theme}.png`, prepare: selectPort },
    { ...ui, name: `ui-rows-${theme}.png`, async prepare(page) {
      await selectPort(page);
      return boundedClip(page, page.getByRole("listbox", { name: "Ports in use" }), { height: 450 });
    } },
    { ...ui, name: `ui-stop-${theme}.png`, async prepare(page) {
      const details = await selectPort(page);
      await details.getByRole("button", { name: /^Stop/ }).click();
      const plan = page.getByRole("alertdialog");
      await expect(plan.getByRole("list", { name: "Plan", exact: true })).toBeVisible();
      return plan;
    } },
    { ...ui, name: `ui-palette-${theme}.png`, async prepare(page) {
      await selectPort(page);
      await page.getByRole("button", { name: /^Commands/ }).click();
      const palette = page.getByRole("dialog", { name: "Command palette", exact: true });
      await palette.getByRole("combobox").fill("stop");
      await expect(palette.getByRole("option").first()).toBeVisible();
      return palette;
    } },
    { ...ui, name: `ui-graph-${theme}.png`, async prepare(page) {
      await openGraph(page);
      await expect(page.getByRole("button", { name: "db on ports 5432", exact: true })).toBeVisible();
      // Locator capture waits for the measured viewport to settle before reading its crop.
      return page.locator(".hull.kind-compose");
    } },
  );
  for (const variant of ["desktop", "mobile"]) {
    const mobile = variant === "mobile";
    captures.push({
      name: `hero-poster-${variant}-${theme}.png`, theme, output: join(siteAssets, "hero"),
      viewport: mobile ? { width: 400, height: 560 } : { width: 1152, height: 720 },
      deviceScaleFactor: mobile ? 2.625 : 1.5, pane: "460",
      async prepare(page) {
        if (mobile) await page.addStyleTag({ content: ".app.mac .titlebar{padding-left:16px!important}" });
        else await expect(page.getByRole("heading", { name: "At a glance", exact: true })).toBeVisible();
      },
    });
  }
}

try {
  const requested = new Set(process.argv.slice(2));
  assert([...requested].every((name) => captures.some((capture) => capture.name === name)), "Unknown screenshot name");
  await mkdir(output, { recursive: true });
  await buildPreview();
  const baseURL = await startPreview();
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
  for (const capture of captures) {
    if (requested.size && !requested.has(capture.name)) continue;
    const context = await browser.newContext({ viewport: capture.viewport ?? viewport, deviceScaleFactor: capture.deviceScaleFactor ?? 2, colorScheme: capture.theme, reducedMotion: "reduce", locale: "en-US", timezoneId: "UTC" });
    try {
      await context.addInitScript(({ theme, pane }) => {
        Object.defineProperty(Navigator.prototype, "platform", { get: () => "MacIntel" });
        Math.random = () => 0.5;
        window.__holdmapScreenshotCaptureRequests = 0;
        if (navigator.mediaDevices) Object.defineProperty(navigator.mediaDevices, "getDisplayMedia", { configurable: true, value: () => {
          window.__holdmapScreenshotCaptureRequests++;
          throw new Error("Screenshot fixtures must never request a real display capture.");
        } });
        for (const [name, value] of Object.entries({ onboarded: "1", theme, view: "list", sort: "group", density: "comfortable", collapsed: "[]", pane, animate: "0", layout: "layered" })) localStorage.setItem(`holdmap.${name}`, value);
      }, { theme: capture.theme, pane: capture.pane ?? "520" });
      const page = await context.newPage();
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.clock.setFixedTime(new Date("2026-10-10T10:00:00Z"));
      await page.goto(baseURL);
      await expect(page.getByRole("listbox", { name: "Ports in use" }).getByRole("option")).toHaveCount(11);
      assert.equal(await page.evaluate(() => "__TAURI_INTERNALS__" in window), false, "Screenshots must use browser sample data");
      await page.evaluate(() => document.fonts.ready);
      const target = await capture.prepare(page);
      await page.mouse.move(0, 0);
      assert.deepEqual(errors, [], `Uncaught errors while preparing ${capture.name}`);
      assert.equal(await page.evaluate(() => window.__holdmapScreenshotCaptureRequests), 0, "Screenshots must never request a display capture");
      const directory = capture.output ?? output;
      await mkdir(directory, { recursive: true });
      const path = join(directory, capture.name);
      await (target && typeof target.screenshot === "function" ? target : page).screenshot({ path, animations: "disabled", caret: "hide", ...(target?.clip ? { clip: target.clip } : {}) });
      const { size } = await stat(path);
      assert(size < 1024 * 1024, `${capture.name} exceeds the 1 MB screenshot budget`);
      console.log(`${capture.name}: ${(size / 1024).toFixed(0)} KB`);
      const guideImage = {
        "desktop-agent-connections-dark.png": "connections-dark.webp",
        "desktop-agent-flow-dark.png": "calls-dark.webp",
      }[capture.name];
      if (guideImage) {
        const sharp = createRequire(resolve(desktop, "../../site/package.json"))("sharp");
        const directory = join(siteAssets, "agent-flow");
        await mkdir(directory, { recursive: true });
        await sharp(path).resize({ width: 1920, withoutEnlargement: true }).webp({ quality: 84 }).toFile(join(directory, guideImage));
      }
    } finally {
      await context.close();
    }
  }
} finally {
  try {
    await browser?.close();
  } finally {
    try {
      await stopPreview();
    } finally {
      await rm(build, { recursive: true, force: true });
    }
  }
}

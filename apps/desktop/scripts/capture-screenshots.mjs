// Refresh README/website screenshots from the production browser build and its sample data.
// Run from any directory: node apps/desktop/scripts/capture-screenshots.mjs
// Install Chromium first with `npx playwright install chromium` in apps/desktop.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdir, mkdtemp, rm, stat } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { chromium, expect } from "@playwright/test";

const desktop = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const output = resolve(desktop, "../../docs/screenshots");
const siteAssets = resolve(desktop, "../../site/src/assets");
const vite = join(desktop, "node_modules/vite/bin/vite.js");
const build = await mkdtemp(join(tmpdir(), "holdmap-screenshots-"));
const executablePath = process.env.HOLDMAP_E2E_CHROMIUM_EXECUTABLE ?? process.env.PORTWISE_E2E_CHROMIUM_EXECUTABLE;
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

async function openAgents(page) {
  await page.getByRole("radio", { name: /^Agents/ }).click();
  await expect(page.getByRole("article")).toHaveCount(3);
  await expect(heading(page, "Cursor")).toContainText("15 processes");
  await expect(page.getByRole("button", { name: /^MCP server Filesystem MCP,/ })).toBeVisible();
}

async function openGraph(page) {
  await page.getByRole("radio", { name: /^Graph/ }).click();
  await expect(page.getByRole("button", { name: "shop-web on ports 3000", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Stop cluster shop", exact: true })).toBeVisible();
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
  { name: "desktop-overview-dark.png", theme: "dark", async prepare(page) { await selectPort(page); } },
  { name: "desktop-overview-light.png", theme: "light", async prepare(page) { await selectPort(page); } },
  { name: "desktop-graph-dark.png", theme: "dark", async prepare(page) {
    await page.getByRole("radio", { name: /^Graph/ }).click();
    await page.getByRole("button", { name: "shop-web on ports 3000", exact: true }).click();
    await expect(page.getByRole("complementary", { name: "Port details" }).getByRole("tab", { name: /^Connections/ })).toBeVisible();
    await expect(page.getByRole("button", { name: "shop-api on ports 3001", exact: true })).toBeVisible();
  } },
  { name: "desktop-agents-dark.png", theme: "dark", prepare: openAgents },
  { name: "desktop-agents-light.png", theme: "light", prepare: openAgents },
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
      return boundedClip(page, page.locator(".hull.kind-compose"), { padding: 8 });
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
      const directory = capture.output ?? output;
      await mkdir(directory, { recursive: true });
      const path = join(directory, capture.name);
      await (target && typeof target.screenshot === "function" ? target : page).screenshot({ path, animations: "disabled", caret: "hide", ...(target?.clip ? { clip: target.clip } : {}) });
      const { size } = await stat(path);
      assert(size < 1024 * 1024, `${capture.name} exceeds the 1 MB screenshot budget`);
      console.log(`${capture.name}: ${(size / 1024).toFixed(0)} KB`);
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

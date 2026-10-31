import type { Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "./fixtures";

// Home → 3D: the machine renders where WebGL 2 is available (offering software rendering on
// request) and falls back to the map, saying why, where it isn't.

async function open3d(page: Page, inventory = true): Promise<"3d" | "fallback" | "list"> {
  await page.evaluate((inv) => { localStorage.setItem("holdmap.home.layout", "3d"); localStorage.setItem("holdmap.agents.inventory", inv ? "1" : "0"); }, inventory);
  await page.reload();
  await page.getByRole("radio", { name: /^Home/ }).click();
  // A phone-sized window shows the list of agents, as the Map does, and starts no WebGL.
  if ((page.viewportSize()?.width ?? 1440) <= 760) return "list";
  const overview = page.getByRole("region", { name: "Overview" });
  const parts = overview.getByRole("navigation", { name: "Machine parts" });
  const fallback = overview.getByText("3D isn't available in this window");
  await expect(parts.or(fallback)).toBeVisible({ timeout: 10_000 });
  // Software-only WebGL (common on CI) is offered, not forced.
  const anyway = overview.getByRole("button", { name: "Use 3D anyway" });
  if (await anyway.isVisible()) await anyway.click();
  await expect(parts.or(fallback)).toBeVisible();
  return (await parts.isVisible()) ? "3d" : "fallback";
}

test("V cycles Home through Cards, Map and 3D, and the choice is remembered", async ({ app }) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  const layout = app.getByRole("radiogroup", { name: "Home layout" });
  await expect(layout.getByRole("radio", { name: /^Cards/ })).toHaveAttribute("aria-checked", "true");
  for (const next of ["Map", "3D"]) {
    await app.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await app.keyboard.press("v");
    await expect(layout.getByRole("radio", { name: new RegExp(`^${next}`) })).toHaveAttribute("aria-checked", "true");
  }
  await app.reload();
  await app.getByRole("radio", { name: /^Home/ }).click();
  await expect(app.getByRole("radiogroup", { name: "Home layout" }).getByRole("radio", { name: /^3D/ })).toHaveAttribute("aria-checked", "true");
});

test("the 3D machine: a button for every part, live readings and honest details", async ({ app }) => {
  test.skip(await open3d(app) !== "3d", "WebGL 2 isn't available in this browser");
  const overview = app.getByRole("region", { name: "Overview" });
  await expect(overview.locator(".stage3d canvas")).toBeVisible();
  const readings = overview.getByRole("region", { name: "Live readings: This computer" });
  await expect(readings).toContainText("MCP running");
  await expect(readings).toContainText("of 7 configured");

  // Selecting an agent lists its parts by layer and scopes the readings to it.
  const parts = overview.getByRole("navigation", { name: "Machine parts" });
  await parts.getByRole("button", { name: /^Cursor / }).click();
  await expect(parts.getByRole("button", { name: /^Cursor / })).toHaveAttribute("aria-pressed", "true");
  await expect(overview.getByRole("region", { name: "Live readings: Cursor" })).toBeVisible();
  for (const h of ["Runtime", "This computer", "Configuration"]) await expect(parts.getByRole("heading", { name: h })).toBeVisible();
  await expect(overview.getByRole("region", { name: "Agent Cursor" })).toContainText("Observed");

  // A running MCP server says how it is known; a configured one says what it doesn't prove.
  await parts.getByRole("button", { name: /^MCP server filesystem Running/ }).click();
  await expect(overview.getByRole("region", { name: "MCP server filesystem" })).toContainText("Running now");
  await parts.getByRole("button", { name: /^MCP server legacy-browser/ }).click();
  await expect(overview.getByRole("region", { name: "MCP server legacy-browser" })).toContainText("MCP definition found");

  // Ports open the same menu as the cards and the map; Stop… always shows the plan first.
  await parts.getByRole("button", { name: /^Port :3000/ }).click();
  const menu = app.getByRole("menu");
  await expect(menu.getByRole("menuitem", { name: "Details" })).toBeVisible();
  await app.keyboard.press("Escape");

  // Keyboard: E pulls the machine apart, X toggles X-ray, Esc clears the part.
  await overview.locator(".stage3d").focus();
  await app.keyboard.press("e");
  await expect(overview.getByRole("radiogroup", { name: "Assembly" }).getByRole("radio", { name: /Exploded/ })).toHaveAttribute("aria-checked", "true");
  await app.keyboard.press("x");
  await expect(overview.getByRole("button", { name: /^X-ray/ })).toHaveAttribute("aria-pressed", "true");
  // The tour is off while reduced motion is on (this suite runs with reduced motion).
  await expect(overview.getByRole("button", { name: /^Tour/ })).toBeDisabled();
});

test("E, X, T and F follow the single-key shortcuts setting and leave focused buttons alone", async ({ app }) => {
  await app.evaluate(() => localStorage.setItem("holdmap.singleKeyShortcuts", "0"));
  test.skip(await open3d(app) !== "3d", "WebGL 2 isn't available in this browser");
  const overview = app.getByRole("region", { name: "Overview" });
  const xray = overview.getByRole("button", { name: /^X-ray/ });
  await overview.locator(".stage3d").focus();
  await app.keyboard.press("x");
  await expect(xray).toHaveAttribute("aria-pressed", "false");
  // With the setting on, a focused button keeps its own keys; the canvas takes the letter.
  await app.evaluate(() => localStorage.setItem("holdmap.singleKeyShortcuts", "1"));
  await open3d(app);
  await overview.getByRole("navigation", { name: "Machine parts" }).getByRole("button", { name: /^Cursor / }).focus();
  await app.keyboard.press("x");
  await expect(xray).toHaveAttribute("aria-pressed", "false");
  await overview.locator(".stage3d").focus();
  await app.keyboard.press("x");
  await expect(xray).toHaveAttribute("aria-pressed", "true");
});

test("Focus, gauges, zoom and recorded calls", async ({ app }) => {
  test.skip(await open3d(app) !== "3d", "WebGL 2 isn't available in this browser");
  const overview = app.getByRole("region", { name: "Overview" });
  const needs = overview.getByRole("heading", { name: /^Needs you/ });
  await expect(needs).toBeVisible();

  // Focus hands Home to the machine and gives it back.
  const focus = overview.getByRole("button", { name: /^Focus/ });
  await focus.click();
  await expect(focus).toHaveAttribute("aria-pressed", "true");
  await expect(needs).toHaveCount(0);
  await focus.click();
  await expect(needs).toBeVisible();

  // A reading that counts parts is a toggle; Esc lets go of it.
  const readings = overview.getByRole("region", { name: "Live readings: This computer" });
  const ports = readings.getByRole("button", { name: /Ports/ });
  await ports.click();
  await expect(ports).toHaveAttribute("aria-pressed", "true");
  await app.keyboard.press("Escape");
  await expect(ports).toHaveAttribute("aria-pressed", "false");

  // Zoom without a wheel.
  await overview.getByRole("button", { name: "Zoom in" }).click();
  await overview.getByRole("button", { name: "Zoom out" }).click();

  // The demo's recorded calls: a reading, a layer of their own, and their own toggle.
  const calls = readings.getByRole("button", { name: /Calls/ });
  await expect(calls).toContainText("recording off");
  await calls.click();
  await expect(calls).toHaveAttribute("aria-pressed", "true");
  await expect(overview.getByRole("group", { name: "Layers" }).getByRole("button", { name: /^Calls/ })).toHaveAttribute("aria-pressed", "true");
});

test("the 3D machine keeps configuration unknown until it is read", async ({ app }) => {
  test.skip(await open3d(app, false) !== "3d", "WebGL 2 isn't available in this browser");
  const overview = app.getByRole("region", { name: "Overview" });
  const readings = overview.getByRole("region", { name: "Live readings: This computer" });
  await expect(readings.getByText("Not read", { exact: true })).toHaveCount(2);
  await overview.getByRole("button", { name: "Read MCP config" }).first().click();
  await expect(readings).toContainText("of 7 configured");
});

test("without WebGL, 3D shows the map and says why", async ({ app }) => {
  await app.addInitScript(() => {
    const original = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, type: string, ...rest: unknown[]) {
      return type === "webgl2" ? null : (original as (...a: unknown[]) => unknown).call(this, type, ...rest);
    } as typeof original;
  });
  expect(await open3d(app)).toBe("fallback");
  const overview = app.getByRole("region", { name: "Overview" });
  await expect(overview.getByText(/WebGL 2 is turned off or unsupported here/)).toBeVisible();
  await expect(overview.getByRole("group", { name: /^Machine map/ })).toBeVisible();
  await expect(overview.getByRole("button", { name: "Use 3D anyway" })).toHaveCount(0);
});

for (const width of [390, 1024, 1440]) {
  test(`3D at ${width}px: no page overflow, and a list of agents on a phone-sized window`, async ({ app }) => {
    await app.setViewportSize({ width, height: 900 });
    const mode = await open3d(app);
    const overview = app.getByRole("region", { name: "Overview" });
    expect(await app.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)).toBeLessThanOrEqual(0);
    if (width <= 760) {
      expect(mode).toBe("list");
      await expect(overview.getByRole("list", { name: "Machine map" })).toBeVisible();
      await expect(overview.locator("canvas")).toHaveCount(0);
    }
    else if (mode === "3d") {
      // The overlays stay inside the canvas.
      const outside = await overview.evaluate((root) => {
        const vp = root.querySelector(".viewport")!.getBoundingClientRect();
        return [...root.querySelectorAll<HTMLElement>(".viewport .hud")].filter((el) => { const r = el.getBoundingClientRect(); return r.width > 0 && (r.left < vp.left - 0.5 || r.right > vp.right + 0.5 || r.top < vp.top - 0.5 || r.bottom > vp.bottom + 0.5); }).map((el) => el.className);
      });
      expect(outside).toEqual([]);
    }
  });
}

for (const theme of ["light", "dark"] as const) {
  test(`the 3D machine has no axe violations (${theme})`, async ({ app }) => {
    await app.evaluate((t) => localStorage.setItem("holdmap.theme", t), theme);
    test.skip(await open3d(app) !== "3d", "WebGL 2 isn't available in this browser");
    const overview = app.getByRole("region", { name: "Overview" });
    await overview.getByRole("navigation", { name: "Machine parts" }).getByRole("button", { name: /^Claude Code / }).click();
    const { violations } = await new AxeBuilder({ page: app }).include(".m3d").withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).analyze();
    expect(violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
  });
}

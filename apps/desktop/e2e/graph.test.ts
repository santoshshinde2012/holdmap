import type { Locator, Page } from "@playwright/test";
import { expect, test } from "./fixtures";

const canvas = (page: Page) => page.locator("#service-graph");
const details = (page: Page, port: number) => page.getByRole("complementary", { name: "Port details", exact: true })
  .or(page.getByRole("dialog", { name: `Details for port ${port}`, exact: true }));

async function openGraph(page: Page) {
  await page.getByRole("radio", { name: /^Graph/ }).click();
  await expect(canvas(page)).toBeVisible();
  await expect(canvas(page).getByRole("button", { name: /^shop\-api :3001(?!\d)/ })).toBeAttached();
}

async function expectReadableCards(page: Page) {
  await expect.poll(() => canvas(page).locator(".node .label").evaluateAll((labels) => {
    if (!labels.length) return 0;
    return Math.min(...labels.map((label) => {
      const viewport = label.closest(".svelte-flow__viewport")!;
      return new DOMMatrix(getComputedStyle(viewport).transform).a * parseFloat(getComputedStyle(label).fontSize);
    }));
  }), { message: "Readable size must keep service names at least 10 rendered pixels" }).toBeGreaterThanOrEqual(10);
  await expect.poll(() => canvas(page).locator(".node").evaluateAll((nodes) => {
    const bounds = document.getElementById("service-graph")!.getBoundingClientRect();
    return nodes.some((node) => {
      const card = node.getBoundingClientRect();
      return card.left >= bounds.left && card.right <= bounds.right && card.top >= bounds.top && card.bottom <= bounds.bottom;
    });
  }), { message: "The readable viewport must show a complete service card" }).toBe(true);
}

async function expectControlsContained(page: Page, controls: Locator) {
  const bounds = await canvas(page).boundingBox();
  expect(bounds).not.toBeNull();
  expect(await controls.count()).toBeGreaterThan(0);
  for (const control of await controls.all()) {
    if (!await control.isVisible()) continue;
    const box = await control.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.x).toBeGreaterThanOrEqual(bounds!.x - 1);
    expect(box!.x + box!.width).toBeLessThanOrEqual(bounds!.x + bounds!.width + 1);
    expect(box!.y).toBeGreaterThanOrEqual(bounds!.y - 1);
    expect(box!.y + box!.height).toBeLessThanOrEqual(bounds!.y + bounds!.height + 1);
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
}

test("Graph uses the full canvas until an explicit service click and returns focus on close", async ({ app }) => {
  await openGraph(app);
  await expect(details(app, 3001)).toHaveCount(0);
  await expect(app.getByRole("heading", { name: "At a glance", exact: true })).toHaveCount(0);
  const bounds = await canvas(app).boundingBox();
  expect(bounds!.width).toBeGreaterThan(1440 * .9);
  const service = canvas(app).getByRole("button", { name: /^shop\-api :3001(?!\d)/ });
  await service.click();
  await expect(details(app, 3001).getByRole("heading", { name: /^shop-api/ })).toBeVisible();
  await expect(service).toHaveAttribute("aria-pressed", "true");
  // WebKit pointer activation can leave focus on the canvas instead of the button.
  await canvas(app).focus();
  const options = canvas(app).getByRole("button", { name: "Graph options", exact: true });
  await options.dispatchEvent("click");
  await expect(options).toHaveAttribute("aria-expanded", "true");
  await app.keyboard.press("Escape");
  await expect(options).toHaveAttribute("aria-expanded", "false");
  await expect(options).toBeFocused();
  await expect(service).toHaveAttribute("aria-pressed", "true");
  await expect(details(app, 3001).getByRole("heading", { name: /^shop-api/ })).toBeVisible();
  await canvas(app).focus();
  await options.dispatchEvent("click");
  await app.keyboard.press("ControlOrMeta+k");
  const palette = app.getByRole("dialog", { name: "Command palette", exact: true });
  await expect(palette).toBeVisible();
  await app.keyboard.press("Escape");
  await expect(palette).toHaveCount(0);
  await expect(options).toHaveAttribute("aria-expanded", "true");
  await app.keyboard.press("Escape");
  await expect(options).toHaveAttribute("aria-expanded", "false");
  await expect(service).toHaveAttribute("aria-pressed", "true");
  const viewport = canvas(app).locator(".svelte-flow__viewport");
  const initialTransform = await viewport.evaluate((element) => getComputedStyle(element).transform);
  const pane = canvas(app).locator(".svelte-flow__pane");
  const paneBounds = (await pane.boundingBox())!;
  await app.mouse.move(paneBounds.x + 24, paneBounds.y + 24);
  await app.mouse.down();
  await app.mouse.move(paneBounds.x + 24, paneBounds.y + paneBounds.height - 60, { steps: 8 });
  await app.mouse.up();
  // Flow batches pointer moves in animation frames; capture the completed pan, not its
  // penultimate frame, before asking the app to refresh.
  await app.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  const transform = await viewport.evaluate((element) => getComputedStyle(element).transform);
  expect(transform).not.toBe(initialTransform);
  const refresh = app.getByRole("button", { name: "Refresh", exact: true });
  await refresh.click();
  await expect(refresh).toBeDisabled();
  await expect(refresh).toBeEnabled();
  await expect(canvas(app).locator(".graph-body")).not.toHaveAttribute("aria-busy", "true");
  await app.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  await expect(service).toHaveAttribute("aria-pressed", "true");
  await expect(details(app, 3001).getByRole("heading", { name: /^shop-api/ })).toBeVisible();
  expect(await viewport.evaluate((element) => getComputedStyle(element).transform)).toBe(transform);
  await details(app, 3001).getByRole("button", { name: "Hide details pane", exact: true }).click();
  await expect(details(app, 3001)).toHaveCount(0);
  await expect(service).toBeFocused();
  expect((await canvas(app).boundingBox())!.width).toBeGreaterThan(1440 * .9);
});

test("Graph search is independent of hidden List filters and keeps observed neighbor context", async ({ app }) => {
  const filters = app.getByRole("toolbar", { name: "View and filters", exact: true });
  await filters.getByRole("button", { name: /^Dev servers/ }).click();
  await app.getByLabel("Search ports", { exact: true }).fill("3000");
  await openGraph(app);
  const search = app.getByLabel("Search services", { exact: true });
  await expect(search).toHaveValue("");
  await canvas(app).getByRole("button", { name: "All services", exact: true }).click();
  await search.fill("11434");
  const ollama = canvas(app).getByRole("button", { name: /^Ollama :11434(?!\d)/ });
  await expect(ollama).toBeVisible();
  await ollama.click();
  await expect(details(app, 11434).getByRole("heading", { name: /^Ollama/ })).toBeVisible();
  await search.fill("shop-api");
  await expect(details(app, 11434)).toHaveCount(0);
  await expect(search).toBeFocused();
  await expect(canvas(app).getByRole("button", { name: /^shop\-api :3001(?!\d)/ })).toBeVisible();
  await expect(canvas(app).getByRole("button", { name: /^db :5432(?!\d)/ })).toBeAttached();
  await expect(canvas(app).getByRole("button", { name: /^shop\-web :3000(?!\d)/ })).toBeAttached();
  await search.press("Enter");
  await expect(details(app, 3001).getByRole("heading", { name: /^shop-api/ })).toBeVisible();
  await details(app, 3001).getByRole("button", { name: "Hide details pane", exact: true }).click();
  await app.getByRole("radio", { name: "List", exact: true }).click();
  await expect(app.getByLabel("Search ports", { exact: true })).toHaveValue("3000");
  await expect(filters.getByRole("button", { name: /^Dev servers/ })).toHaveAttribute("aria-pressed", "true");
  await expect(app.getByRole("listbox", { name: "Ports in use" }).getByRole("option")).toHaveCount(1);
  await openGraph(app);
  await expect(search).toHaveValue("shop-api");
  await search.fill("no-such-service");
  await expect(canvas(app).getByRole("heading", { name: "No services match this search", exact: true })).toBeVisible();
  await canvas(app).locator(".empty").getByRole("button", { name: "Clear search", exact: true }).click();
  await expect(search).toHaveValue("");
  await expect(canvas(app).getByRole("button", { name: /^Ollama :11434(?!\d)/ })).toBeAttached();
});

test("External search selects only its read-only peer and hiding hosts clears that selection", async ({ app }) => {
  await openGraph(app);
  const search = app.getByLabel("Search services", { exact: true });
  await search.fill("External");
  await expect(canvas(app).getByRole("button", { name: "External Internet & remote hosts", exact: true })).toBeVisible();
  await search.press("Enter");
  const peer = canvas(app).getByRole("complementary", { name: "Details for External", exact: true });
  await expect(peer).toBeVisible();
  await expect(peer).toContainText("No listening ports reported");
  await expect(app.getByRole("complementary", { name: "Port details", exact: true })).toHaveCount(0);
  await expect(peer.getByRole("button", { name: /^Stop/ })).toHaveCount(0);
  await canvas(app).getByRole("button", { name: "Graph options", exact: true }).click();
  await canvas(app).getByRole("region", { name: "Graph display options", exact: true }).getByRole("button", { name: "External", exact: true }).click();
  await expect(peer).toHaveCount(0);
  await expect(canvas(app).getByRole("heading", { name: "No services match this search", exact: true })).toBeVisible();
  await expect(app.getByRole("dialog", { name: /^Details for port/ })).toHaveCount(0);
});

for (const width of [390, 820, 1440]) for (const theme of ["dark", "light"]) {
  test(`${width}px ${theme} dense All services graph stays readable with contained optional controls`, async ({ app }) => {
    await app.setViewportSize({ width, height: 900 });
    await app.evaluate((theme) => {
      localStorage.setItem("holdmap.theme", theme);
      localStorage.setItem("holdmap.layout", "force");
    }, theme);
    await app.goto("/?mockPorts=40");
    await expect(app.getByRole("listbox", { name: "Ports in use" }).getByRole("option").first()).toBeVisible();
    await expect(app.locator("html")).toHaveAttribute("data-theme", theme);
    await openGraph(app);
    await canvas(app).getByRole("button", { name: "All services", exact: true }).click();
    await expect.poll(() => canvas(app).locator(".node").count()).toBeGreaterThan(25);
    await expect(canvas(app).getByRole("radio", { name: "Force", exact: true })).toHaveAttribute("aria-checked", "true");
    await expect(canvas(app).locator(".svelte-flow__edge")).toHaveCount(6);
    await expectReadableCards(app);
    await expect.poll(() => canvas(app).getByRole("button", { name: /^shop\-web :3000(?!\d)/ }).evaluate((node) => {
      const card = node.getBoundingClientRect(), bounds = node.closest(".graph-body")!.getBoundingClientRect();
      return card.left >= bounds.left && card.right <= bounds.right && card.top >= bounds.top && card.bottom <= bounds.bottom;
    }), { message: "Start on a complete development service card, including on mobile" }).toBe(true);
    await expectControlsContained(app, canvas(app).getByRole("toolbar", { name: "Graph view options", exact: true }).locator("button")
      .or(canvas(app).locator(".svelte-flow__controls button")));
    await expect(canvas(app).getByRole("button", { name: "Traffic", exact: true })).toHaveCount(0);
    const options = canvas(app).getByRole("button", { name: "Graph options", exact: true });
    await options.click();
    const displayOptions = canvas(app).getByRole("region", { name: "Graph display options", exact: true });
    const traffic = displayOptions.getByRole("button", { name: "Traffic", exact: true });
    await expect(traffic).toBeVisible();
    await expect(traffic).toBeDisabled();
    await expect(displayOptions.getByRole("button", { name: "External", exact: true })).toBeVisible();
    const legend = displayOptions.getByRole("button", { name: "Legend", exact: true });
    await expect(legend).toBeVisible();
    await legend.click();
    await expectControlsContained(app, displayOptions.getByRole("button"));
    await app.keyboard.press("Escape");
    await expect(traffic).toHaveCount(0);
    await expect(options).toBeFocused();
    await options.press("Enter");
    await options.press("Shift+Tab");
    await expect(displayOptions).toHaveCount(0);
    await canvas(app).getByRole("button", { name: "Fit all services in view", exact: true }).click();
    await canvas(app).getByRole("button", { name: /^\d+%, reset to readable size$/ }).click();
    await expectReadableCards(app);
    expect(await app.evaluate(() => "__TAURI_INTERNALS__" in window)).toBe(false);
  });
}

for (const key of ["Enter", "Space"]) {
  test(`390px native ${key} selects the exact service and drawer dismissal restores its focus`, async ({ app }) => {
    await app.setViewportSize({ width: 390, height: 844 });
    await openGraph(app);
    const service = canvas(app).getByRole("button", { name: /^shop\-api :3001(?!\d)/ });
    await service.focus();
    await service.press(key);
    const drawer = app.getByRole("dialog", { name: "Details for port 3001", exact: true });
    await expect(drawer.getByRole("heading", { name: /^shop-api/ })).toBeVisible();
    await drawer.getByRole("button", { name: "Close details", exact: true }).click();
    await expect(drawer).toHaveCount(0);
    await expect(service).toBeFocused();
    await expectReadableCards(app);
  });
}

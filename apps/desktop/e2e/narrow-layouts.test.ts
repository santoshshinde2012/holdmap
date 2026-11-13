import AxeBuilder from "@axe-core/playwright";
import type { Page } from "@playwright/test";
import { agentChip, expect, openAgents, openAgentsPicture, test } from "./fixtures";

// 390 × 844 regressions: layered graph direction, peer chrome, map controls and minimap,
// and detail tabs. Every check also asserts there is no horizontal scroll.
const canvas = (page: Page) => page.locator("#service-graph");

async function openGraph(page: Page) {
  await page.getByRole("radio", { name: /^Graph/ }).click();
  await expect(canvas(page)).toBeVisible();
  await expect(canvas(page).getByRole("button", { name: /^shop\-api :3001(?!\d)/ })).toBeAttached();
}

async function noHorizontalScroll(page: Page) {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
}

for (const theme of ["light", "dark"] as const) {
  test.describe(`${theme} 390px`, () => {
    test.beforeEach(async ({ app }) => {
      await app.setViewportSize({ width: 390, height: 844 });
      await app.evaluate((t) => localStorage.setItem("holdmap.theme", t), theme);
      await app.evaluate(() => localStorage.setItem("holdmap.layout", "layered"));
      await app.reload();
      await expect(app.locator("html")).toHaveAttribute("data-theme", theme);
    });

    test("layered graph runs top to bottom and keeps a whole card in view", async ({ app }) => {
      await openGraph(app);
      await expect.poll(() => canvas(app).locator(".node").evaluateAll((nodes) => {
        const bounds = document.getElementById("service-graph")!.getBoundingClientRect();
        return nodes.some((node) => {
          const card = node.getBoundingClientRect();
          return card.left >= bounds.left && card.right <= bounds.right && card.top >= bounds.top && card.bottom <= bounds.bottom;
        });
      })).toBe(true);
      // Top-to-bottom: a dependency sits below the service that uses it.
      const web = (await canvas(app).getByRole("button", { name: /^shop\-web :3000(?!\d)/ }).boundingBox())!;
      const api = (await canvas(app).getByRole("button", { name: /^shop\-api :3001(?!\d)/ }).boundingBox())!;
      expect(api.y).toBeGreaterThan(web.y + web.height);
      await expect(canvas(app).locator(".svelte-flow__minimap")).toHaveCount(0);
      await noHorizontalScroll(app);
    });

    test("selecting a peer keeps the floating chrome full width above the sheet", async ({ app }) => {
      await openGraph(app);
      const before = (await canvas(app).getByRole("group", { name: "Graph summary", exact: true }).boundingBox())!;
      const search = app.getByLabel("Search services", { exact: true });
      await search.fill("External");
      await search.press("Enter");
      const peer = canvas(app).getByRole("complementary", { name: "Details for External", exact: true });
      await expect(peer).toBeVisible();
      const graph = (await canvas(app).boundingBox())!;
      const stats = (await canvas(app).getByRole("group", { name: "Graph summary", exact: true }).boundingBox())!;
      // The summary keeps one row instead of squeezing into a narrow column.
      expect(stats.height).toBeLessThanOrEqual(before.height + 2);
      expect(stats.width).toBeGreaterThan(graph.width * 0.4);
      const sheet = (await peer.boundingBox())!;
      expect(sheet.y).toBeGreaterThan(stats.y + stats.height);
      expect(sheet.x).toBeGreaterThanOrEqual(graph.x - 1);
      expect(sheet.x + sheet.width).toBeLessThanOrEqual(graph.x + graph.width + 1);
      await noHorizontalScroll(app);
    });

    test("detail tabs wrap instead of being cut off", async ({ app }) => {
      await openGraph(app);
      const service = canvas(app).getByRole("button", { name: /^shop\-api :3001(?!\d)/ });
      await service.focus();
      await service.press("Enter");
      const drawer = app.getByRole("dialog", { name: "Details for port 3001", exact: true });
      const tabs = drawer.getByRole("tablist", { name: "Details sections", exact: true });
      await expect(tabs).toBeVisible();
      const list = (await tabs.boundingBox())!;
      for (const tab of await tabs.getByRole("tab").all()) {
        const box = (await tab.boundingBox())!;
        expect(box.x).toBeGreaterThanOrEqual(list.x - 1);
        expect(box.x + box.width).toBeLessThanOrEqual(list.x + list.width + 1);
      }
      const axe = await new AxeBuilder({ page: app }).include("[role=dialog]").analyze();
      expect(axe.violations.map((v) => v.id)).toEqual([]);
      await noHorizontalScroll(app);
    });

    test("agents: chips above a full-height inspector, the map behind a toggle", async ({ app }) => {
      await openAgents(app);
      const map = app.locator("#agents-overview .map");
      await expect(map).toBeHidden();
      await agentChip(app, "Cursor").click();
      const details = app.getByRole("complementary", { name: "Agent details", exact: true });
      await expect(details).toBeVisible();
      const box = (await details.boundingBox())!;
      // The inspector gets most of the window instead of a bottom sheet over the map.
      expect(box.height).toBeGreaterThan(844 * 0.5);
      await expect(app.getByRole("article", { name: "Cursor", exact: true })).toBeHidden();
      await agentChip(app, "Map").click();
      await expect(details).toBeHidden();
      await expect(map).toBeVisible();
      await noHorizontalScroll(app);
    });

    test("footprint zoom controls sit inside the canvas as one short row without a minimap", async ({ app }) => {
      await openAgentsPicture(app, "Footprint");
      const flow = app.locator(".svelte-flow").first();
      await expect(flow).toBeVisible();
      await expect(flow.locator(".svelte-flow__minimap")).toHaveCount(0);
      const area = (await flow.boundingBox())!;
      const controls = (await flow.locator(".svelte-flow__controls").boundingBox())!;
      expect(controls.x).toBeGreaterThanOrEqual(area.x - 1);
      expect(controls.y + controls.height).toBeLessThanOrEqual(area.y + area.height + 1);
      // One horizontal row keeps the controls short so they cover as little of the map as possible.
      expect(controls.width).toBeGreaterThan(controls.height);
      await noHorizontalScroll(app);
    });
  });
}

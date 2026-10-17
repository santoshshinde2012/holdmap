import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures";

// Regressions from the end-to-end audit: names, landmarks and keyboard paths that axe or a
// keyboard user would otherwise lose.

async function searchTargetsResolve(page: Page) {
  return page.evaluate(() => [...document.querySelectorAll<HTMLInputElement>("input[aria-controls]")]
    .filter((input) => input.getClientRects().length)
    .map((input) => ({ id: input.getAttribute("aria-controls"), found: !!document.getElementById(input.getAttribute("aria-controls") ?? "") })));
}

test("the title-bar search always controls a results region in the current view", async ({ app }) => {
  const views = app.getByRole("radiogroup", { name: "View", exact: true });
  const check = async (id: string) => expect.poll(() => searchTargetsResolve(app)).toEqual([{ id, found: true }]);
  await check("port-list");
  await views.getByRole("radio", { name: "Home", exact: true }).click();
  await check("home-overview");
  await views.getByRole("radio", { name: /^Graph/ }).click();
  await check("service-graph");
  await views.getByRole("radio", { name: /^Agents/ }).click();
  await check("agents-overview");
  await app.getByRole("radio", { name: "MCP & skills", exact: true }).click();
  await check("agent-capabilities");
  await app.getByRole("radio", { name: "Flow", exact: true }).click();
  await check("agents-flow");
});

test("one top-level heading, and dialogs add no extra banner or footer landmarks", async ({ app }) => {
  await expect(app.getByRole("heading", { level: 1 })).toHaveText("holdmap");
  await app.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(app.getByRole("dialog", { name: "Settings", exact: true })).toBeVisible();
  await expect(app.getByRole("banner")).toHaveCount(1);
  await expect(app.getByRole("contentinfo")).toHaveCount(1);
});

test("the Live indicator keeps a name and a tooltip on a narrow window", async ({ app }) => {
  await app.setViewportSize({ width: 390, height: 844 });
  const live = app.locator(".titlebar .live");
  await expect(live).toContainText("Live");
  await live.hover();
  await expect(app.getByRole("tooltip")).toBeVisible();
});

test("Needs you actions meet the 24px target size and explain Open", async ({ app }) => {
  await app.getByRole("radiogroup", { name: "View", exact: true }).getByRole("radio", { name: "Home", exact: true }).click();
  const open = app.getByRole("button", { name: /^Open: / }).first();
  await expect(open).toBeVisible();
  for (const button of await app.locator(".act").all()) {
    const box = await button.boundingBox();
    expect(box && Math.min(box.width, box.height)).toBeGreaterThanOrEqual(24);
  }
  await open.hover();
  await expect(app.getByRole("tooltip", { name: "Open details" })).toBeVisible();
});

test("the timeline names each lane with a row header", async ({ app }) => {
  await app.getByRole("radiogroup", { name: "View", exact: true }).getByRole("radio", { name: "Home", exact: true }).click();
  await app.getByRole("radiogroup", { name: "Home layout", exact: true }).getByRole("radio", { name: "Map", exact: true }).click();
  const lanes = app.getByRole("table", { name: "Agent lanes", exact: true });
  // The first row holds the column headers; every agent lane after it is named by a row header.
  const laneRows = lanes.locator(".row.lane");
  await expect(laneRows.first()).toBeVisible();
  for (const row of await laneRows.all()) await expect(row.getByRole("rowheader")).toHaveCount(1);
  await expect(lanes.getByRole("columnheader", { name: "Agent", exact: true })).toBeVisible();
});

test("an overflowing dialog body is reachable by keyboard", async ({ app }) => {
  await app.setViewportSize({ width: 900, height: 520 });
  await app.locator("body").press("?");
  const dialog = app.getByRole("dialog", { name: "Keyboard shortcuts", exact: true });
  await expect(dialog).toBeVisible();
  await expect(dialog.locator(".db")).toHaveAttribute("tabindex", "0");
});

test("Esc still closes Scheduled actions after a schedule is cancelled with the mouse", async ({ app }) => {
  await app.getByRole("option", { name: /^Port 3000 tcp,/ }).click();
  await app.getByRole("complementary", { name: "Port details", exact: true }).getByRole("button", { name: "Schedule stop…", exact: true }).click();
  const review = app.getByRole("alertdialog");
  await review.getByRole("button", { name: /^Schedule simulation/ }).click();
  await review.getByRole("button", { name: "View scheduled actions", exact: true }).click();
  const manager = app.getByRole("dialog", { name: "Scheduled actions", exact: true });
  await manager.getByRole("button", { name: "Cancel Stop :3000/tcp", exact: true }).click();
  await expect(manager).toContainText("Cancellation confirmed for Stop :3000/tcp.");
  await app.keyboard.press("Escape");
  await expect(manager).toHaveCount(0);
});

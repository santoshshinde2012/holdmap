import type { Locator } from "@playwright/test";
import { expect, test } from "./fixtures";

async function expectContained(container: Locator, children: Locator) {
  const bounds = await container.boundingBox();
  expect(bounds).not.toBeNull();
  for (const child of await children.all()) {
    const box = await child.boundingBox();
    expect(box, await child.textContent() ?? "visible detail control").not.toBeNull();
    expect(box!.x).toBeGreaterThanOrEqual(bounds!.x - 1);
    expect(box!.x + box!.width).toBeLessThanOrEqual(bounds!.x + bounds!.width + 1);
    expect(box!.y).toBeGreaterThanOrEqual(bounds!.y - 1);
    expect(box!.y + box!.height).toBeLessThanOrEqual(bounds!.y + bounds!.height + 1);
  }
}

test.describe("narrow docked details", () => {
  test.use({ viewport: { width: 1100, height: 760 } });

  test("tabs, resource metrics and separate stop actions fit the sidebar", async ({ app }) => {
    // Connections is available while the live service graph is loaded.
    await app.getByRole("radio", { name: "Graph", exact: true }).click();
    await app.getByRole("button", { name: "shop-web on ports 3000", exact: true }).click();
    const details = app.getByRole("complementary", { name: "Port details", exact: true });
    const tabs = details.getByRole("tablist", { name: "Details sections", exact: true });
    await expect(tabs.getByRole("tab")).toHaveCount(5);
    await expectContained(details, tabs);

    const metrics = details.getByLabel("Port activity", { exact: true });
    await expect(metrics).toContainText("Memory");
    await expect(metrics).toContainText("CPU");
    await expect(metrics).toContainText("Connections");
    // Wait for a second scan so the layout includes actual resource trend lines.
    await expect(metrics.locator("svg.spark")).not.toHaveCount(0);
    await expectContained(metrics, metrics.locator(".stat"));
    const clippedValues = await metrics.locator(".sv").evaluateAll((values) => values
      .filter((value) => value.scrollWidth > value.clientWidth + 1)
      .map((value) => value.textContent));
    expect(clippedValues, "Resource values and units should fit beside their trends").toEqual([]);

    const actions = details.getByLabel("Port actions", { exact: true });
    const stop = actions.getByRole("button", { name: /^Stop…/ });
    const schedule = actions.getByRole("button", { name: "Schedule stop…", exact: true });
    await expect(stop).toBeEnabled();
    await expect(schedule).toBeEnabled();
    await expect(actions).toContainText("keeps this computer running");
    await expectContained(details, actions.getByRole("button"));

    const overview = tabs.getByRole("tab", { name: "Overview", exact: true });
    await overview.focus();
    await overview.press("End");
    const commands = tabs.getByRole("tab", { name: /^Commands/ });
    await expect(commands).toBeFocused();
    await expect(commands).toHaveAttribute("aria-selected", "true");
    await expect(details.getByRole("tabpanel", { name: /^Commands/ })).toContainText("holdmap explain 3000");
    await expectContained(details, commands);
    await commands.press("Home");
    await expect(overview).toBeFocused();
    await expect(overview).toHaveAttribute("aria-selected", "true");
    await overview.press("ArrowRight");
    await expect(tabs.getByRole("tab", { name: /^Connections/ })).toHaveAttribute("aria-selected", "true");

    // Scrolling a long panel must leave both distinct stop controls reachable.
    await tabs.getByRole("tab", { name: "Process", exact: true }).click();
    await details.getByRole("tabpanel", { name: "Process", exact: true }).getByText("Project", { exact: true }).scrollIntoViewIfNeeded();
    await expectContained(details, actions.getByRole("button"));
    await stop.click();
    const review = app.getByRole("alertdialog");
    await expect(review.getByRole("button", { name: "Stop :3000", exact: true })).toBeVisible();
    await expect(review.getByRole("button", { name: "Stop :3000", exact: true })).toBeFocused();
    await expect(review.getByRole("combobox", { name: "When", exact: true })).toHaveCount(0);
    await review.getByRole("button", { name: "Cancel", exact: true }).click();
    await expect(app.getByRole("button", { name: "shop-web on ports 3000", exact: true })).toBeVisible();
    await expect(stop).toBeFocused();
  });

  test("blocked owners cannot be stopped or scheduled and protected overrides stay explicit", async ({ app }) => {
    const details = app.getByRole("complementary", { name: "Port details", exact: true });
    await app.getByRole("option", { name: /^Port 3000 tcp,/ }).click();
    await details.getByRole("tab", { name: "Process", exact: true }).click();
    for (const [port, reason] of [[5000, "Operating-system feature"], [631, "owner is hidden"]] as const) {
      await app.getByRole("option", { name: new RegExp(`^Port ${port} tcp,`) }).click();
      const actions = details.getByLabel("Port actions", { exact: true });
      await expect(actions).toContainText(reason);
      await expect(actions.getByRole("button", { name: /^Stop…/ })).toBeDisabled();
      await expect(actions.getByRole("button", { name: "Schedule stop…", exact: true })).toHaveCount(0);
      await expect(actions.getByRole("button", { name: "Restart", exact: true })).toHaveCount(0);
    }
    await expect(details.getByRole("tab", { name: "Process", exact: true })).toHaveCount(0);
    await expect(details.getByRole("tab", { name: /^Overview/ })).toHaveAttribute("aria-selected", "true");
    await expect(details.getByRole("tabpanel", { name: /^Overview/ })).toBeVisible();
    await app.getByRole("option", { name: /^Port 49152 tcp,/ }).click();
    const actions = details.getByLabel("Port actions", { exact: true });
    await expect(actions.getByRole("button", { name: "Schedule stop…", exact: true })).toHaveCount(0);
    await actions.getByRole("button", { name: /^Stop…/ }).click();
    const review = app.getByRole("alertdialog");
    await expect(review).toContainText("This process is protected");
    await expect(review.getByRole("button", { name: "Review stop", exact: true })).toBeDisabled();
    await review.getByRole("button", { name: "Cancel", exact: true }).click();
    await expect(app.getByRole("option", { name: /^Port 49152 tcp,/ })).toBeVisible();
  });

  test("the right-edge sort popup fits the window and remains keyboard operable after resize", async ({ app }) => {
    const select = app.getByRole("combobox", { name: /^Sort:/ });
    await select.click();
    const popup = app.getByRole("listbox", { name: "Sort by", exact: true });
    await expect(popup).toBeVisible();
    for (const width of [1100, 800]) {
      await app.setViewportSize({ width, height: 760 });
      await expect(async () => {
        const bounds = await popup.boundingBox();
        expect(bounds).not.toBeNull();
        expect(bounds!.x).toBeGreaterThanOrEqual(0);
        expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
        expect(bounds!.y).toBeGreaterThanOrEqual(0);
        expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(760);
      }).toPass({ timeout: 2_000 });
    }
    await select.press("End");
    await select.press("Enter");
    await expect(select).toContainText("Memory");
    await expect(select).toBeFocused();
    await expect(popup).toHaveCount(0);
  });
});

test.describe("compact detail drawer", () => {
  test.use({ viewport: { width: 800, height: 760 } });

  test("the drawer retains visible tabs and separate stop actions", async ({ app }) => {
    await app.getByRole("option", { name: /^Port 3000 tcp,/ }).click();
    const drawer = app.getByRole("dialog", { name: "Details for port 3000", exact: true });
    const details = drawer.getByRole("complementary", { name: "Port details", exact: true });
    await expect(details.getByRole("tab")).toHaveCount(4);
    await expectContained(drawer, details.getByRole("tab"));
    const actions = details.getByLabel("Port actions", { exact: true });
    await expectContained(drawer, actions.getByRole("button"));
    await actions.getByRole("button", { name: "Schedule stop…", exact: true }).click();
    const review = app.getByRole("alertdialog", { name: "Schedule stop :3000?", exact: true });
    await expect(review).toBeVisible();
    await review.getByRole("button", { name: "Cancel", exact: true }).click();
    await expect(drawer).toBeVisible();
    await drawer.getByRole("button", { name: "Close details", exact: true }).click();
    await expect(app.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
  });
});

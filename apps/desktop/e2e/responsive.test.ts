import { expect, openAgents, test } from "./fixtures";

// A mid-size window must never clip the toolbar or footer, and small controls keep a 24px target.
test("a 1000px window keeps every map picture reachable and the footer hints whole", async ({ app }) => {
  await app.setViewportSize({ width: 1000, height: 700 });
  await openAgents(app);
  const bar = app.getByRole("toolbar", { name: "View and filters" });
  await expect.poll(() => bar.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  for (const name of ["Computer", "Footprint", "Project"]) {
    const box = (await app.getByRole("radio", { name, exact: true }).boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(1000);
  }
  const hints = app.locator(".status .hints");
  const clipped = await hints.evaluate((el) => el.scrollWidth > el.clientWidth + 1);
  expect(clipped).toBe(false);
});

test("segmented controls are at least 24px tall", async ({ app }) => {
  await openAgents(app);
  await app.getByRole("radio", { name: /^Security\b/ }).click();
  const ws = app.getByRole("region", { name: "Security workspace" });
  for (const radio of await ws.getByRole("radiogroup", { name: "Security view" }).getByRole("radio").all()) {
    expect((await radio.boundingBox())!.height).toBeGreaterThanOrEqual(24);
  }
});

test("Security: exports belong to Findings, and a result note never moves the page", async ({ app }) => {
  await app.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await openAgents(app);
  await app.getByRole("radio", { name: /^Security\b/ }).click();
  const ws = app.getByRole("region", { name: "Security workspace" });
  const tools = ws.getByRole("group", { name: "Export findings" });
  const before = (await ws.getByRole("button", { name: "Rescan" }).boundingBox())!;
  await tools.getByRole("button", { name: "Copy summary" }).click();
  await expect(tools.getByRole("status")).toHaveText("Summary copied");
  const after = (await ws.getByRole("button", { name: "Rescan" }).boundingBox())!;
  expect(after.y).toBe(before.y);
  await ws.getByRole("radio", { name: "Analyzers" }).click();
  await expect(tools).toHaveCount(0);
});

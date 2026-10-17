import { expect, test } from "./fixtures";

test("the overview answers what is running and opens an agent in one more click", async ({ app }) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  const overview = app.getByRole("region", { name: "Overview" });
  await expect(overview.getByRole("navigation", { name: "Summary" })).toBeVisible();
  await expect(overview.getByRole("heading", { name: "Running agents" })).toBeVisible();
  await expect(overview.getByRole("heading", { name: "Needs attention" })).toBeVisible();
  await expect(overview.getByRole("heading", { name: "Recent changes" })).toBeVisible();
  await overview.getByRole("button", { name: /^Cursor,/ }).click();
  await expect(app.getByRole("radio", { name: /^Agents/ })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByRole("article", { name: "Cursor", exact: true }).getByRole("button", { name: /^Cursor /, pressed: true })).toBeVisible();
});

test("the overview opt-in shows each agent's MCP servers, and j/k moves between rows", async ({ app }) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  const overview = app.getByRole("region", { name: "Overview" });
  await overview.getByRole("button", { name: "Turn on" }).click();
  await expect(overview.getByRole("button", { name: /^MCP servers: 1 running of 3 configured/ })).toBeVisible();
  await expect(overview.getByRole("button", { name: "Turn on" })).toHaveCount(0);
  await overview.getByRole("button", { name: /^Cursor,/ }).focus();
  await app.keyboard.press("j");
  await expect(overview.getByRole("button", { name: /^Cursor,/ })).not.toBeFocused();
  await app.keyboard.press("k");
  await expect(overview.getByRole("button", { name: /^Cursor,/ })).toBeFocused();
  await overview.getByRole("button", { name: /^MCP servers: 1 running/ }).click();
  await expect(app.getByRole("radio", { name: "MCP & skills", exact: true })).toHaveAttribute("aria-checked", "true");
  await app.getByRole("radio", { name: "Overview", exact: true }).click();
  const config = app.getByRole("article", { name: "Cursor", exact: true }).getByRole("region", { name: "Cursor configuration" });
  await expect(config).toContainText("filesystem");
  await expect(config).toContainText("unpinned");
});

test("typing in the overview search continues on the port list", async ({ app }) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  await app.getByLabel("Search ports", { exact: true }).fill("vite");
  await expect(app.getByRole("listbox", { name: "Ports in use" })).toBeVisible();
});

for (const width of [390, 1440]) {
  test(`${width}px overview has no horizontal overflow`, async ({ app }) => {
    await app.setViewportSize({ width, height: 900 });
    await app.getByRole("radio", { name: /^Home/ }).click();
    await expect(app.getByRole("region", { name: "Overview" })).toBeVisible();
    const overflow = await app.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow).toBeLessThanOrEqual(0);
  });
}

test("first open shows the welcome once", async ({ page }) => {
  await page.addInitScript(() => { if (!sessionStorage.getItem("seeded")) { localStorage.clear(); sessionStorage.setItem("seeded", "1"); } });
  await page.goto("/");
  const welcome = page.getByRole("dialog", { name: "Welcome to holdmap" });
  await expect(welcome).toBeVisible();
  await expect(welcome.getByRole("switch", { name: /Notify me/ })).toBeVisible();
  await welcome.getByRole("button", { name: "Start" }).click();
  await expect(page.getByRole("region", { name: "Overview" })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("region", { name: "Overview" })).toBeVisible();
  await expect(page.getByRole("dialog", { name: "Welcome to holdmap" })).toHaveCount(0);
});

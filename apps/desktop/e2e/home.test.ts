import { expect, test } from "./fixtures";

test("home combines the machine map and agent lanes, synced, and opens an agent", async ({ app }) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  const overview = app.getByRole("region", { name: "Overview" });
  await expect(overview.getByRole("group", { name: "Machine map" })).toBeVisible();
  await expect(overview.getByRole("table", { name: "Agent lanes" })).toBeVisible();
  await expect(overview.getByRole("heading", { name: /^Needs you/ })).toBeVisible();
  const cursor = overview.getByRole("button", { name: /^Cursor, (Standard|Restricted|Elevated|Unknown)/ });
  await cursor.click();
  await expect(cursor).toHaveAttribute("aria-pressed", "true");
  await expect(overview.locator(".lane.active")).toContainText("Cursor");
  await cursor.click();
  await expect(app.getByRole("radio", { name: /^Agents/ })).toHaveAttribute("aria-checked", "true");
});

test("the inventory button adds MCP servers to the map and lanes", async ({ app }) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  const overview = app.getByRole("region", { name: "Overview" });
  await overview.getByRole("button", { name: "Read MCP config" }).click();
  await expect(overview.getByRole("button", { name: "MCP filesystem, running, unpinned" })).toBeVisible();
  await expect(overview.getByRole("button", { name: "Read MCP config" })).toHaveCount(0);
});

test("typing in the overview search continues on the port list", async ({ app }) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  await app.getByLabel("Search ports", { exact: true }).fill("vite");
  await expect(app.getByRole("listbox", { name: "Ports in use" })).toBeVisible();
});

for (const width of [390, 1024, 1280, 1440]) {
  test(`${width}px home has no overflow, no clipped text and no layout jump`, async ({ app }) => {
    await app.evaluate(() => localStorage.setItem("holdmap.agents.inventory", "1"));
    await app.reload();
    await app.setViewportSize({ width, height: 900 });
    await app.getByRole("radio", { name: /^Home/ }).click();
    const overview = app.getByRole("region", { name: "Overview" });
    await expect(overview.getByRole("table", { name: "Agent lanes" })).toBeVisible();
    await expect(overview.getByRole("button", { name: /^MCP filesystem/ }).first()).toBeVisible();
    const box = async () => (await overview.getByRole("table", { name: "Agent lanes" }).boundingBox())!;
    const before = await box();
    // Hover and select an agent: highlighting must not move anything.
    const lane = overview.getByRole("button", { name: /^Claude Code, up / });
    await lane.hover();
    await lane.click();
    await overview.getByRole("button", { name: "Docker Desktop is listening on all interfaces", exact: true }).hover();
    expect(await box()).toEqual(before);
    expect(await app.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)).toBeLessThanOrEqual(0);
    const clipped = await overview.evaluate((root) => [...root.querySelectorAll<HTMLElement>("*")]
      .filter((el) => el.offsetParent !== null && el.scrollWidth > el.clientWidth + 1 && getComputedStyle(el).overflowX !== "visible" && !el.matches(".stage, .needs-scroll"))
      .map((el) => `${el.className}: ${el.textContent?.slice(0, 60)}`));
    expect(clipped).toEqual([]);
    // Lane labels stay inside their lane.
    const outside = await overview.evaluate((root) => [...root.querySelectorAll<HTMLElement>(".lane .pin span")].filter((s) => {
      const lane = s.closest(".lane")!.getBoundingClientRect(); const r = s.getBoundingClientRect();
      return r.left < lane.left - 0.5 || r.right > lane.right + 0.5;
    }).map((s) => s.textContent));
    expect(outside).toEqual([]);
    if (width <= 760) await expect(overview.getByRole("list", { name: "Machine map" })).toBeVisible();
    else await expect(overview.getByRole("group", { name: "Machine map" })).toBeVisible();
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

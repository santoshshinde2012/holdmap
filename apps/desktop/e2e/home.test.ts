import { expect, test } from "./fixtures";

const home = async (app: import("@playwright/test").Page) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  return app.getByRole("region", { name: "Overview" });
};

test("home opens as cards, flips to the map with V, remembers it and keeps the selection", async ({ app }) => {
  const overview = await home(app);
  const layout = overview.getByRole("radiogroup", { name: "Home layout" });
  await expect(layout.getByRole("radio", { name: /^Cards/ })).toHaveAttribute("aria-checked", "true");
  await expect(overview.getByRole("region", { name: "Agents" }).or(overview.locator("section[aria-label='Agents']"))).toBeVisible();
  await overview.getByRole("button", { name: /^Cursor, (Standard|Restricted|Elevated|Unknown)/ }).first().click();
  // Single-key shortcuts skip focused controls (like G), so leave the card button first.
  await app.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await app.keyboard.press("v");
  await expect(layout.getByRole("radio", { name: /^Map/ })).toHaveAttribute("aria-checked", "true");
  await expect(overview.getByRole("group", { name: "Machine map" })).toBeVisible();
  await expect(overview.locator(".map g.agent[aria-pressed='true']")).toHaveAttribute("aria-label", /^Cursor,/);
  await expect(overview.locator(".lane.active")).toContainText("Cursor");
  await app.reload();
  await home(app);
  await expect(app.getByRole("radiogroup", { name: "Home layout" }).getByRole("radio", { name: /^Map/ })).toHaveAttribute("aria-checked", "true");
});

test("the map selects on the first click and opens the agent on the second", async ({ app }) => {
  const overview = await home(app);
  await overview.getByRole("radiogroup", { name: "Home layout" }).getByRole("radio", { name: /^Map/ }).click();
  await expect(overview.getByRole("table", { name: "Agent lanes" })).toBeVisible();
  const cursor = overview.locator(".map g.agent[aria-label^='Cursor,']");
  await cursor.click();
  await expect(cursor).toHaveAttribute("aria-pressed", "true");
  await cursor.click();
  await expect(app.getByRole("radio", { name: /^Agents/ })).toHaveAttribute("aria-checked", "true");
});

test("the inventory button adds MCP servers to the cards", async ({ app }) => {
  const overview = await home(app);
  await overview.getByRole("button", { name: "Read MCP config" }).click();
  await expect(overview.getByRole("button", { name: "MCP filesystem, running, unpinned" })).toBeVisible();
  await expect(overview.getByRole("button", { name: "Read MCP config" })).toHaveCount(0);
});

test("typing in the overview search continues on the port list", async ({ app }) => {
  await home(app);
  await app.getByLabel("Search ports", { exact: true }).fill("vite");
  await expect(app.getByRole("listbox", { name: "Ports in use" })).toBeVisible();
});

for (const layout of ["cards", "map"] as const) for (const width of [390, 1024, 1280, 1440]) {
  test(`${layout} at ${width}px: no overflow, no clipped text, no layout jump`, async ({ app }) => {
    await app.evaluate((l) => { localStorage.setItem("holdmap.agents.inventory", "1"); localStorage.setItem("holdmap.home.layout", l); }, layout);
    await app.reload();
    await app.setViewportSize({ width, height: 900 });
    const overview = await home(app);
    await expect(overview.getByRole("button", { name: /^MCP filesystem/ }).first()).toBeVisible();
    const target = layout === "cards" ? overview.locator("section[aria-label='Agents']") : overview.getByRole("table", { name: "Agent lanes" });
    // Page coordinates: clicking may scroll the page, which is not a layout jump.
    const box = () => target.evaluate((el) => { const r = el.getBoundingClientRect(); const top = el.closest(".home")!.firstElementChild!.getBoundingClientRect(); return { x: r.left - top.left, y: r.top - top.top, w: r.width, h: r.height }; });
    const before = await box();
    // Hover and select an agent: highlighting must not move anything.
    const who = layout === "cards" ? overview.getByRole("button", { name: /^Claude Code, / }).first() : overview.getByRole("button", { name: /^Claude Code, up / });
    await who.hover();
    await who.click();
    await overview.getByRole("button", { name: "Docker Desktop is listening on all interfaces", exact: true }).hover();
    expect(await box()).toEqual(before);
    expect(await app.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)).toBeLessThanOrEqual(0);
    const clipped = await overview.evaluate((root) => [...root.querySelectorAll<HTMLElement>("*")]
      .filter((el) => el.offsetParent !== null && el.scrollWidth > el.clientWidth + 1 && getComputedStyle(el).overflowX !== "visible" && !el.matches(".stage, .needs-scroll"))
      .map((el) => `${el.className}: ${el.textContent?.slice(0, 60)}`));
    expect(clipped).toEqual([]);
    if (layout === "cards") {
      // Cards in a row share one height.
      const uneven = await overview.evaluate((root) => {
        const rows = new Map<number, number[]>();
        for (const c of root.querySelectorAll<HTMLElement>("section[aria-label='Agents'] > article")) { const r = c.getBoundingClientRect(); rows.set(Math.round(r.top), [...(rows.get(Math.round(r.top)) ?? []), Math.round(r.height)]); }
        return [...rows.values()].filter((hs) => new Set(hs).size > 1);
      });
      expect(uneven).toEqual([]);
    } else {
      const outside = await overview.evaluate((root) => [...root.querySelectorAll<HTMLElement>(".lane .pin span")].filter((s) => {
        const lane = s.closest(".lane")!.getBoundingClientRect(); const r = s.getBoundingClientRect();
        return r.left < lane.left - 0.5 || r.right > lane.right + 0.5;
      }).map((s) => s.textContent));
      expect(outside).toEqual([]);
      if (width <= 760) await expect(overview.getByRole("list", { name: "Machine map" })).toBeVisible();
      else {
        // Every map label sits inside the stage: nothing clips at the edges.
        const off = await overview.evaluate((root) => {
          const stage = root.querySelector(".stage")!.getBoundingClientRect();
          return [...root.querySelectorAll<SVGTextElement>(".map text")].filter((t) => { const r = t.getBoundingClientRect(); return r.width > 0 && (r.left < stage.left || r.right > stage.right || r.top < stage.top || r.bottom > stage.bottom); }).map((t) => t.textContent);
        });
        expect(off).toEqual([]);
      }
    }
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

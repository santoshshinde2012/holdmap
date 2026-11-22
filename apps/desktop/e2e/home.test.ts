import { expect, test } from "./fixtures";

const home = async (app: import("@playwright/test").Page) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  return app.getByRole("region", { name: "Overview" });
};

test("home opens as cards and selects an agent", async ({ app }) => {
  const overview = await home(app);
  await expect(overview.getByRole("radiogroup", { name: "Home layout" })).toHaveCount(0);
  await expect(overview.locator("section[aria-label='Agents']")).toBeVisible();
  const cursor = overview.getByRole("button", { name: /^Cursor, (Standard|Restricted|Elevated|Unknown)/ }).first();
  await cursor.click();
  await expect(cursor).toHaveAttribute("aria-pressed", "true");
  await expect(cursor.locator("xpath=ancestor::article[1]")).toHaveClass(/selected/);
});

test("the inventory button adds MCP servers to the cards", async ({ app }) => {
  const overview = await home(app);
  await overview.getByRole("button", { name: "Read local config" }).click();
  await expect(overview.getByRole("button", { name: "MCP filesystem, running, unpinned" })).toBeVisible();
  await expect(overview.getByRole("button", { name: "Read local config" })).toHaveCount(0);
});

test("Home search filters agents, and a port number opens the list", async ({ app }) => {
  const overview = await home(app);
  const search = app.getByLabel("Search agents", { exact: true });
  await search.fill("claude");
  await expect(overview.getByRole("article", { name: "Claude Code" }).first()).toBeVisible();
  await expect(overview.getByRole("article", { name: "Docker Desktop" })).toHaveCount(0);
  await search.fill("3000");
  await expect(app.getByRole("listbox", { name: "Ports in use" })).toBeVisible();
  await expect(app.getByLabel("Search ports", { exact: true })).toHaveValue("3000");
});

for (const width of [390, 1024, 1280, 1440]) {
  test(`cards at ${width}px: no overflow, no clipped text, no layout jump`, async ({ app }) => {
    await app.evaluate(() => localStorage.setItem("holdmap.agents.inventory", "1"));
    await app.reload();
    await app.setViewportSize({ width, height: 900 });
    const overview = await home(app);
    await expect(overview.getByRole("button", { name: /^MCP filesystem/ }).first()).toBeVisible();
    const target = overview.locator("section[aria-label='Agents']");
    // Page coordinates: clicking may scroll the page, which is not a layout jump.
    const box = () => target.evaluate((el) => { const r = el.getBoundingClientRect(); const top = el.closest(".home")!.firstElementChild!.getBoundingClientRect(); return { x: r.left - top.left, y: r.top - top.top, w: r.width, h: r.height }; });
    const before = await box();
    // Hover and select an agent: highlighting must not move anything.
    const who = overview.getByRole("button", { name: /^Claude Code, / }).first();
    await who.hover();
    await who.click();
    await overview.getByRole("button", { name: "Docker Desktop is listening on all interfaces", exact: true }).hover();
    expect(await box()).toEqual(before);
    expect(await app.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)).toBeLessThanOrEqual(0);
    const clipped = await overview.evaluate((root) => [...root.querySelectorAll<HTMLElement>("*")]
      .filter((el) => el.offsetParent !== null && el.scrollWidth > el.clientWidth + 1 && getComputedStyle(el).overflowX !== "visible" && !el.matches(".stage, .needs-scroll, .sr-only"))
      .map((el) => `${el.className}: ${el.textContent?.slice(0, 60)}`));
    expect(clipped).toEqual([]);
    // Stacked blocks (summary, body, timeline) never run into each other, even when the summary wraps.
    const overlaps = await app.evaluate(() => {
      // Each block's real extent includes descendants that spill out of it; rows inside a scroll
      // area (the stage, the Needs you list) are clipped by it, so they don't count.
      const kids = [...document.querySelector(".home")!.children].filter((el) => (el as HTMLElement).offsetParent !== null)
        .map((el) => ({ top: el.getBoundingClientRect().top, bottom: Math.max(...[el, ...el.querySelectorAll("*")].filter((d) => !d.closest(".stage, .needs-scroll")).map((d) => d.getBoundingClientRect().bottom)) }));
      return kids.slice(1).filter((r, i) => r.top < kids[i].bottom - 0.5).length;
    });
    expect(overlaps).toBe(0);
    // That only holds while the Needs you list scrolls inside its panel: the scroll area itself,
    // and the skill-trust alert above it, stay within the panel without overlapping.
    const needsBox = await overview.evaluate((root) => {
      const panel = root.querySelector("#home-needs-h")!.closest("section")!.getBoundingClientRect();
      const box = (sel: string) => root.querySelector(sel)?.getBoundingClientRect() ?? null;
      const list = box(".needs-scroll"), alert = box("[data-trust-alert]");
      return { listInside: !list || (list.top >= panel.top - 0.5 && list.bottom <= panel.bottom + 0.5), alertAboveList: !list || !alert || alert.bottom <= list.top + 0.5 };
    });
    expect(needsBox).toEqual({ listInside: true, alertAboveList: true });
    const uneven = await overview.evaluate((root) => {
      const rows = new Map<number, number[]>();
      for (const c of root.querySelectorAll<HTMLElement>("section[aria-label='Agents'] > article")) { const r = c.getBoundingClientRect(); rows.set(Math.round(r.top), [...(rows.get(Math.round(r.top)) ?? []), Math.round(r.height)]); }
      return [...rows.values()].filter((hs) => new Set(hs).size > 1);
    });
    expect(uneven).toEqual([]);
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

test("a port chip on a Home card stops through the stop plan, and Cancel stops nothing", async ({ app }) => {
  const overview = await home(app);
  const chip = overview.locator("[data-card]").filter({ hasText: "Cursor" }).locator("xpath=ancestor::article[1]").getByRole("button", { name: /^Port 3000 / });
  await chip.click();
  const menu = app.getByRole("menu", { name: /^:3000 / });
  await expect(menu.getByRole("menuitem", { name: "Details", exact: true })).toBeFocused();
  await app.keyboard.press("ArrowDown");
  await expect(menu.getByRole("menuitem", { name: "Stop…", exact: true })).toBeFocused();
  await app.keyboard.press("Enter");
  const review = app.getByRole("alertdialog");
  await expect(review.getByRole("button", { name: "Stop :3000", exact: true })).toBeFocused();
  await review.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(review).toHaveCount(0);
  await expect(chip).toBeVisible();
});


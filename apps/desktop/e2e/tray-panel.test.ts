import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

// The tray popup panel's own page, at the size the app opens it, with the browser sample data.
for (const theme of ["light", "dark"] as const) {
  test(`the tray panel fits its window and passes axe (${theme})`, async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.setViewportSize({ width: 360, height: 600 });
    await page.addInitScript((t) => localStorage.setItem("holdmap.theme", t), theme);
    await page.goto("/tray.html");
    await expect(page.getByRole("heading", { name: "holdmap" })).toBeVisible();
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    await expect(page.getByRole("img", { name: /Agent CPU over the last \d+ scans/ })).toBeVisible();
    await expect(page.getByText("Claude Code listens on all interfaces")).toBeVisible();

    // Nothing spills sideways; the lists scroll inside the panel, the footer stays put.
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow).toBe(0);
    await expect(page.getByRole("button", { name: "Quit" })).toBeInViewport();
    const body = page.locator(".body");
    expect(await body.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
    for (const row of await page.locator(".row .text, .trow .name").all()) {
      expect(await row.evaluate((el) => el.getBoundingClientRect().right <= innerWidth)).toBe(true);
    }

    const { violations } = await new AxeBuilder({ page }).analyze();
    expect(violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
    expect(errors).toEqual([]);
  });
}

test("every tray panel row is a keyboard-reachable button", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 600 });
  await page.goto("/tray.html");
  await expect(page.getByText(":5173 is open to the network")).toBeVisible();
  const names: string[] = [];
  for (let i = 0; i < 14; i++) {
    await page.keyboard.press("Tab");
    names.push(await page.evaluate(() => document.activeElement?.textContent?.trim() ?? ""));
  }
  expect(names[0]).toBe("");
  expect(names).toContain("Claude Code listens on all interfaces Look now");
  expect(names).toContain("Stop all…");
  expect(names.some((n) => n.startsWith(":3000"))).toBe(true);
  expect(names.some((n) => n.startsWith("Claude Code · storefront"))).toBe(true);
});

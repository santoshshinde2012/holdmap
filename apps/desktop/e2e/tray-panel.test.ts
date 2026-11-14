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

    // The first reading starts below the header, not under it; no stray chart scale words.
    const gap = await page.evaluate(() => document.querySelector(".body > *")!.getBoundingClientRect().top - document.querySelector(".head")!.getBoundingClientRect().bottom);
    expect(gap).toBeGreaterThanOrEqual(0);
    expect(await page.evaluate(() => [...document.querySelectorAll("svg text, .scale")].map((el) => el.textContent?.trim()).filter((t) => t === "closed" || t === "opened"))).toEqual([]);
    // "Stop all…" sits in the section header, apart from the server rows it acts on.
    const stopAll = page.getByRole("button", { name: "Stop all…" });
    expect(await stopAll.evaluate((el) => !!el.closest("li, ul"))).toBe(false);
    // Docker Desktop is a developer tool, not an agent; long names keep their full text in a tooltip.
    await expect(page.getByText("Developer tools")).toBeVisible();
    for (const row of await page.locator(".trow").all()) expect(await row.getAttribute("title")).toBeTruthy();
    // Icon-only buttons say what they do, both to screen readers and on hover.
    const unlabelled = await page.evaluate(() => [...document.querySelectorAll("button")]
      .filter((b) => !b.textContent?.trim() && !(b.getAttribute("aria-label") && b.getAttribute("title")))
      .map((b) => b.outerHTML.slice(0, 80)));
    expect(unlabelled).toEqual([]);

    const { violations } = await new AxeBuilder({ page }).analyze();
    expect(violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
    expect(errors).toEqual([]);
    if (process.env.HOLDMAP_UPDATE_SCREENSHOTS) await page.screenshot({ path: `../../docs/screenshots/desktop-tray-panel-${theme}.png` });
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
  for (const name of ["Quick settings", "Refresh", "Quit"]) {
    const button = page.getByRole("button", { name, exact: true });
    await button.focus();
    await expect(button).toBeFocused();
    await expect(button).toBeInViewport();
  }
});

for (const height of [480, 600, 720]) {
  test(`quick settings and footer fit a ${height}px window`, async ({ page }) => {
    await page.setViewportSize({ width: 360, height });
    await page.goto("/tray.html");
    const footer = page.locator(".foot");
    expect(Math.abs((await footer.boundingBox())!.y + (await footer.boundingBox())!.height - height)).toBeLessThanOrEqual(1);
    await page.getByRole("button", { name: /Quick settings/ }).click();
    await expect(page.getByRole("region", { name: "Quick settings" })).toBeVisible();
    const notifications = page.getByRole("switch", { name: "Desktop notifications" });
    await expect(notifications).toBeChecked();
    await notifications.click();
    await expect(notifications).not.toBeChecked();
    const compact = page.getByRole("button", { name: "Compact", exact: true });
    await expect(compact).toHaveAttribute("aria-pressed", "true");
    await page.getByRole("button", { name: "Every reading", exact: true }).click();
    await expect(page.getByRole("switch", { name: /Combine|one item/ })).toBeVisible();
    await compact.click();
    await page.getByRole("button", { name: "Dark", exact: true }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await expect(page.getByRole("button", { name: "Quit", exact: true })).toBeInViewport();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBe(0);
    const { violations } = await new AxeBuilder({ page }).analyze();
    expect(violations.map(v => v.id)).toEqual([]);
    if (process.env.HOLDMAP_UPDATE_SCREENSHOTS && height === 600) {
      await page.locator(".settings").evaluate(el => el.scrollTop = 0);
      await page.screenshot({ path: "../../docs/screenshots/desktop-tray-settings-dark.png" });
    }
    await page.keyboard.press("Escape");
    await expect(page.getByRole("button", { name: /Quick settings/ })).toBeFocused();
    await expect(page.locator(".body").first()).toBeVisible();
  });
}

test("menu-bar section changes scroll only the readings, never the window chrome", async ({ page }) => {
  const { demoPanel } = await import("../src/lib/tray-panel");
  await page.setViewportSize({ width: 360, height: 600 });
  await page.addInitScript((data) => {
    const fixture = window as unknown as { panelData: typeof data; seenSeq: number; __TAURI_INTERNALS__: unknown };
    fixture.panelData = data;
    fixture.__TAURI_INTERNALS__ = { invoke: async (command: string) => {
      fixture.seenSeq = fixture.panelData.focusSeq;
      return command === "tray_panel" ? structuredClone(fixture.panelData) : undefined;
    } };
  }, demoPanel());
  await page.goto("/tray.html");
  await expect(page.getByRole("heading", { name: "holdmap" })).toBeVisible();
  for (const [i, section] of ["agents", "needs", "dev", "cpu", "top", "agents"].entries()) {
    await page.evaluate(({ section, seq }) => {
      const fixture = window as unknown as { panelData: { focus: string; focusSeq: number } };
      fixture.panelData.focus = section;
      fixture.panelData.focusSeq = seq;
    }, { section, seq: i + 1 });
    await expect.poll(() => page.evaluate(() => (window as unknown as { seenSeq: number }).seenSeq)).toBe(i + 1);
    await expect(page.locator(`[data-section="${section}"]`)).toBeInViewport();
    const bounds = await page.evaluate(() => ({
      head: document.querySelector(".head")!.getBoundingClientRect().top,
      foot: document.querySelector(".foot")!.getBoundingClientRect().bottom,
      root: document.scrollingElement!.scrollTop,
      height: innerHeight,
    }));
    expect(bounds.root, section).toBe(0);
    expect(bounds.head, section).toBe(0);
    expect(bounds.foot, section).toBe(bounds.height);
    await expect(page.getByRole("button", { name: "Quit", exact: true })).toBeInViewport();
  }
});

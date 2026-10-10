import { expect, test as base, type Locator, type Page } from "@playwright/test";

/** A fresh browser context uses the app's browser-only sample data, never the Tauri backend. */
export const test = base.extend<{ app: Page }>({
  app: async ({ page }, use) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.addInitScript(() => {
      localStorage.setItem("holdmap.onboarded", "1");
      localStorage.setItem("holdmap.theme", "dark");
      localStorage.setItem("holdmap.view", "list");
    });
    await page.goto("/");
    await expect(page.getByRole("listbox", { name: "Ports in use" }).getByRole("option").first()).toBeVisible();
    await use(page);
    expect.soft(errors, "The app should not raise uncaught browser errors").toEqual([]);
  },
});

export { expect };

export async function openAgents(page: Page): Promise<Locator> {
  const tab = page.getByRole("radio", { name: /^Agents/ });
  await tab.click();
  await expect(tab).toHaveAttribute("aria-checked", "true");
  await expect(page.getByRole("article", { name: "Cursor", exact: true })).toBeVisible();
  return page.getByLabel("Search agents", { exact: true });
}

export function agentCard(page: Page, name: string): Locator {
  return page.getByRole("article", { name, exact: true });
}

export function agentHeading(page: Page, name: string): Locator {
  return agentCard(page, name).getByRole("button", { name: new RegExp(`^${name} `) });
}

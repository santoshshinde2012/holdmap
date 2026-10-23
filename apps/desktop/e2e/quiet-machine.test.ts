import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

// A machine with no agents and no listeners (`?mockPorts=0`): every view says so and what to do,
// and none is blank, shows an orphan node, or leaves a list role with no options.

for (const theme of ["light", "dark"] as const) {
  test(`every view has a clear empty state and passes axe (${theme})`, async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.addInitScript((t) => { localStorage.setItem("holdmap.onboarded", "1"); localStorage.setItem("holdmap.theme", t); localStorage.setItem("holdmap.view", "home"); }, theme);
    await page.goto("/?mockPorts=0");
    const clean = async (label: string) => {
      const { violations } = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).analyze();
      expect(violations.map((v) => `${v.id} (${v.impact})`), `${label} should have no axe violations`).toEqual([]);
    };
    const view = (name: RegExp) => page.getByRole("radiogroup", { name: "View", exact: true }).getByRole("radio", { name });

    await expect(page.getByText("No agents or dev servers running").first()).toBeVisible();
    await clean("Home cards");
    await page.getByRole("radiogroup", { name: "Home layout" }).getByRole("radio", { name: /^Map/ }).click();
    await expect(page.getByText("No agents or dev servers running").first()).toBeVisible();
    await clean("Home map");

    await view(/^List/).click();
    await expect(page.getByRole("listbox")).toHaveCount(0);
    await clean("List");

    await view(/^Graph/).click();
    await expect(page.locator(".react-flow__node")).toHaveCount(0);
    await clean("Graph");

    await view(/^Agents/).click();
    await clean("Agents");
    await page.getByRole("radio", { name: "Security", exact: true }).click();
    await page.getByRole("radio", { name: "Findings", exact: true }).click();
    await expect(page.getByText("No analyzer is on")).toBeVisible();
    await expect(page.getByRole("button", { name: "Choose an analyzer" })).toBeVisible();
    await clean("Security first run");

    await page.keyboard.press("Control+k");
    await page.keyboard.type("zzzz");
    await expect(page.getByRole("listbox")).toHaveCount(0);
    await clean("Command palette with no match");
    expect(errors).toEqual([]);
  });
}

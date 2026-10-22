import AxeBuilder from "@axe-core/playwright";
import { expect, openAgents, test } from "./fixtures";
import type { Page } from "@playwright/test";

// The Security tab: summary, grouped findings with inline detail, filters, rescan progress and the
// analyzer gallery, with an axe pass in both themes.
async function axe(page: Page) {
  const { violations } = await new AxeBuilder({ page }).include("#agent-insights").withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).analyze();
  expect(violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
}
async function security(page: Page, theme: "light" | "dark") {
  await page.evaluate((t) => localStorage.setItem("holdmap.theme", t), theme);
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
  await openAgents(page);
  await page.getByRole("radio", { name: /^Security\b/ }).click();
  const ws = page.getByRole("region", { name: "Security workspace" });
  await ws.getByRole("radio", { name: /^Findings/ }).click();
  return ws;
}

for (const t of ["light", "dark"] as const) {
  test(`${t}: summary, grouped findings and inline detail`, async ({ app }) => {
    const ws = await security(app, t);
    const summary = ws.getByRole("group", { name: "Security summary" });
    await expect(summary).toContainText("Last scan");
    await expect(summary).toContainText("Skills scanned");
    await expect(ws.getByRole("region", { name: "High findings" })).toBeVisible();
    const row = ws.getByRole("button", { name: /Instruction Override/ });
    await row.click();
    await expect(row).toHaveAttribute("aria-expanded", "true");
    await expect(ws).toContainText("SKILL.md:12");
    await expect(ws.getByRole("button", { name: "Rescan with SkillSpector" })).toBeVisible();
    await axe(app);
    await row.click();
    await expect(row).toHaveAttribute("aria-expanded", "false");
  });

  test(`${t}: analyzer gallery has one primary action per card`, async ({ app }) => {
    const ws = await security(app, t);
    await ws.getByRole("radio", { name: "Analyzers" }).click();
    await expect(ws.getByRole("button", { name: "Run SkillSpector" })).toBeVisible();
    await expect(ws.getByRole("button", { name: "Install Bandit" })).toBeVisible();
    await expect(ws.getByRole("button", { name: /Add your own/ })).toBeVisible();
    await axe(app);
  });

  test(`${t}: Add your own opens the add form in place; Cancel closes it`, async ({ app }) => {
    const ws = await security(app, t);
    await ws.getByRole("radio", { name: "Analyzers" }).click();
    await ws.getByRole("button", { name: /Add your own/ }).click();
    const form = ws.getByTestId("add-analyzer");
    await expect(form).toBeVisible();
    await expect(form.getByRole("radio", { name: "Presets" })).toHaveCount(0);
    await expect(form.getByRole("radio", { name: "Command" })).toBeChecked();
    await axe(app);
    await form.getByRole("button", { name: "Cancel" }).click();
    await expect(form).toHaveCount(0);
  });
}

test("filters by severity, search and group; clearing restores the list", async ({ app }) => {
  const ws = await security(app, "light");
  await ws.getByRole("button", { name: "2 medium" }).click();
  await expect(ws.getByRole("region", { name: "High findings" })).toHaveCount(0);
  await expect(ws.getByRole("region", { name: "Medium findings" })).toBeVisible();
  await ws.getByRole("button", { name: "2 medium" }).click();
  await ws.getByRole("searchbox", { name: "Search findings" }).fill("nothing-like-this");
  await expect(ws).toContainText("No findings match");
  await ws.getByRole("button", { name: "Clear filters" }).click();
  await ws.getByRole("radio", { name: "Skill", exact: true }).click();
  await expect(ws.getByRole("region", { name: "release-check findings" })).toBeVisible();
  await ws.getByRole("radio", { name: "Analyzer", exact: true }).click();
  await expect(ws.getByRole("region", { name: "SkillSpector findings" })).toBeVisible();
});

test("Rescan runs approved analyzers with progress, then reports completion", async ({ app }) => {
  const ws = await security(app, "light");
  await ws.getByRole("button", { name: "Rescan", exact: true }).click();
  const status = ws.getByRole("status", { name: "Scan status" });
  await expect(status).toContainText("SkillSpector");
  await expect(status).toContainText("Scanned SkillSpector");
  await expect(ws.getByRole("group", { name: "Security summary" })).toContainText("just now");
  await status.getByRole("button", { name: "Dismiss" }).click();
  await expect(status).toHaveCount(0);
});

test("Enable opens Settings, where consent happens; nothing runs from the card", async ({ app }) => {
  const ws = await security(app, "light");
  await ws.getByRole("radio", { name: "Analyzers" }).click();
  await ws.getByRole("button", { name: "Enable OSV vulnerability lookup" }).click();
  await expect(app.getByRole("dialog", { name: "Settings", exact: true })).toBeVisible();
});

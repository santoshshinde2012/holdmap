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
  const dock = app.locator("[data-run-dock]");
  await expect(dock).toContainText("SkillSpector");
  await expect(app.locator("[data-status-run]")).toContainText("Scan done", { timeout: 15_000 });
  await expect(ws.getByRole("group", { name: "Security summary" })).toContainText("just now");
  await dock.getByRole("button", { name: "Dismiss" }).click();
  await expect(dock).toHaveCount(0);
});

test("Enable opens Settings, where consent happens; nothing runs from the card", async ({ app }) => {
  const ws = await security(app, "light");
  await ws.getByRole("radio", { name: "Analyzers" }).click();
  await ws.getByRole("button", { name: "Enable OSV vulnerability lookup" }).click();
  await expect(app.getByRole("dialog", { name: "Settings", exact: true })).toBeVisible();
});

for (const theme of ["light", "dark"] as const) test(`gallery: add an MCP analyzer step by step, with inline checks, tool hints and a review that leaves it off (${theme})`, async ({ app }) => {
  const ws = await security(app, theme);
  await ws.getByRole("radio", { name: "Analyzers" }).click();
  await ws.getByRole("button", { name: /Add your own/ }).click();
  const form = ws.getByTestId("add-analyzer");
  const steps = form.getByRole("list", { name: "Add analyzer steps" });
  await expect(steps.locator('[aria-current="step"]')).toContainText("Source");
  await form.getByRole("button", { name: "Add and choose tools…" }).click();
  await expect(form.getByRole("textbox", { name: "Command", exact: true })).toHaveAttribute("aria-invalid", "true");
  await form.getByLabel("Name", { exact: true }).fill("Skill lint (sample)");
  await expect(form.getByLabel("Id", { exact: true })).toHaveValue("local.skill-lint-sample");
  await form.getByRole("textbox", { name: "Command", exact: true }).fill("/usr/local/bin/skill-lint-mcp");
  await form.getByRole("button", { name: "Add and choose tools…" }).click();
  await expect(steps.locator('[aria-current="step"]')).toContainText("Tools");
  await form.getByRole("button", { name: "List tools", exact: true }).click();
  const tools = form.getByRole("region", { name: "Tools of Skill lint (sample)" });
  await expect(tools.getByRole("list", { name: "What the server says about list_files" })).toContainText("Read-only");
  await expect(tools).toContainText("hidden character");
  await axe(app);
  await tools.getByRole("button", { name: "Save tools", exact: true }).click();
  await expect(steps.locator('[aria-current="step"]')).toContainText("Review");
  await expect(form).toContainText("is added and off");
  await axe(app);
  await form.getByRole("button", { name: "Done", exact: true }).click();
  await expect(form).toHaveCount(0);
});

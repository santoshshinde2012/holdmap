import AxeBuilder from "@axe-core/playwright";
import { expect, openAgents, test } from "./fixtures";
import type { Page } from "@playwright/test";

// Inline details and the new security entry points: where details open, keyboard and focus,
// and an axe pass over each surface in both themes.
const frontendSkill = "Inspect skill frontend-checks, Project scope. Agents: Cursor (PID 52000). Project: /Users/dev/code/shop-web. Source: /Users/dev/code/shop-web/.agents/skills/frontend-checks/SKILL.md";
const workspace = (page: Page) => page.getByRole("region", { name: "MCP and skills workspace", exact: true });

async function axe(page: Page, selector: string) {
  // label-content-name-mismatch is experimental in axe 4.14 (not in its default set): cards
  // deliberately name kind, scope, owners and source, which their visible text abbreviates.
  const { violations } = await new AxeBuilder({ page }).include(selector).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).disableRules(["label-content-name-mismatch"]).analyze();
  expect(violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
}
async function theme(page: Page, value: "light" | "dark") {
  await page.evaluate((t) => localStorage.setItem("holdmap.theme", t), value);
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", value);
}
async function capabilities(page: Page) {
  await openAgents(page);
  await page.getByRole("radio", { name: "MCP & skills", exact: true }).click();
  await expect(workspace(page).getByRole("button", { name: frontendSkill, exact: true })).toBeVisible();
}

for (const t of ["light", "dark"] as const) {
  test(`${t}: a card's details open in the right sidebar, keep scroll and close with Esc`, async ({ app }) => {
    await theme(app, t);
    await capabilities(app);
    const host = workspace(app).locator(".canvas-host");
    const card = workspace(app).getByRole("button", { name: frontendSkill, exact: true });
    const scroll = await host.evaluate((el) => el.scrollTop);
    await card.focus(); await card.press("Enter");
    const panel = workspace(app).getByRole("complementary", { name: "Capability details", exact: true });
    await expect(panel).toBeVisible();
    await expect(card).toHaveAttribute("aria-expanded", "true");
    const cardBox = (await card.boundingBox())!;
    const panelBox = (await panel.boundingBox())!;
    expect(panelBox.x).toBeGreaterThan(cardBox.x);
    expect(panelBox.x + panelBox.width).toBeGreaterThan(cardBox.x + cardBox.width);
    expect(await host.evaluate((el) => el.scrollTop)).toBe(scroll);
    await expect(panel.getByRole("heading", { name: "frontend-checks", exact: true })).toBeFocused();
    await expect(panel.getByRole("region", { name: "Scan with analyzers" })).toBeVisible();
    await axe(app, "#agent-capabilities");
    await app.keyboard.press("Escape");
    await expect(panel).toHaveCount(0);
    await expect(card).toBeFocused();
    await expect(card).toHaveAttribute("aria-expanded", "false");
  });

  test(`${t}: a process opens the existing agent details screen`, async ({ app }) => {
    await theme(app, t);
    await capabilities(app);
    const toggle = app.getByRole("button", { name: "Show processes for Cursor", exact: true });
    await toggle.click();
    await expect(app.getByRole("button", { name: "Hide processes for Cursor", exact: true })).toHaveAttribute("aria-expanded", "true");
    await app.getByRole("button", { name: "Inspect agent card Cursor, PID 52000", exact: true }).click();
    await expect(app.getByRole("radio", { name: "Overview", exact: true })).toHaveAttribute("aria-checked", "true");
    const details = app.getByRole("complementary", { name: "Agent details", exact: true });
    await expect(details.getByRole("heading", { name: "Cursor", exact: true })).toBeVisible();
    await expect(details).toContainText("52000");
    await expect(app.getByRole("complementary", { name: "Capability details" })).toHaveCount(0);
    await axe(app, "#agent-details");
  });

  test(`${t}: Security is one click away, with analyzers and their install state`, async ({ app }) => {
    await theme(app, t);
    await openAgents(app);
    const tab = app.getByRole("radio", { name: /^Security\b/ });
    await expect(tab).toContainText(/\d/);
    await tab.click();
    const ws = app.getByRole("region", { name: "Security workspace" });
    await ws.getByRole("radio", { name: "Analyzers" }).click();
    await expect(ws.getByRole("region", { name: /^Enabled/ })).toContainText("SkillSpector");
    const available = ws.getByRole("region", { name: /^Available/ });
    await expect(available).toContainText("Not installed");
    await expect(available.getByRole("button", { name: "Enable OSV vulnerability lookup" })).toBeVisible();
    await expect(ws).toContainText("Off by default");
    await axe(app, "#agent-insights");
  });
}

test("a skill card shows its findings and its details offer Scan with…, gated by consent", async ({ app }) => {
  await capabilities(app);
  const card = workspace(app).getByRole("button", { name: frontendSkill, exact: true });
  await expect(card.locator(".badge")).toHaveText("1 low");
  await card.click();
  const scan = workspace(app).getByRole("region", { name: "Scan with analyzers" });
  await expect(scan).toContainText("SkillSpector");
  // Approved analyzers run on request; others only link to their permissions in Settings.
  await expect(scan.getByRole("button", { name: "Scan", exact: true })).toBeVisible();
  await expect(scan).toContainText("Off until you approve it");
  await scan.getByRole("button", { name: "Scan", exact: true }).click();
  await expect(scan.getByRole("status")).toContainText("SkillSpector");
});

test("Home cards and Needs you pass axe", async ({ app }) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  await expect(app.locator("section[aria-label='Agents'] article.card").first()).toBeVisible();
  await axe(app, "#home-overview");
});

test("Flow's relationship details close with Esc and return focus", async ({ app }) => {
  await openAgents(app);
  await app.getByRole("radio", { name: "Flow", exact: true }).click();
  const participant = app.getByRole("region", { name: "Flow participants" }).getByRole("button").first();
  await participant.click();
  const inspector = app.getByRole("region", { name: "Selected relationship details" });
  await expect(inspector).toBeVisible();
  await participant.focus();
  await app.keyboard.press("Escape");
  await expect(inspector).toHaveCount(0);
  await expect(participant).toBeFocused();
});

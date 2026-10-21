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
  test(`${t}: a card's details open inline under it, keep scroll and close with Esc`, async ({ app }) => {
    await theme(app, t);
    await capabilities(app);
    const host = workspace(app).locator(".canvas-host");
    const card = workspace(app).getByRole("button", { name: frontendSkill, exact: true });
    const scroll = await host.evaluate((el) => el.scrollTop);
    await card.focus(); await card.press("Enter");
    const panel = workspace(app).getByRole("region", { name: "Capability details", exact: true });
    await expect(panel).toBeVisible();
    await expect(card).toHaveAttribute("aria-expanded", "true");
    // Anchored: directly after the card, starting below it, with no side panel.
    expect(await card.evaluate((el) => el.nextElementSibling?.getAttribute("aria-label"))).toBe("Capability details");
    expect((await panel.boundingBox())!.y).toBeGreaterThan((await card.boundingBox())!.y);
    await expect(app.getByRole("complementary", { name: "Capability details", exact: true })).toHaveCount(0);
    expect(await host.evaluate((el) => el.scrollTop)).toBe(scroll);
    await expect(panel.getByRole("heading", { name: "frontend-checks", exact: true })).toBeFocused();
    await expect(panel.getByRole("region", { name: "Scan with analyzers" })).toBeVisible();
    await axe(app, "#agent-capabilities");
    await app.keyboard.press("Escape");
    await expect(panel).toHaveCount(0);
    await expect(card).toBeFocused();
    await expect(card).toHaveAttribute("aria-expanded", "false");
  });

  test(`${t}: process details open under the process row`, async ({ app }) => {
    await theme(app, t);
    await capabilities(app);
    const toggle = app.getByRole("button", { name: "Show processes for Cursor", exact: true });
    await toggle.click();
    await expect(app.getByRole("button", { name: "Hide processes for Cursor", exact: true })).toHaveAttribute("aria-expanded", "true");
    const row = app.getByRole("button", { name: "Inspect agent card Cursor, PID 52000", exact: true });
    await row.click();
    const panel = app.getByRole("complementary", { name: "Capability agents" }).getByRole("region", { name: "Capability details", exact: true });
    await expect(panel).toContainText("PID 52000");
    expect(await row.evaluate((el) => el.nextElementSibling?.getAttribute("aria-label"))).toBe("Capability details");
    await axe(app, "#agent-capabilities");
    await app.keyboard.press("Escape");
    await expect(panel).toHaveCount(0);
    await expect(row).toBeFocused();
  });

  test(`${t}: Security is one click away, with analyzers and their install state`, async ({ app }) => {
    await theme(app, t);
    await openAgents(app);
    const tab = app.getByRole("radio", { name: /^Security\b/ });
    await expect(tab).toContainText(/\d/);
    await tab.click();
    const ws = app.getByRole("region", { name: "Security workspace" });
    await ws.getByRole("radio", { name: "Analyzers" }).click();
    const gallery = ws.getByRole("region", { name: "Analyzers" });
    await expect(gallery).toContainText("SkillSpector");
    await expect(gallery).toContainText("Not installed");
    await expect(gallery).toContainText("Off by default");
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

test("the map's zoom toolbar and expandable chip pass axe", async ({ app }) => {
  await app.evaluate(() => localStorage.setItem("holdmap.home.layout", "map"));
  await app.reload();
  await app.getByRole("radio", { name: /^Home/ }).click();
  const map = app.getByRole("group", { name: /^Machine map/ });
  await map.getByRole("button", { name: /^6 remote hosts/ }).click();
  await axe(app, ".map-wrap");
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

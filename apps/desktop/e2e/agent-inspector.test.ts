import AxeBuilder from "@axe-core/playwright";
import { agentDetails, agentHeading, expect, openAgents, test } from "./fixtures";
import type { Page } from "@playwright/test";

// The selected agent's full picture lives in four sections beside the Overview map, and each
// opens the matching full workspace already scoped to that agent.
const sections = (page: Page) => agentDetails(page).getByRole("tablist", { name: "Agent sections", exact: true });
const section = (page: Page, name: string) => sections(page).getByRole("tab", { name: new RegExp(`^${name}`) });

test("the agent inspector has Overview, MCP & skills, Flow and Security sections beside the map", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  await expect(sections(app).getByRole("tab")).toHaveText([/^Overview/, /^MCP & skills/, /^Flow/, /^Security/]);
  await expect(section(app, "Overview")).toHaveAttribute("aria-selected", "true");
  await expect(agentDetails(app).getByRole("region", { name: "Memory" })).toBeVisible();
  await expect(agentDetails(app).getByRole("button", { name: "Stop all (2)", exact: true })).toBeVisible();
  // The Overview map stays in place while a section is open.
  await section(app, "MCP & skills").click();
  await expect(section(app, "MCP & skills")).toHaveAttribute("aria-selected", "true");
  await expect(agentDetails(app).getByRole("tabpanel")).toContainText("MCP servers & skills");
  await expect(agentDetails(app).getByRole("region", { name: /Locations & layers/ })).toBeVisible();
  await expect(agentDetails(app).getByRole("region", { name: "Memory" })).toHaveCount(0);
  await expect(app.locator("#agents-overview .svelte-flow")).toBeVisible();
  // Arrow keys move between sections.
  await section(app, "MCP & skills").press("ArrowRight");
  await expect(section(app, "Flow")).toHaveAttribute("aria-selected", "true");
  await expect(section(app, "Flow")).toBeFocused();
  await expect(agentDetails(app).getByRole("region", { name: "Live socket links" })).toContainText("live socket");
  await expect(agentDetails(app).getByRole("region", { name: "Ports it serves" })).toBeVisible();
});

test("the open section survives switching agents with a card click or ↑/↓", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  await section(app, "Security").click();
  await agentHeading(app, "Claude Code").click();
  await expect(agentDetails(app).getByRole("heading", { name: "Claude Code", exact: true })).toBeVisible();
  await expect(section(app, "Security")).toHaveAttribute("aria-selected", "true");
  const findings = agentDetails(app).getByRole("list", { name: "Open findings", exact: true });
  await expect(findings).toContainText("Instruction Override");
  await findings.locator("summary").filter({ hasText: "Instruction Override" }).click();
  await expect(findings).toContainText("Rule");
  await expect(findings).toContainText("Folder");
  await agentHeading(app, "Claude Code").focus();
  await app.keyboard.press("ArrowDown");
  await expect(agentDetails(app).getByRole("heading", { level: 3 })).not.toHaveText("Claude Code");
  await expect(section(app, "Security")).toHaveAttribute("aria-selected", "true");
});

test("Open full Flow scopes the Flow workspace to the selected agent with Show all", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  await section(app, "Flow").click();
  await agentDetails(app).getByRole("button", { name: "Open full Flow", exact: true }).click();
  const flow = app.getByRole("region", { name: "Agent flow", exact: true });
  await expect(flow).toBeVisible();
  await expect(app.getByRole("radio", { name: "Flow", exact: true })).toHaveAttribute("aria-checked", "true");
  const scope = flow.locator("[data-flow-scope]");
  await expect(scope).toContainText("Scoped to Cursor");
  const participants = flow.getByRole("region", { name: "Flow participants", exact: true });
  await expect(participants).toContainText("Cursor");
  await expect(participants).not.toContainText("Docker Desktop");
  const scopedCount = await participants.getByRole("button").count();
  await scope.getByRole("button", { name: "Show all", exact: true }).click();
  await expect(scope).toContainText("Showing every agent");
  await expect(participants).toContainText("Docker Desktop");
  expect(await participants.getByRole("button").count()).toBeGreaterThan(scopedCount);
  await scope.getByRole("button", { name: "Scope to Cursor", exact: true }).click();
  await expect(scope).toContainText("Scoped to Cursor");
});

test("Open full MCP & skills and Open full Security keep the selected agent's scope", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Claude Code").click();
  await section(app, "Security").click();
  await agentDetails(app).getByRole("button", { name: "Open full Security", exact: true }).click();
  const security = app.getByRole("region", { name: "Security workspace" });
  await expect(security.getByRole("combobox", { name: "Agent" })).toHaveValue("claude-code");
  await app.getByRole("radio", { name: "Overview", exact: true }).click();
  await expect(section(app, "Security")).toHaveAttribute("aria-selected", "true");
  await section(app, "MCP & skills").click();
  await agentDetails(app).getByRole("button", { name: "Open full MCP & skills", exact: true }).click();
  await expect(app.getByRole("radio", { name: "MCP & skills", exact: true })).toHaveAttribute("aria-checked", "true");
});

for (const theme of ["light", "dark"] as const) {
  test(`${theme}: every agent inspector section passes axe`, async ({ app }) => {
    await app.evaluate((v) => localStorage.setItem("holdmap.theme", v), theme);
    await app.reload();
    await expect(app.locator("html")).toHaveAttribute("data-theme", theme);
    await openAgents(app);
    await agentHeading(app, "Claude Code").click();
    for (const name of ["MCP & skills", "Flow", "Security"]) {
      await section(app, name).click();
      await expect(section(app, name)).toHaveAttribute("aria-selected", "true");
      const { violations } = await new AxeBuilder({ page: app }).include("#agent-details").withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).disableRules(["label-content-name-mismatch"]).analyze();
      expect(violations.map((v) => `${name} ${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
    }
  });
}

test("390px: the inspector sections fit the bottom sheet without horizontal overflow", async ({ app }) => {
  await app.setViewportSize({ width: 390, height: 844 });
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  for (const name of ["Overview", "MCP & skills", "Flow", "Security"]) {
    await section(app, name).click();
    await expect(section(app, name)).toHaveAttribute("aria-selected", "true");
    expect(await agentDetails(app).evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  }
  expect(await app.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
});

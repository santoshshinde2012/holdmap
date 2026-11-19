import AxeBuilder from "@axe-core/playwright";
import { agentDetails, agentHeading, expect, openAgents, test } from "./fixtures";

// One details screen beside the Overview map. MCP & skills, Flow and Security are the
// screens for those subjects, opened from here already scoped to the selected agent.

test("the agent details screen shows the agent and opens the other screens", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  const details = agentDetails(app);
  await expect(details.getByRole("tablist", { name: "Agent sections" })).toHaveCount(0);
  await expect(details.getByRole("region", { name: "RAM" })).toBeVisible();
  await expect(details.getByRole("region", { name: "Ports it serves" })).toBeVisible();
  await expect(details.getByRole("button", { name: "Stop all (2)", exact: true })).toBeVisible();
  await expect(details.getByRole("button", { name: "Open in MCP & skills", exact: true })).toBeVisible();
  await expect(details.getByRole("button", { name: "Open in Flow", exact: true })).toBeVisible();
  await expect(details.getByRole("button", { name: "Open in Security", exact: true })).toBeVisible();
  await expect(details.getByText("What Cursor can use")).toHaveCount(0);
  await expect(details.getByRole("region", { name: "Live socket links" })).toHaveCount(0);
  await expect(app.locator("#agents-overview .svelte-flow")).toBeVisible();
});

test("switching agents keeps the same details screen", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  await agentHeading(app, "Claude Code").click();
  await expect(agentDetails(app).getByRole("heading", { name: "Claude Code", exact: true })).toBeVisible();
  await expect(agentDetails(app).getByRole("region", { name: "RAM" })).toBeVisible();
  await expect(agentDetails(app).getByRole("button", { name: "Open in Security", exact: true })).toBeVisible();
  await agentHeading(app, "Claude Code").focus();
  await app.keyboard.press("ArrowDown");
  await expect(agentDetails(app).getByRole("heading", { level: 3 })).not.toHaveText("Claude Code");
  await expect(agentDetails(app).getByRole("region", { name: "RAM" })).toBeVisible();
});

test("Open in Flow scopes the Flow workspace to the selected agent with Show all", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  await agentDetails(app).getByRole("button", { name: "Open in Flow", exact: true }).click();
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

test("Open in MCP & skills and Open in Security keep the selected agent's scope", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Claude Code").click();
  await agentDetails(app).getByRole("button", { name: "Open in Security", exact: true }).click();
  const security = app.getByRole("region", { name: "Security workspace" });
  await expect(security.getByRole("combobox", { name: "Agent" })).toHaveValue("claude-code");
  await app.getByRole("radio", { name: "Overview", exact: true }).click();
  await expect(agentDetails(app).getByRole("heading", { name: "Claude Code", exact: true })).toBeVisible();
  await agentDetails(app).getByRole("button", { name: "Open in MCP & skills", exact: true }).click();
  await expect(app.getByRole("radio", { name: "MCP & skills", exact: true })).toHaveAttribute("aria-checked", "true");
});

for (const theme of ["light", "dark"] as const) {
  test(`${theme}: the agent details screen passes axe`, async ({ app }) => {
    await app.evaluate((v) => localStorage.setItem("holdmap.theme", v), theme);
    await app.reload();
    await expect(app.locator("html")).toHaveAttribute("data-theme", theme);
    await openAgents(app);
    await agentHeading(app, "Claude Code").click();
    const { violations } = await new AxeBuilder({ page: app }).include("#agent-details").withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).disableRules(["label-content-name-mismatch"]).analyze();
    expect(violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
  });
}

test("390px: the agent details fit the bottom sheet without horizontal overflow", async ({ app }) => {
  await app.setViewportSize({ width: 390, height: 844 });
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  expect(await agentDetails(app).evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  expect(await app.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
});

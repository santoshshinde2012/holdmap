import { agentCard, agentHeading, expect, openAgents, test } from "./fixtures";
import type { Page } from "@playwright/test";

const cursorMcp = (name: string) => `Inspect MCP server ${name}, Project scope. Agents: Cursor (PID 52000). Project: /Users/dev/code/shop-web. Source: /Users/dev/code/shop-web/.cursor/mcp.json`;
const frontendSkill = "Inspect skill frontend-checks, Project scope. Agents: Cursor (PID 52000). Project: /Users/dev/code/shop-web. Source: /Users/dev/code/shop-web/.agents/skills/frontend-checks/SKILL.md";
const claudeFilesystem = "Inspect MCP server filesystem, Project scope. Agents: Claude Code (PID 51200). Project: /Users/dev/code/shop-api. Source: /Users/dev/code/shop-api/.mcp.json";

async function openCapabilities(page: Page) {
  const search = await openAgents(page);
  const workspace = page.getByRole("radio", { name: "MCP & skills", exact: true });
  await workspace.click();
  await expect(workspace).toHaveAttribute("aria-checked", "true");
  await expect(page.getByRole("region", { name: "MCP and skills workspace", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Inspect capabilities for Cursor, PID 52000", exact: true }).click();
  return search;
}

test("capability workspace connects declared sources to projects and keeps runtime evidence separate", async ({ app }) => {
  await openCapabilities(app);
  await app.getByRole("button", { name: cursorMcp("filesystem"), exact: true }).click();
  const relation = app.getByRole("region", { name: "Declared capability relationship", exact: true });
  await expect(relation).toContainText("mcp.json");
  await expect(relation).toContainText("filesystem");
  await expect(relation).toContainText("shop-web");
  await expect(relation).toContainText("not proof of use");
  const details = app.getByRole("complementary", { name: "Capability details", exact: true });
  await expect(details).toContainText("/Users/dev/code/shop-web/.cursor/mcp.json");
  await expect(details).toContainText("STDIO");
  await expect(details).toContainText("Enabled in config");
  await expect(details).toContainText("Configured does not mean connected, loaded or invoked");
  await app.getByRole("button", { name: "Inspect running process Filesystem MCP, PID 52070", exact: true }).click();
  await expect(app.getByRole("region", { name: "Observed process relationship", exact: true })).toBeVisible();
  await expect(details).toContainText("has not been matched to a configuration entry or invocation");
  await expect(details).toContainText("inferred");
  await expect(details).not.toContainText("Source file");
});

test("skill purpose and source are searchable and recorded usage requires explicit navigation", async ({ app }) => {
  const search = await openCapabilities(app);
  await search.fill("accessibility");
  await expect(app.getByRole("button", { name: frontendSkill, exact: true })).toBeVisible();
  await app.getByRole("button", { name: frontendSkill, exact: true }).click();
  const details = app.getByRole("complementary", { name: "Capability details", exact: true });
  await expect(details).toContainText("Check UI behavior, accessibility and responsive layouts before shipping");
  await expect(details).toContainText("frontend-checks/SKILL.md");
  await expect(details).toContainText("does not prove this skill was loaded or activated");
  await expect(details).toContainText("skill instructions are not collected or executed");
  await app.getByRole("button", { name: "Open recorded calls", exact: true }).click();
  await expect(app.getByRole("radio", { name: "Flow", exact: true })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByRole("radio", { name: "Calls", exact: true })).toHaveAttribute("aria-checked", "true");
});

test("enablement filters use declared state while runtime candidates stay independent", async ({ app }) => {
  await openCapabilities(app);
  const enablement = app.getByRole("combobox", { name: /^State:/ });
  await enablement.click();
  await app.getByRole("option", { name: "Disabled MCP", exact: true }).click();
  await expect(app.getByRole("button", { name: /^Inspect MCP server/ })).toHaveCount(1);
  await expect(app.getByRole("button", { name: cursorMcp("legacy-browser"), exact: true })).toBeVisible();
  await expect(app.getByRole("button", { name: /^Inspect skill/ })).toHaveCount(0);
  await expect(app.getByRole("button", { name: "Inspect running process Filesystem MCP, PID 52070", exact: true })).toBeVisible();
  await app.getByRole("radio", { name: /^Skills/ }).click();
  await expect(enablement).toBeDisabled();
  await expect(app.getByRole("button", { name: /^Inspect skill/ })).toHaveCount(2);
  await expect(app.getByRole("button", { name: /^Inspect MCP server/ })).toHaveCount(0);
  await app.getByRole("button", { name: "Inspect capabilities for Claude Code, PID 51200", exact: true }).click();
  await app.getByRole("radio", { name: /^MCP servers/ }).click();
  await app.getByRole("button", { name: claudeFilesystem, exact: true }).click();
  await expect(app.getByRole("complementary", { name: "Capability details", exact: true })).toContainText("Enablement unknown");
});

test("unsupported sources and no-match recovery preserve bounded visibility claims", async ({ app }) => {
  const search = await openCapabilities(app);
  await app.getByRole("button", { name: "Inspect capabilities for Docker Desktop, PID 48000", exact: true }).click();
  await expect(app.getByRole("heading", { name: "Inventory not inspected", exact: true })).toBeVisible();
  const docker = app.getByRole("button", { name: "Inspect capabilities for Docker Desktop, PID 48000", exact: true });
  await expect(docker).toContainText("Inventory not inspected");
  await expect(docker).not.toContainText("0 MCP");
  await expect(app.getByText("Capability source discovery is not supported for Docker Desktop.", { exact: false })).toBeVisible();
  await expect(app.getByText(/Missing entries do not prove an agent has no capabilities/)).toBeVisible();
  await app.getByRole("button", { name: "Inspect capabilities for Cursor, PID 52000", exact: true }).click();
  await search.fill("no-such-capability");
  await expect(app.getByRole("heading", { name: "No inventory matches", exact: true })).toBeVisible();
  await app.getByRole("button", { name: "Clear search", exact: true }).first().click();
  await expect(search).toHaveValue("");
  await expect(app.getByRole("button", { name: /^Inspect skill/ })).toHaveCount(2);
});

test("agent-card entry opens the same opt-in workspace without a default inventory scan", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  const entry = agentCard(app, "Cursor").getByRole("button", { name: /MCP & skills.*inspect sources/ });
  await entry.click();
  await expect(app.getByRole("radio", { name: "MCP & skills", exact: true })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByRole("button", { name: "Inspect capabilities for Cursor, PID 52000", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(app.getByRole("button", { name: /^Inspect MCP server/ })).toHaveCount(3);
});

for (const theme of ["dark", "light"]) {
  test(`capability details remain reachable by keyboard in a narrow ${theme} layout`, async ({ app }) => {
    await app.setViewportSize({ width: 390, height: 844 });
    await app.evaluate((theme) => localStorage.setItem("holdmap.theme", theme), theme);
    await app.reload();
    await openCapabilities(app);
    const skill = app.getByRole("button", { name: frontendSkill, exact: true });
    await skill.focus();
    await skill.press("Enter");
    const details = app.getByRole("complementary", { name: "Capability details", exact: true });
    await expect(details.getByRole("heading", { name: "frontend-checks", exact: true })).toBeVisible();
    await expect(details).toContainText("frontend-checks/SKILL.md");
    expect(await app.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    expect(await app.locator("#agent-capabilities").evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
    expect(await details.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
  });
}

test("same-named MCP entries remain distinguishable by agent and complete source path", async ({ app }) => {
  await openCapabilities(app);
  await app.getByRole("button", { name: /^All agents Shared definitions shown once$/ }).click();
  await expect(app.getByRole("button", { name: /^Inspect MCP server filesystem, Project scope\./ })).toHaveCount(2);
  const details = app.getByRole("complementary", { name: "Capability details", exact: true });
  for (const [name, source] of [[claudeFilesystem, "/Users/dev/code/shop-api/.mcp.json"], [cursorMcp("filesystem"), "/Users/dev/code/shop-web/.cursor/mcp.json"]]) {
    const entry = app.getByRole("button", { name, exact: true });
    await entry.focus();
    await entry.press("Enter");
    await expect(entry).toHaveAttribute("aria-pressed", "true");
    await expect(details).toContainText(source);
  }
});

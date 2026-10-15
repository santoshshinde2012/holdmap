import { agentCard, agentHeading, expect, openAgents, test } from "./fixtures";
import type { Page } from "@playwright/test";

const cursorMcp = (name: string) => `Inspect MCP server ${name}, Project scope. Agents: Cursor (PID 52000). Project: /Users/dev/code/shop-web. Source: /Users/dev/code/shop-web/.cursor/mcp.json`;
const frontendSkill = "Inspect skill frontend-checks, Project scope. Agents: Cursor (PID 52000). Project: /Users/dev/code/shop-web. Source: /Users/dev/code/shop-web/.agents/skills/frontend-checks/SKILL.md";
const claudeFilesystem = "Inspect MCP server filesystem, Project scope. Agents: Claude Code (PID 51200). Project: /Users/dev/code/shop-api. Source: /Users/dev/code/shop-api/.mcp.json";
const details = (page: Page) => page.getByRole("complementary", { name: "Capability details", exact: true });
const graph = (page: Page) => page.getByRole("region", { name: "Agent capability graph", exact: true });

async function openCapabilities(page: Page) {
  const search = await openAgents(page);
  const workspace = page.getByRole("radio", { name: "MCP & skills", exact: true });
  await workspace.click();
  await expect(workspace).toHaveAttribute("aria-checked", "true");
  await expect(graph(page)).toBeVisible();
  return search;
}
async function scopeCursor(page: Page) {
  const browse = page.getByRole("button", { name: "Browse agents", exact: true });
  const rail = page.getByRole("button", { name: "Inspect agent card Cursor, PID 52000", exact: true });
  if (!await rail.isVisible()) await browse.click();
  await rail.click();
  await expect(details(page).getByRole("heading", { name: "Cursor", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Close capability details", exact: true }).click();
  await expect(details(page)).toHaveCount(0);
}

test("graph starts without an inspector and real pointer clicks reveal the chosen definition", async ({ app }) => {
  await openCapabilities(app);
  await expect(details(app)).toHaveCount(0);
  await expect(app.getByRole("region", { name: "Visibility limits", exact: true })).toHaveCount(0);
  await expect(app.getByRole("combobox", { name: /^Scope:/ })).toHaveCount(0);
  const bounds = await graph(app).boundingBox();
  expect(bounds!.width).toBeGreaterThan(1440 * .6);
  // A native pointer click is essential: programmatic activation would miss a
  // focus handler that moves the viewport between pointerdown and mouseup.
  const card = app.getByRole("button", { name: cursorMcp("filesystem"), exact: true });
  await expect(card).toBeVisible();
  await card.click();
  await expect(details(app)).toBeVisible();
  await expect(details(app).getByRole("heading", { name: "filesystem", exact: true })).toBeVisible();
  await expect(details(app)).toContainText("/Users/dev/code/shop-web/.cursor/mcp.json");
  await expect(details(app)).toContainText("STDIO");
  await expect(details(app)).toContainText("Configured does not mean connected, loaded or invoked");
  await app.getByRole("button", { name: "Close capability details", exact: true }).click();
  await expect(details(app)).toHaveCount(0);
  await expect(card).toBeFocused();
});

test("agent graph clicks scope the map and local Escape preserves that scope and focus", async ({ app }) => {
  await openCapabilities(app);
  const cursor = graph(app).getByRole("button", { name: "Inspect capabilities for Cursor, PID 52000", exact: true });
  await cursor.click();
  await expect(details(app).getByRole("heading", { name: "Cursor", exact: true })).toBeVisible();
  await expect(graph(app).getByRole("button", { name: /^Inspect MCP server/ })).toHaveCount(3);
  await app.keyboard.press("Escape");
  await expect(details(app)).toHaveCount(0);
  await expect(cursor).toBeFocused();
  await expect(app.getByRole("radio", { name: "MCP & skills", exact: true })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByRole("button", { name: "Show all", exact: true })).toBeVisible();
  await app.getByRole("button", { name: "Show all", exact: true }).click();
  await expect(graph(app).getByRole("button", { name: /^Inspect capabilities for/ })).toHaveCount(3);
  await expect(details(app)).toHaveCount(0);
});

test("skill purpose/source are searchable and recorded usage requires explicit navigation", async ({ app }) => {
  const search = await openCapabilities(app);
  await search.fill("accessibility");
  const skill = app.getByRole("button", { name: frontendSkill, exact: true });
  await skill.click();
  await expect(details(app)).toBeVisible();
  await expect(details(app)).toContainText("Check UI behavior, accessibility and responsive layouts before shipping");
  await expect(details(app)).toContainText("frontend-checks/SKILL.md");
  await expect(details(app)).toContainText("does not prove this skill was loaded or activated");
  await expect(details(app)).toContainText("skill instructions are not collected or executed");
  await expect(graph(app).getByRole("button", { name: "Inspect capabilities for Cursor, PID 52000", exact: true })).toBeInViewport({ ratio: .99 });
  await app.getByRole("button", { name: "Open recorded calls", exact: true }).click();
  await expect(app.getByRole("radio", { name: "Flow", exact: true })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByRole("radio", { name: "Calls", exact: true })).toHaveAttribute("aria-checked", "true");
});

test("Filters are optional and Running stays separate from disabled definitions", async ({ app }) => {
  await openCapabilities(app); await scopeCursor(app);
  await app.getByRole("button", { name: "Filters", exact: true }).click();
  const scope = app.getByRole("combobox", { name: /^Scope:/ });
  const enablement = app.getByRole("combobox", { name: /^State:/ });
  await enablement.click(); await app.getByRole("option", { name: "Disabled MCP", exact: true }).click();
  await expect(graph(app).getByRole("button", { name: /^Inspect MCP server/ })).toHaveCount(1);
  await expect(app.getByRole("button", { name: cursorMcp("legacy-browser"), exact: true })).toBeVisible();
  const runtime = app.getByRole("button", { name: "Inspect running process Filesystem MCP, PID 52070", exact: true });
  await expect(runtime).toBeVisible();
  await app.getByRole("radio", { name: /^Running/ }).click();
  await expect(app.getByRole("combobox", { name: /^Scope:/ })).toHaveCount(0);
  await app.getByRole("button", { name: "Filters", exact: true }).click();
  await expect(scope).toBeDisabled(); await expect(enablement).toBeDisabled();
  await expect(graph(app).getByRole("button", { name: /^Inspect MCP server/ })).toHaveCount(0);
  await expect(graph(app).getByRole("button", { name: /^Inspect skill/ })).toHaveCount(0);
  await runtime.click();
  await expect(details(app)).toBeVisible();
  await expect(details(app)).toContainText("has not been matched to a configuration entry or invocation");
  await expect(details(app)).toContainText("inferred");
  await expect(details(app)).not.toContainText("Source file");
  await app.keyboard.press("Escape");
  await expect(details(app)).toHaveCount(0);
  await expect(runtime).toBeFocused();
  await expect(app.getByRole("combobox", { name: /^Scope:/ })).toHaveCount(0);
});

test("empty agents keep their graph card and compact recovery without expanded coverage", async ({ app }) => {
  await openCapabilities(app);
  await graph(app).getByRole("button", { name: "Inspect capabilities for Docker Desktop, PID 48000", exact: true }).click();
  await expect(details(app)).toBeVisible();
  await expect(details(app)).toContainText("Inventory not inspected");
  await expect(details(app)).not.toContainText("0MCP definitions");
  await app.getByRole("button", { name: "Close capability details", exact: true }).click();
  await expect(details(app)).toHaveCount(0);
  await expect(graph(app).getByRole("button", { name: "Inspect capabilities for Docker Desktop, PID 48000", exact: true })).toBeVisible();
  await expect(app.getByRole("heading", { name: "Inventory not inspected", exact: true })).toBeVisible();
  await expect(app.getByRole("region", { name: "Visibility limits", exact: true })).toHaveCount(0);
  const coverage = app.getByRole("button", { name: "Coverage", exact: true });
  await coverage.click();
  await expect(app.getByRole("region", { name: "Visibility limits", exact: true })).toContainText("not supported for Docker Desktop");
  await app.keyboard.press("Escape");
  await expect(coverage).toBeFocused();
  await expect(app.getByRole("radio", { name: "MCP & skills", exact: true })).toHaveAttribute("aria-checked", "true");
});

test("no-match recovery never selects a replacement card automatically", async ({ app }) => {
  const search = await openCapabilities(app);
  await app.getByRole("button", { name: cursorMcp("filesystem"), exact: true }).click();
  await search.focus();
  await search.pressSequentially("no-such-capability");
  await expect(search).toHaveValue("no-such-capability");
  await expect(search).toBeFocused();
  await expect(details(app)).toHaveCount(0);
  await expect(app.getByRole("heading", { name: "No inventory matches", exact: true })).toBeVisible();
  await app.getByRole("region", { name: "MCP and skills workspace", exact: true }).getByRole("button", { name: "Clear search", exact: true }).click();
  await expect(search).toHaveValue("");
  await expect(details(app)).toHaveCount(0);
  await expect(graph(app).getByRole("button", { name: /^Inspect skill/ }).first()).toBeVisible();
});

test("Overview entry opens the opt-in graph in the same agent scope without an automatic inspector", async ({ app }) => {
  await openAgents(app); await agentHeading(app, "Cursor").click();
  await agentCard(app, "Cursor").getByRole("button", { name: /MCP & skills.*inspect sources/ }).click();
  await expect(app.getByRole("radio", { name: "MCP & skills", exact: true })).toHaveAttribute("aria-checked", "true");
  await expect(graph(app)).toBeVisible();
  await expect(graph(app).getByRole("button", { name: /^Inspect MCP server/ })).toHaveCount(3);
  await expect(details(app)).toHaveCount(0);
});

for (const [theme, key, width] of [["dark", "Enter", 390], ["light", "Space", 390], ["dark", "Enter", 820]] as const) {
  test(`${width}px ${theme} map supports native ${key}, inspector dismissal and the agent browser`, async ({ app }) => {
    await app.setViewportSize({ width, height: 844 });
    await app.evaluate((theme) => localStorage.setItem("holdmap.theme", theme), theme); await app.reload();
    await expect(app.locator("html")).toHaveAttribute("data-theme", theme);
    await openCapabilities(app);
    const agent = graph(app).getByRole("button", { name: /^Inspect capabilities for/ }).first();
    await expect.poll(() => agent.evaluate((element) => {
      const viewport = element.closest(".svelte-flow__viewport")!;
      return new DOMMatrix(getComputedStyle(viewport).transform).a * parseFloat(getComputedStyle(element.querySelector("strong")!).fontSize);
    })).toBeGreaterThanOrEqual(10);
    await scopeCursor(app);
    const skill = app.getByRole("button", { name: frontendSkill, exact: true });
    await skill.focus(); await skill.press(key);
    await expect(details(app)).toBeVisible();
    await expect(details(app).getByRole("heading", { name: "frontend-checks", exact: true })).toBeVisible();
    await expect(details(app)).toContainText("frontend-checks/SKILL.md");
    await app.keyboard.press("Escape");
    await expect(details(app)).toHaveCount(0); await expect(skill).toBeFocused();
    await expect.poll(() => skill.evaluate((element) => {
      const viewport = element.closest(".svelte-flow__viewport")!;
      return new DOMMatrix(getComputedStyle(viewport).transform).a * parseFloat(getComputedStyle(element.querySelector("strong")!).fontSize);
    })).toBeGreaterThanOrEqual(10);
    const browse = app.getByRole("button", { name: "Browse agents", exact: true });
    await browse.click();
    await expect(app.getByRole("button", { name: /^All agents/ })).toBeFocused();
    await app.getByRole("button", { name: "Close agent browser", exact: true }).click();
    await expect(browse).toBeFocused();
    expect(await app.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    expect(await app.locator("#agent-capabilities").evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
  });
}

test("same aliases at different sources remain distinct graph cards with complete accessible context", async ({ app }) => {
  await openCapabilities(app);
  await expect(graph(app).getByRole("button", { name: /^Inspect MCP server filesystem, Project scope\./ })).toHaveCount(2);
  for (const [name, source] of [[claudeFilesystem, "/Users/dev/code/shop-api/.mcp.json"], [cursorMcp("filesystem"), "/Users/dev/code/shop-web/.cursor/mcp.json"]]) {
    const card = app.getByRole("button", { name, exact: true });
    await card.focus(); await card.press("Enter");
    await expect(details(app)).toBeVisible(); await expect(details(app)).toContainText(source);
    await expect(card).toHaveAttribute("aria-pressed", "true");
    await app.keyboard.press("Escape"); await expect(card).toBeFocused();
  }
});

test("mobile inspector and agent browser reveal controls when keyboard focus leaves them", async ({ app }) => {
  await app.setViewportSize({ width: 390, height: 844 });
  await openCapabilities(app);
  const card = graph(app).getByRole("button", { name: /^Inspect skill frontend-checks/ });
  await card.press("Enter");
  const inspector = details(app);
  await expect(inspector).toBeVisible();
  await inspector.getByRole("button", { name: "Open recorded calls", exact: true }).focus();
  await app.keyboard.press("Tab");
  const browse = app.getByRole("button", { name: "Browse agents", exact: true });
  await expect(inspector).toHaveCount(0);
  await expect(browse).toBeFocused();
  await browse.click();
  await expect(app.getByRole("button", { name: /^All agents/ })).toBeFocused();
  await app.keyboard.press("Shift+Tab");
  await expect(app.getByRole("button", { name: "Close agent browser", exact: true })).toBeFocused();
  await app.keyboard.press("Shift+Tab");
  await expect(app.getByRole("complementary", { name: "Capability agents", exact: true })).toBeHidden();
  await expect(app.getByRole("button", { name: "Recorded calls", exact: true })).toBeFocused();
});

for (const theme of ["dark", "light"] as const) {
  test(`${theme} capability cards stay readable and nonmodal disclosures dismiss without stealing focus`, async ({ app }) => {
    await app.evaluate((theme) => localStorage.setItem("holdmap.theme", theme), theme);
    await app.reload();
    await expect(app.locator("html")).toHaveAttribute("data-theme", theme);
    const search = await openCapabilities(app);
    await graph(app).getByRole("button", { name: cursorMcp("filesystem"), exact: true }).click();
    await app.mouse.move(0, 0);
    const unrelated = graph(app).locator("button.card.dim");
    await expect.poll(() => unrelated.count()).toBeGreaterThan(0);
    expect(await unrelated.evaluateAll((cards) => cards.every((card) => getComputedStyle(card).opacity === "1"))).toBe(true);
    await app.getByRole("button", { name: "Coverage", exact: true }).click();
    const coverage = app.getByRole("region", { name: "Visibility limits", exact: true });
    await expect(coverage).toBeVisible();
    await app.getByRole("button", { name: "Close coverage", exact: true }).focus();
    await app.keyboard.press("Shift+Tab");
    await expect(coverage).toHaveCount(0);
    await app.getByRole("button", { name: "Filters", exact: true }).click();
    await expect(app.getByRole("combobox", { name: /^Scope:/ })).toBeFocused();
    await search.click();
    await expect(search).toBeFocused();
    await expect(app.getByRole("combobox", { name: /^Scope:/ })).toHaveCount(0);
    await app.getByRole("button", { name: "Coverage", exact: true }).click();
    await expect(coverage).toBeVisible();
    await app.getByRole("button", { name: "Coverage", exact: true }).click();
    await expect(coverage).toHaveCount(0);
  });
}

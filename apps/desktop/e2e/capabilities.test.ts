import { agentCard, agentHeading, expect, openAgents, test } from "./fixtures";
import type { Page } from "@playwright/test";

const cursorMcp = (name: string) => `Inspect MCP server ${name}, Project scope. Agents: Cursor (PID 52000). Project: /Users/dev/code/shop-web. Source: /Users/dev/code/shop-web/.cursor/mcp.json`;
const frontendSkill = "Inspect skill frontend-checks, Project scope. Agents: Cursor (PID 52000). Project: /Users/dev/code/shop-web. Source: /Users/dev/code/shop-web/.agents/skills/frontend-checks/SKILL.md";
const claudeFilesystem = "Inspect MCP server filesystem, Project scope. Agents: Claude Code (PID 51200). Project: /Users/dev/code/shop-api. Source: /Users/dev/code/shop-api/.mcp.json";
const workspace = (page: Page) => page.getByRole("region", { name: "MCP and skills workspace", exact: true });
const details = (page: Page) => page.getByRole("complementary", { name: "Capability details", exact: true });
const graph = (page: Page) => page.getByRole("region", { name: "Agent capability graph", exact: true });
const view = (page: Page, name: "Explore" | "Map" | "Compare") => workspace(page).getByRole("radiogroup", { name: "Capability view", exact: true }).getByRole("radio", { name, exact: true });
const workspaceChunk = /\/AgentCapabilitiesView-[^/]+\.js(?:\?|$)/;

async function openCapabilities(page: Page) {
  const search = await openAgents(page);
  const tab = page.getByRole("radio", { name: "MCP & skills", exact: true });
  await tab.click();
  await expect(tab).toHaveAttribute("aria-checked", "true");
  await expect(view(page, "Explore")).toHaveAttribute("aria-checked", "true");
  return search;
}
async function scopeCursor(page: Page) {
  const context = page.getByRole("button", { name: /^Explore context Cursor, 1 observed process/ });
  if (!await context.isVisible()) await page.getByRole("button", { name: "Browse agents", exact: true }).click();
  await context.click();
  await expect(details(page)).toHaveCount(0);
}

test("Overview defers the capability workspace chunk until explicit MCP and skills navigation", async ({ app }) => {
  const requests: string[] = [];
  app.on("request", (request) => { if (workspaceChunk.test(request.url())) requests.push(request.url()); });
  await openAgents(app);
  await expect(agentCard(app, "Cursor")).toBeVisible();
  expect(await app.evaluate(() => performance.getEntriesByType("resource").map((entry) => entry.name).filter((name) => /\/AgentCapabilitiesView-[^/]+\.js(?:\?|$)/.test(name)))).toEqual([]);
  expect(requests).toEqual([]);
  await app.getByRole("radio", { name: "MCP & skills", exact: true }).click();
  await expect(workspace(app).getByRole("button", { name: frontendSkill, exact: true })).toBeVisible();
  expect(requests).toHaveLength(1);
  await app.getByRole("radio", { name: "Overview", exact: true }).click();
  await expect(agentCard(app, "Cursor")).toBeVisible();
  await app.getByRole("radio", { name: "MCP & skills", exact: true }).click();
  await expect(workspace(app).getByRole("button", { name: frontendSkill, exact: true })).toBeVisible();
  expect(requests).toHaveLength(1);
});

test("failed capability chunk reports a recoverable error and Reload app retries in a fresh document", async ({ app }) => {
  let attempts = 0;
  await app.route(workspaceChunk, (route) => { attempts++; return route.abort("failed"); });
  await openAgents(app);
  await app.getByRole("radio", { name: "MCP & skills", exact: true }).click();
  const failure = app.getByRole("alert").filter({ hasText: "Couldn't load MCP & skills" });
  await expect(failure).toBeVisible();
  await expect(workspace(app)).toHaveCount(0);
  await expect(failure.getByRole("button", { name: "Try again", exact: true })).toBeVisible();
  // Vite's module preload and the import itself can each request a failed file.
  expect(attempts).toBeGreaterThan(0);
  await app.unroute(workspaceChunk);
  await Promise.all([app.waitForEvent("domcontentloaded"), failure.getByRole("button", { name: "Reload app", exact: true }).click()]);
  await expect(app.getByRole("listbox", { name: "Ports in use" }).getByRole("option").first()).toBeVisible();
  // The section preference survives reload; reopening Agents resumes MCP & skills.
  await app.getByRole("radio", { name: /^Agents/ }).click();
  await expect(view(app, "Explore")).toHaveAttribute("aria-checked", "true");
  await expect(workspace(app).getByRole("button", { name: frontendSkill, exact: true })).toBeVisible();
  await expect(failure).toHaveCount(0);
});

test("Explore starts with declared purpose cards and no automatic inspector or catalog claims", async ({ app }) => {
  await openCapabilities(app);
  await expect(details(app)).toHaveCount(0);
  await expect(graph(app)).toHaveCount(0);
  await expect(app.getByRole("region", { name: "Visibility limits", exact: true })).toHaveCount(0);
  await expect(app.getByRole("combobox", { name: /^Scope:/ })).toHaveCount(0);
  const skill = workspace(app).getByRole("button", { name: frontendSkill, exact: true });
  await expect(skill).toContainText("Check UI behavior, accessibility and responsive layouts");
  await expect(skill).toContainText("Skill file found");
  // Real pointer activation catches focus handlers moving a target between down/up.
  await skill.click();
  await expect(details(app).getByRole("heading", { name: "frontend-checks", exact: true })).toBeVisible();
  await expect(details(app).getByRole("tab", { name: "Purpose", exact: true })).toHaveAttribute("aria-selected", "true");
  await app.getByRole("button", { name: "Close capability details", exact: true }).click();
  await expect(details(app)).toHaveCount(0); await expect(skill).toBeFocused();
});

test("skill inspector separates declared purpose, exact source, evidence and unlinked activity", async ({ app }) => {
  const search = await openCapabilities(app);
  await search.fill("accessibility");
  await workspace(app).getByRole("button", { name: frontendSkill, exact: true }).click();
  const inspector = details(app);
  await expect(inspector).toContainText("Check UI behavior, accessibility and responsive layouts before shipping");
  await inspector.getByRole("tab", { name: "Purpose", exact: true }).focus();
  await app.keyboard.press("ArrowRight");
  await expect(inspector.getByRole("tab", { name: "Source", exact: true })).toBeFocused();
  await expect(inspector.getByRole("tabpanel", { name: "Source", exact: true })).toBeVisible();
  await expect(inspector).toContainText("/Users/dev/code/shop-web/.agents/skills/frontend-checks/SKILL.md");
  await inspector.getByRole("tab", { name: "Evidence", exact: true }).click();
  await expect(inspector).toContainText(/Loading & activation/);
  await expect(inspector).toContainText(/instructions.*not (?:collected|retained|executed)/i);
  await inspector.getByRole("tab", { name: "Activity", exact: true }).click();
  await expect(inspector).toContainText(/No activity association has been established/);
  await expect(app.getByRole("radio", { name: "MCP & skills", exact: true })).toHaveAttribute("aria-checked", "true");
  await inspector.getByRole("button", { name: "Open recorded calls", exact: true }).click();
  await expect(app.getByRole("radio", { name: "Flow", exact: true })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByRole("radio", { name: "Calls", exact: true })).toHaveAttribute("aria-checked", "true");
});

test("MCP definitions keep same aliases at distinct sources and never infer a tool catalog", async ({ app }) => {
  await openCapabilities(app);
  await workspace(app).getByRole("radio", { name: /^MCP servers/ }).click();
  await expect(workspace(app).getByRole("button", { name: /^Inspect MCP server filesystem, Project scope\./ })).toHaveCount(2);
  for (const [label, source] of [[claudeFilesystem, "/Users/dev/code/shop-api/.mcp.json"], [cursorMcp("filesystem"), "/Users/dev/code/shop-web/.cursor/mcp.json"]]) {
    const card = workspace(app).getByRole("button", { name: label, exact: true });
    await card.focus(); await card.press("Enter");
    await details(app).getByRole("tab", { name: "Source", exact: true }).click();
    await expect(details(app)).toContainText(source);
    await details(app).getByRole("tab", { name: "Evidence", exact: true }).click();
    await expect(details(app)).toContainText(/Connection & catalog/);
    await app.keyboard.press("Escape"); await expect(card).toBeFocused();
  }
  await workspace(app).getByRole("radio", { name: /^Tools/ }).click();
  await expect(workspace(app).getByRole("heading", { name: "Catalog not reported", exact: true })).toBeVisible();
  await expect(workspace(app)).toContainText(/does not report MCP tool catalogs or built-in actions/i);
  await expect(workspace(app)).not.toContainText("0 tools");
  expect(await app.evaluate(() => "__TAURI_INTERNALS__" in window)).toBe(false);
});

test("Map remains available and shares explicit capability selection with Explore", async ({ app }) => {
  await openCapabilities(app);
  await workspace(app).getByRole("button", { name: cursorMcp("filesystem"), exact: true }).click();
  await view(app, "Map").click();
  await expect(graph(app)).toBeVisible();
  await expect(details(app).getByRole("heading", { name: "filesystem", exact: true })).toBeVisible();
  await graph(app).getByRole("button", { name: frontendSkill, exact: true }).click();
  await expect(details(app).getByRole("heading", { name: "frontend-checks", exact: true })).toBeVisible();
  await view(app, "Explore").click();
  await expect(details(app).getByRole("heading", { name: "frontend-checks", exact: true })).toBeVisible();
  const selected = workspace(app).getByRole("button", { name: frontendSkill, exact: true });
  await app.getByRole("button", { name: "Close capability details", exact: true }).click();
  await expect(selected).toBeFocused(); await expect(details(app)).toHaveCount(0);
});

test("context selection scopes definitions while PID expansion keeps inspection explicit", async ({ app }) => {
  await openCapabilities(app); await scopeCursor(app);
  await expect(workspace(app).getByRole("button", { name: /^Inspect MCP server/ })).toHaveCount(3);
  await app.getByRole("button", { name: "Show processes for Cursor", exact: true }).click();
  const root = app.getByRole("button", { name: "Inspect agent card Cursor, PID 52000", exact: true });
  await root.click();
  await expect(details(app).getByRole("heading", { name: "Cursor", exact: true })).toBeVisible();
  await app.keyboard.press("Escape");
  await expect(details(app)).toHaveCount(0); await expect(root).toBeFocused();
  await expect(app.getByRole("radio", { name: "MCP & skills", exact: true })).toHaveAttribute("aria-checked", "true");
});

test("Map cards retain native Enter/Space activation and exact focus return", async ({ app }) => {
  await openCapabilities(app); await view(app, "Map").click();
  const card = graph(app).getByRole("button", { name: frontendSkill, exact: true });
  for (const key of ["Enter", "Space"]) {
    await card.focus(); await card.press(key);
    await expect(details(app).getByRole("heading", { name: "frontend-checks", exact: true })).toBeVisible();
    await app.keyboard.press("Escape");
    await expect(details(app)).toHaveCount(0); await expect(card).toBeFocused();
  }
});

test("Compare identifies exact-source differences without treating catalogs as empty", async ({ app }) => {
  await openCapabilities(app); await view(app, "Compare").click();
  const left = workspace(app).getByRole("combobox", { name: /^Left(?: context)?:/ });
  const right = workspace(app).getByRole("combobox", { name: /^Right(?: context)?:/ });
  await left.click(); await app.getByRole("option", { name: /^Claude Code/ }).click();
  await right.click(); await app.getByRole("option", { name: /^Cursor/ }).click();
  await expect(workspace(app)).toContainText("/Users/dev/code/shop-api/.mcp.json");
  await expect(workspace(app)).toContainText("/Users/dev/code/shop-web/.cursor/mcp.json");
  const comparison = workspace(app).getByRole("region", { name: "Capability comparison", exact: true });
  const aliases = comparison.getByRole("button", { name: /^Inspect compared MCP server filesystem, / });
  await expect(aliases).toHaveCount(2);
  for (const alias of await aliases.all()) {
    await expect(alias).toContainText("Definition observed");
    await expect(alias).toContainText("Not reported here");
  }
  await expect(workspace(app)).toContainText(/not reported|unknown/i);
  await expect(workspace(app).getByText("Not installed", { exact: true })).toHaveCount(0);
  await expect(workspace(app).getByText("Unused", { exact: true })).toHaveCount(0);
  await expect(workspace(app)).not.toContainText("0 tools");
});

test("definition filters never turn runtime candidates into connected or callable tools", async ({ app }) => {
  await openCapabilities(app); await scopeCursor(app);
  await workspace(app).getByRole("button", { name: "Filters", exact: true }).click();
  const enablement = app.getByRole("combobox", { name: /^State:/ });
  await enablement.click(); await app.getByRole("option", { name: "Disabled MCP", exact: true }).click();
  await expect(workspace(app).getByRole("button", { name: /^Inspect MCP server/ })).toHaveCount(1);
  await expect(workspace(app).getByRole("button", { name: cursorMcp("legacy-browser"), exact: true })).toBeVisible();
  await workspace(app).getByRole("radio", { name: /^Running/ }).click();
  const runtime = workspace(app).getByRole("button", { name: "Inspect running process Filesystem MCP, PID 52070", exact: true });
  await runtime.click();
  await details(app).getByRole("tab", { name: "Evidence", exact: true }).click();
  await expect(details(app)).toContainText(/has not been matched to a configuration entry or invocation/);
  await expect(details(app)).toContainText("inferred");
  await details(app).getByRole("tab", { name: "Source", exact: true }).click();
  await expect(details(app)).not.toContainText("Source file");
  await app.keyboard.press("Escape"); await expect(runtime).toBeFocused();
});

test("uninspected context stays recoverable and shows coverage only on request", async ({ app }) => {
  await openCapabilities(app);
  await app.getByRole("button", { name: /^Explore context Docker Desktop, 1 observed process/ }).click();
  await expect(details(app)).toHaveCount(0);
  await expect(workspace(app)).toContainText(/(?:inventory|sources) not inspected/i);
  await expect(app.getByRole("region", { name: "Visibility limits", exact: true })).toHaveCount(0);
  const coverage = workspace(app).getByRole("button", { name: "Coverage", exact: true });
  await coverage.click();
  await expect(app.getByRole("region", { name: "Visibility limits", exact: true })).toContainText("not supported for Docker Desktop");
  await app.keyboard.press("Escape"); await expect(coverage).toBeFocused();
});

test("typing search closes disappearing selection without stealing focus or replacing it", async ({ app }) => {
  const search = await openCapabilities(app);
  await workspace(app).getByRole("button", { name: cursorMcp("filesystem"), exact: true }).click();
  await search.click(); await search.pressSequentially("no-such-capability");
  await expect(search).toHaveValue("no-such-capability"); await expect(search).toBeFocused();
  await expect(details(app)).toHaveCount(0);
  await expect(workspace(app).getByRole("heading", { name: /No .*match/ })).toBeVisible();
  await workspace(app).getByRole("button", { name: "Clear search", exact: true }).click();
  await expect(search).toHaveValue(""); await expect(details(app)).toHaveCount(0);
  await expect(workspace(app).getByRole("button", { name: frontendSkill, exact: true })).toBeVisible();
});

test("Overview entry keeps requested agent scope without automatically opening details", async ({ app }) => {
  await openAgents(app); await agentHeading(app, "Cursor").click();
  await agentCard(app, "Cursor").getByRole("button", { name: /MCP & skills.*inspect sources/ }).click();
  await expect(view(app, "Explore")).toHaveAttribute("aria-checked", "true");
  await expect(workspace(app).getByRole("button", { name: /^Inspect MCP server/ })).toHaveCount(3);
  await expect(details(app)).toHaveCount(0);
});

for (const [theme, key, width] of [["dark", "Enter", 390], ["light", "Space", 390], ["dark", "Enter", 820]] as const) {
  test(`${width}px ${theme} Explore supports native ${key}, mobile return and readable controls`, async ({ app }) => {
    await app.setViewportSize({ width, height: 844 });
    await app.evaluate((theme) => localStorage.setItem("holdmap.theme", theme), theme); await app.reload();
    await expect(app.locator("html")).toHaveAttribute("data-theme", theme);
    await openCapabilities(app);
    const skill = workspace(app).getByRole("button", { name: frontendSkill, exact: true });
    await skill.focus(); await skill.press(key);
    await expect(details(app).getByRole("heading", { name: "frontend-checks", exact: true })).toBeVisible();
    await details(app).getByRole("tab", { name: "Source", exact: true }).click();
    await expect(details(app)).toContainText("frontend-checks/SKILL.md");
    await app.keyboard.press("Escape");
    await expect(details(app)).toHaveCount(0); await expect(skill).toBeFocused();
    const browse = workspace(app).getByRole("button", { name: "Browse agents", exact: true });
    await browse.click();
    await expect(app.getByRole("button", { name: /^All (?:agents|contexts)/ })).toBeFocused();
    await app.getByRole("button", { name: "Close agent browser", exact: true }).click();
    await expect(browse).toBeFocused();
    expect(await app.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    expect(await workspace(app).evaluate((element) => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
    expect(await skill.evaluate((element) => parseFloat(getComputedStyle(element.querySelector(".card-title b")!).fontSize))).toBeGreaterThanOrEqual(10);
  });
}

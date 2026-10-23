import AxeBuilder from "@axe-core/playwright";
import { agentCard, agentHeading, expect, openAgents, test } from "./fixtures";
import type { Page } from "@playwright/test";

// One selection everywhere: list, Computer map node, Home radial map and ↑/↓ all select the same
// agent, its details open beside the map, and it survives tabs, pictures and a reload.
const details = (page: Page) => page.getByRole("complementary", { name: "Agent details", exact: true });
const flowNode = (page: Page, pid: number) => page.locator(`.svelte-flow__node[data-id="agent:${pid}"]`);

async function expectSelected(page: Page, name: string, pid: number) {
  await expect(details(page).getByRole("heading", { name, exact: true })).toBeVisible();
  await expect(agentCard(page, name)).toHaveClass(/\bon\b/);
  await expect(flowNode(page, pid).locator(".node")).toHaveClass(/\bsel\b/);
  await expect(page).toHaveURL(new RegExp(`#agent=agent%3A${pid}$`));
}

test("selecting from the list opens details beside the map without toggling off", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  await expectSelected(app, "Cursor", 52000);
  await agentHeading(app, "Cursor").click();
  await expect(details(app)).toBeVisible();
  const panel = (await details(app).boundingBox())!;
  expect(panel.width).toBeGreaterThanOrEqual(340);
  await details(app).getByRole("button", { name: "Close agent details" }).click();
  await expect(details(app)).toHaveCount(0);
  await expect(app).not.toHaveURL(/#agent=/);
});

test("a map node selects; Esc and an empty-canvas click clear it", async ({ app }) => {
  await openAgents(app);
  await flowNode(app, 51200).click();
  await expectSelected(app, "Claude Code", 51200);
  await app.locator(".svelte-flow__pane").click({ position: { x: 60, y: 320 } });
  await expect(details(app)).toHaveCount(0);
  await flowNode(app, 52000).click();
  await expectSelected(app, "Cursor", 52000);
  await app.keyboard.press("Escape");
  await expect(details(app)).toHaveCount(0);
});

test("↑/↓ moves the shared selection", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  await app.keyboard.press("ArrowDown");
  await expect(details(app).getByRole("heading", { name: "Cursor", exact: true })).toHaveCount(0);
  await expect(details(app).getByRole("heading", { level: 3 })).toHaveCount(1);
  await expect(app.locator(".rail .card.on > .head")).toBeFocused();
  await app.keyboard.press("ArrowUp");
  await expect(details(app).getByRole("heading", { name: "Cursor", exact: true })).toBeVisible();
});

test("the Home radial map shares the selection with Agents", async ({ app }) => {
  await app.evaluate(() => localStorage.setItem("holdmap.home.layout", "map"));
  await app.reload();
  await app.getByRole("radio", { name: /^Home/ }).click();
  const node = app.getByRole("group", { name: /^Machine map/ }).getByRole("button", { name: /^Claude Code, / });
  await node.click();
  await expect(node).toHaveAttribute("aria-pressed", "true");
  await openAgents(app);
  await expectSelected(app, "Claude Code", 51200);
});

test("the selection survives tabs, pictures and a reload via its link", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Claude Code").click();
  await app.getByRole("radio", { name: "MCP & skills", exact: true }).click();
  await app.getByRole("radio", { name: "Overview", exact: true }).click();
  await app.getByRole("radio", { name: "Footprint", exact: true }).click();
  await expect(details(app).getByRole("heading", { name: "Claude Code", exact: true })).toBeVisible();
  await app.getByRole("radio", { name: "Computer", exact: true }).click();
  await app.reload();
  await expectSelected(app, "Claude Code", 51200);
});

for (const t of ["light", "dark"] as const) {
  test(`${t}: the agent details panel passes axe`, async ({ app }) => {
    await app.evaluate((v) => localStorage.setItem("holdmap.theme", v), t);
    await app.reload();
    await expect(app.locator("html")).toHaveAttribute("data-theme", t);
    await openAgents(app);
    await agentHeading(app, "Cursor").click();
    await expect(details(app)).toBeVisible();
    const { violations } = await new AxeBuilder({ page: app }).include("#agent-details").withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).disableRules(["label-content-name-mismatch"]).analyze();
    expect(violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
  });
}

test("a narrow window stacks the agent list over its details instead of covering the map", async ({ app }) => {
  await app.setViewportSize({ width: 700, height: 800 });
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  await expect(details(app)).toBeVisible();
  const [rail, panel] = [(await app.locator(".rail").boundingBox())!, (await details(app).boundingBox())!];
  expect(panel.y).toBeGreaterThanOrEqual(rail.y + rail.height - 1);
  expect(panel.height).toBeGreaterThan(300);
  await details(app).getByRole("button", { name: "Close agent details" }).click();
  await expect(app.locator(".svelte-flow")).toBeVisible();
});

test("a mid-size window pans the selected node clear of the floating details panel", async ({ app }) => {
  await app.setViewportSize({ width: 1000, height: 700 });
  await openAgents(app);
  for (const [name, pid] of [["Cursor", 52000], ["Claude Code", 51200]] as const) {
    await agentHeading(app, name).click();
    await expect(details(app)).toBeVisible();
    const node = flowNode(app, pid).locator(".node");
    await expect.poll(async () => {
      const [n, d] = [await node.boundingBox(), await details(app).boundingBox()];
      if (!n || !d) return "missing";
      return n.x + n.width <= d.x || n.x >= d.x + d.width || n.y + n.height <= d.y || n.y >= d.y + d.height ? "clear" : "overlaps";
    }).toBe("clear");
  }
});

import { agentCard, agentHeading, expect, openAgents, test } from "./fixtures";

test("Agents shows aggregate resources and inferred MCP servers without listeners", async ({ app }) => {
  await openAgents(app);
  const cursor = agentCard(app, "Cursor");
  await expect(agentHeading(app, "Cursor")).toContainText("15 processes");
  await expect(agentHeading(app, "Cursor")).toContainText("5 tools");
  await expect(agentHeading(app, "Cursor")).toContainText("6.2% CPU");
  await expect(agentHeading(app, "Cursor")).toContainText("2.2 GB total memory");
  await expect(agentHeading(app, "Claude Code")).toContainText("3 processes");
  await expect(agentHeading(app, "Docker Desktop")).toContainText("6 processes");

  // An MCP server can exist with no socket; activating its graph node opens its owner.
  await app.getByRole("button", { name: /^MCP server Filesystem MCP,.*Opens its agent card$/ }).click();
  await expect(agentHeading(app, "Cursor")).toHaveAttribute("aria-pressed", "true");
  const filesystem = cursor.getByRole("region", { name: "Agent tools" }).getByRole("listitem").filter({ hasText: "Filesystem MCP" });
  await expect(filesystem).toContainText("MCP server · inferred · no listening port");
  await expect(filesystem).toContainText("52070");
  await expect(filesystem).toContainText("0.1% CPU");
  await expect(cursor).toContainText("+8 more processes");
});

const searches = [
  { query: "Claude Code", agent: "Claude Code", reason: "identity" },
  { query: "filesystem", agent: "Cursor", reason: "tool name" },
  { query: "design-system", agent: "Cursor", reason: "folder" },
  { query: "pid:52000", agent: "Cursor", reason: "root PID" },
  { query: "pid:51230", agent: "Claude Code", reason: "child PID" },
  { query: "52080", agent: "Cursor", reason: "an owned PID omitted from the visible process list" },
];

for (const { query, agent, reason } of searches) {
  test(`agent search finds ${reason}`, async ({ app }) => {
    const search = await openAgents(app);
    await search.fill(query);
    await expect(app.getByRole("article")).toHaveCount(1);
    await expect(agentCard(app, agent)).toBeVisible();
    await search.press("ArrowDown");
    await search.press("Enter");
    await expect(agentHeading(app, agent)).toHaveAttribute("aria-pressed", "true");
  });
}

test("port search finds both the listener owner and agents using it", async ({ app }) => {
  const search = await openAgents(app);
  await search.fill("port:3001");
  await expect(app.getByRole("article")).toHaveCount(2);
  await expect(agentCard(app, "Cursor")).toBeVisible();
  await expect(agentCard(app, "Claude Code")).toBeVisible();
});

test("no matches and exact PID search recover when Escape clears search", async ({ app }) => {
  const search = await openAgents(app);
  for (const query of ["definitely-no-agent", "4300"]) {
    await search.fill(query);
    await expect(app.getByText("No agents match this search", { exact: true })).toBeVisible();
    await expect(app.getByRole("article")).toHaveCount(0);
    await search.press("Escape");
    await expect(search).toHaveValue("");
    await expect(app.getByRole("article")).toHaveCount(3);
  }
});

test("parent and child navigation clear a filter hiding the destination", async ({ app }) => {
  const search = await openAgents(app);
  await search.fill("Claude Code");
  await agentCard(app, "Claude Code").getByRole("button", { name: "Cursor 52000", exact: true }).click();
  await expect(search).toHaveValue("");
  await expect(agentHeading(app, "Cursor")).toHaveAttribute("aria-pressed", "true");
  await agentCard(app, "Cursor").getByRole("button", { name: "Claude Code 51200", exact: true }).click();
  await expect(agentHeading(app, "Claude Code")).toHaveAttribute("aria-pressed", "true");
});

test("agent folder actions and stop-all planning preserve the current listeners", async ({ app }) => {
  await openAgents(app);
  await agentHeading(app, "Cursor").click();
  const cursor = agentCard(app, "Cursor");
  await cursor.getByRole("group", { name: "Open shop-web", exact: true }).getByRole("button", { name: "Editor", exact: true }).click();
  await expect(app.getByText("Opened in VS Code", { exact: true })).toBeVisible();
  await cursor.getByRole("button", { name: "Stop all (2)", exact: true }).click();
  const plan = app.getByRole("alertdialog");
  await expect(plan.getByRole("heading", { name: "Stop Cursor's 2 ports?", exact: true })).toBeVisible();
  await expect(plan.getByRole("button", { name: /^Stop 2 ports/ })).toBeVisible();
  await expect(plan).toContainText("Stop 2 ports Cursor started: :3000, :5173");
  await expect(plan.getByRole("list", { name: "Plan", exact: true }).getByRole("listitem")).toHaveCount(4);
  await expect(plan).toContainText("43000");
  await expect(plan).toContainText("45173");
  await expect(plan).toContainText("port 5173 is free");
  await plan.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(plan).toHaveCount(0);
  await expect(cursor).toContainText("2 stoppable");
  await app.getByRole("radio", { name: "List", exact: true }).click();
  await expect(app.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
  await expect(app.getByRole("option", { name: /^Port 5173 tcp,/ })).toBeVisible();
});

test("typing in agent search does not trigger port actions or filters", async ({ app }) => {
  // Retain a selected listener while changing views so shortcut protection is exercised.
  await app.getByRole("option", { name: /^Port 3000 tcp,/ }).click();
  const search = await openAgents(app);
  await expect(app.getByRole("dialog")).toHaveCount(0);
  await search.pressSequentially("claude");
  await search.press("Backspace");
  await search.press("Delete");
  await expect(search).toHaveValue("claud");
  await expect(app.getByRole("alertdialog")).toHaveCount(0);
  await expect(app.getByRole("radio", { name: /^Agents/ })).toHaveAttribute("aria-checked", "true");

  await app.getByRole("radio", { name: "List", exact: true }).click();
  await expect(app.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
  const filters = app.getByRole("toolbar", { name: "View and filters", exact: true });
  await expect(filters.getByRole("button", { name: /^Dev servers/ })).toHaveAttribute("aria-pressed", "false");
  await expect(filters.getByRole("button", { name: /^Exposed/ })).toHaveAttribute("aria-pressed", "false");
  await expect(app.getByRole("radio", { name: "Listening", exact: true })).toHaveAttribute("aria-checked", "true");
});

test("an agent map listener opens port details and can be dismissed", async ({ app }) => {
  await openAgents(app);
  const listener = app.getByRole("button", { name: /^Port shop-api, :3001.*Opens details$/ });
  await listener.click();
  const details = app.getByRole("dialog", { name: "Details for port 3001", exact: true });
  await expect(details).toBeVisible();
  await expect(details.getByRole("heading", { name: /^shop-api/ })).toBeVisible();
  await details.getByRole("tab", { name: "Process", exact: true }).click();
  await expect(details.getByRole("tabpanel", { name: "Process", exact: true })).toContainText("43001");
  await details.press("Escape");
  await expect(details).toHaveCount(0);
  await expect(app.getByRole("radio", { name: /^Agents/ })).toHaveAttribute("aria-checked", "true");
  await expect(listener).toBeVisible();
});

import { agentCard, agentHeading, expect, openAgents, test } from "./fixtures";

test("an analyzer turns on only after its permissions sheet, and runs on demand", async ({ app }) => {
  await app.getByRole("button", { name: "Settings", exact: true }).click();
  const settings = app.getByRole("dialog", { name: "Settings", exact: true });
  await settings.getByRole("tab", { name: "Analyzers", exact: true }).click();
  await expect(settings).toContainText("Browser demo: results are sample data");
  const osv = settings.getByRole("listitem").filter({ hasText: "OSV vulnerability lookup" });
  await expect(osv).toContainText("Off");
  await expect(osv.getByRole("button", { name: "Run now" })).toHaveCount(0);

  await osv.getByRole("button", { name: "Review and turn on…" }).click();
  const sheet = settings.getByRole("region", { name: /Turn on OSV vulnerability lookup/ });
  await expect(sheet).toContainText("It gets");
  await expect(sheet).toContainText("api.osv.dev");
  await expect(sheet.getByLabel("Exact request body")).toContainText('"ecosystem":"npm"');
  await expect(sheet).toContainText("is not pinned to a version, so it was not checked");
  await sheet.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(osv).toContainText("Off");

  await osv.getByRole("button", { name: "Review and turn on…" }).click();
  await settings.getByRole("button", { name: "Turn on OSV vulnerability lookup", exact: true }).click();
  await expect(osv).toContainText("On");
  await osv.getByRole("button", { name: "Run now", exact: true }).click();
  await expect(osv).toContainText("Last run");
  await osv.getByRole("button", { name: "Turn off", exact: true }).click();
  await expect(osv).toContainText("Off");
});

test("findings reach Needs you, the agent card and the Insights tab, and can be reviewed", async ({ app }) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  await expect(app.getByText(/Claude Code: SkillSpector found 1 high, 1 medium in release-check/)).toBeVisible();
  await expect(agentCard(app, "Claude Code").getByText("Analyzer findings:", { exact: false })).toBeAttached();

  await openAgents(app);
  await agentHeading(app, "Claude Code").click();
  const card = agentCard(app, "Claude Code");
  await expect(card.getByRole("heading", { name: "Insights", exact: true })).toBeVisible();
  await expect(card).toContainText("Open findings: 1 high, 1 medium");
  await card.getByRole("button", { name: "Open Insights", exact: true }).click();

  const tab = app.getByRole("region", { name: "Insights workspace" });
  await expect(tab.getByRole("heading", { level: 3 })).toContainText("Claude Code");
  await expect(tab).toContainText("Instruction Override");
  await expect(tab).toContainText("Risk score: 64 / 100");
  const high = tab.getByRole("listitem").filter({ hasText: "Instruction Override" });
  await high.getByRole("button", { name: "Mark reviewed", exact: true }).click();
  await expect(tab.getByRole("listitem").filter({ hasText: "Instruction Override" })).toHaveCount(0);
  await tab.getByRole("checkbox", { name: "Show reviewed and snoozed" }).check();
  await tab.getByRole("listitem").filter({ hasText: "Instruction Override" }).getByRole("button", { name: "Reopen", exact: true }).click();
  await expect(tab.getByRole("listitem").filter({ hasText: "Instruction Override" }).getByRole("button", { name: "Mark reviewed" })).toBeVisible();
});

test("the Insights tab is an agent workspace tab and covers agents that aren't running", async ({ app }) => {
  await openAgents(app);
  await app.getByRole("radio", { name: "Insights", exact: true }).click();
  const tab = app.getByRole("region", { name: "Insights workspace" });
  await expect(tab).toBeVisible();
  const list = tab.getByRole("navigation", { name: "Agents with insights" });
  await expect(list).toContainText("Claude Code");
  await expect(list).not.toContainText("Codex CLI");
  await tab.getByRole("button", { name: "Turn on inventory", exact: true }).click();
  await list.getByRole("button", { name: /^Codex CLI/ }).click();
  await expect(tab.getByRole("heading", { level: 3 })).toContainText("not running");
  await expect(tab).toContainText("Destructive Shell Command");
});

test("the browser demo simulates adding, approving and removing your own MCP analyzer", async ({ app }) => {
  await app.getByRole("button", { name: "Settings", exact: true }).click();
  const settings = app.getByRole("dialog", { name: "Settings", exact: true });
  await settings.getByRole("tab", { name: "Analyzers", exact: true }).click();
  await settings.getByRole("button", { name: "Add your own analyzer…" }).click();
  await settings.getByRole("radio", { name: "Manifest", exact: true }).click();
  const box = settings.getByLabel("Analyzer manifest (TOML)");
  await box.fill('schema = 1\nid = "nvidia.skillspector"\n');
  await settings.getByRole("button", { name: "Add analyzer", exact: true }).click();
  await expect(settings).toContainText("invalid manifest");
  await box.fill([
    "schema = 1", 'id = "example.mcp-scan"', 'name = "Example MCP Scan"', 'publisher = "Example"',
    'homepage = "https://example.com"', 'license = "MIT"', 'summary = "Scans a skill folder through a local MCP server."',
    "[runner]", 'kind = "mcp"', 'command = "example-mcp"', 'args = ["--stdio"]', 'per_subject = "skill"', 'tool = "scan_skill"',
    "[runner.tool_args]", 'path = "{subject.dir}"', "[permissions]", 'read = ["{subject.dir}"]', "spawns_mcp_servers = true",
    "[output]", 'format = "sarif"',
  ].join("\n"));
  await settings.getByRole("button", { name: "Add analyzer", exact: true }).click();
  const row = settings.getByRole("listitem").filter({ hasText: "Example MCP Scan" });
  await expect(row).toContainText("added by you");
  await expect(row).toContainText("MCP server");
  await row.getByRole("button", { name: "Review and turn on…" }).click();
  const sheet = settings.getByRole("region", { name: /Turn on Example MCP Scan/ });
  await expect(sheet).toContainText("starts it only when you run this analyzer");
  await expect(sheet).toContainText("calls tool scan_skill");
  await settings.getByRole("button", { name: "Turn on Example MCP Scan", exact: true }).click();
  await expect(row).toContainText("On");
  await expect(row.getByRole("checkbox", { name: "Also run when items change" })).toHaveCount(0);
  await row.getByRole("button", { name: "Run now", exact: true }).click();
  await expect(row).toContainText("Last run");
  await row.getByRole("button", { name: "Remove", exact: true }).click();
  await expect(settings.getByRole("listitem").filter({ hasText: "Example MCP Scan" })).toHaveCount(0);
});

test("any MCP server is added from a command, its tools listed and mapped, approved, run and explained as a graph", async ({ app }) => {
  await app.getByRole("button", { name: "Settings", exact: true }).click();
  const settings = app.getByRole("dialog", { name: "Settings", exact: true });
  await settings.getByRole("tab", { name: "Analyzers", exact: true }).click();
  await settings.getByRole("button", { name: "Add your own analyzer…" }).click();
  await settings.getByLabel("Id", { exact: true }).fill("local.skill-lint");
  await settings.getByLabel("Name", { exact: true }).fill("Skill lint (sample)");
  await settings.getByLabel("Command", { exact: true }).fill("/usr/local/bin/skill-lint-mcp");
  await settings.getByLabel("Arguments (one per line)").fill("--stdio");
  await settings.getByRole("button", { name: "Add and choose tools…" }).click();
  await settings.getByRole("button", { name: "List tools", exact: true }).click();
  const tools = settings.getByRole("region", { name: "Tools of Skill lint (sample)" });
  await expect(tools).toContainText("mentions credential or configuration files");
  await expect(tools.getByRole("checkbox", { name: "helper" })).not.toBeChecked();
  await expect(tools.getByLabel("Example arguments for list_files")).toContainText("scripts/run.sh");
  await tools.getByRole("button", { name: "Use the mapping suggested by list_files" }).click();
  await tools.getByRole("button", { name: "Save tools", exact: true }).click();

  const row = settings.getByRole("listitem").filter({ hasText: "Skill lint (sample)" });
  await expect(row).toContainText("tools: list_files, scan_skill, word_count");
  await row.getByRole("button", { name: "Review and turn on…" }).click();
  const sheet = settings.getByRole("region", { name: /Turn on Skill lint/ });
  await expect(sheet).toContainText("Exact tool calls");
  await expect(sheet).toContainText("pins them");
  await settings.getByRole("button", { name: "Turn on Skill lint (sample)", exact: true }).click();
  await expect(row).toContainText("On");
  await row.getByRole("button", { name: "Run now", exact: true }).click();
  await expect(row).toContainText("Last run");

  // Settings is narrow, so the pipeline is the stacked list; the arrow keys still move between steps.
  await row.getByRole("button", { name: "How it works", exact: true }).click();
  const graph = row.getByRole("group", { name: "How Skill lint (sample) works" });
  const steps = graph.getByRole("list", { name: "How Skill lint (sample) works (list)" });
  await expect(steps.locator("[data-graph-node]")).toHaveCount(9);
  await expect(steps.locator('[data-graph-node="analyzer"]')).toContainText("3 tools");
  await steps.locator('[data-graph-node="sources"]').focus();
  await app.keyboard.press("ArrowRight");
  await expect(steps.locator('[data-graph-node="projection"]')).toBeFocused();
  await steps.locator('[data-graph-node="findings"]').click();
  await expect(steps).toContainText("Took");
  await expect(steps).toContainText("1.8 s");
});

test("Insights explains which analyzer covers which agent, as a graph", async ({ app }) => {
  await openAgents(app);
  await app.getByRole("radio", { name: "Insights", exact: true }).click();
  const tab = app.getByRole("region", { name: "Insights workspace" });
  await tab.getByRole("radio", { name: "How it works" }).click();
  const overview = tab.getByRole("group", { name: "Analyzers and the agents they cover" });
  await expect(overview.locator(".svelte-flow")).toBeVisible();
  await expect(overview.locator('[data-graph-node="analyzer:nvidia.skillspector"]')).toContainText("SkillSpector");
  await expect(overview.locator('[data-graph-node="agent:claude-code"]')).toBeVisible();
  await overview.locator('[data-graph-node="analyzer:nvidia.skillspector"]').click();
  await expect(overview.getByRole("complementary", { name: "Details: SkillSpector" })).toContainText("Last run");
  await overview.getByRole("button", { name: "How SkillSpector works" }).click();
  const pipeline = tab.getByRole("group", { name: "How SkillSpector works" });
  await expect(pipeline.locator(".svelte-flow")).toBeVisible();
  await expect(pipeline.locator('[data-graph-node="parser"]')).toContainText("JSON field mapping");
  await pipeline.locator('[data-graph-node="sandbox"]').focus();
  await app.keyboard.press("Enter");
  await expect(pipeline.getByRole("complementary", { name: "Details: Sandbox" })).toContainText("api.osv.dev");
  await app.keyboard.press("Escape");
  await expect(pipeline.getByRole("complementary")).toHaveCount(0);
});

test("narrow windows get the analyzer graphs as a stacked list", async ({ app }) => {
  await app.setViewportSize({ width: 560, height: 900 });
  await openAgents(app);
  await app.getByRole("radio", { name: "Insights", exact: true }).click();
  const tab = app.getByRole("region", { name: "Insights workspace" });
  await tab.getByRole("radio", { name: "How it works" }).click();
  const list = tab.getByRole("list", { name: "Analyzers and the agents they cover (list)" });
  await expect(list).toBeVisible();
  await list.getByRole("button", { name: /SkillSpector/ }).click();
  await tab.getByRole("button", { name: "How SkillSpector works" }).click();
  const steps = tab.getByRole("list", { name: "How SkillSpector works (list)" });
  await steps.getByRole("button", { name: /^Findings/ }).click();
  await expect(steps).toContainText("Took");
});

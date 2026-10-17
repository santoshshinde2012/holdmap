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

test("findings reach Needs you, the agent card and the agent's Insights, and can be reviewed", async ({ app }) => {
  await app.getByRole("radio", { name: /^Home/ }).click();
  await expect(app.getByText(/Claude Code: SkillSpector found 1 high, 1 medium in release-check/)).toBeVisible();
  await expect(agentCard(app, "Claude Code").getByText("Analyzer findings:", { exact: false })).toBeAttached();

  await openAgents(app);
  await agentHeading(app, "Claude Code").click();
  const card = agentCard(app, "Claude Code");
  await expect(card.getByRole("heading", { name: "Insights", exact: true })).toBeVisible();
  await expect(card).toContainText("Instruction Override");
  await expect(card).toContainText("Risk score: 64 / 100");
  const high = card.getByRole("listitem").filter({ hasText: "Instruction Override" });
  await high.getByRole("button", { name: "Mark reviewed", exact: true }).click();
  await expect(card.getByRole("listitem").filter({ hasText: "Instruction Override" })).toHaveCount(0);
  await card.getByRole("checkbox", { name: "Show reviewed and snoozed" }).check();
  await card.getByRole("listitem").filter({ hasText: "Instruction Override" }).getByRole("button", { name: "Reopen", exact: true }).click();
  await expect(card.getByRole("listitem").filter({ hasText: "Instruction Override" }).getByRole("button", { name: "Mark reviewed" })).toBeVisible();
});

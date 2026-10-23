import { readFile } from "node:fs/promises";
import { expect, openAgents, test } from "./fixtures";

// Copy summary and Download JSON / SARIF from Home and Security: built from what the screen
// already shows, saved by the browser here and by the native save dialog in the app.
test("Security: copy a summary and download JSON and SARIF", async ({ app }) => {
  await app.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await openAgents(app);
  await app.getByRole("radio", { name: /^Security\b/ }).click();
  const ws = app.getByRole("region", { name: "Security workspace" });
  const tools = ws.getByRole("group", { name: "Export findings" });

  await tools.getByRole("button", { name: "Copy summary" }).click();
  await expect(tools.getByRole("status")).toHaveText("Summary copied");
  const text = await app.evaluate(() => navigator.clipboard.readText());
  expect(text).toContain("Holdmap security summary");
  expect(text).toContain("Instruction Override");

  const json = app.waitForEvent("download");
  await tools.getByRole("button", { name: "Download JSON" }).click();
  const jsonFile = await json;
  expect(jsonFile.suggestedFilename()).toMatch(/^holdmap-security-\d{4}-\d{2}-\d{2}\.json$/);
  const report = JSON.parse(await readFile((await jsonFile.path())!, "utf8"));
  expect(report).toMatchObject({ schema: "holdmap-report/1", kind: "security" });
  expect(report.findings.length).toBeGreaterThan(0);

  const sarif = app.waitForEvent("download");
  await tools.getByRole("button", { name: "Download SARIF" }).click();
  const sarifFile = await sarif;
  expect(sarifFile.suggestedFilename()).toMatch(/\.sarif$/);
  const log = JSON.parse(await readFile((await sarifFile.path())!, "utf8"));
  expect(log.version).toBe("2.1.0");
  expect(log.runs[0].results.length).toBe(report.findings.length);
});

test("Home: copy a summary and download the overview as JSON", async ({ app }) => {
  await app.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await app.getByRole("radio", { name: /^Home/ }).click();
  const tools = app.getByRole("group", { name: "Export overview" });
  await expect(tools.getByRole("button", { name: "Copy summary" })).toBeEnabled();
  await tools.getByRole("button", { name: "Copy summary" }).click();
  await expect(tools.getByRole("status")).toHaveText("Summary copied");
  const text = await app.evaluate(() => navigator.clipboard.readText());
  expect(text).toMatch(/^Holdmap summary · \d{4}-\d{2}-\d{2}\n\d+ agents?, \d+ listening/);
  expect(text).toContain("Needs you");

  const download = app.waitForEvent("download");
  await tools.getByRole("button", { name: "Download JSON" }).click();
  const file = await download;
  const report = JSON.parse(await readFile((await file.path())!, "utf8"));
  expect(report).toMatchObject({ kind: "home", schema: "holdmap-report/1" });
  expect(report.agents.length).toBeGreaterThan(0);
  expect(JSON.stringify(report)).not.toMatch(/--[a-z-]+=|SECRET|TOKEN/i);
});

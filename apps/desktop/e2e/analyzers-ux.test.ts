import AxeBuilder from "@axe-core/playwright";
import { readFile } from "node:fs/promises";
import { expect, openAgents, test } from "./fixtures";
import type { Page } from "@playwright/test";

// Guided install with consent, persistent run progress, the results table and every export.
// Browser demo only: nothing is downloaded, installed or run.
async function axe(page: Page, include = "body") {
  const { violations } = await new AxeBuilder({ page }).include(include).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).analyze();
  expect(violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
}
async function security(page: Page, query = "") {
  if (query) {
    await page.goto(`/${query}`);
    await expect(page.getByRole("listbox", { name: "Ports in use" }).getByRole("option").first()).toBeVisible();
  }
  await openAgents(page);
  await page.getByRole("radio", { name: /^Security\b/ }).click();
  return page.getByRole("region", { name: "Security workspace" });
}
async function openInstall(page: Page, query = "") {
  const ws = await security(page, query);
  await ws.getByRole("radio", { name: "Analyzers" }).click();
  await ws.getByRole("button", { name: "Install Bandit" }).click();
  const dlg = page.getByRole("dialog", { name: "Install Bandit" });
  await expect(dlg).toBeVisible();
  return { ws, dlg };
}

test("install shows what it does and needs consent before anything runs", async ({ app }) => {
  const { dlg } = await openInstall(app);
  await expect(dlg).toContainText("bandit[sarif]==1.9.4");
  await expect(dlg).toContainText("PyPI");
  await expect(dlg).toContainText("holdmap/analyzers/tools/pycqa.bandit");
  await expect(dlg).toContainText("no admin rights");
  await expect(dlg).toContainText("pypi.org, files.pythonhosted.org");
  const go = dlg.locator("[data-install-go]");
  await expect(go).toBeDisabled();
  await expect(dlg.locator("[data-install-status]")).toHaveCount(0);
  await axe(app, "[role=dialog]");
  await dlg.getByRole("checkbox").check();
  await go.click();
  const status = dlg.locator("[data-install-status]");
  await expect(status).toHaveAttribute("data-install-status", "done", { timeout: 10_000 });
  await expect(status).toContainText("version 1.9.4");
  await expect(dlg.getByLabel("Install log")).toContainText("Browser demo");
  await dlg.getByRole("button", { name: "Review and turn on…" }).click();
  await expect(app.getByRole("dialog", { name: "Settings", exact: true })).toBeVisible();
});

test("install can be cancelled and keeps a copy-command fallback", async ({ app }) => {
  const { dlg } = await openInstall(app);
  await dlg.getByText("Prefer to run it yourself?").click();
  await expect(dlg).toContainText("bandit");
  await dlg.getByRole("checkbox").check();
  await dlg.locator("[data-install-go]").click();
  await dlg.getByRole("button", { name: "Cancel install" }).click();
  await expect(dlg.locator("[data-install-status]")).toHaveAttribute("data-install-status", "cancelled");
  await expect(dlg).toContainText("You cancelled the install");
  await expect(dlg.locator("[data-install-go]")).toHaveText(/Retry/);
});

test("a failed install says why and offers Retry", async ({ app }) => {
  const { dlg } = await openInstall(app, "?demoInstall=fail");
  await dlg.getByRole("checkbox").check();
  await dlg.locator("[data-install-go]").click();
  const status = dlg.locator("[data-install-status]");
  await expect(status).toHaveAttribute("data-install-status", "failed", { timeout: 10_000 });
  await expect(status).toContainText("exit code 2");
  await expect(dlg.locator("[data-install-go]")).toHaveText(/Retry/);
});

test("a missing package manager is detected and blocks the install", async ({ app }) => {
  const { dlg } = await openInstall(app, "?demoInstall=no-uv");
  await expect(dlg).toContainText("needs uv, which wasn't found");
  await expect(dlg.getByRole("link", { name: /How to install uv/ })).toBeVisible();
  await expect(dlg.locator("[data-install-go]")).toHaveCount(0);
  await expect(dlg.getByRole("checkbox")).toHaveCount(0);
});

test("run progress stays visible across tabs and views, and can be cancelled", async ({ app }) => {
  const ws = await security(app, "?demoRunMs=700");
  await ws.getByRole("button", { name: "Rescan", exact: true }).click();
  const dock = app.locator("[data-run-dock]");
  await expect(dock).toBeVisible();
  await expect(dock.getByRole("progressbar", { name: "Run progress" })).toBeVisible();
  await expect(app.locator("[data-status-run]")).toContainText("Scanning");
  await ws.getByRole("radio", { name: "Analyzers" }).click();
  await expect(app.locator("[data-card-step]").first()).toBeVisible();
  await app.getByRole("radio", { name: /^Home/ }).click();
  await expect(dock).toBeVisible();
  await expect(app.locator("[data-status-run]")).toBeVisible();
  await axe(app, "[data-run-dock]");
  await dock.getByRole("button", { name: "Cancel" }).click();
  await expect(app.locator("[data-status-run]")).toContainText("Scan cancelled");
  await expect(dock).toContainText(/cancel/i);
});

test("a finished run shows a summary and opens the results", async ({ app }) => {
  const ws = await security(app);
  await ws.getByRole("button", { name: "Rescan", exact: true }).click();
  const dock = app.locator("[data-run-dock]");
  await expect(app.locator("[data-status-run]")).toContainText("Scan done", { timeout: 15_000 });
  await dock.getByRole("button", { name: "View results" }).click();
  await expect(ws.getByRole("radio", { name: /^Results/ })).toHaveAttribute("aria-checked", "true");
  await expect(app.locator("[data-run-history]")).toContainText("SkillSpector");
});

test("results table: filter, sort, group, details with folder, scope and path", async ({ app }) => {
  const ws = await security(app);
  await ws.getByRole("radio", { name: /^Results/ }).click();
  const table = ws.locator("[data-results-table]").first();
  await expect(table).toBeVisible();
  for (const h of ["Severity", "Rule", "Message", "Agent", "Skill", "Folder", "Path"]) await expect(table.getByRole("columnheader", { name: h })).toBeVisible();
  await table.getByRole("button", { name: "Rule" }).click();
  await expect(table.getByRole("columnheader", { name: "Rule" })).toHaveAttribute("aria-sort", "ascending");
  await table.getByRole("button", { name: "Rule" }).click();
  await expect(table.getByRole("columnheader", { name: "Rule" })).toHaveAttribute("aria-sort", "descending");
  await ws.getByRole("button", { name: /Show details for Instruction Override/ }).click();
  const place = ws.locator("[data-finding-place]").first();
  await expect(place).toContainText("Scope");
  await expect(place).toContainText("Folder");
  await expect(place).toContainText("Source path");
  await expect(place).toContainText("hasn't verified");
  await expect(place).toContainText("Suggested next steps");
  await ws.getByRole("radio", { name: "Agent", exact: true }).click();
  await expect(ws.getByRole("region", { name: "Claude Code findings" })).toBeVisible();
  await ws.getByRole("radio", { name: "Folder", exact: true }).click();
  await ws.getByRole("button", { name: "2 medium" }).click();
  await expect(ws.locator("[data-results-table] tbody tr .sev-pill.high")).toHaveCount(0);
  await expect(ws.locator("[data-export-scope]")).toContainText("severity medium");
  await axe(app, "#agent-insights");
});

test("exports carry only what's on screen, in JSON, SARIF, CSV and Markdown", async ({ app }) => {
  const ws = await security(app);
  await ws.getByRole("radio", { name: /^Results/ }).click();
  await ws.getByRole("button", { name: "2 medium" }).click();
  const tools = ws.getByRole("group", { name: "Export findings" });
  async function save(label: string, ext: RegExp) {
    const dl = app.waitForEvent("download");
    await tools.getByRole("button", { name: label }).click();
    const f = await dl;
    expect(f.suggestedFilename()).toMatch(ext);
    return readFile((await f.path())!, "utf8");
  }
  const json = JSON.parse(await save("Download JSON", /\.json$/));
  expect(json.findings).toHaveLength(2);
  expect(json.included).toContain("severity medium");
  expect(json.findings.every((f: { severity: string }) => f.severity === "medium")).toBe(true);
  const sarif = JSON.parse(await save("Download SARIF", /\.sarif$/));
  expect(sarif.version).toBe("2.1.0");
  expect(sarif.runs[0].results).toHaveLength(2);
  expect(sarif.runs[0].properties.unverified).toBe(true);
  const csv = await save("Download CSV", /\.csv$/);
  expect(csv.trimEnd().split("\r\n")).toHaveLength(3);
  expect(csv.split("\r\n")[0]).toContain("severity,rule,message");
  const md = await save("Download Markdown", /\.md$/);
  expect(md).toContain("**Included:**");
  expect(md).toContain("hasn't verified");
});

test("explainer, Home summary and per-agent skill findings", async ({ app }) => {
  const ws = await security(app);
  await expect(ws.locator("[data-skill-explainer]")).toContainText("What a skill analyzer does");
  await app.getByRole("radio", { name: /^Home/ }).click();
  const tile = app.getByRole("button", { name: /Skill findings/ });
  await expect(tile).toBeVisible();
  await tile.click();
  await expect(app.getByRole("region", { name: "Security workspace" })).toBeVisible();
});

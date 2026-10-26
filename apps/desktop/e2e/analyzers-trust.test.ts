import AxeBuilder from "@axe-core/playwright";
import { readFile } from "node:fs/promises";
import { expect, openAgents, test } from "./fixtures";
import type { Page } from "@playwright/test";

// Skill trust, top risks, custom snoozes and scheduled scans in the browser demo (sample data:
// nothing is hashed, scheduled or notified for real), in both themes and at phone width.
async function axe(page: Page, include = "#agent-insights") {
  const { violations } = await new AxeBuilder({ page }).include(include).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).analyze();
  expect(violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
}
async function security(page: Page, theme: "light" | "dark" = "light") {
  await page.evaluate((t) => localStorage.setItem("holdmap.theme", t), theme);
  await page.reload();
  await openAgents(page);
  await page.getByRole("radio", { name: /^Security\b/ }).click();
  return page.getByRole("region", { name: "Security workspace" });
}

for (const t of ["light", "dark"] as const) {
  test(`${t}: a changed skill shows what changed by name and size, and trusting it clears the alert`, async ({ app }) => {
    const ws = await security(app, t);
    await expect(ws.getByText(/skill.* changed or not trusted yet/)).toBeVisible();
    await ws.getByRole("radio", { name: /^Skills/ }).click();
    const row = ws.getByRole("button", { name: /release-check.*Changed since trusted/ });
    await row.click();
    const changes = ws.getByRole("list", { name: "Changed files for release-check" });
    await expect(changes).toContainText("scripts/upload.sh");
    await expect(changes).toContainText("2.3 KB → 2.9 KB");
    await axe(app);
    await ws.getByRole("textbox", { name: "Reason for trusting release-check" }).fill("Read the new upload script");
    await ws.getByRole("button", { name: "Trust this version" }).click();
    await expect(ws.getByRole("button", { name: /release-check.*Trusted/ })).toBeVisible();
    await expect(ws).toContainText("Read the new upload script");
  });

  test(`${t}: top risks are banded, explained, sortable and exportable`, async ({ app }) => {
    const ws = await security(app, t);
    await ws.getByRole("radio", { name: "Risks" }).click();
    const table = ws.getByRole("table");
    await expect(table.getByRole("row").nth(1)).toContainText(/Critical|High|Elevated/);
    await expect(ws).toContainText("Whether a skill is reachable from outside isn't counted");
    await ws.getByRole("button", { name: "Agent or folder" }).click();
    await expect(ws.getByRole("columnheader", { name: "Agent or folder" })).toHaveAttribute("aria-sort", "descending");
    await axe(app);
    const csv = app.waitForEvent("download");
    await ws.getByRole("group", { name: "Export risks" }).getByRole("button", { name: "Download CSV" }).click();
    const file = await csv;
    expect(file.suggestedFilename()).toMatch(/^holdmap-risks-.*\.csv$/);
    expect(await readFile((await file.path())!, "utf8")).toMatch(/^scope,key,band,points/);
  });
}

test("snooze to a custom date up to 90 days, shown in the list and in Markdown", async ({ app }) => {
  const ws = await security(app);
  await ws.getByRole("radio", { name: /^Findings/ }).click();
  await ws.getByRole("button", { name: /Instruction Override/ }).first().click();
  await ws.getByRole("combobox", { name: /Snooze length for Instruction Override/ }).first().selectOption("date");
  const day = ws.getByLabel("Snooze until (up to 90 days)");
  const max = (await day.getAttribute("max"))!;
  await day.fill(max);
  await ws.getByRole("textbox", { name: "Reason for Instruction Override" }).first().fill("Fix lands next sprint");
  await ws.getByRole("button", { name: "Snooze", exact: true }).click();
  await ws.getByLabel("Show reviewed").check();
  await expect(ws.getByRole("button", { name: /Instruction Override.*snoozed until/ }).first()).toBeVisible();
  await axe(app);
  const md = app.waitForEvent("download");
  await ws.getByRole("group", { name: "Export findings" }).getByRole("button", { name: "Download Markdown" }).click();
  const text = await readFile((await (await md).path())!, "utf8");
  expect(text).toContain(`snoozed until ${max}`);
  expect(text).toContain("Fix lands next sprint");
});

test("scheduled scans are off, and only offline analyzers can be picked", async ({ app }) => {
  const ws = await security(app);
  await ws.getByRole("button", { name: "Analyzer settings" }).click();
  const sched = app.getByRole("dialog", { name: "Settings", exact: true }).getByRole("region", { name: "Scheduled scans" });
  await sched.scrollIntoViewIfNeeded();
  await expect(sched).toContainText("Off. Nothing runs unless you start it.");
  // The demo's analyzers both use OSV, so neither can run on a schedule.
  await expect(sched.getByRole("checkbox", { name: "SkillSpector" })).toBeDisabled();
  await expect(sched).toContainText("Uses the network, so it only runs when you start it.");
  await expect(sched.getByRole("button", { name: "Review plan and turn on…" })).toBeDisabled();
  await axe(app, "[role=dialog]");
});

test("390px: the Security view switcher scrolls instead of overflowing", async ({ app }) => {
  await app.setViewportSize({ width: 390, height: 800 });
  const ws = await security(app);
  const over = await ws.evaluate((el) => {
    const r = el.getBoundingClientRect();
    return [...el.querySelectorAll(".head *")].filter((n) => n.getBoundingClientRect().right > r.right + 0.5 && !n.closest(".tabs")).length;
  });
  expect(over).toBe(0);
  await expect(ws.getByRole("radio", { name: "Risks" })).toBeAttached();
  expect(await app.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

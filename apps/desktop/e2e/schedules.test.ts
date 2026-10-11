import type { Locator, Page } from "@playwright/test";
import { agentCard, agentHeading, expect, openAgents, test } from "./fixtures";

// Install the virtual clock before loading the application. Every action stays in
// browser simulation, including cases which advance through the scheduled deadline.
const initialTime = new Date("2026-10-11T07:00:00Z");
const browserErrors = new WeakMap<Page, string[]>();

test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  browserErrors.set(page, errors);
  page.on("pageerror", (error) => errors.push(error.message));
  const bootTime = new Date(initialTime.getTime() - 500);
  await page.clock.install({ time: bootTime });
  await page.clock.pauseAt(bootTime);
  await page.addInitScript(() => {
    localStorage.setItem("holdmap.onboarded", "1");
    localStorage.setItem("holdmap.theme", "dark");
    localStorage.setItem("holdmap.view", "list");
  });
  await page.goto("/");
  // Finish the demo's initial scan and the animation frame which renders it.
  await page.clock.runFor(500);
  await expect(page.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
});

test.afterEach(async ({ page }) => {
  expect.soft(browserErrors.get(page), "Scheduling should not raise browser errors").toEqual([]);
});

async function chooseTiming(page: Page, dialog: Locator, label: string) {
  await dialog.getByRole("combobox", { name: "When", exact: true }).click();
  await page.getByRole("option", { name: label, exact: true }).click();
}

async function reviewShutdown(page: Page) {
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("tab", { name: "Power", exact: true }).click();
  await page.getByRole("button", { name: "Schedule simulation…", exact: true }).click();
  const dialog = page.getByRole("alertdialog", { name: "Schedule shutdown simulation?", exact: true });
  await expect(dialog).toBeVisible();
  return dialog;
}

async function reviewPort(page: Page) {
  await page.getByRole("option", { name: /^Port 3000 tcp,/ }).click();
  // Finish the explanation debounce before its reviewed-plan-only actions appear.
  await page.clock.runFor(100);
  const details = page.getByRole("complementary", { name: "Port details", exact: true });
  await details.getByRole("button", { name: "Schedule stop…", exact: true }).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toHaveAccessibleName("Schedule stop :3000?");
  await expect(dialog).toContainText("43000");
  return dialog;
}

async function viewSchedules(page: Page, review?: Locator) {
  if (review) await review.getByRole("button", { name: "View scheduled actions", exact: true }).click();
  else await page.getByRole("button", { name: /^Scheduled actions(?: \(\d+ active\))?$/, exact: false }).click();
  const dialog = page.getByRole("dialog", { name: "Scheduled actions", exact: true });
  await expect(dialog).toBeVisible();
  return dialog;
}

async function scheduleOneMinutePort(page: Page) {
  const review = await reviewPort(page);
  await chooseTiming(page, review, "Custom delay…");
  await review.getByRole("spinbutton", { name: "Delay in minutes", exact: true }).fill("1");
  await review.getByRole("button", { name: /^Schedule simulation/ }).click();
  return viewSchedules(page, review);
}

test("one-hour computer shutdown has a separate acknowledgement, visible countdown and cancellation", async ({ page }) => {
  const review = await reviewShutdown(page);
  await expect(review.getByRole("button", { name: "Cancel", exact: true })).toBeFocused();
  const when = review.getByRole("combobox", { name: "When", exact: true });
  await expect(when).toContainText("In 1 hour");
  await expect(review.getByRole("button", { name: "Simulate shutdown", exact: true })).toHaveCount(0);
  await when.click();
  await expect(page.getByRole("listbox", { name: "When", exact: true }).getByRole("option", { name: "Now", exact: true })).toHaveCount(0);
  await when.press("Escape");
  await chooseTiming(page, review, "In 30 min");
  await review.getByRole("checkbox").check();
  await chooseTiming(page, review, "In 1 hour");
  await expect(review.getByRole("checkbox")).not.toBeChecked();
  const submit = review.getByRole("button", { name: "Schedule simulation", exact: true });
  await expect(submit).toBeDisabled();
  await expect(review).toContainText("Shutdown at");
  await expect(review).toContainText("No computer or process is changed");
  await review.getByRole("checkbox").check();
  await submit.click();
  await expect(review).toContainText("Simulation scheduled");
  await expect(review).not.toContainText("Simulation complete");
  const manager = await viewSchedules(page, review);
  await expect(manager.getByRole("button", { name: "Done", exact: true })).toBeFocused();
  await expect(manager).toContainText("Shut down Demo computer");
  await expect(manager).toContainText("In 1h 0m");
  const due = manager.locator("time");
  await expect(due).toHaveAttribute("datetime", "2026-10-11T08:00:00.000Z");
  await expect(manager).toContainText("Your computer, agents and ports are never changed");
  await manager.getByRole("button", { name: "Cancel Shut down Demo computer", exact: true }).click();
  await expect(manager).toContainText("Cancelled before execution. No action was taken.");
  await expect(manager.getByRole("button", { name: /^Cancel / })).toHaveCount(0);
  await manager.getByRole("button", { name: "Done", exact: true }).click();
  await expect(page.getByRole("region", { name: "Active scheduled actions", exact: true })).toHaveCount(0);
  await expect(page.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
});

test("a port schedule keeps its reviewed protocol and process, updates countdown, and cancels with keyboard", async ({ page }) => {
  const review = await reviewPort(page);
  const when = review.getByRole("combobox", { name: "When", exact: true });
  await expect(when).toContainText("In 1 hour");
  await expect(review.getByRole("button", { name: "Stop :3000", exact: true })).toHaveCount(0);
  await when.click();
  await expect(page.getByRole("listbox", { name: "When", exact: true }).getByRole("option", { name: "Now", exact: true })).toHaveCount(0);
  await when.press("Escape");
  await expect(review.getByRole("list", { name: "Plan", exact: true })).toContainText("43000");
  await expect(review).toContainText("Only the reviewed services are scheduled");
  await review.getByRole("button", { name: /^Schedule simulation/ }).click();
  await expect(review).toContainText("Your agents and ports will not be changed");
  await review.getByRole("button", { name: "Done", exact: true }).click();
  const banner = page.getByRole("region", { name: "Active scheduled actions", exact: true });
  await expect(banner).toContainText("Stop :3000/tcp");
  await expect(banner).toContainText("In 1h 0m");
  // The countdown refreshes once a second, independently of its creation time.
  await page.clock.runFor(2_000);
  await expect(banner).toContainText("In 59m");
  await expect(page.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
  await banner.getByRole("button", { name: "View / cancel", exact: true }).click();
  const manager = page.getByRole("dialog", { name: "Scheduled actions", exact: true });
  await expect(manager).toContainText("Reviewed ports: :3000/tcp");
  const cancel = manager.getByRole("button", { name: "Cancel Stop :3000/tcp", exact: true });
  await cancel.focus();
  await cancel.press("Enter");
  await expect(manager).toContainText("Cancellation confirmed for Stop :3000/tcp.");
  await expect(manager).toContainText("Cancelled");
  await manager.press("Escape");
  await expect(manager).toHaveCount(0);
  await expect(page.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
});

test("an individual agent service retains scheduling without selecting its other port", async ({ page }) => {
  await openAgents(page);
  await agentHeading(page, "Cursor").click();
  const cursor = agentCard(page, "Cursor");
  await cursor.getByTitle("Stop :3000", { exact: true }).click();
  const review = page.getByRole("alertdialog");
  await expect(review.getByRole("combobox", { name: "When", exact: true })).toContainText("Now");
  await expect(review.getByRole("list", { name: "Plan", exact: true })).toContainText("43000");
  await expect(review.getByRole("list", { name: "Plan", exact: true })).not.toContainText("45173");
  await chooseTiming(page, review, "In 1 hour");
  await review.getByRole("button", { name: /^Schedule simulation/ }).click();
  const manager = await viewSchedules(page, review);
  await expect(manager).toContainText("Stop agent services (1 port)");
  await expect(manager).toContainText("Reviewed ports: :3000/tcp");
  await expect(manager).not.toContainText(":5173");
  await manager.getByRole("button", { name: "Cancel Stop agent services (1 port)", exact: true }).click();
  await manager.getByRole("button", { name: "Done", exact: true }).click();
  await expect(cursor).toContainText("2 stoppable");
  await expect(agentHeading(page, "Cursor")).toContainText("52000");
  await page.getByRole("radio", { name: "List", exact: true }).click();
  await expect(page.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
  await expect(page.getByRole("option", { name: /^Port 5173 tcp,/ })).toBeVisible();
});

test("an agent schedule retains both reviewed owned ports and keeps its session running", async ({ page }) => {
  await openAgents(page);
  await agentHeading(page, "Cursor").click();
  await agentCard(page, "Cursor").getByRole("button", { name: "Stop all (2)", exact: true }).click();
  const review = page.getByRole("alertdialog");
  await expect(review.getByRole("list", { name: "Plan", exact: true }).getByRole("listitem")).toHaveCount(4);
  await expect(review).toContainText("43000");
  await expect(review).toContainText("45173");
  await chooseTiming(page, review, "In 30 min");
  await review.getByRole("button", { name: /^Schedule simulation/ }).click();
  const manager = await viewSchedules(page, review);
  await expect(manager).toContainText("Stop agent services (2 ports)");
  await expect(manager).toContainText("Reviewed ports: :3000/tcp, :5173/tcp");
  await expect(manager).toContainText("the agent itself stays running");
  await expect(manager).not.toContainText("52000");
  await manager.getByRole("button", { name: "Cancel Stop agent services (2 ports)", exact: true }).click();
  await manager.getByRole("button", { name: "Done", exact: true }).click();
  await expect(agentCard(page, "Cursor")).toContainText("2 stoppable");
  await expect(agentHeading(page, "Cursor")).toContainText("52000");
  await page.getByRole("radio", { name: "List", exact: true }).click();
  await expect(page.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
  await expect(page.getByRole("option", { name: /^Port 5173 tcp,/ })).toBeVisible();
});

test("invalid custom delays prevent submission and recover with an explicit local time", async ({ page }) => {
  const review = await reviewShutdown(page);
  await chooseTiming(page, review, "Custom delay…");
  const delay = review.getByRole("spinbutton", { name: "Delay in minutes", exact: true });
  for (const value of ["0", "1.5", "1441", ""]) {
    await delay.fill(value);
    await expect(delay).toHaveAttribute("aria-invalid", "true");
    await expect(review).toContainText("Enter a whole number between 1 and 1,440 minutes.");
    await expect(review.getByRole("button", { name: /^(Schedule simulation|Simulate shutdown)$/ })).toBeDisabled();
  }
  await chooseTiming(page, review, "At a time…");
  const localTime = review.getByLabel("Local date and time", { exact: true });
  const localTimes = await page.evaluate((epoch) => {
    const pad = (value: number) => String(value).padStart(2, "0");
    const format = (ms: number) => {
      const date = new Date(ms);
      return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
    };
    return { valid: format(epoch + 3_600_000), invalid: [format(epoch), format(epoch + 25 * 3_600_000)] };
  }, initialTime.getTime());
  for (const invalid of localTimes.invalid) {
    await localTime.fill(invalid);
    await expect(localTime).toHaveAttribute("aria-invalid", "true");
    await expect(review).toContainText("Choose a time between 1 minute and 24 hours from now.");
    await expect(review.getByRole("button", { name: /^(Schedule simulation|Simulate shutdown)$/ })).toBeDisabled();
  }
  await localTime.fill(localTimes.valid);
  await expect(localTime).not.toHaveAttribute("aria-invalid", "true");
  await review.getByRole("checkbox").check();
  await review.getByRole("button", { name: "Schedule simulation", exact: true }).click();
  const manager = await viewSchedules(page, review);
  await expect(manager.locator("time")).toHaveAttribute("datetime", "2026-10-11T08:00:00.000Z");
});

test("a due browser schedule completes without removing ports or recording a real stop", async ({ page }) => {
  const manager = await scheduleOneMinutePort(page);
  await page.clock.runFor(61_000);
  await expect(manager).toContainText("Completed");
  await expect(manager).toContainText("Simulation complete. Your computer, agents and ports were not changed.");
  await expect(manager.getByRole("button", { name: /^Cancel / })).toHaveCount(0);
  await manager.getByRole("button", { name: "Done", exact: true }).click();
  await expect(page.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
  await expect(page.getByRole("region", { name: "Active scheduled actions", exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Recently stopped", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Recently stopped", exact: true }).getByText("Nothing yet", { exact: true })).toBeVisible();
});

test("a deadline missed while the page is suspended requires review and never executes later", async ({ page }) => {
  const manager = await scheduleOneMinutePort(page);
  await page.clock.fastForward(121_001);
  await expect(manager).toContainText("Needs review");
  await expect(manager).toContainText("The deadline was missed");
  await expect(manager).toContainText("Nothing runs automatically");
  await expect(manager.getByRole("button", { name: /^Cancel / })).toHaveCount(0);
  await page.clock.runFor(5_000);
  await expect(manager).not.toContainText("Simulation complete");
  await manager.getByRole("button", { name: "Done", exact: true }).click();
  await expect(page.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
});

test("a wall-clock shift marks schedules for review instead of performing their action", async ({ page }) => {
  const manager = await scheduleOneMinutePort(page);
  await page.clock.setSystemTime(new Date(initialTime.getTime() + 3_600_000));
  await page.clock.runFor(1_000);
  await expect(manager).toContainText("Needs review");
  await expect(manager).toContainText("The clock changed");
  await expect(manager).not.toContainText("Simulation complete");
  await manager.getByRole("button", { name: "Done", exact: true }).click();
  await expect(page.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
});

test("expired shutdown review stays safe and supports keyboard cancellation and a fresh review", async ({ page }) => {
  const review = await reviewShutdown(page);
  await chooseTiming(page, review, "In 1 hour");
  await review.getByRole("checkbox").check();
  await page.clock.runFor(60_000);
  await expect(review).toContainText("Confirmation expired");
  await expect(review.getByRole("button", { name: "Schedule simulation", exact: true })).toBeDisabled();
  await review.getByRole("button", { name: "Review new preview", exact: true }).click();
  await expect(review.getByRole("checkbox")).not.toBeChecked();
  await expect(review.getByRole("button", { name: /^(Schedule simulation|Simulate shutdown)$/ })).toBeDisabled();
  await review.getByRole("button", { name: "Cancel", exact: true }).press("Enter");
  await expect(review).toHaveCount(0);
  await page.getByRole("dialog", { name: "Settings", exact: true }).getByRole("button", { name: "View scheduled actions", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Scheduled actions", exact: true })).toContainText("No scheduled actions");
});

test("scheduled actions are discoverable from commands and remote inspection remains read-only", async ({ page }) => {
  await page.getByRole("button", { name: /^Commands/ }).click();
  const palette = page.getByRole("dialog");
  await palette.getByRole("combobox").fill("scheduled");
  await palette.getByRole("option", { name: /^Scheduled actions/ }).click();
  const manager = page.getByRole("dialog", { name: "Scheduled actions", exact: true });
  await expect(manager).toContainText("for the computer running Holdmap");
  await expect(manager).toContainText("No scheduled actions");
  await manager.getByRole("button", { name: "Done", exact: true }).click();
  await page.getByRole("button", { name: "Remote host…", exact: true }).click();
  const remote = page.getByRole("dialog", { name: "Remote host", exact: true });
  await remote.getByRole("button", { name: "devbox", exact: true }).click();
  await page.clock.runFor(1_000);
  await expect(remote.getByRole("table", { name: "Ports on devbox", exact: true })).toBeVisible();
  await expect(remote).toContainText("Read-only");
  await expect(remote.getByRole("button", { name: /stop|shutdown|schedule/i })).toHaveCount(0);
  await remote.press("Escape");
  const review = await reviewShutdown(page);
  await expect(review).toContainText("the local computer, even when you're viewing a remote host");
  await expect(review).toContainText("Demo computer");
});

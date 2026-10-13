import { expect, test } from "./fixtures";

test("live Restart requires a reviewed stop and completes the bound browser simulation", async ({ app }) => {
  await app.getByRole("option", { name: /^Port 3000 tcp,/ }).click();
  const details = app.getByRole("complementary", { name: "Port details", exact: true });
  await details.getByRole("button", { name: "Restart", exact: true }).click();
  const review = app.getByRole("alertdialog");
  await expect(review).toContainText("Nothing is sent until you confirm.");
  await expect(app.getByText("Restarted :3000", { exact: true })).toHaveCount(0);
  await review.getByRole("button", { name: "Restart :3000", exact: true }).click();
  await expect(review).toHaveCount(0);
  await expect(app.getByText("Restarted :3000", { exact: true })).toBeVisible();
  await app.getByRole("button", { name: "Recently stopped", exact: true }).click();
  await expect(app.getByRole("dialog", { name: "Recently stopped", exact: true })).toContainText(":3000");
});

test("stopped-service history copies the original command, restarts, and clears with confirmation", async ({ app }) => {
  await app.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await app.getByRole("option", { name: /^Port 3000 tcp,/ }).click();
  const details = app.getByRole("complementary", { name: "Port details", exact: true });
  await details.getByLabel("Port actions", { exact: true }).getByRole("button", { name: /^Stop…/ }).click();
  const review = app.getByRole("alertdialog");
  await review.getByRole("button", { name: "Stop :3000", exact: true }).click();
  await expect(review).toHaveCount(0);

  const trigger = app.getByRole("button", { name: "Recently stopped", exact: true });
  await trigger.click();
  const history = app.getByRole("dialog", { name: "Recently stopped", exact: true });
  await expect(history).toContainText(":3000");
  await expect(history.getByRole("button", { name: "Restart", exact: true })).toBeEnabled();
  await history.getByRole("button", { name: "Copy command", exact: true }).click();
  await expect.poll(() => app.evaluate(() => navigator.clipboard.readText())).toBe("cd -- /Users/dev/code/shop-web && 'node' /Users/dev/code/shop-web/node_modules/.bin/next dev");
  await history.getByRole("button", { name: "Restart", exact: true }).click();
  await expect(history).toHaveCount(0);
  await expect(app.getByText("Restarted :3000", { exact: true })).toBeVisible();

  await trigger.click();
  await history.getByRole("button", { name: "Clear history…", exact: true }).click();
  await expect(history).toContainText("Clear all 1 entries?");
  await history.getByRole("button", { name: "Keep", exact: true }).click();
  await expect(history).toContainText(":3000");
  await history.getByRole("button", { name: "Clear history…", exact: true }).click();
  await history.getByRole("button", { name: "Clear history", exact: true }).click();
  await expect(history).toContainText("Nothing yet");
  await history.getByRole("button", { name: "Done", exact: true }).click();
  await expect(history).toHaveCount(0);
  await expect(trigger).toBeFocused();
});

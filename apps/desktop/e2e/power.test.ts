import { expect, test } from "./fixtures";

async function review(app: import("@playwright/test").Page) {
  await app.getByRole("button", { name: "Settings", exact: true }).click();
  await app.getByRole("tab", { name: "Power", exact: true }).click();
  await app.getByRole("button", { name: /Simulate shutdown/ }).click();
  const dialog = app.getByRole("alertdialog", { name: "Simulate computer shutdown?" });
  await expect(dialog).toBeVisible();
  return dialog;
}

test("shutdown simulation requires acknowledgement and starts focused on Cancel", async ({ app }) => {
  const dialog = await review(app);
  await expect(dialog.getByRole("button", { name: "Cancel", exact: true })).toBeFocused();
  const submit = dialog.getByRole("button", { name: "Simulate shutdown", exact: true });
  await expect(submit).toBeDisabled();
  await expect(dialog).toContainText("All apps, agents and local services will stop");
  await expect(dialog).toContainText("Demo computer");
  await dialog.getByRole("checkbox").check();
  await expect(submit).toBeEnabled();
  await submit.click();
  await expect(dialog).toContainText("Simulation complete");
  await expect(dialog).toContainText("Nothing was stopped");
  await dialog.getByRole("button", { name: "Done", exact: true }).click();
  await expect(app.getByRole("dialog", { name: "Settings", exact: true })).toBeVisible();
  await app.getByRole("button", { name: "Done", exact: true }).click();
  await expect(app.getByRole("option", { name: /^Port 3000 tcp,/ })).toBeVisible();
});

test("Escape cancels shutdown and a fresh preview requires acknowledgement again", async ({ app }) => {
  const dialog = await review(app);
  await dialog.getByRole("checkbox").check();
  await app.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await app.getByRole("button", { name: /Simulate shutdown/ }).click();
  const fresh = app.getByRole("alertdialog", { name: "Simulate computer shutdown?" });
  await expect(fresh.getByRole("checkbox")).not.toBeChecked();
  await expect(fresh.getByRole("button", { name: "Simulate shutdown", exact: true })).toBeDisabled();
});

import { expect, test } from "./fixtures";

test("settings controls retain successful changes across sections and reopening", async ({ app }) => {
  await app.getByRole("button", { name: "Settings", exact: true }).click();
  const settings = app.getByRole("dialog", { name: "Settings", exact: true });
  await settings.getByRole("tab", { name: "Notifications", exact: true }).click();
  const notify = settings.getByRole("switch", { name: "Notify me", exact: true });
  const devOnly = settings.getByRole("checkbox", { name: "Only dev servers and pinned ports", exact: true });
  await notify.click();
  await expect(notify).toHaveAttribute("aria-checked", "false");
  await expect(devOnly).toBeDisabled();
  await notify.click();
  await expect(notify).toHaveAttribute("aria-checked", "true");
  await devOnly.uncheck();
  await expect(devOnly).not.toBeChecked();

  await settings.getByRole("tab", { name: "General", exact: true }).click();
  const shortcut = settings.getByRole("combobox", { name: /^Global shortcut:/ });
  await shortcut.click();
  await shortcut.press("End");
  await shortcut.press("Enter");
  await expect(shortcut).toContainText("Off");
  await expect(settings).toContainText("No global shortcut is active.");

  await settings.getByRole("tab", { name: "Scanning & data", exact: true }).click();
  const interval = settings.getByRole("spinbutton", { name: "Scan interval", exact: true });
  await interval.fill("7");
  await interval.press("Enter");
  await expect(settings).toContainText("Every 7 s while the window is open");
  const history = settings.getByRole("spinbutton", { name: "History length", exact: true });
  await history.fill("200");
  await history.press("Enter");
  await expect(history).toHaveValue("200");

  await settings.getByRole("tab", { name: "Appearance", exact: true }).click();
  await settings.getByRole("radio", { name: "Light", exact: true }).click();
  await expect(app.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(settings.getByRole("radio", { name: "Light", exact: true })).toHaveAttribute("aria-checked", "true");
  await settings.getByRole("radio", { name: "Compact", exact: true }).click();
  await expect(settings.getByRole("radio", { name: "Compact", exact: true })).toHaveAttribute("aria-checked", "true");
  await settings.getByRole("button", { name: "Done", exact: true }).click();
  await app.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(settings.getByRole("combobox", { name: "Global shortcut: Off", exact: true })).toBeVisible();
  await settings.getByRole("tab", { name: "Notifications", exact: true }).click();
  await expect(notify).toHaveAttribute("aria-checked", "true");
  await expect(devOnly).not.toBeChecked();
  await settings.getByRole("tab", { name: "Scanning & data", exact: true }).click();
  await expect(interval).toHaveValue("7");
  await expect(history).toHaveValue("200");
});

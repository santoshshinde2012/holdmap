import { expect, test } from "./fixtures";

test("About identifies the browser preview and help stays explicit and available from commands", async ({ app }, testInfo) => {
  await app.evaluate(() => {
    const calls: string[][] = [];
    window.open = (url, target, features) => { calls.push([String(url), target ?? "", features ?? ""]); return null; };
    Object.assign(window, { __desktopHelpOpens: calls });
  });
  await app.setViewportSize({ width: 390, height: 900 });
  await app.getByRole("button", { name: /^Commands/ }).click();
  let palette = app.getByRole("dialog", { name: "Command palette", exact: true });
  await palette.getByRole("combobox", { name: "Search commands" }).fill("About holdmap");
  await expect(palette.getByRole("option", { name: /About holdmap/ })).toContainText("Preview version");
  await palette.getByRole("option", { name: /About holdmap/ }).click();
  const settings = app.getByRole("dialog", { name: "Settings", exact: true });
  await expect(settings.getByRole("tab", { name: "About", exact: true })).toHaveAttribute("aria-selected", "true");
  await expect(settings).toContainText("Browser preview");
  await expect(settings).toContainText("Preview version");
  await expect(settings).not.toContainText("Settings folder");
  await expect(settings).not.toContainText("Running app location");
  await expect(settings).not.toContainText("requires manual desktop updates");
  expect(await app.evaluate(() => Reflect.get(window, "__desktopHelpOpens"))).toEqual([]);
  expect(await app.evaluate(() => document.documentElement.scrollWidth > innerWidth)).toBe(false);
  await app.screenshot({ path: testInfo.outputPath("about-narrow.png"), animations: "disabled" });
  await settings.getByRole("button", { name: "Download desktop app", exact: true }).click();
  await settings.getByRole("button", { name: "Installation guide", exact: true }).click();
  await settings.getByRole("button", { name: "Done", exact: true }).click();
  await app.getByRole("button", { name: /^Commands/ }).click();
  palette = app.getByRole("dialog", { name: "Command palette", exact: true });
  await palette.getByRole("combobox", { name: "Search commands" }).fill("Download desktop app");
  await palette.getByRole("option", { name: /Download desktop app/ }).click();
  await expect.poll(() => app.evaluate(() => Reflect.get(window, "__desktopHelpOpens"))).toEqual([
    ["https://santoshshinde2012.github.io/holdmap/#install", "_blank", "noopener,noreferrer"],
    ["https://santoshshinde2012.github.io/holdmap/docs/desktop/#first-open", "_blank", "noopener,noreferrer"],
    ["https://santoshshinde2012.github.io/holdmap/#install", "_blank", "noopener,noreferrer"],
  ]);
});

test("settings preserve composition, modified navigation and the modal tab order", async ({ app }) => {
  await app.getByRole("button", { name: "Settings", exact: true }).click();
  const settings = app.getByRole("dialog", { name: "Settings", exact: true });
  const general = settings.getByRole("tab", { name: "General", exact: true });
  await expect(general).toBeFocused();
  const extras = [{ isComposing: true }, { keyCode: 229 }, { altKey: true }, { ctrlKey: true }, { metaKey: true }];
  for (const extra of extras) await general.dispatchEvent("keydown", { key: "End", ...extra });
  await expect(general).toHaveAttribute("aria-selected", "true");

  await settings.getByRole("tab", { name: "Scanning & data", exact: true }).click();
  const interval = settings.getByRole("spinbutton", { name: "Scan interval", exact: true });
  const original = await interval.inputValue();
  for (const extra of extras) await interval.dispatchEvent("keydown", { key: "End", ...extra });
  await expect(interval).toHaveValue(original);
  await interval.press("ArrowUp");
  await expect(interval).toHaveValue(String(Number(original) + 1));

  const appearance = settings.getByRole("tab", { name: "Appearance", exact: true });
  await appearance.click();
  const dark = settings.getByRole("radio", { name: "Dark", exact: true });
  for (const extra of extras) await dark.dispatchEvent("keydown", { key: "Home", ...extra });
  await expect(dark).toHaveAttribute("aria-checked", "true");
  const done = settings.getByRole("button", { name: "Done", exact: true });
  await done.focus();
  await app.keyboard.press("Tab");
  await expect(settings.getByRole("button", { name: "Close", exact: true })).toBeFocused();
  await app.keyboard.press("Tab");
  await expect(appearance).toBeFocused();
  await app.keyboard.press("Tab");
  await expect(dark).toBeFocused();
  await app.keyboard.press("Tab");
  await expect(settings.getByRole("radio", { name: "Comfortable", exact: true })).toBeFocused();
  await app.keyboard.press("Tab");
  await expect(done).toBeFocused();
});

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

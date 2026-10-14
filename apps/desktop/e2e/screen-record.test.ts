import { expect, test as base } from "./fixtures";

const test = base.extend<{ app: import("@playwright/test").Page }>({
  app: async ({ page }, use) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.addInitScript(`
      localStorage.setItem("holdmap.onboarded", "1");
      localStorage.setItem("holdmap.theme", "dark");
      localStorage.setItem("holdmap.view", "list");
      const canvas = document.createElement("canvas");
      canvas.width = 320;
      canvas.height = 180;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      const paint = (t) => {
        ctx.fillStyle = "hsl(" + ((t / 20) % 360) + " 75% 45%)";
        ctx.fillRect(0, 0, 320, 180);
        ctx.fillStyle = "#fff";
        ctx.font = "28px sans-serif";
        ctx.fillText("holdmap", 70, 105);
        requestAnimationFrame(paint);
      };
      requestAnimationFrame(paint);
      const stream = canvas.captureStream(12);
      Object.defineProperty(navigator.mediaDevices, "getDisplayMedia", {
        configurable: true,
        value: async () => stream,
      });
    `);
    await page.goto("/");
    await expect(page.getByRole("listbox", { name: "Ports in use" }).getByRole("option").first()).toBeVisible();
    await use(page);
    expect.soft(errors, "The app should not raise uncaught browser errors").toEqual([]);
  },
});

test("records a local MP4 and GIF that can be downloaded", async ({ app }) => {
  await app.getByRole("button", { name: "Record screen" }).click();
  const confirm = app.getByRole("dialog", { name: "Record screen" });
  await expect(confirm).toBeVisible();
  await expect(confirm).toContainText("never leaves this machine");
  await expect(confirm).toContainText("Microphone and system audio are not captured");
  await confirm.getByRole("button", { name: "Choose screen…" }).click();
  await expect(app.getByRole("button", { name: "Stop screen recording" })).toBeVisible({ timeout: 10_000 });
  await expect(app.getByRole("status").filter({ hasText: /0:\d\d/ })).toBeVisible();
  await app.waitForTimeout(700);
  await app.getByRole("button", { name: "Stop screen recording" }).click();
  const done = app.getByRole("dialog", { name: "Download recording" });
  await expect(done).toBeVisible({ timeout: 20_000 });
  await expect(done).toContainText("Files stay on this computer");
  const mp4 = done.getByRole("button", { name: "Download MP4" });
  const gif = done.getByRole("button", { name: "Download GIF" });
  if (await mp4.count()) {
    const [download] = await Promise.all([app.waitForEvent("download"), mp4.click()]);
    expect(download.suggestedFilename()).toMatch(/^holdmap-\d{8}-\d{6}\.mp4$/);
  } else {
    await expect(done).toContainText("GIF");
  }
  await expect(gif).toBeVisible();
  const [gifDownload] = await Promise.all([app.waitForEvent("download"), gif.click()]);
  expect(gifDownload.suggestedFilename()).toMatch(/^holdmap-\d{8}-\d{6}\.gif$/);
});

test("command palette opens the local recorder and Escape cancels before capture", async ({ app }) => {
  await app.getByRole("button", { name: /^Commands/ }).click();
  const palette = app.getByRole("dialog", { name: "Command palette", exact: true });
  await palette.getByRole("combobox", { name: "Search commands" }).fill("Record screen");
  await palette.getByRole("option", { name: /Record screen/ }).click();
  const confirm = app.getByRole("dialog", { name: "Record screen" });
  await expect(confirm).toBeVisible();
  await confirm.press("Escape");
  await expect(confirm).toHaveCount(0);
  await expect(app.getByRole("button", { name: "Stop screen recording" })).toHaveCount(0);
});

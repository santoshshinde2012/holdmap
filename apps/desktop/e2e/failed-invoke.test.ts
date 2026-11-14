import { expect, test, type Page } from "@playwright/test";

// The desktop shell, faked: every native command fails the way a real one can.
async function failingShell(page: Page, message: string) {
  const seen: string[] = [];
  page.on("pageerror", (error) => seen.push(error.message));
  page.on("console", (m) => { if (m.type() === "error" && m.text().includes("recovering UI")) seen.push(m.text()); });
  await page.addInitScript((message) => {
    localStorage.setItem("holdmap.onboarded", "1");
    localStorage.setItem("holdmap.view", "list");
    const fake = window as unknown as { __TAURI_INTERNALS__: unknown; calls: string[] };
    fake.calls = [];
    fake.__TAURI_INTERNALS__ = {
      transformCallback: () => 0,
      unregisterCallback: () => {},
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
      invoke: async (command: string) => {
        fake.calls.push(command);
        throw message;
      },
    };
  }, message);
  return seen;
}

test("a failed scan invoke explains itself, offers a retry and never blanks or remounts the app", async ({ page }) => {
  const seen = await failingShell(page, "permission denied reading the socket table");
  await page.goto("/");
  const notice = page.getByRole("main").getByRole("status").filter({ hasText: "Couldn't read the socket table" });
  await expect(notice).toBeVisible();
  await expect(notice).toContainText("permission denied reading the socket table");
  await expect(page.getByRole("banner").getByText("Scan failed")).toBeVisible();
  // Other failed reads say what failed, and settings offers the way to fix it.
  const toasts = page.getByRole("region", { name: "Notifications" }).getByRole("alert");
  await expect(toasts.filter({ hasText: "Couldn't load settings" }).getByRole("button", { name: "Open settings" })).toBeVisible();
  const calls = () => page.evaluate(() => (window as unknown as { calls: string[] }).calls.filter((c) => c === "scan").length);
  const before = await calls();
  await notice.getByRole("button", { name: "Try again" }).click();
  await expect.poll(calls).toBeGreaterThan(before);
  await expect(notice).toBeVisible();
  expect(seen, "no uncaught errors and no UI recovery").toEqual([]);
});

test("the tray panel stays usable when every native call fails", async ({ page }) => {
  const seen = await failingShell(page, "holdmap isn't running");
  await page.setViewportSize({ width: 360, height: 600 });
  await page.goto("/tray.html");
  await expect(page.getByText("Can't read holdmap")).toBeVisible();
  await expect(page.getByRole("button", { name: "Try now" })).toBeVisible();
  await expect(page.getByRole("button", { name: /Open holdmap/ })).toBeVisible();
  expect(seen).toEqual([]);
});

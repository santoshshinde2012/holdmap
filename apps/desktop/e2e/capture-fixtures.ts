import { expect, test as base } from "./fixtures";
import { readFile } from "node:fs/promises";

export { expect };

const { app: { security: { csp } } } = JSON.parse(await readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));

type CaptureFixture = {
  mode: "ready" | "pending" | "denied";
  requests: number;
  pngOverlayPresence: boolean[];
  pngPalettePresence: boolean[];
  streams: MediaStream[];
  pending: (() => void)[];
};
declare global { interface Window { __holdmapCapture: CaptureFixture } }

export const test = base.extend<{ app: import("@playwright/test").Page }>({
  app: async ({ page }, use) => {
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.route("**/", async (route) => {
      const response = await route.fetch();
      await route.fulfill({ response, headers: { ...response.headers(), "content-security-policy": csp } });
    });
    await page.addInitScript(() => {
      localStorage.setItem("holdmap.onboarded", "1");
      if (!localStorage.getItem("holdmap.theme")) localStorage.setItem("holdmap.theme", "dark");
      localStorage.setItem("holdmap.view", "list");
      const canvas = document.createElement("canvas");
      canvas.width = 320;
      canvas.height = 180;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      const paint = (t: number) => {
        ctx.fillStyle = "hsl(" + ((t / 20) % 360) + " 75% 45%)";
        ctx.fillRect(0, 0, 320, 180);
        ctx.fillStyle = "#fff";
        ctx.font = "28px sans-serif";
        ctx.fillText("holdmap", 70, 105);
        requestAnimationFrame(paint);
      };
      requestAnimationFrame(paint);
      const capture: CaptureFixture = { mode: "ready", requests: 0, pngOverlayPresence: [], pngPalettePresence: [], streams: [], pending: [] };
      window.__holdmapCapture = capture;
      const toBlob = HTMLCanvasElement.prototype.toBlob;
      HTMLCanvasElement.prototype.toBlob = function (callback, type, quality) {
        if (type === "image/png") {
          capture.pngOverlayPresence.push(!!document.querySelector("[data-snapshot-ui]"));
          capture.pngPalettePresence.push(!!document.querySelector('[role="dialog"][aria-label="Command palette"]'));
        }
        return toBlob.call(this, callback, type, quality);
      };
      Object.defineProperty(navigator.mediaDevices, "getDisplayMedia", {
        configurable: true,
        value: async () => {
          capture.requests++;
          if (capture.mode === "denied") throw new DOMException("Permission denied", "NotAllowedError");
          const stream = canvas.captureStream(12);
          capture.streams.push(stream);
          if (capture.mode === "pending") await new Promise<void>((resolve) => capture.pending.push(resolve));
          return stream;
        },
      });
    });
    await page.goto("/");
    await expect(page.getByRole("listbox", { name: "Ports in use" }).getByRole("option").first()).toBeVisible();
    await use(page);
    expect.soft(errors, "The app should not raise uncaught browser errors").toEqual([]);
  },
});

import { expect, test } from "./capture-fixtures";
import { readFile } from "node:fs/promises";

test("records a local MP4 and GIF that can be downloaded", async ({ app }) => {
  expect(await app.evaluate(() => !!window.VideoEncoder)).toBe(true);
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
  await expect(mp4).toBeVisible();
  const [download] = await Promise.all([app.waitForEvent("download"), mp4.click()]);
  expect(download.suggestedFilename()).toMatch(/^holdmap-\d{8}-\d{6}\.mp4$/);
  const mp4Bytes = await readFile((await download.path())!);
  expect(mp4Bytes.subarray(4, 8).toString()).toBe("ftyp");
  const decoded = await app.evaluate(async (bytes) => {
    const href = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "video/mp4" }));
    const video = document.createElement("video");
    video.muted = true;
    video.src = href;
    try {
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("MP4 did not decode")), 8_000);
        video.onerror = () => { clearTimeout(timer); reject(new Error("MP4 decode failed")); };
        video.onloadeddata = () => { clearTimeout(timer); resolve(); };
        video.load();
      });
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("MP4 frame was not presented")), 8_000);
        video.requestVideoFrameCallback(() => { clearTimeout(timer); video.pause(); resolve(); });
        void video.play().catch((cause) => { clearTimeout(timer); reject(cause); });
      });
      const canvas = document.createElement("canvas");
      canvas.width = video.videoWidth;
      canvas.height = video.videoHeight;
      const context = canvas.getContext("2d")!;
      context.drawImage(video, 0, 0);
      const rgba = [...context.getImageData(1, 1, 1, 1).data];
      return { duration: video.duration, width: video.videoWidth, height: video.videoHeight, rgba };
    } finally {
      video.removeAttribute("src");
      video.load();
      URL.revokeObjectURL(href);
    }
  }, [...mp4Bytes]);
  expect(decoded.width).toBe(320);
  expect(decoded.height).toBe(180);
  expect(decoded.duration).toBeGreaterThan(0.4);
  expect(decoded.duration).toBeLessThan(5);
  expect(Math.max(...decoded.rgba.slice(0, 3)) - Math.min(...decoded.rgba.slice(0, 3)), JSON.stringify(decoded)).toBeGreaterThan(40);
  await expect(gif).toBeVisible();
  const [gifDownload] = await Promise.all([app.waitForEvent("download"), gif.click()]);
  expect(gifDownload.suggestedFilename()).toMatch(/^holdmap-\d{8}-\d{6}\.gif$/);
  const gifBytes = await readFile((await gifDownload.path())!);
  const timeline = gifTimeline(gifBytes);
  expect(timeline.frames).toBeGreaterThan(1);
  expect(timeline.frames).toBeLessThanOrEqual(240);
  expect(timeline.durationMs).toBeGreaterThan(400);
  expect(Math.abs(timeline.durationMs - decoded.duration * 1000)).toBeLessThan(500);
  const gifFrame = await app.evaluate(async (bytes) => {
    const href = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/gif" }));
    const image = new Image();
    try {
      image.src = href;
      await image.decode();
      const canvas = document.createElement("canvas");
      canvas.width = image.naturalWidth;
      canvas.height = image.naturalHeight;
      const context = canvas.getContext("2d")!;
      context.drawImage(image, 0, 0);
      return { width: image.naturalWidth, height: image.naturalHeight, rgba: [...context.getImageData(1, 1, 1, 1).data] };
    } finally { URL.revokeObjectURL(href); }
  }, [...gifBytes]);
  expect(gifFrame.width).toBe(320);
  expect(gifFrame.height).toBe(180);
  expect(Math.max(...gifFrame.rgba.slice(0, 3)) - Math.min(...gifFrame.rgba.slice(0, 3))).toBeGreaterThan(40);
  expect(await app.evaluate(() => window.__holdmapCapture.streams.every((stream) => stream.getTracks().every((track) => track.readyState === "ended")))).toBe(true);
  await done.press("Escape");
  await expect(done).toHaveCount(0);
  await expect(app.getByRole("button", { name: "Review recording", exact: true })).toBeFocused();
  await app.keyboard.press("Shift+V");
  await expect(done).toBeVisible();
  const [sameGif] = await Promise.all([app.waitForEvent("download"), done.getByRole("button", { name: "Download GIF" }).click()]);
  expect(sameGif.suggestedFilename()).toBe(gifDownload.suggestedFilename());
  expect(await app.evaluate(() => window.__holdmapCapture.streams.length)).toBe(1);
  await done.getByRole("button", { name: "Discard recording" }).click();
  await expect(done).toHaveCount(0);
  await expect(app.getByRole("button", { name: "Record screen", exact: true })).toBeFocused();
});

/** Read animation timing from the downloaded GIF; browser decoding below checks its pixel data. */
function gifTimeline(bytes: Buffer): { frames: number; durationMs: number } {
  expect(bytes.subarray(0, 6).toString()).toMatch(/^GIF8[79]a$/);
  let cursor = 13 + ((bytes[10] & 0x80) ? 3 * (2 ** ((bytes[10] & 7) + 1)) : 0);
  let frames = 0, durationMs = 0, delayMs = 0;
  const skipBlocks = () => {
    while (cursor < bytes.length && bytes[cursor]) cursor += 1 + bytes[cursor];
    cursor++;
  };
  while (cursor < bytes.length) {
    const marker = bytes[cursor++];
    if (marker === 0x3b) break;
    if (marker === 0x21) {
      const label = bytes[cursor++];
      if (label === 0xf9) delayMs = bytes.readUInt16LE(cursor + 2) * 10;
      skipBlocks();
    } else if (marker === 0x2c) {
      const packed = bytes[cursor + 8];
      cursor += 9 + ((packed & 0x80) ? 3 * (2 ** ((packed & 7) + 1)) : 0);
      cursor++; // LZW minimum code size.
      skipBlocks();
      frames++;
      durationMs += delayMs;
    } else throw new Error(`Unexpected GIF block ${marker}`);
  }
  return { frames, durationMs };
}

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

test("cancelling a pending screen picker ignores and releases its later stream", async ({ app }) => {
  await app.evaluate(() => { window.__holdmapCapture.mode = "pending"; });
  const trigger = app.getByRole("button", { name: "Record screen", exact: true });
  await trigger.click();
  const dialog = app.getByRole("dialog", { name: "Record screen" });
  await dialog.getByRole("button", { name: "Choose screen…" }).click();
  await expect(dialog.getByRole("status")).toContainText("picker");
  await dialog.press("Escape");
  await app.evaluate(() => { window.__holdmapCapture.pending.shift()?.(); });
  await expect.poll(() => app.evaluate(() => window.__holdmapCapture.streams[0].getVideoTracks()[0].readyState)).toBe("ended");
  await expect(app.getByRole("dialog", { name: "Record screen" })).toHaveCount(0);
  await expect(app.getByRole("button", { name: "Stop screen recording" })).toHaveCount(0);
  await expect(trigger).toBeFocused();
});

test("permission failure can be retried and ending sharing opens the saved results", async ({ app }) => {
  await app.evaluate(() => { window.__holdmapCapture.mode = "denied"; });
  await app.getByRole("button", { name: "Record screen", exact: true }).click();
  const dialog = app.getByRole("dialog", { name: "Record screen" });
  await dialog.getByRole("button", { name: "Choose screen…" }).click();
  await expect(dialog.getByRole("alert")).toContainText("cancelled or permission was denied");
  await app.evaluate(() => { window.__holdmapCapture.mode = "ready"; });
  await dialog.getByRole("button", { name: "Choose screen…" }).click();
  await expect(app.getByRole("button", { name: "Stop screen recording" })).toBeVisible();
  await app.waitForTimeout(400);
  await app.evaluate(() => {
    const track = window.__holdmapCapture.streams[0].getVideoTracks()[0];
    track.stop();
    track.dispatchEvent(new Event("ended"));
  });
  const done = app.getByRole("dialog", { name: "Download recording" });
  await expect(done).toBeVisible({ timeout: 20_000 });
  await expect(done.getByRole("button", { name: "Download GIF" })).toBeVisible();
  await expect(app.getByRole("button", { name: "Stop screen recording" })).toHaveCount(0);
  await app.mouse.click(5, 5); // Closing the backdrop keeps an unsaved preview available.
  await expect(done).toHaveCount(0);
  await expect(app.getByRole("button", { name: "Review recording", exact: true })).toBeFocused();
  await app.getByRole("button", { name: "Commands" }).click();
  const palette = app.getByRole("dialog", { name: "Command palette", exact: true });
  await palette.getByRole("combobox", { name: "Search commands" }).fill("Review recording");
  await palette.getByRole("option", { name: /Review recording/ }).click();
  await expect(done).toBeVisible();
  await expect(done.getByRole("button", { name: "Download GIF" })).toBeVisible();
  expect(await app.evaluate(() => window.__holdmapCapture.streams.length)).toBe(1);
  await done.press("Escape");
  await expect(app.getByRole("button", { name: "Review recording", exact: true })).toBeFocused();
  await expect(app.getByRole("button", { name: "Review recording", exact: true })).toHaveAttribute("data-screen-record", "true");
});

test("unsupported capture explains the current window without promising desktop support", async ({ app }) => {
  await app.evaluate(() => { Object.defineProperty(navigator.mediaDevices, "getDisplayMedia", { configurable: true, value: undefined }); });
  await app.getByRole("button", { name: "Record screen", exact: true }).click();
  const dialog = app.getByRole("dialog", { name: "Record screen" });
  await expect(dialog).toContainText("Screen capture unavailable");
  await expect(dialog.getByRole("button", { name: "Choose screen…" })).toHaveCount(0);
  await dialog.press("Escape");
  await expect(app.getByRole("button", { name: "Record screen", exact: true })).toBeFocused();
});

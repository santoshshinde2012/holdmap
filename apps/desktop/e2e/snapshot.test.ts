import type { Locator, Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { expect, test } from "./capture-fixtures";

const chooser = (page: Page) => page.getByRole("dialog", { name: "Take snapshot", exact: true });
const preview = (page: Page) => page.getByRole("dialog", { name: "Snapshot preview", exact: true });
const source = (page: Page, name: string) => chooser(page).getByRole("radio", { name, exact: true });

async function downloadPng(page: Page): Promise<{ filename: string; bytes: Buffer }> {
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    preview(page).getByRole("button", { name: "Download PNG", exact: true }).click(),
  ]);
  const bytes = await readFile((await download.path())!);
  expect(download.suggestedFilename()).toMatch(/^holdmap-snapshot-\d{8}-\d{6}-\d{3}\.png$/);
  expect(bytes.subarray(0, 8).toString("hex")).toBe("89504e470d0a1a0a");
  expect(bytes.length).toBeLessThanOrEqual(32 * 1024 * 1024);
  return { filename: download.suggestedFilename(), bytes };
}

async function decodeImage(page: Page, bytes: Buffer, type = "image/png") {
  return page.evaluate(async ({ bytes, type }) => {
    const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type }));
    const image = new Image();
    try {
      image.src = url;
      await image.decode();
      const canvas = document.createElement("canvas");
      canvas.width = image.naturalWidth; canvas.height = image.naturalHeight;
      const context = canvas.getContext("2d")!;
      context.drawImage(image, 0, 0);
      const rgba = [...context.getImageData(1, 1, 1, 1).data];
      return { width: image.naturalWidth, height: image.naturalHeight, rgba };
    } finally { URL.revokeObjectURL(url); }
  }, { bytes: [...bytes], type });
}

async function contained(page: Page, dialog: Locator) {
  const bounds = (await dialog.boundingBox())!;
  expect(bounds.x).toBeGreaterThanOrEqual(-1);
  expect(bounds.y).toBeGreaterThanOrEqual(-1);
  expect(bounds.x + bounds.width).toBeLessThanOrEqual((page.viewportSize()?.width ?? 0) + 1);
  expect(bounds.y + bounds.height).toBeLessThanOrEqual((page.viewportSize()?.height ?? 0) + 1);
  for (const control of await dialog.getByRole("button").or(dialog.getByRole("radio")).all()) {
    if (!await control.isVisible()) continue;
    const box = (await control.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(bounds.x - 1);
    expect(box.x + box.width).toBeLessThanOrEqual(bounds.x + bounds.width + 1);
    expect(box.y).toBeGreaterThanOrEqual(bounds.y - 1);
    expect(box.y + box.height).toBeLessThanOrEqual(bounds.y + bounds.height + 1);
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
}

test("Holdmap-only PNG includes the real Graph cards and SVG without a screen-sharing API", async ({ app }) => {
  test.setTimeout(45_000);
  await app.evaluate(() => Object.defineProperty(navigator.mediaDevices, "getDisplayMedia", { configurable: true, value: undefined }));
  await app.getByRole("radio", { name: /^Graph/ }).click();
  const graph = app.locator("#service-graph");
  const card = graph.getByRole("button", { name: "shop-api on ports 3001", exact: true });
  await expect(card).toBeVisible();
  await expect(graph.locator(".svelte-flow__edge-path").first()).toBeAttached();
  await expect(graph.locator(".graph-body")).not.toHaveAttribute("aria-busy", "true");
  await graph.getByRole("button", { name: "Fit all services in view", exact: true }).click();
  await expect(card).toBeInViewport({ ratio: .99 });
  await app.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  // The export must contain the app's actual card text and SVG topology, rather than
  // merely being a decodable solid PNG. Compare those regions to the browser image.
  const probes = await app.evaluate(() => {
    const root = document.getElementById("holdmap-capture-root")!.getBoundingClientRect();
    const graph = document.getElementById("service-graph")!.getBoundingClientRect();
    const label = document.querySelector<HTMLElement>('[data-service-id] .label')!;
    const rect = label.getBoundingClientRect();
    const edge = [...document.querySelectorAll<SVGPathElement>(".svelte-flow__edge-path")].map((path) => {
      // Edge labels sit at the midpoint; sample the SVG stroke away from that HTML label.
      const point = path.getPointAtLength(path.getTotalLength() / 4).matrixTransform(path.getScreenCTM()!);
      return { x: point.x - 5, y: point.y - 5, width: 10, height: 10 };
    }).find((box) => box.x > graph.left && box.y > graph.top && box.x + box.width < graph.right && box.y + box.height < graph.bottom);
    if (!edge) throw new Error("Expected a visible SVG topology edge for the PNG check");
    return { root: { x: root.x, y: root.y, width: root.width, height: root.height }, regions: [
      { name: "card text", x: rect.x, y: rect.y, width: rect.width, height: rect.height },
      { name: "SVG edge", ...edge },
    ] };
  });
  const baseline = await app.screenshot();
  await app.getByRole("button", { name: "Take snapshot", exact: true }).press("Shift+S");
  await expect(chooser(app)).toBeVisible();
  await expect(source(app, "Screen or window")).toBeDisabled();
  await expect(source(app, "Holdmap view")).toHaveAttribute("aria-checked", "true");
  await chooser(app).getByRole("button", { name: "Capture PNG", exact: true }).click();
  await expect(preview(app)).toBeVisible({ timeout: 20_000 });
  await expect(preview(app)).toContainText("Holdmap view");
  const downloaded = await downloadPng(app);
  const pixels = await app.evaluate(async ({ exported, baseline, probes }) => {
    const read = async (bytes: number[]) => {
      const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "image/png" }));
      try {
        const image = new Image(); image.src = url; await image.decode();
        const canvas = document.createElement("canvas"); canvas.width = image.naturalWidth; canvas.height = image.naturalHeight;
        const context = canvas.getContext("2d")!; context.drawImage(image, 0, 0);
        return { width: canvas.width, height: canvas.height, context };
      } finally { URL.revokeObjectURL(url); }
    };
    const [image, reference] = await Promise.all([read(exported), read(baseline)]);
    const scale = image.width / probes.root.width;
    const regions = probes.regions.map((region) => {
      const x = Math.floor((region.x - probes.root.x) * scale), y = Math.floor((region.y - probes.root.y) * scale);
      const width = Math.max(1, Math.floor(region.width * scale)), height = Math.max(1, Math.floor(region.height * scale));
      const actual = image.context.getImageData(x, y, width, height).data;
      // Playwright's default browser context has deviceScaleFactor=1, as does this
      // snapshot. A future explicit DPR fixture must adapt both image coordinates.
      const expected = reference.context.getImageData(Math.floor(region.x), Math.floor(region.y), width, height).data;
      let min = 255, max = 0, difference = 0, opaque = 0;
      for (let index = 0; index < actual.length; index += 4) {
        const brightness = (actual[index] + actual[index + 1] + actual[index + 2]) / 3;
        min = Math.min(min, brightness); max = Math.max(max, brightness);
        if (actual[index + 3] === 255) opaque++;
        for (let channel = 0; channel < 3; channel++) difference += Math.abs(actual[index + channel] - expected[index + channel]);
      }
      return { name: region.name, contrast: max - min, meanDifference: difference / (actual.length / 4 * 3), opaque: opaque / (actual.length / 4) };
    });
    return { width: image.width, height: image.height, regions };
  }, { exported: [...downloaded.bytes], baseline: [...baseline], probes });
  expect(pixels.width).toBe(Math.ceil(probes.root.width));
  expect(pixels.height).toBe(Math.ceil(probes.root.height));
  for (const region of pixels.regions) {
    expect(region.opaque, region.name).toBeGreaterThan(.95);
    expect(region.contrast, region.name).toBeGreaterThan(15);
    expect(region.meanDifference, region.name).toBeLessThan(35);
  }
  expect(await app.evaluate(() => window.__holdmapCapture.requests)).toBe(0);
  expect(await app.evaluate(() => "__TAURI_INTERNALS__" in window)).toBe(false);
  await preview(app).press("Escape");
  await expect(preview(app)).toHaveCount(0);
  const review = app.getByRole("button", { name: "Review snapshot", exact: true });
  await expect(review).toBeFocused();
  await review.click();
  const retained = await downloadPng(app);
  expect(retained.filename).toBe(downloaded.filename);
  expect(retained.bytes.equals(downloaded.bytes)).toBe(true);
  await preview(app).getByRole("button", { name: "Discard snapshot", exact: true }).click();
  await expect(app.getByRole("button", { name: "Take snapshot", exact: true })).toBeFocused();
});

test("screen PNG stops its source, preserves the last result after denial and retries", async ({ app }) => {
  await app.emulateMedia({ reducedMotion: "no-preference" });
  await app.getByRole("button", { name: "Take snapshot", exact: true }).click();
  await source(app, "Screen or window").click();
  await chooser(app).getByRole("button", { name: "Choose screen…", exact: true }).click();
  await expect(preview(app)).toBeVisible();
  expect(await app.evaluate(() => window.__holdmapCapture.pngOverlayPresence)).toEqual([false]);
  const original = await downloadPng(app);
  expect(await decodeImage(app, original.bytes)).toMatchObject({ width: 320, height: 180 });
  expect(await app.evaluate(() => window.__holdmapCapture.streams[0].getTracks().every((track) => track.readyState === "ended"))).toBe(true);
  await preview(app).getByRole("button", { name: "New snapshot", exact: true }).click();
  await source(app, "Screen or window").click();
  await app.evaluate(() => { window.__holdmapCapture.mode = "denied"; });
  await chooser(app).getByRole("button", { name: "Choose screen…", exact: true }).click();
  await expect(chooser(app).getByRole("alert")).toContainText("cancelled or permission was denied");
  await expect(chooser(app)).toContainText("Your previous snapshot is still available");
  await chooser(app).getByRole("button", { name: "Review last snapshot", exact: true }).click();
  const retained = await downloadPng(app);
  expect(retained.filename).toBe(original.filename);
  expect(retained.bytes.equals(original.bytes)).toBe(true);
  await preview(app).getByRole("button", { name: "New snapshot", exact: true }).click();
  await source(app, "Screen or window").click();
  await app.evaluate(() => { window.__holdmapCapture.mode = "ready"; });
  await chooser(app).getByRole("button", { name: "Choose screen…", exact: true }).click();
  await expect(preview(app)).toBeVisible();
  const retried = await downloadPng(app);
  expect(retried.filename).not.toBe(original.filename);
  expect(await app.evaluate(() => window.__holdmapCapture.requests)).toBe(3);
  expect(await app.evaluate(() => window.__holdmapCapture.streams.every((stream) => stream.getTracks().every((track) => track.readyState === "ended")))).toBe(true);
});

for (const cancel of ["Escape", "toolbar"] as const) {
  test(`${cancel} cancels a pending snapshot and releases its late picker stream`, async ({ app }) => {
    await app.evaluate(() => { window.__holdmapCapture.mode = "pending"; });
    await app.getByRole("button", { name: "Take snapshot", exact: true }).click();
    await source(app, "Screen or window").click();
    await chooser(app).getByRole("button", { name: "Choose screen…", exact: true }).click();
    await expect(chooser(app)).toHaveCount(0);
    const abort = app.getByRole("button", { name: "Cancel snapshot capture", exact: true });
    await expect(abort).toBeVisible();
    if (cancel === "Escape") await app.keyboard.press("Escape"); else await abort.click();
    await app.evaluate(() => { window.__holdmapCapture.pending.shift()?.(); });
    await expect.poll(() => app.evaluate(() => window.__holdmapCapture.streams[0]?.getVideoTracks()[0].readyState)).toBe("ended");
    await expect(preview(app)).toHaveCount(0);
    await expect(app.getByRole("button", { name: "Take snapshot", exact: true })).toBeFocused();
    await expect(app.getByRole("button", { name: "Review snapshot", exact: true })).toHaveCount(0);
  });
}

test("the direct recording snapshot command waits for the animated palette to leave the pixels", async ({ app }) => {
  await app.emulateMedia({ reducedMotion: "no-preference" });
  await app.getByRole("button", { name: "Record screen", exact: true }).click();
  await app.getByRole("dialog", { name: "Record screen", exact: true }).getByRole("button", { name: "Choose screen…", exact: true }).click();
  await expect(app.getByRole("button", { name: "Stop screen recording", exact: true })).toBeVisible();
  await app.getByRole("button", { name: "Commands", exact: true }).click();
  const palette = app.getByRole("dialog", { name: "Command palette", exact: true });
  await palette.getByRole("combobox", { name: "Search commands" }).fill("Snapshot recording source");
  await palette.getByRole("option", { name: /Snapshot recording source/ }).click();
  await expect(preview(app)).toBeVisible();
  expect(await app.evaluate(() => window.__holdmapCapture.pngPalettePresence)).toEqual([false]);
  expect(await app.evaluate(() => window.__holdmapCapture.pngOverlayPresence)).toEqual([false]);
  expect(await app.evaluate(() => window.__holdmapCapture.requests)).toBe(1);
  expect(await app.evaluate(() => window.__holdmapCapture.streams[0].getVideoTracks()[0].readyState)).toBe("live");
});

test("a recording snapshot reuses the source and leaves advancing video and both exports intact", async ({ app }) => {
  test.setTimeout(45_000);
  await app.getByRole("button", { name: "Record screen", exact: true }).click();
  await app.getByRole("dialog", { name: "Record screen", exact: true }).getByRole("button", { name: "Choose screen…", exact: true }).click();
  const stop = app.getByRole("button", { name: "Stop screen recording", exact: true });
  await expect(stop).toBeVisible();
  await app.getByRole("button", { name: "Take snapshot", exact: true }).click();
  await expect(source(app, "Screen or window")).toHaveCount(0);
  await source(app, "Recording source").click();
  await chooser(app).getByRole("button", { name: "Capture PNG", exact: true }).click();
  await expect(preview(app)).toBeVisible();
  await expect(preview(app)).toContainText("Your video is still recording");
  const png = await downloadPng(app);
  const decoded = await decodeImage(app, png.bytes);
  expect(decoded).toMatchObject({ width: 320, height: 180 });
  expect(Math.max(...decoded.rgba.slice(0, 3)) - Math.min(...decoded.rgba.slice(0, 3))).toBeGreaterThan(40);
  await preview(app).getByRole("button", { name: "Close", exact: true }).filter({ hasText: "Close" }).click();
  const time = await app.locator("video").evaluate((video: HTMLVideoElement) => video.currentTime);
  await expect.poll(() => app.locator("video").evaluate((video: HTMLVideoElement) => video.currentTime)).toBeGreaterThan(time + .25);
  expect(await app.evaluate(() => window.__holdmapCapture.requests)).toBe(1);
  expect(await app.evaluate(() => window.__holdmapCapture.streams[0].getVideoTracks()[0].readyState)).toBe("live");
  await stop.click();
  const done = app.getByRole("dialog", { name: "Download recording", exact: true });
  await expect(done).toBeVisible({ timeout: 20_000 });
  const [mp4] = await Promise.all([app.waitForEvent("download"), done.getByRole("button", { name: "Download MP4", exact: true }).click()]);
  const mp4Bytes = await readFile((await mp4.path())!);
  expect(mp4Bytes.subarray(4, 8).toString()).toBe("ftyp");
  const video = await app.evaluate(async (bytes) => {
    const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "video/mp4" }));
    const video = document.createElement("video"); video.muted = true; video.src = url;
    try {
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("Recording PNG interrupted MP4 decoding")), 8_000);
        video.onloadeddata = () => { clearTimeout(timer); resolve(); };
        video.onerror = () => { clearTimeout(timer); reject(new Error("MP4 did not decode")); };
        video.load();
      });
      return { width: video.videoWidth, height: video.videoHeight, duration: video.duration };
    } finally { video.removeAttribute("src"); video.load(); URL.revokeObjectURL(url); }
  }, [...mp4Bytes]);
  expect(video).toMatchObject({ width: 320, height: 180 });
  expect(video.duration).toBeGreaterThan(.25);
  const [gif] = await Promise.all([app.waitForEvent("download"), done.getByRole("button", { name: "Download GIF", exact: true }).click()]);
  const gifBytes = await readFile((await gif.path())!);
  expect(gifBytes.subarray(0, 6).toString()).toMatch(/^GIF8[79]a$/);
  expect(await decodeImage(app, gifBytes, "image/gif")).toMatchObject({ width: 320, height: 180 });
  expect(await app.evaluate(() => window.__holdmapCapture.streams[0].getTracks().every((track) => track.readyState === "ended"))).toBe(true);
  await done.press("Escape");
  await app.getByRole("button", { name: "Review snapshot", exact: true }).click();
  expect((await downloadPng(app)).bytes.equals(png.bytes)).toBe(true);
});

for (const theme of ["dark", "light"]) {
  test(`390px ${theme} snapshot source and preview controls stay contained`, async ({ app }) => {
    await app.setViewportSize({ width: 390, height: 844 });
    await app.evaluate((theme) => localStorage.setItem("holdmap.theme", theme), theme);
    await app.reload();
    await expect(app.locator("html")).toHaveAttribute("data-theme", theme);
    await app.getByRole("button", { name: "Take snapshot", exact: true }).click();
    await contained(app, chooser(app));
    const holdmap = source(app, "Holdmap view");
    await holdmap.focus();
    await holdmap.press("ArrowRight");
    await expect(source(app, "Screen or window")).toBeFocused();
    await expect(source(app, "Screen or window")).toHaveAttribute("aria-checked", "true");
    await chooser(app).getByRole("button", { name: "Choose screen…", exact: true }).click();
    await expect(preview(app)).toBeVisible();
    await contained(app, preview(app));
    const downloaded = await downloadPng(app);
    expect(await decodeImage(app, downloaded.bytes)).toMatchObject({ width: 320, height: 180 });
    await preview(app).getByRole("button", { name: "Discard snapshot", exact: true }).click();
    await expect(app.getByRole("button", { name: "Take snapshot", exact: true })).toBeFocused();
  });
}

// Owned WKWebView fixture: inspect recorder gates and export the actual App's PNG.
// The Python runner substitutes absolute imports. No display picker is permitted.
import { invoke } from __IPC__;
import __APP_CSS__;
import __FONTS_CSS__;

const violations = [];
const errors = [];
document.addEventListener("securitypolicyviolation", (event) => {
  violations.push({ directive: event.violatedDirective, blocked: event.blockedURI });
});
window.addEventListener("error", (event) => errors.push(String(event.message)));
window.addEventListener("unhandledrejection", (event) => errors.push(String(event.reason)));
const report = {
  status: "failed",
  files: {},
  violations,
  errors,
  screenPickerInvoked: false,
  userAgent: navigator.userAgent,
  url: location.href,
  secureContext: globalThis.isSecureContext,
  tauri: "__TAURI_INTERNALS__" in window,
  originalAPIs: {
    mediaDevices: typeof navigator.mediaDevices,
    getDisplayMedia: typeof navigator.mediaDevices?.getDisplayMedia,
    getUserMedia: typeof navigator.mediaDevices?.getUserMedia,
    canvasCaptureStream: typeof HTMLCanvasElement.prototype.captureStream,
  },
};
const originalDisplayCapture = navigator.mediaDevices?.getDisplayMedia;
// Keep a stable, explicitly owned device object across the allocation-heavy DOM
// export. Re-reading a WebKit host getter must never replace our device tripwires.
if (navigator.mediaDevices) Object.defineProperty(navigator, "mediaDevices", {
  configurable: true, value: navigator.mediaDevices,
});

// Preserve absence so the UI sees the real capability, but block every existing device API.
for (const name of ["getDisplayMedia", "getUserMedia"]) {
  if (typeof navigator.mediaDevices?.[name] !== "function") continue;
  const refuse = () => {
    report.screenPickerInvoked = true;
    return Promise.reject(new Error("This UI fixture refuses real device capture."));
  };
  Object.defineProperty(navigator.mediaDevices, name, { configurable: true, value: refuse });
  if (navigator.mediaDevices[name] !== refuse) throw new Error(`Could not guard ${name}`);
}

let app;
let unmount;
const originalObjectURL = URL.createObjectURL;
let exportedPng;
// Observe the actual preview's Blob without changing the production capture or save adapter.
URL.createObjectURL = function (value) {
  if (value instanceof Blob && value.type === "image/png") exportedPng = value;
  return originalObjectURL.call(URL, value);
};
const waitFor = async (read, description, timeout = 8_000) => {
  const deadline = performance.now() + timeout;
  while (performance.now() < deadline) {
    const value = read();
    if (value) return value;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error(`Timed out waiting for ${description}`);
};
const entryReady = async (dialog) => {
  await waitFor(() => [...dialog.querySelectorAll("button")].some((button) => button.textContent.includes("Choose screen"))
    || dialog.textContent.includes("Screen capture unavailable"), "recording capability discovery");
  const choose = [...dialog.querySelectorAll("button")].find((button) => button.textContent.includes("Choose screen"));
  return {
    choose,
    ui: {
      title: document.getElementById(dialog.getAttribute("aria-labelledby"))?.textContent,
      chooseScreenVisible: !!choose && choose.getClientRects().length > 0,
      chooseScreenDisabled: choose?.disabled ?? null,
      unavailableMessage: dialog.textContent.includes("Screen capture unavailable"),
      text: dialog.textContent.replace(/\s+/g, " ").trim(),
    },
  };
};

try {
  localStorage.setItem("holdmap.onboarded", "1");
  localStorage.setItem("holdmap.theme", "dark");
  const target = document.createElement("div");
  target.id = "app";
  document.body.replaceChildren(target);
  const [{ mount, unmount: dispose, tick }, { default: App }, { captureSupport }] = await Promise.all([
    import(__SVELTE__), import(__APP__), import(__CAPTURE__),
  ]);
  unmount = dispose;
  report.support = captureSupport();
  app = mount(App, { target });
  await tick();
  const trigger = await waitFor(() => document.querySelector('button[aria-label="Record screen"]'), "recording trigger");
  report.triggerDisabled = trigger.disabled;
  trigger.click();
  const dialog = await waitFor(() => document.querySelector('[role="dialog"][aria-labelledby="dialog-title"]')
    ?? [...document.querySelectorAll('[role="dialog"]')].find((element) => element.textContent.includes("Record screen")), "recording dialog");
  await tick();
  const { choose, ui } = await entryReady(dialog);
  report.ui = ui;
  const originalAvailable = report.originalAPIs.getDisplayMedia === "function";
  if (report.support.display !== originalAvailable) throw new Error("Helper disagrees with original browser capability");
  if (originalAvailable && (!choose || report.ui.unavailableMessage)) {
    throw new Error("Production App/dialog disagrees with the original browser capability");
  }
  report.originalSupportedViaNative = !originalAvailable && !!choose;
  // Close without ever clicking Choose screen.
  [...dialog.querySelectorAll("button")].find((button) => button.textContent.trim() === "Cancel")?.click();
  await tick();
  await waitFor(() => !dialog.isConnected, "recording dialog outro");
  report.closedWithoutCapture = !document.querySelector('[role="dialog"]');
  await unmount(app);
  app = null;
  target.replaceChildren();
  // Reproduce only the missing-API condition. The real capability command may run;
  // the native fixture rejects capture-start commands, and this test never clicks Choose.
  if (!navigator.mediaDevices) Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: {} });
  Object.defineProperty(navigator.mediaDevices, "getDisplayMedia", { configurable: true, value: undefined });
  report.forcedAPIs = { getDisplayMedia: typeof navigator.mediaDevices.getDisplayMedia };
  app = mount(App, { target });
  await tick();
  const fallbackTrigger = await waitFor(() => document.querySelector('button[aria-label="Record screen"]'), "fallback recording trigger");
  fallbackTrigger.click();
  const fallbackDialog = await waitFor(() => [...document.querySelectorAll('[role="dialog"]')].find((element) => element.textContent.includes("Record screen")), "fallback recording dialog");
  const fallback = await entryReady(fallbackDialog);
  report.fallbackUI = fallback.ui;
  report.nativeFallbackSkipped = fallback.ui.unavailableMessage && /macOS 14/.test(fallback.ui.text);
  if (!report.nativeFallbackSkipped && (!fallback.ui.chooseScreenVisible || fallback.ui.chooseScreenDisabled || fallback.ui.unavailableMessage)) throw new Error(`Native fallback did not enable the screen picker entry point: ${fallback.ui.text}`);
  [...fallbackDialog.querySelectorAll("button")].find((button) => button.textContent.trim() === "Cancel")?.click();
  await tick();
  await waitFor(() => !fallbackDialog.isConnected, "fallback recording dialog outro");
  report.fallbackClosedWithoutCapture = !document.querySelector('[role="dialog"]');

  // Capture this owned App, including its bundled fonts and icon, through the real UI.
  // Native source-start commands remain tripwires and getDisplayMedia is absent.
  report.snapshotStage = "prepare owned App";
  const captureRoot = document.getElementById("holdmap-capture-root");
  if (!captureRoot) throw new Error("The production App did not expose its capture viewport");
  await document.fonts.ready;
  const bounds = captureRoot.getBoundingClientRect();
  const region = (selector) => {
    const element = captureRoot.querySelector(selector);
    const box = element?.getBoundingClientRect();
    if (!box || box.width < 1 || box.height < 1) throw new Error(`Missing App pixel region ${selector}`);
    return { x: box.left - bounds.left, y: box.top - bounds.top, width: box.width, height: box.height };
  };
  const regions = [{ name: "bundled icon", ...region(".brand img") }, { name: "bundled font", ...region(".brand .name") }];
  report.snapshotStage = "open snapshot chooser";
  const snapshotTrigger = await waitFor(() => document.querySelector('button[aria-label="Take snapshot"]'), "snapshot trigger");
  snapshotTrigger.click();
  const chooser = await waitFor(() => [...document.querySelectorAll('[role="dialog"]')].find((element) => element.textContent.includes("This Holdmap view")), "snapshot source chooser");
  const capture = [...chooser.querySelectorAll("button")].find((button) => button.textContent.trim() === "Capture PNG");
  if (!capture || capture.disabled) throw new Error("Holdmap-only PNG was not available without a display API");
  capture.click();
  report.snapshotStage = "capture App PNG";
  const preview = await waitFor(() => document.querySelector('img[alt="PNG snapshot preview"]'), "actual App PNG preview", 20_000);
  await preview.decode();
  report.snapshotStage = "decode App PNG";
  if (!exportedPng) throw new Error("The App did not create a PNG preview Blob");
  const ratio = Math.max(1, Math.min(2, window.devicePixelRatio || 1));
  const scale = Math.min(ratio, 3840 / Math.ceil(bounds.width), 2160 / Math.ceil(bounds.height));
  const expectedWidth = Math.max(1, Math.floor(Math.ceil(bounds.width) * scale));
  const expectedHeight = Math.max(1, Math.floor(Math.ceil(bounds.height) * scale));
  if (preview.naturalWidth !== expectedWidth || preview.naturalHeight !== expectedHeight) throw new Error("App PNG dimensions do not match the bounded viewport");
  // WKWebView can return transparent canvas readback for this connected preview
  // despite a fully rendered saved PNG. The runner independently decodes its exact
  // native-saved bytes and checks these font/icon regions with FFmpeg instead.
  const pngBytes = await exportedPng.arrayBuffer();
  const sha256 = [...new Uint8Array(await crypto.subtle.digest("SHA-256", pngBytes))].map((value) => value.toString(16).padStart(2, "0")).join("");
  report.files.png = { bytes: pngBytes.byteLength, sha256, type: exportedPng.type };
  report.appSnapshot = { width: preview.naturalWidth, height: preview.naturalHeight, regions, devicePixelRatio: window.devicePixelRatio, cssBounds: { x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height }, missingDisplayAPI: typeof navigator.mediaDevices.getDisplayMedia === "undefined" };
  const previewDialog = preview.closest('[role="dialog"]');
  const download = [...previewDialog.querySelectorAll("button")].find((button) => button.textContent.trim() === "Download PNG");
  if (!download || download.disabled) throw new Error("App PNG has no enabled save action");
  download.click();
  report.snapshotStage = "save App PNG";
  await waitFor(() => previewDialog.textContent.includes("Saved to "), "actual App binary PNG save");
  const close = [...previewDialog.querySelectorAll("button")].find((button) => button.textContent.trim() === "Close");
  close.click();
  await waitFor(() => !previewDialog.isConnected, "snapshot preview dismissal");
  report.appSnapshot.closedAfterSave = true;
  report.status = "passed";
} catch (cause) {
  report.error = `${String(cause?.message || cause)}\n${String(cause?.stack || "")}`.slice(0, 4_000);
  report.activeDialog = [...document.querySelectorAll('[role="dialog"]')].map((element) => element.textContent.replace(/\s+/g, " ").trim()).join("\n").slice(0, 4_000);
  report.viewport = { width: innerWidth, height: innerHeight };
} finally {
  if (app && unmount) await unmount(app);
  URL.createObjectURL = originalObjectURL;
  // Keep the fixture guarded even after restoring the original API's presence.
  if (typeof originalDisplayCapture === "function") Object.defineProperty(navigator.mediaDevices, "getDisplayMedia", {
    configurable: true,
    value: () => { report.screenPickerInvoked = true; return Promise.reject(new Error("Real capture is disabled in this fixture.")); },
  });
  await invoke("finish_fixture", { report });
}

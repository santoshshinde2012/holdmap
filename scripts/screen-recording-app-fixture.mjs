// Owned WKWebView fixture: mount the production App, inspect its recorder gate, then exit.
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
const waitFor = async (read, description) => {
  const deadline = performance.now() + 8_000;
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
  report.status = "passed";
} catch (cause) {
  report.error = String(cause?.stack || cause).slice(0, 4_000);
} finally {
  if (app && unmount) await unmount(app);
  // Keep the fixture guarded even after restoring the original API's presence.
  if (typeof originalDisplayCapture === "function") Object.defineProperty(navigator.mediaDevices, "getDisplayMedia", {
    configurable: true,
    value: () => { report.screenPickerInvoked = true; return Promise.reject(new Error("Real capture is disabled in this fixture.")); },
  });
  await invoke("finish_fixture", { report });
}

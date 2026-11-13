import { mount, unmount } from "svelte";
import "./fonts.css";
import "./app.css";
import App from "./App.svelte";
import { prefGet } from "./lib/prefs";
import { isBenignError } from "./lib/recover";

const target = document.getElementById("app")!;

type Mounted = ReturnType<typeof mount>;

/** Why the last remount happened, shown once in the header so a blank screen isn't silent. */
declare global {
  interface Window {
    __holdmapRecovered?: string;
  }
}

/** The UI kit gallery loads on demand and is left out of the website demo build. */
async function gallery(): Promise<void> {
  document.documentElement.dataset.theme =
    prefGet("theme") === "dark" ? "dark" : "light";
  const { default: Gallery } = await import("./dev/Gallery.svelte");
  mount(Gallery, { target });
}

function app(): Mounted {
  return mount(App, { target });
}

function start(): Mounted | null {
  if (!__HOLDMAP_SITE_DEMO__ && location.hash === "#ui-gallery") {
    void gallery();
    return null;
  }
  return app();
}

let current: Mounted | null = start();
let remounting = false;

/**
 * Remount the app after an uncaught error. Without this, a throw in a Svelte effect or an
 * IPC handler leaves the WebView blank (white content, only the native title bar) until the
 * user quits. We remount at most once every few seconds so a stuck error doesn't loop.
 */
function recover(why: string): void {
  if (remounting || location.hash === "#ui-gallery") return;
  remounting = true;
  console.error("[holdmap] recovering UI after", why);
  window.__holdmapRecovered = why;
  try {
    if (current) unmount(current);
  } catch (e) {
    console.error("[holdmap] unmount during recover failed", e);
  }
  target.replaceChildren();
  try {
    current = app();
  } catch (e) {
    console.error("[holdmap] remount failed", e);
    target.textContent = "holdmap hit a problem and couldn't recover. Press ⌘R or quit and reopen.";
  }
  setTimeout(() => {
    remounting = false;
  }, 3_000);
}

window.addEventListener("error", (e) => {
  if (!isBenignError(e.message ?? "")) recover(e.message || "error");
});
window.addEventListener("unhandledrejection", (e) =>
  recover(e.reason instanceof Error ? e.reason.message : String(e.reason ?? "rejection")),
);

export default current;

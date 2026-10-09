import { mount, unmount } from "svelte";
import "./fonts.css";
import "./app.css";
import App from "./App.svelte";
import Gallery from "./dev/Gallery.svelte";
import { prefGet } from "./lib/prefs";

const target = document.getElementById("app")!;

type Mounted = ReturnType<typeof mount>;

/** Why the last remount happened, shown once in the header so a blank screen isn't silent. */
declare global {
  interface Window {
    __holdmapRecovered?: string;
  }
}

function gallery(): Mounted {
  document.documentElement.dataset.theme =
    prefGet("theme") === "dark" ? "dark" : "light";
  return mount(Gallery, { target });
}

function app(): Mounted {
  return mount(App, { target });
}

function start(): Mounted {
  return location.hash === "#ui-gallery" ? gallery() : app();
}

let current: Mounted = start();
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
    unmount(current);
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

window.addEventListener("error", (e) => recover(e.message || "error"));
window.addEventListener("unhandledrejection", (e) =>
  recover(e.reason instanceof Error ? e.reason.message : String(e.reason ?? "rejection")),
);

export default current;

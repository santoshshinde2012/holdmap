import { configDefaults, defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { liveCollector } from "./vite-live.ts";

const host = process.env.TAURI_DEV_HOST;

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [svelte(), liveCollector()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
    // The browser demo reads the analyzer presets the Rust crate compiles in.
    fs: { allow: [".", "../../crates/holdmap-analyzers/presets"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  // The website demo leaves out developer-only pages such as the #ui-gallery UI kit.
  define: { __HOLDMAP_SITE_DEMO__: JSON.stringify(!!process.env.HOLDMAP_SITE_DEMO) },
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari15",
    // Vite 8 minifies with Oxc (esbuild is no longer bundled).
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // The app, and the tray popup panel as its own small page (the site's browser demo has no
    // tray, so its build leaves the panel out).
    rollupOptions: { input: process.env.HOLDMAP_SITE_DEMO ? { main: "index.html" } : { main: "index.html", tray: "tray.html" } },
    // The Home 3D view carries three.js in its own on-demand chunk (budgeted in site/scripts/check-budget.mjs).
    chunkSizeWarningLimit: 700,
  },
  // Component tests mount real Svelte 5 components in jsdom (client build, not SSR).
  resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    exclude: [...configDefaults.exclude, "e2e/**"],
    // Bound concurrent jsdom/Svelte transforms so CPU contention cannot starve test timers.
    maxWorkers: 4,
  },
});

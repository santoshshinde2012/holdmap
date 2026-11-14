import { configDefaults, defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { liveCollector } from "./vite-live.ts";
import { fileURLToPath } from "node:url";

const host = process.env.TAURI_DEV_HOST;

// https://v2.tauri.app/start/frontend/vite/
// d3 3.0.2+ ships each package as one bundled module, so a bundler can no longer drop files
// we never use. Mark d3-interpolate's top-level colour-space factories pure: they only build
// closures, and the graphs use none of them (HSL, HCL, cubehelix, basis splines).
const D3_PURE = /^(var \w+ = )((?:hsl|hcl|cubehelix|rgbSpline)\()/gm;
// d3-color's Lab, HCL and cubehelix classes are only reached through those interpolators, so
// their prototype setup is pure for this app too. Never import them from the app (a test checks).
const D3_COLOR_PURE = /^define\((Lab|Hcl|Cubehelix), /gm;
const d3Pure = { name: "holdmap-d3-pure", transform(code: string, id: string) {
  if (/[\\/]d3-interpolate[\\/]dist[\\/][^\\/]+\.mjs$/.test(id)) return code.replace(D3_PURE, "$1/*#__PURE__*/$2");
  if (/[\\/]d3-color[\\/]dist[\\/][^\\/]+\.mjs$/.test(id)) return code.replace(D3_COLOR_PURE, "/*#__PURE__*/define($1, ");
  return null;
} };

export default defineConfig({
  plugins: [svelte(), liveCollector(), d3Pure],
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
  resolve: {
    // d3-transition needs one easing; the shim keeps the rest of d3-ease out of the graph chunk.
    alias: { "d3-ease": fileURLToPath(new URL("./src/lib/vendor/d3-ease.ts", import.meta.url)) },
    ...(process.env.VITEST ? { conditions: ["browser"] } : {}),
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    exclude: [...configDefaults.exclude, "e2e/**"],
    // Bound concurrent jsdom/Svelte transforms so CPU contention cannot starve test timers.
    maxWorkers: 4,
  },
});

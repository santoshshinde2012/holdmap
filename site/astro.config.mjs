// @ts-check
import { defineConfig } from "astro/config";

// Served from GitHub Pages at https://santoshshinde2012.github.io/holdmap/.
export default defineConfig({
  site: "https://santoshshinde2012.github.io",
  base: "/holdmap",
  trailingSlash: "always",
  build: { format: "directory", inlineStylesheets: "always" },
  compressHTML: true,
  devToolbar: { enabled: false },
  markdown: {
    shikiConfig: { themes: { light: "github-light", dark: "github-dark-dimmed" }, defaultColor: false },
  },
  image: { responsiveStyles: false },
  vite: {
    // The guide renders ../docs/cli.md and the hero uses ../docs/screenshots.
    server: { fs: { allow: [".."] } },
  },
});

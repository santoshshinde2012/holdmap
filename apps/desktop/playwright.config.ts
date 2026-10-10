import { defineConfig } from "@playwright/test";

const baseURL = "http://127.0.0.1:4175";
const executablePath = process.env.HOLDMAP_E2E_CHROMIUM_EXECUTABLE ?? process.env.PORTWISE_E2E_CHROMIUM_EXECUTABLE;

export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/*.test.ts",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 1 : 2,
  timeout: 30_000,
  reporter: "list",
  use: {
    baseURL,
    headless: true,
    viewport: { width: 1440, height: 1000 },
    reducedMotion: "reduce",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [{
    name: "chromium",
    use: {
      browserName: "chromium",
      ...(executablePath ? { launchOptions: { executablePath } } : {}),
    },
  }],
  webServer: {
    command: "npm run preview -- --host 127.0.0.1 --port 4175 --strictPort",
    url: baseURL,
    reuseExistingServer: false,
    timeout: 30_000,
    stdout: "ignore",
    stderr: "pipe",
    gracefulShutdown: { signal: "SIGTERM", timeout: 1_000 },
  },
});

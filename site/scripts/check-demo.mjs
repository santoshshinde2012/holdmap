// End-to-end check of the built website, or an explicitly supplied public/preview URL:
// the iframe loads, the "Try" buttons drive the real app, and a simulated stop removes :3000.
//   [CHROME=/path/to/chrome] node scripts/check-demo.mjs [base-url]
// Reuses the locked desktop browser-test dependency; install both projects before running.
const playwright = process.env.PLAYWRIGHT ?? new URL("../../apps/desktop/node_modules/playwright/index.mjs", import.meta.url).href;
const { chromium } = await import(playwright);
const { previewSite } = await import("./preview.mjs");
const preview = process.argv[2] ? null : await previewSite();
const base = process.argv[2] ?? preview.url;

let browser;
const results = [];
try {
  browser = await chromium.launch({ executablePath: process.env.CHROME || undefined });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  const check = async (name, fn) => {
    try { await fn(); results.push(["PASS", name]); } catch (e) { results.push(["FAIL", `${name}: ${e.message.split("\n")[0]}`]); }
  };

  await page.goto(base, { waitUntil: "domcontentloaded" });
  await page.locator("#demo").scrollIntoViewIfNeeded();
  const frame = page.frameLocator("#demo iframe");
  const try_ = (label) => page.locator(".try", { hasText: label }).click();

  await check("iframe loads the app with sample ports", async () => {
    await frame.locator('[id^="row-tcp:3000:"]').waitFor({ timeout: 15000 });
    await page.locator(".try:not([disabled])").first().waitFor({ timeout: 5000 });
  });
  await check("Inspect :3000 selects the port and shows its details", async () => {
    await try_("Inspect :3000");
    await frame.locator('[id^="row-tcp:3000:"][aria-selected="true"]').waitFor({ timeout: 5000 });
    await frame.getByText("shop-web").first().waitFor();
  });
  await check("Command palette opens", async () => {
    await try_("Command palette");
    await frame.locator("#palette-list").waitFor({ timeout: 5000 });
    await page.keyboard.press("Escape");
    await frame.locator("#palette-list").waitFor({ state: "detached", timeout: 5000 });
  });
  await check("Real keystrokes reach the app (Ctrl/⌘+K inside the frame)", async () => {
    await frame.locator("#port-list").click({ position: { x: 5, y: 5 } });
    await page.keyboard.press("Control+k");
    await frame.locator("#palette-list").waitFor({ timeout: 5000 });
    await page.keyboard.press("Escape");
  });
  await check("Service graph renders", async () => {
    await try_("Service graph");
    await frame.locator(".svelte-flow").waitFor({ timeout: 8000 });
    await frame.getByText("shop-api").first().waitFor();
  });
  await check("Agents shows searchable tool metadata without a listening port", async () => {
    await try_("Agents");
    await frame.getByRole("article", { name: "Cursor", exact: true }).waitFor({ timeout: 8000 });
    const search = frame.getByLabel("Search agents", { exact: true });
    await search.fill("filesystem");
    await frame.getByRole("article", { name: "Cursor", exact: true }).waitFor();
    await frame.getByRole("button", { name: /^MCP server Filesystem MCP,.*Opens its agent card$/ }).click();
    await frame.getByText("MCP server · inferred · no listening port", { exact: true }).waitFor();
    await search.press("Escape");
  });
  await check("Stop :3000 asks first, then frees the port (simulated)", async () => {
    await try_("Stop :3000");
    const dialog = frame.locator('[role="alertdialog"]');
    await dialog.waitFor({ timeout: 5000 });
    await dialog.locator("[data-primary]").click();
    await frame.locator('[id^="row-tcp:3000:"]').waitFor({ state: "detached", timeout: 10000 });
  });
  await check("Reset brings :3000 back", async () => {
    await try_("Reset");
    await frame.locator('[id^="row-tcp:3000:"]').waitFor({ timeout: 15000 });
  });
  await check("No console errors", async () => { if (errors.length) throw new Error(errors.join(" | ")); });

} finally {
  try { await browser?.close(); } finally { await preview?.close(); }
}
for (const [s, n] of results) console.log(`${s}  ${n}`);
process.exit(results.some(([s]) => s === "FAIL") ? 1 : 0);

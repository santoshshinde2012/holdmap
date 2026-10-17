// Records the hero video: the live-demo build driven by a scripted tour, captured frame by frame
// against a fake clock (Web Animations are paused and seeked to it), so nothing drops frames.
//   npm run build && npx astro preview --port 4330
//   PLAYWRIGHT=/path/to/node_modules/playwright/index.mjs [CHROME=/path/to/chrome] \
//     node scripts/record-hero.mjs <desktop|mobile> <dark|light> [fps] [base-url]
// Frames land in .hero-frames/<variant>-<theme>/; scripts/encode-hero.sh turns them into the videos.
import { writeFileSync, mkdirSync, rmSync } from "node:fs";
const { chromium } = await import(process.env.PLAYWRIGHT ?? new URL("../../apps/desktop/node_modules/playwright/index.mjs", import.meta.url).href);
const [, , variant = "desktop", theme = "dark", fpsArg = "60", base = "http://127.0.0.1:4330/holdmap/"] = process.argv;
const FPS = +fpsArg, M = variant === "mobile";
if (!["desktop", "mobile"].includes(variant) || !["dark", "light"].includes(theme) || !Number.isInteger(FPS) || FPS < 1 || FPS > 120) {
  throw new Error("Expected <desktop|mobile> <dark|light> and an integer frame rate from 1 to 120.");
}
const VW = M ? 400 : 1152, VH = M ? 560 : 720, DPR = M ? 3 : 2;
const dir = `.hero-frames/${variant}-${theme}`;
rmSync(dir, { recursive: true, force: true }); mkdirSync(dir, { recursive: true });

const b = await chromium.launch({ executablePath: process.env.CHROME || undefined, args: ["--hide-scrollbars", `--force-device-scale-factor=${DPR}`] });
try {
const ctx = await b.newContext({ viewport: { width: VW, height: VH }, deviceScaleFactor: DPR, hasTouch: M, isMobile: false,
  userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36" });
await ctx.addInitScript(() => { Object.defineProperty(Navigator.prototype, "platform", { get: () => "MacIntel" }); });
const p = await ctx.newPage();
const errors = [];
p.on("pageerror", (error) => errors.push(error.message));
await p.clock.install();
await p.goto(`${base}demo/?theme=${theme}`, { waitUntil: "networkidle" });
if (await p.evaluate(() => "__TAURI_INTERNALS__" in window)) throw new Error("Hero capture requires the browser demo with simulated stops.");
await p.addStyleTag({ content: `a[style*="position:fixed"]{display:none!important} *{cursor:none!important;caret-color:transparent!important} ::-webkit-scrollbar{display:none}${M ? " .app.mac .titlebar{padding-left:16px!important}" : ""}` });
await p.locator("#home-overview").waitFor();
await p.getByRole("button", { name: "Port 3000 Next.js", exact: true }).first().waitFor();
await p.waitForTimeout(600);
await p.evaluate(({ M, dark }) => {
  const ease = (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
  const c = document.createElement("div");
  c.style.cssText = "position:fixed;left:0;top:0;z-index:2147483647;pointer-events:none;opacity:0;transition:opacity 220ms ease";
  c.innerHTML = M
    ? `<div class="dot" style="width:34px;height:34px;margin:-17px 0 0 -17px;border-radius:50%;background:${dark ? "rgb(255 255 255/.2)" : "rgb(10 10 11/.14)"};box-shadow:0 0 0 1.5px ${dark ? "rgb(255 255 255/.6)" : "rgb(10 10 11/.35)"};transition:transform 160ms ease"></div>`
    : `<svg class="dot" width="22" height="22" viewBox="0 0 22 22" style="display:block;margin:-2px 0 0 -3px;filter:drop-shadow(0 1px 2px rgb(0 0 0/.35));transition:transform 140ms ease;transform-origin:4px 3px"><path d="M4 2.5v15.2l3.9-3.6 2.5 5.6 2.6-1.1-2.5-5.5 5.3-.3z" fill="#111" stroke="#fff" stroke-width="1.4" stroke-linejoin="round"/></svg>`;
  const ring = document.createElement("div");
  ring.style.cssText = `position:fixed;z-index:2147483646;pointer-events:none;width:28px;height:28px;margin:-14px 0 0 -14px;border-radius:50%;border:2px solid ${dark ? "rgb(255 255 255/.55)" : "rgb(79 75 223/.55)"};opacity:0`;
  const k = document.createElement("div");
  k.style.cssText = `position:fixed;left:50%;bottom:${M ? 20 : 32}px;translate:-50% 8px;z-index:2147483647;pointer-events:none;display:flex;gap:6px;padding:8px;border-radius:12px;opacity:0;transition:opacity 180ms ease,translate 180ms ease;background:rgb(17 17 19/.94);box-shadow:0 0 0 1px rgb(255 255 255/.1),0 12px 32px rgb(0 0 0/.35);font:500 15px/1 Inter,system-ui,sans-serif;color:#fff`;
  document.body.append(c, ring, k);
  let x = 0, y = 0;
  const place = () => (c.style.transform = `translate(${x}px,${y}px)`);
  window.__cur = {
    show(nx, ny) { x = nx; y = ny; place(); c.style.opacity = "1"; },
    hide() { c.style.opacity = "0"; },
    move(nx, ny, ms) { const sx = x, sy = y, t0 = performance.now(); const f = () => { const t = Math.min(1, (performance.now() - t0) / ms), e = ease(t); x = sx + (nx - sx) * e; y = sy + (ny - sy) * e; place(); if (t < 1) requestAnimationFrame(f); }; requestAnimationFrame(f); },
    click() {
      const d = c.querySelector(".dot"); d.style.transform = "scale(.86)"; setTimeout(() => (d.style.transform = ""), 150);
      ring.style.left = x + "px"; ring.style.top = y + "px";
      ring.animate([{ opacity: 0.9, transform: "scale(.4)" }, { opacity: 0, transform: "scale(1.5)" }], { duration: 460, easing: "ease-out" });
    },
  };
  window.__keys = (keys, ms) => {
    k.innerHTML = keys.map((s) => `<span style="min-width:30px;height:30px;padding:0 9px;display:grid;place-items:center;border-radius:7px;background:rgb(255 255 255/.1)">${s}</span>`).join("");
    k.style.opacity = "1"; k.style.translate = "-50% 0";
    setTimeout(() => { k.style.opacity = "0"; k.style.translate = "-50% 8px"; }, ms);
  };
  // Drive every CSS/Web animation from the (fake) performance clock.
  const starts = new WeakMap(), done = new WeakSet();
  window.__sync = () => {
    const now = performance.now();
    for (const a of document.getAnimations()) {
      if (done.has(a)) continue;
      if (!starts.has(a)) starts.set(a, now - (a.currentTime ?? 0));
      const t = (now - starts.get(a)) * (a.playbackRate || 1);
      const end = a.effect?.getComputedTiming().endTime;
      if (Number.isFinite(end) && t >= end) { done.add(a); try { a.finish(); } catch {} continue; }
      a.pause(); a.currentTime = t;
    }
  };
}, { M, dark: theme === "dark" });
await p.clock.pauseAt(await p.evaluate(() => Date.now() + 50));
await p.clock.runFor(50);

const cdp = await ctx.newCDPSession(p);
let frame = 0;
const marks = {};
const mark = (k) => (marks[k] = +(frame / FPS).toFixed(3));
async function step() {
  const ms = Math.round(((frame + 1) * 1000) / FPS) - Math.round((frame * 1000) / FPS);
  await p.clock.runFor(ms);
  await p.evaluate(() => window.__sync());
  const { data } = await cdp.send("Page.captureScreenshot", { format: "png", optimizeForSpeed: true });
  writeFileSync(`${dir}/${String(frame).padStart(5, "0")}.png`, Buffer.from(data, "base64"));
  frame++;
}
const hold = async (ms) => { for (let i = 0, n = Math.round((ms * FPS) / 1000); i < n; i++) await step(); };
const until = async (fn, max = 4000) => { for (let i = 0; i < (max * FPS) / 1000; i++) { if (await fn()) return; await step(); } throw new Error(`Tour condition timed out after ${max}ms.`); };
const center = async (loc) => { const r = await loc.boundingBox(); if (!r) throw new Error(`Tour target is not visible: ${loc}`); return [r.x + r.width / 2, r.y + r.height / 2]; };
const move = async (xy, ms) => { await p.evaluate(([x, y, ms]) => window.__cur.move(x, y, ms), [...xy, ms]); await hold(ms); await p.mouse.move(...xy); };
const click = async (xy) => { await p.evaluate(() => window.__cur.click()); if (M) { await p.touchscreen.tap(...xy); await p.mouse.move(-1, -1); } else await p.mouse.click(...xy); }; // touch leaves no hover
const keys = (k, ms = 900) => p.evaluate(([k, ms]) => window.__keys(k, ms), [k, ms]);
const show = (xy) => p.evaluate(([x, y]) => window.__cur.show(x, y), xy);
const t0 = Date.now();

// Home Cards → Map → a port's menu → Stop… → the reviewed plan → the port freed → Agents' MCP &
// skills, then back to Home Cards so encode-hero.sh can dissolve the last frame into the first.
const portNode = () => M
  ? p.getByRole("button", { name: "Port 3000 Next.js", exact: true }).first()
  : p.locator('svg[aria-label="Machine map"] [role="button"][aria-label*=":3000"]').first();
// Bring the target into view by easing its scrollers over `ms` on the fake clock, so the video
// shows the movement (the toolbar scrolls sideways on narrow windows, the page vertically).
const reveal = async (loc, ms = 600) => {
  const plan = await loc.evaluate((el) => {
    const out = [];
    const r = el.getBoundingClientRect();
    for (let s = el.parentElement; s; s = s.parentElement) {
      const cs = getComputedStyle(s), v = s.getBoundingClientRect();
      const top = s === document.documentElement ? 0 : v.top, bottom = s === document.documentElement ? innerHeight : v.bottom;
      const left = s === document.documentElement ? 0 : v.left, right = s === document.documentElement ? innerWidth : v.right;
      const dx = s.scrollWidth > s.clientWidth + 1 && /auto|scroll/.test(cs.overflowX) && (r.left < left + 8 || r.right > right - 8) ? Math.round(r.left + r.width / 2 - (left + right) / 2) : 0;
      const dy = s.scrollHeight > s.clientHeight + 1 && /auto|scroll/.test(cs.overflowY) && (r.top < top + 40 || r.bottom > bottom - 90) ? Math.round(r.top + r.height / 2 - (top + bottom) / 2) : 0;
      if (dx || dy) { s.dataset.heroScroll = String(out.length); out.push([dx, dy]); }
    }
    return out;
  });
  if (!plan.length) return;
  const ease = (t) => (t < 0.5 ? 4 * t ** 3 : 1 - (-2 * t + 2) ** 3 / 2);
  const n = Math.max(1, Math.round((ms * FPS) / 1000));
  for (let i = 1; i <= n; i++) {
    const k = ease(i / n) - ease((i - 1) / n);
    await p.evaluate(([plan, k]) => plan.forEach(([dx, dy], j) => document.querySelector(`[data-hero-scroll="${j}"]`)?.scrollBy(dx * k, dy * k)), [plan, k]);
    await step();
  }
  await p.evaluate(() => document.querySelectorAll("[data-hero-scroll]").forEach((s) => delete s.dataset.heroScroll));
};
const choose = async (loc, ms = 750) => { await reveal(loc); await move(await center(loc), ms); await hold(120); await click(await center(loc)); };

mark("home");
await hold(1500);
// The first card in view: Cursor's :3000 beside the map on desktop, Claude Code's :3001 on phones.
const chip = p.getByRole("article", { name: M ? "Claude Code" : "Cursor", exact: true }).getByRole("button", { name: M ? "Port 3001 Express" : "Port 3000 Next.js", exact: true });
await show(M ? [VW * 0.62, VH * 0.6] : [VW * 0.66, VH * 0.62]);
await hold(250);
await move(await center(chip), 900);
await hold(1300);

mark("map");
const layout = p.getByRole("radiogroup", { name: "Home layout", exact: true });
if (M) await choose(layout.getByRole("radio", { name: "Map", exact: true }));
else {
  await keys(["V"], 900);
  await hold(200);
  await p.keyboard.press("v");
}
await until(async () => (await layout.getByRole("radio", { name: "Map", exact: true }).getAttribute("aria-checked")) === "true");
await hold(1700);
await choose(portNode());
const menu = p.getByRole("menu");
await until(async () => (await menu.count()) > 0);
await hold(900);

mark("stop");
await choose(menu.getByRole("menuitem", { name: "Stop…", exact: true }), 650);
const plan = p.locator('[role="alertdialog"]');
await until(async () => (await plan.count()) > 0);
await hold(2600);
await choose(plan.locator("[data-primary]").first(), 800);
const dismiss = p.getByRole("button", { name: "Dismiss notification" });
await until(async () => (await dismiss.count()) > 0, 6000);
await until(async () => (await p.getByRole("button", { name: /^Port 3000 / }).count()) === 0 && (await plan.count()) === 0, 6000);
await hold(1800);
await choose(dismiss.first(), 650);
await until(async () => (await dismiss.count()) === 0);
await hold(300);

mark("agents");
const views = p.getByRole("radiogroup", { name: "View", exact: true });
if (M) await choose(views.getByRole("radio", { name: /^Agents/ }));
else {
  await p.evaluate(() => window.__cur.hide());
  await keys(["⇧", "A"], 900);
  await hold(200);
  await p.keyboard.press("Shift+A");
}
await until(async () => (await views.getByRole("radio", { name: /^Agents/ }).getAttribute("aria-checked")) === "true");
await hold(700);
await choose(p.getByRole("radio", { name: "MCP & skills", exact: true }));
await until(async () => (await p.locator("#agent-capabilities").count()) > 0);
await hold(2800);

// Back to the opening frame: Home, Cards and no hover.
await choose(views.getByRole("radio", { name: "Home", exact: true }), 700);
await until(async () => (await p.locator("#home-overview").count()) > 0);
await hold(300);
await choose(layout.getByRole("radio", { name: "Cards", exact: true }), 650);
await hold(300);
await p.mouse.move(-1, -1);
await p.evaluate(() => { window.__cur.hide(); for (const s of document.querySelectorAll("*")) if (s.scrollTop || s.scrollLeft) s.scrollTo(0, 0); });
await hold(900);
mark("end");
if (await p.locator('[role="alertdialog"]').count()) throw new Error("The simulated stop dialog remained open.");
if ((await p.getByRole("radiogroup", { name: "Home layout", exact: true }).getByRole("radio", { name: "Cards", exact: true }).getAttribute("aria-checked")) !== "true") throw new Error("The tour must end on Home Cards.");
if (errors.length) throw new Error(`Browser errors during the tour: ${errors.join("; ")}`);
writeFileSync(`${dir}/meta.json`, JSON.stringify({ variant, theme, VW, VH, DPR, FPS, frames: frame, marks }, null, 1));
console.log(variant, theme, frame, "frames in", ((Date.now() - t0) / 1000).toFixed(0), "s", JSON.stringify(marks));
} finally {
  await b.close();
}

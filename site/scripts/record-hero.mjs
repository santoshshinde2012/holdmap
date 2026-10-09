// Records the hero video: the live-demo build driven by a scripted tour, captured frame by frame
// against a fake clock (Web Animations are paused and seeked to it), so nothing drops frames.
//   npm run build && npx astro preview --port 4330
//   PLAYWRIGHT=/path/to/node_modules/playwright/index.mjs [CHROME=/path/to/chrome] \
//     node scripts/record-hero.mjs <desktop|mobile> <dark|light> [fps] [base-url]
// Frames land in .hero-frames/<variant>-<theme>/; scripts/encode-hero.sh turns them into the videos.
import { writeFileSync, mkdirSync, rmSync } from "node:fs";
const { chromium } = await import(process.env.PLAYWRIGHT ?? "playwright");
const [, , variant = "desktop", theme = "dark", fpsArg = "60", base = "http://127.0.0.1:4330/holdmap/"] = process.argv;
const FPS = +fpsArg, M = variant === "mobile";
const VW = M ? 400 : 1152, VH = M ? 560 : 720, DPR = M ? 3 : 2;
const dir = `.hero-frames/${variant}-${theme}`;
rmSync(dir, { recursive: true, force: true }); mkdirSync(dir, { recursive: true });

const b = await chromium.launch({ executablePath: process.env.CHROME || undefined, args: ["--hide-scrollbars", `--force-device-scale-factor=${DPR}`] });
const ctx = await b.newContext({ viewport: { width: VW, height: VH }, deviceScaleFactor: DPR, hasTouch: M, isMobile: false,
  userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36" });
await ctx.addInitScript(() => { Object.defineProperty(Navigator.prototype, "platform", { get: () => "MacIntel" }); });
const p = await ctx.newPage();
await p.clock.install();
await p.goto(`${base}demo/?theme=${theme}`, { waitUntil: "networkidle" });
await p.addStyleTag({ content: `a[style*="position:fixed"]{display:none!important} *{cursor:none!important;caret-color:transparent!important} ::-webkit-scrollbar{display:none}${M ? " .app.mac .titlebar{padding-left:16px!important}" : ""}` });
await p.locator('[id^="row-tcp:3000:"]').waitFor();
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
const until = async (fn, max = 4000) => { for (let i = 0; i < (max * FPS) / 1000; i++) { if (await fn()) return; await step(); } };
const center = async (loc) => { const r = await loc.boundingBox(); return [r.x + r.width / 2, r.y + r.height / 2]; };
const move = async (xy, ms) => { await p.evaluate(([x, y, ms]) => window.__cur.move(x, y, ms), [...xy, ms]); await hold(ms); await p.mouse.move(...xy); };
const click = async (xy) => { await p.evaluate(() => window.__cur.click()); if (M) { await p.touchscreen.tap(...xy); await p.mouse.move(-1, -1); } else await p.mouse.click(...xy); }; // touch leaves no hover
const keys = (k, ms = 900) => p.evaluate(([k, ms]) => window.__keys(k, ms), [k, ms]);
const type = async (s, d = 130) => { for (const ch of s) { await p.keyboard.type(ch); await hold(d); } };
const show = (xy) => p.evaluate(([x, y]) => window.__cur.show(x, y), xy);
const t0 = Date.now();

mark("find");
await hold(1000);
const sxy = await center(p.locator('input[type="search"]').first());
const at = M ? [sxy[0] - 40, sxy[1]] : [sxy[0] - 120, sxy[1]];
await show(M ? [VW * 0.62, VH * 0.62] : [VW * 0.68, VH * 0.62]);
await hold(300);
await move(at, 850);
await click(at);
await hold(300);
await type("shop", 150);
await hold(700);
const rxy = await center(p.locator('[id^="row-tcp:3000:"]').first());
const rowAt = M ? [rxy[0] - 60, rxy[1]] : [rxy[0] - 250, rxy[1]];
await move(rowAt, 750);
await hold(120);
mark("explain");
await click(rowAt);
await hold(700);
if (!M) {
  await move(await center(p.locator("aside.pane").getByText("1.1 GB").first()), 900);
  await hold(2500);
  mark("stop");
  await p.evaluate(() => window.__cur.hide());
  await keys(["⌘", "K"], 1000);
  await hold(250);
  await p.keyboard.press("Meta+k");
  await hold(650);
  await type("stop", 140);
  await hold(900);
  await keys(["↵"], 800);
  await hold(250);
  await p.keyboard.press("Enter");
  await hold(2700);
  const bxy = await center(p.locator('[role="alertdialog"] [data-primary]').first());
  await show([bxy[0] - 170, bxy[1] - 110]);
  await hold(250);
  await move(bxy, 800);
  await hold(150);
  mark("free");
  await click(bxy);
} else {
  await hold(1600);
  const bxy = await center(p.getByRole("button", { name: /^Stop/ }).last());
  await move(bxy, 800);
  await hold(150);
  mark("stop");
  await click(bxy);
  await hold(2700);
  const cxy = await center(p.locator('[role="alertdialog"] [data-primary]').first());
  await move(cxy, 800);
  await hold(150);
  mark("free");
  await click(cxy);
}
// Land on the freed state (no details pane, search cleared, the toast saying so), then tidy up so
// the last frame is the opening list minus :3000, which encode-hero.sh dissolves back into frame 0.
const dismiss = p.getByRole("button", { name: "Dismiss notification" });
await until(async () => (await dismiss.count()) > 0, 6000);
await hold(500);
if (M) {
  await move(await center(p.getByRole("button", { name: "Close details" })), 650);
  await hold(100);
  await click(await center(p.getByRole("button", { name: "Close details" })));
  await hold(450);
  const clear = await center(p.getByRole("button", { name: /^Clear search/ }));
  await move(clear, 650);
  await hold(100);
  await click(clear);
  await hold(150);
  await p.keyboard.press("Escape"); // leaves the search field
  await hold(100);
  await p.keyboard.press("Escape"); // drops the row selection
} else {
  await keys(["esc"], 900);
  await hold(250);
  await p.keyboard.press("Escape"); // clears the search
  await hold(300);
  await p.keyboard.press("Escape"); // closes the details pane
}
await hold(1700);
const x = await center(dismiss.first());
await move(x, 700);
await hold(100);
await click(x);
await p.mouse.move(-1, -1); // no hover left on whatever was under the toast
await hold(250);
await p.evaluate(() => window.__cur.hide());
await hold(700);
mark("end");
writeFileSync(`${dir}/meta.json`, JSON.stringify({ variant, theme, VW, VH, DPR, FPS, frames: frame, marks }, null, 1));
console.log(variant, theme, frame, "frames in", ((Date.now() - t0) / 1000).toFixed(0), "s", JSON.stringify(marks));
await b.close();

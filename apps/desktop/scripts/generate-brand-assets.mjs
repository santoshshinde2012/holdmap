// Generate committed app, website and native icons from assets/icon.svg.
// Run from any directory after installing the desktop dependencies and Chromium:
//   node apps/desktop/scripts/generate-brand-assets.mjs
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { chromium } from "@playwright/test";

const desktop = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const site = resolve(desktop, "../../site");
const native = join(desktop, "src-tauri/icons");
const master = await readFile(join(desktop, "assets/icon.svg"), "utf8");
const temporary = await mkdtemp(join(tmpdir(), "holdmap-brand-"));
const execute = promisify(execFile);
const nativeSizes = {
  "32x32.png": 32, "64x64.png": 64, "128x128.png": 128, "128x128@2x.png": 256,
  "icon.png": 512, "StoreLogo.png": 50, "Square30x30Logo.png": 30,
  "Square44x44Logo.png": 44, "Square71x71Logo.png": 71, "Square89x89Logo.png": 89,
  "Square107x107Logo.png": 107, "Square142x142Logo.png": 142,
  "Square150x150Logo.png": 150, "Square284x284Logo.png": 284, "Square310x310Logo.png": 310,
};
const outputs = new Map();
let browser;

function verifyPng(buffer, size, name) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", `${name}: expected PNG`);
  assert.equal(buffer.readUInt32BE(16), size, `${name}: width`);
  assert.equal(buffer.readUInt32BE(20), size, `${name}: height`);
}

// Windows/browser ICO supports PNG frames. Each frame retains its own alpha channel.
function ico(frames) {
  const header = Buffer.alloc(6 + frames.length * 16);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(frames.length, 4);
  let offset = header.length;
  frames.forEach(({ size, buffer }, i) => {
    const at = 6 + i * 16;
    header.writeUInt8(size === 256 ? 0 : size, at);
    header.writeUInt8(size === 256 ? 0 : size, at + 1);
    header.writeUInt16LE(1, at + 4);
    header.writeUInt16LE(32, at + 6);
    header.writeUInt32LE(buffer.length, at + 8);
    header.writeUInt32LE(offset, at + 12);
    offset += buffer.length;
  });
  return Buffer.concat([header, ...frames.map(({ buffer }) => buffer)]);
}

function verifyIco(buffer, sizes, name) {
  assert.equal(buffer.readUInt16LE(0), 0, `${name}: reserved header`);
  assert.equal(buffer.readUInt16LE(2), 1, `${name}: icon type`);
  assert.equal(buffer.readUInt16LE(4), sizes.length, `${name}: frame count`);
  sizes.forEach((size, i) => {
    const at = 6 + i * 16;
    assert.equal(buffer[at] || 256, size, `${name}: frame width`);
    assert.equal(buffer[at + 1] || 256, size, `${name}: frame height`);
    const length = buffer.readUInt32LE(at + 8), offset = buffer.readUInt32LE(at + 12);
    assert(offset >= 6 + sizes.length * 16 && offset + length <= buffer.length, `${name}: frame bounds`);
    verifyPng(buffer.subarray(offset, offset + length), size, `${name} frame ${size}`);
  });
}

// Tauri emits equivalent ICNS chunks in hash-map order. Ordering independent chunks
// by type keeps committed output stable without changing any image or mask payload.
function canonicalIcns(buffer) {
  assert.equal(buffer.subarray(0, 4).toString(), "icns", "Native icon.icns: signature");
  assert.equal(buffer.readUInt32BE(4), buffer.length, "Native icon.icns: container length");
  const chunks = [], seen = new Set();
  for (let offset = 8; offset < buffer.length;) {
    assert(offset + 8 <= buffer.length, "Native icon.icns: chunk header bounds");
    const kind = buffer.subarray(offset, offset + 4).toString("ascii");
    const length = buffer.readUInt32BE(offset + 4);
    assert(length >= 8 && offset + length <= buffer.length, "Native icon.icns: chunk bounds");
    assert(kind !== "TOC " && !seen.has(kind), "Native icon.icns: unexpected indexed or duplicate chunks; review the generator");
    seen.add(kind);
    chunks.push({ kind, buffer: buffer.subarray(offset, offset + length) });
    offset += length;
  }
  assert(chunks.length, "Native icon.icns: missing image chunks");
  chunks.sort((a, b) => a.kind < b.kind ? -1 : a.kind > b.kind ? 1 : 0);
  return Buffer.concat([buffer.subarray(0, 8), ...chunks.map(({ buffer }) => buffer)]);
}

try {
  const executablePath = process.env.HOLDMAP_E2E_CHROMIUM_EXECUTABLE;
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
  const page = await browser.newPage({ viewport: { width: 1024, height: 1024 }, deviceScaleFactor: 1, reducedMotion: "reduce" });
  await page.route("**/*", (route) => route.abort());
  await page.setContent('<!doctype html><html><head><style>html,body{margin:0;background:transparent}svg{display:block}</style></head><body></body></html>');
  await page.evaluate((source) => {
    const document = new DOMParser().parseFromString(source, "image/svg+xml");
    const svg = document.documentElement;
    if (document.querySelector("parsererror") || svg.localName !== "svg") throw new Error("Master must be valid SVG.");
    const box = svg.viewBox.baseVal;
    if (!(box.width > 0 && box.width === box.height)) throw new Error("Master must have a square viewBox.");
    if (svg.querySelectorAll("g#holdmap-mark").length !== 1) throw new Error("Master must contain one g#holdmap-mark glyph.");
    if (svg.querySelector("script, foreignObject, style, animate, animateTransform, animateMotion, set")) throw new Error("Master must contain only static, inert vector content.");
    for (const element of [svg, ...svg.querySelectorAll("*")]) {
      for (const attribute of element.attributes) {
        const references = [...attribute.value.matchAll(/url\(([^)]*)\)/gi)];
        if (/^on/i.test(attribute.name) || (attribute.localName === "href" && !attribute.value.startsWith("#")) || references.some((match) => !match[1].trim().replace(/^['"]|['"]$/g, "").startsWith("#"))) {
          throw new Error("Master must be self-contained, with no event handlers or external references.");
        }
      }
    }
  }, master);

  async function render(source, size) {
    await page.setViewportSize({ width: size, height: size });
    await page.evaluate(({ source, size }) => {
      const svg = new DOMParser().parseFromString(source, "image/svg+xml").documentElement;
      svg.setAttribute("width", String(size));
      svg.setAttribute("height", String(size));
      document.body.replaceChildren(document.importNode(svg, true));
    }, { source, size });
    const buffer = await page.screenshot({ omitBackground: true, animations: "disabled" });
    verifyPng(buffer, size, `Rendered ${size} px icon`);
    return buffer;
  }

  const png = await render(master, 1024);
  outputs.set(join(desktop, "assets/icon.png"), png);
  outputs.set(join(desktop, "public/icon.svg"), Buffer.from(master));
  outputs.set(join(site, "public/favicon.svg"), Buffer.from(master));
  outputs.set(join(site, "public/apple-touch-icon.png"), await render(master, 180));
  const frames = [];
  for (const size of [16, 32, 48]) frames.push({ size, buffer: await render(master, size) });
  const favicon = ico(frames);
  verifyIco(favicon, [16, 32, 48], "favicon.ico");
  outputs.set(join(site, "public/favicon.ico"), favicon);
  outputs.set(join(native, "tray.png"), await render(master, 64));

  const template = await page.evaluate((source) => {
    const parsed = new DOMParser().parseFromString(source, "image/svg+xml").documentElement;
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("xmlns", "http://www.w3.org/2000/svg");
    for (const defs of parsed.querySelectorAll(":scope > defs")) svg.append(document.importNode(defs, true));
    const mark = document.importNode(parsed.querySelector("g#holdmap-mark"), true);
    svg.append(mark);
    document.body.replaceChildren(svg);
    for (const element of [mark, ...mark.querySelectorAll("*")]) {
      const style = getComputedStyle(element);
      if (style.fill !== "none") element.setAttribute("fill", "#000000");
      if (style.stroke !== "none") element.setAttribute("stroke", "#000000");
      element.setAttribute("opacity", "1");
      element.setAttribute("fill-opacity", "1");
      element.setAttribute("stroke-opacity", "1");
      element.removeAttribute("filter");
    }
    const box = mark.getBBox();
    if (!(box.width > 0 && box.height > 0)) throw new Error("Template glyph must have visible geometry.");
    const side = Math.max(box.width, box.height) * 1.16;
    svg.setAttribute("viewBox", `${box.x + box.width / 2 - side / 2} ${box.y + box.height / 2 - side / 2} ${side} ${side}`);
    return new XMLSerializer().serializeToString(svg);
  }, master);
  const trayTemplate = await render(template, 64);
  await page.evaluate(async (source) => {
    const image = new Image();
    image.src = source;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 64;
    const context = canvas.getContext("2d");
    context.drawImage(image, 0, 0);
    const pixels = context.getImageData(0, 0, 64, 64).data;
    let visible = 0, transparent = 0;
    for (let i = 0; i < pixels.length; i += 4) {
      if (pixels[i + 3]) {
        if (pixels[i] || pixels[i + 1] || pixels[i + 2]) throw new Error("macOS template must contain only black glyph pixels.");
        visible++;
      } else transparent++;
    }
    if (!visible || !transparent) throw new Error("macOS template requires a glyph and transparent background.");
  }, `data:image/png;base64,${trayTemplate.toString("base64")}`);
  outputs.set(join(native, "tray-template.png"), trayTemplate);

  const input = join(temporary, "icon.png"), generated = join(temporary, "icons");
  await writeFile(input, png);
  await execute(process.execPath, [join(desktop, "node_modules/@tauri-apps/cli/tauri.js"), "icon", input, "--output", generated], { cwd: desktop, timeout: 60_000 });
  for (const [name, size] of Object.entries(nativeSizes)) {
    const buffer = await readFile(join(generated, name));
    verifyPng(buffer, size, name);
    outputs.set(join(native, name), buffer);
  }
  const nativeIco = await readFile(join(generated, "icon.ico"));
  verifyIco(nativeIco, [16, 24, 32, 48, 64, 256], "Native icon.ico");
  outputs.set(join(native, "icon.ico"), nativeIco);
  const icns = canonicalIcns(await readFile(join(generated, "icon.icns")));
  outputs.set(join(native, "icon.icns"), icns);

  // All formats are validated before replacing committed derivatives. Mobile icon output
  // stays in the temporary directory; this repository packages desktop applications only.
  for (const [path, buffer] of outputs) {
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, buffer);
  }
  console.log(`Generated ${outputs.size} branding assets; SVG copies, PNG dimensions, ICO frames and macOS template alpha verified.`);
} finally {
  try { await browser?.close(); } finally { await rm(temporary, { recursive: true, force: true }); }
}

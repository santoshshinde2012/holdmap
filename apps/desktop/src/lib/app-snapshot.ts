import interUrl from "../assets/fonts/inter-variable.woff2?url";
import monoUrl from "../assets/fonts/jetbrains-mono-variable.woff2?url";
import { makeSnapshotResult, snapshotCancelled, snapshotSize, snapshotWait, type SnapshotResult } from "./screen-snapshot";

const CAPTURE_TIMEOUT_MS = 15_000;
const ASSET_TIMEOUT_MS = 5_000;
const MAX_FONT_BYTES = 512 * 1024;
const MAX_BRAND_BYTES = 64 * 1024;
const MAX_INLINE_IMAGE_CHARS = 1024 * 1024;
const MAX_DOM_ELEMENTS = 20_000;
const TIMEOUT_MESSAGE = "Preparing the Holdmap snapshot took too long. Try again.";
const SVG_NS = "http://www.w3.org/2000/svg";
type Renderer = typeof import("modern-screenshot");
type RenderContext = Awaited<ReturnType<Renderer["createContext"]>>;
/** Extra draws of the rendered SVG; see where it is set. */
const FONT_REDRAWS = 3;

function inlineImage(url: string): boolean {
  return url.length <= MAX_INLINE_IMAGE_CHARS && /^data:image\/(?:png|jpeg|gif|webp|svg\+xml)[;,]/i.test(url);
}

function sameDocumentReference(value: string, base: URL): string | null {
  if (value.startsWith("#")) return value;
  const url = new URL(value, base);
  return url.protocol === base.protocol && url.host === base.host && url.pathname === base.pathname
    && url.search === base.search && url.hash ? url.hash : null;
}

/** Only in-document SVG definitions and bounded inline images may survive DOM embedding. */
function checkedCssUrl(value: string, base: URL): string {
  return value.replace(/url\((?:"([^"]*)"|'([^']*)'|([^)]*))\)/gi, (_match, double: string | undefined, single: string | undefined, plain: string | undefined) => {
    const url = (double ?? single ?? plain ?? "").trim();
    const reference = sameDocumentReference(url, base);
    if (reference) return `url("${reference}")`;
    if (inlineImage(url)) return `url("${url}")`;
    throw new Error("This view contains an image that cannot be included in a local snapshot.");
  });
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 8192) binary += String.fromCharCode(...bytes.subarray(offset, offset + 8192));
  return btoa(binary);
}

/** Only callers' fixed bundled asset URLs enter this bounded loader. */
async function bundledBytes(asset: string, base: URL, signal: AbortSignal, maximum: number): Promise<Uint8Array> {
  if (signal.aborted) throw snapshotCancelled();
  const url = new URL(asset, base);
  // Comparing origin alone is insufficient for custom schemes whose URL.origin is "null".
  if (url.protocol !== base.protocol || url.host !== base.host || url.username || url.password || url.hash) {
    throw new Error("Holdmap's bundled assets are unavailable in this window.");
  }
  let response: Response;
  try {
    response = await fetch(url.href, { signal, mode: "same-origin", credentials: "omit", redirect: "error", cache: "force-cache" });
  } catch {
    if (signal.aborted) throw snapshotCancelled();
    throw new Error("Couldn't load Holdmap's bundled assets. Try again.");
  }
  const declared = response.headers.get("content-length");
  if (!response.ok || response.redirected || (declared !== null && (!/^\d+$/.test(declared) || Number(declared) > maximum)) || !response.body) {
    throw new Error("Couldn't load Holdmap's bundled assets. Try again.");
  }
  const reader = response.body.getReader();
  const cancel = () => { void reader.cancel().catch(() => undefined); };
  signal.addEventListener("abort", cancel, { once: true });
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (signal.aborted) throw snapshotCancelled();
      if (length > maximum) throw new Error("Holdmap's bundled asset is too large to prepare a snapshot.");
      chunks.push(value);
    }
  } finally {
    signal.removeEventListener("abort", cancel);
    await reader.cancel().catch(() => undefined);
    reader.releaseLock();
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return bytes;
}

/**
 * These two build-generated font URLs bypass automatic remote stylesheet discovery. `swap`, like
 * the app's own fonts.css: a font that finishes decoding after the image was drawn leaves text in
 * a fallback face instead of invisible (the default `auto` hides it), which WKWebView did at random.
 */
async function fontCss(base: URL, signal: AbortSignal): Promise<string> {
  const load = async (asset: string): Promise<string> => {
    const bytes = await bundledBytes(asset, base, signal, MAX_FONT_BYTES);
    if (String.fromCharCode(...bytes.subarray(0, 4)) !== "wOF2") throw new Error("Couldn't read Holdmap's bundled fonts.");
    return `data:font/woff2;base64,${base64(bytes)}`;
  };
  const [inter, mono] = await Promise.all([load(interUrl), load(monoUrl)]);
  return `@font-face{font-family:"Inter Variable";src:url("${inter}") format("woff2");font-style:normal;font-weight:100 900;font-display:swap}`
    + `@font-face{font-family:"JetBrains Mono Variable";src:url("${mono}") format("woff2");font-style:normal;font-weight:100 800;font-display:swap}`;
}

/** Rasterize only the current Holdmap viewport; it never requests a display or native IPC. */
export async function captureAppSnapshot(root: HTMLElement, signal: AbortSignal): Promise<SnapshotResult> {
  if (signal.aborted) throw snapshotCancelled();
  if (!root.isConnected || root.ownerDocument !== document || root.closest("[data-snapshot-ui]")) {
    throw new Error("The Holdmap view is no longer available. Try again.");
  }
  const bounds = root.getBoundingClientRect();
  const cssWidth = Math.ceil(bounds.width), cssHeight = Math.ceil(bounds.height);
  const ratio = Math.max(1, Math.min(2, window.devicePixelRatio || 1));
  const size = snapshotSize(cssWidth * ratio, cssHeight * ratio);
  const scale = Math.min(size.width / cssWidth, size.height / cssHeight);
  const width = Math.max(1, Math.floor(cssWidth * scale)), height = Math.max(1, Math.floor(cssHeight * scale));
  const capturedAtMs = Date.now(), deadline = capturedAtMs + CAPTURE_TIMEOUT_MS;
  const controller = new AbortController();
  const abort = () => controller.abort();
  signal.addEventListener("abort", abort, { once: true });
  let renderer: Renderer | null = null, context: RenderContext | null = null;
  let destroyed = false, finished = false;
  const destroy = () => { if (renderer && context && !destroyed) { destroyed = true; renderer.destroyContext(context); } };
  const active = () => {
    if (controller.signal.aborted || signal.aborted || finished) throw snapshotCancelled();
    if (Date.now() >= deadline) throw new Error(TIMEOUT_MESSAGE);
    if (!root.isConnected) throw new Error("The Holdmap view is no longer available. Try again.");
  };
  const base = new URL(document.baseURI);
  const brand = new URL(`${import.meta.env.BASE_URL}icon.svg`, base);
  const isBrand = (source: string) => new URL(source, base).href === brand.href;
  let needsBrand = false;
  const include = (node: Node): boolean => {
    active();
    if (node.nodeType !== Node.ELEMENT_NODE) return true;
    const element = node as Element;
    if (element.closest("[data-snapshot-ui]") || ["IFRAME", "OBJECT", "EMBED", "SCRIPT", "LINK", "STYLE"].includes(element.tagName)) return false;
    if (element.tagName === "VIDEO" || element.tagName === "CANVAS") {
      const style = getComputedStyle(element);
      if (element.hasAttribute("hidden") || style.display === "none" || style.visibility === "hidden" || style.opacity === "0") return false;
    }
    if (element.tagName === "IMG" || element.localName === "image") {
      const source = (element as HTMLImageElement).currentSrc || element.getAttribute("src") || element.getAttribute("href") || element.getAttribute("xlink:href");
      if (source && !inlineImage(source)) {
        if (!isBrand(source)) throw new Error("This view contains an image that cannot be included in a local snapshot.");
        needsBrand = true;
      }
    }
    if (element.localName === "use") {
      const source = element.getAttribute("href") || element.getAttribute("xlink:href");
      if (source && !sameDocumentReference(source, base)) throw new Error("This view contains an external image that cannot be included in a local snapshot.");
    }
    return true;
  };
  const work = async (): Promise<SnapshotResult> => {
    try {
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT, { acceptNode: (node) => include(node) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT });
      let count = 1;
      while (walker.nextNode()) if (++count > MAX_DOM_ELEMENTS) throw new Error("This Holdmap view is too large to prepare a snapshot. Narrow the view and retry.");
      const [module, cssText, brandData] = await Promise.all([
        import("modern-screenshot"),
        snapshotWait(fontCss(base, controller.signal), controller.signal, ASSET_TIMEOUT_MS, "Couldn't load Holdmap's bundled fonts. Try again."),
        needsBrand ? snapshotWait(bundledBytes(brand.href, base, controller.signal, MAX_BRAND_BYTES).then(bytes => `data:image/svg+xml;base64,${base64(bytes)}`), controller.signal, ASSET_TIMEOUT_MS, "Couldn't load Holdmap's icon. Try again.") : Promise.resolve(null),
        document.fonts?.ready ?? Promise.resolve(),
      ]);
      renderer = module;
      active();
      context = await renderer.createContext(root, {
        type: "image/png", width: cssWidth, height: cssHeight, scale, maximumCanvasSize: 3840,
        timeout: ASSET_TIMEOUT_MS, backgroundColor: getComputedStyle(root).getPropertyValue("--bg").trim() || getComputedStyle(document.documentElement).backgroundColor,
        font: { cssText }, debug: false, workerNumber: 0,
        features: { restoreScrollPosition: true, fixSvgXmlDecode: true },
        filter: include,
        fetch: { requestInit: { mode: "same-origin", credentials: "omit", redirect: "error" }, placeholderImage: "" },
        fetchFn: async (url) => {
          active();
          if (inlineImage(url)) return url;
          if (brandData && isBrand(url)) return brandData;
          throw new Error("This view contains an image that cannot be included in a local snapshot.");
        },
        onCloneEachNode: (node) => {
          active();
          if (node.nodeType !== Node.ELEMENT_NODE) return;
          const element = node as HTMLElement | SVGElement;
          for (let index = 0; index < element.style.length; index++) {
            const property = element.style.item(index), value = element.style.getPropertyValue(property);
            if (value.includes("url(")) element.style.setProperty(property, checkedCssUrl(value, base), element.style.getPropertyPriority(property));
          }
          if (element.namespaceURI === SVG_NS && element.localName === "use") {
            const source = element.getAttribute("href") || element.getAttribute("xlink:href");
            if (source) element.setAttribute("href", sameDocumentReference(source, base)!);
          }
        },
      });
      // In WebKit, modern-screenshot redraws its SVG once per embedded image (here just the icon),
      // never for the embedded fonts, so the last draw could still land inside the font's block
      // period and save the text invisible (CI's macOS app check saw this at random).
      // Redraw a few times, about 100 ms apart, so the text is drawn after the font is ready.
      context.drawImageCount = Math.max(context.drawImageCount ?? 0, FONT_REDRAWS);
      active();
      const blob = await renderer.domToBlob(context);
      active();
      const result = await makeSnapshotResult(blob, "app", { width, height, capturedAtMs });
      active();
      return result;
    } finally { destroy(); }
  };
  try {
    return await snapshotWait(work(), signal, CAPTURE_TIMEOUT_MS, TIMEOUT_MESSAGE);
  } finally {
    finished = true;
    controller.abort();
    signal.removeEventListener("abort", abort);
    destroy();
  }
}

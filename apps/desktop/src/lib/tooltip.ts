// Tooltip action: `use:tooltip={"Copy URL"}` or `use:tooltip={{ text, kbd, mono, onlyIfTruncated }}`.
// One shared, positioned `role="tooltip"` element; shows on hover (after a short delay) and on
// keyboard focus, hides on leave/blur/Escape/press. Adds aria-describedby unless the text is
// already the trigger's accessible name.

export interface TooltipOptions {
  text: string;
  kbd?: string | string[];
  mono?: boolean;
  /** Only show when the trigger's text is clipped (for truncated values). */
  onlyIfTruncated?: boolean;
  placement?: "top" | "bottom";
  delay?: number;
}
type Param = string | TooltipOptions | null | undefined;

let el: HTMLDivElement | null = null;
let owner: HTMLElement | null = null;

function surface(): HTMLDivElement {
  if (el && document.body.contains(el)) return el;
  el = document.createElement("div");
  el.className = "pw-tooltip";
  el.id = "pw-tooltip";
  el.setAttribute("role", "tooltip");
  document.body.appendChild(el);
  return el;
}

/** Where to put a tooltip of size `tip` for an anchor `a` inside a `vw`×`vh` viewport. */
export function place(a: { top: number; bottom: number; left: number; width: number }, tip: { width: number; height: number }, vw: number, vh: number, prefer: "top" | "bottom" = "top") {
  const gap = 6;
  const above = a.top - tip.height - gap;
  const below = a.bottom + gap;
  const top = prefer === "top" ? (above >= 4 ? above : below) : below + tip.height <= vh - 4 ? below : above;
  const left = Math.min(Math.max(4, a.left + a.width / 2 - tip.width / 2), Math.max(4, vw - tip.width - 4));
  return { top: Math.round(top), left: Math.round(left) };
}

function norm(p: Param): TooltipOptions | null {
  if (!p) return null;
  return typeof p === "string" ? { text: p } : p.text ? p : null;
}

export function tooltip(node: HTMLElement, param: Param) {
  let opts = norm(param);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const describe = () => {
    const name = node.getAttribute("aria-label") ?? node.textContent?.trim();
    if (opts && name !== opts.text) node.setAttribute("aria-describedby", "pw-tooltip");
  };
  const show = () => {
    if (!opts) return;
    if (opts.onlyIfTruncated && node.scrollWidth <= node.clientWidth && node.scrollHeight <= node.clientHeight + 1) return;
    const t = surface();
    t.replaceChildren(document.createTextNode(opts.text));
    for (const k of opts.kbd ? (Array.isArray(opts.kbd) ? opts.kbd : [opts.kbd]) : []) {
      const kb = document.createElement("kbd");
      kb.textContent = k;
      t.appendChild(kb);
    }
    t.classList.toggle("mono", !!opts.mono);
    const r = node.getBoundingClientRect();
    const pos = place(r, { width: t.offsetWidth, height: t.offsetHeight }, innerWidth, innerHeight, opts.placement);
    t.style.top = `${pos.top}px`;
    t.style.left = `${pos.left}px`;
    owner = node;
    describe();
    t.classList.add("on");
  };
  const hide = () => {
    clearTimeout(timer);
    if (owner === node && el) { el.classList.remove("on"); owner = null; }
    if (node.getAttribute("aria-describedby") === "pw-tooltip") node.removeAttribute("aria-describedby");
  };
  const enter = () => { clearTimeout(timer); timer = setTimeout(show, opts?.delay ?? 450); };
  const focus = () => { if (node.matches(":focus-visible")) show(); };
  const key = (e: KeyboardEvent) => { if (e.key === "Escape" && owner === node) hide(); };

  node.addEventListener("pointerenter", enter);
  node.addEventListener("pointerleave", hide);
  node.addEventListener("pointerdown", hide);
  node.addEventListener("focus", focus);
  node.addEventListener("blur", hide);
  node.addEventListener("keydown", key);
  return {
    update(p: Param) { opts = norm(p); if (owner === node) { if (opts) show(); else hide(); } },
    destroy() {
      hide();
      node.removeEventListener("pointerenter", enter);
      node.removeEventListener("pointerleave", hide);
      node.removeEventListener("pointerdown", hide);
      node.removeEventListener("focus", focus);
      node.removeEventListener("blur", hide);
      node.removeEventListener("keydown", key);
    },
  };
}

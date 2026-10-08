// Accessible tabs (WAI-ARIA pattern): click or arrow keys switch; panels are linked by
// aria-controls. A tab list with data-tabs-os starts on the visitor's OS.
function select(list: HTMLElement, tab: HTMLElement, focus = false) {
  const tabs = [...list.querySelectorAll<HTMLElement>('[role="tab"]')];
  for (const t of tabs) {
    const on = t === tab;
    t.setAttribute("aria-selected", String(on));
    t.tabIndex = on ? 0 : -1;
    const panel = document.getElementById(t.getAttribute("aria-controls") ?? "");
    if (panel) panel.hidden = !on;
  }
  if (focus) tab.focus();
  list.dispatchEvent(new CustomEvent("tabchange", { detail: tab.dataset.key, bubbles: true }));
}

const os = document.documentElement.dataset.os;
for (const list of document.querySelectorAll<HTMLElement>("[data-tabs]")) {
  const tabs = () => [...list.querySelectorAll<HTMLElement>('[role="tab"]')];
  list.addEventListener("click", (e) => {
    const t = (e.target as HTMLElement).closest<HTMLElement>('[role="tab"]');
    if (t) select(list, t);
  });
  list.addEventListener("keydown", (e) => {
    const all = tabs();
    const i = all.indexOf(document.activeElement as HTMLElement);
    if (i < 0) return;
    const next = e.key === "ArrowRight" || e.key === "ArrowDown" ? all[(i + 1) % all.length]
      : e.key === "ArrowLeft" || e.key === "ArrowUp" ? all[(i - 1 + all.length) % all.length]
      : e.key === "Home" ? all[0] : e.key === "End" ? all[all.length - 1] : null;
    if (next) { e.preventDefault(); select(list, next, true); }
  });
  if (list.hasAttribute("data-tabs-os") && os) {
    const want = tabs().find((t) => t.dataset.key === os) ?? tabs().find((t) => t.dataset.key === (os === "windows" ? "windows" : "unix"));
    if (want && want.getAttribute("aria-selected") !== "true") select(list, want);
  }
}

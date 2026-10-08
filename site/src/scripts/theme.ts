// Theme toggle (dark first) and the mobile menu. The initial theme is applied inline in <head>.
const KEY = "portwise-site-theme";
const root = document.documentElement;

function apply(theme: "dark" | "light") {
  root.dataset.theme = theme;
  document.querySelector('meta[name="theme-color"]')?.setAttribute("content", theme === "light" ? "#fafaf9" : "#0a0a0b");
  for (const b of document.querySelectorAll<HTMLButtonElement>("[data-theme-toggle]")) {
    b.setAttribute("aria-label", theme === "light" ? "Switch to dark theme" : "Switch to light theme");
  }
  window.dispatchEvent(new CustomEvent("portwise:theme", { detail: theme }));
}

apply(root.dataset.theme === "light" ? "light" : "dark");
for (const b of document.querySelectorAll<HTMLButtonElement>("[data-theme-toggle]")) {
  b.addEventListener("click", () => {
    const next = root.dataset.theme === "light" ? "dark" : "light";
    try { localStorage.setItem(KEY, next); } catch { /* private mode */ }
    apply(next);
  });
}

const nav = document.querySelector<HTMLElement>("[data-nav]");
const menu = document.querySelector<HTMLButtonElement>("[data-menu]");
if (nav && menu) {
  const set = (open: boolean) => {
    nav.toggleAttribute("data-open", open);
    menu.setAttribute("aria-expanded", String(open));
    menu.setAttribute("aria-label", open ? "Close menu" : "Open menu");
  };
  menu.addEventListener("click", () => set(!nav.hasAttribute("data-open")));
  nav.querySelectorAll(".links a").forEach((a) => a.addEventListener("click", () => set(false)));
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && nav.hasAttribute("data-open")) { set(false); menu.focus(); } });
}

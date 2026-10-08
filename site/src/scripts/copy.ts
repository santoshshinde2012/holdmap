// Copy buttons: <button data-copy="text"> or data-copy-from="#id" (copies that element's text).
const live = document.createElement("div");
live.className = "sr-only";
live.setAttribute("role", "status");
document.body.append(live);

document.addEventListener("click", async (e) => {
  const btn = (e.target as HTMLElement).closest<HTMLButtonElement>("[data-copy], [data-copy-from]");
  if (!btn) return;
  const from = btn.dataset.copyFrom ? document.querySelector(btn.dataset.copyFrom) : null;
  const text = btn.dataset.copy ?? from?.textContent?.trim() ?? "";
  try {
    await navigator.clipboard.writeText(text);
    btn.dataset.copied = "";
    live.textContent = "Copied to clipboard";
    setTimeout(() => { delete btn.dataset.copied; live.textContent = ""; }, 1600);
  } catch {
    live.textContent = "Couldn't copy; select the text instead";
  }
});

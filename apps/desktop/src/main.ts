import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import Gallery from "./dev/Gallery.svelte";

const target = document.getElementById("app")!;
// `#ui-gallery` shows the UI kit reference (visual review / screenshots) instead of the app.
function start() {
  if (location.hash !== "#ui-gallery") return mount(App, { target });
  document.documentElement.dataset.theme = localStorage.getItem("pw.theme") === "dark" ? "dark" : "light";
  return mount(Gallery, { target });
}
const app = start();

export default app;

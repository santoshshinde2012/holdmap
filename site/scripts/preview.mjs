// Managed preview of the built website, using the desktop's locked Vite dependency.
import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";

async function freePort() {
  const listener = createServer();
  await new Promise((resolve, reject) => {
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", resolve);
  });
  const port = listener.address().port;
  await new Promise((resolve, reject) => listener.close((error) => error ? reject(error) : resolve()));
  return port;
}

export async function previewSite() {
  const port = await freePort();
  const desktop = fileURLToPath(new URL("../../apps/desktop/", import.meta.url));
  const vite = fileURLToPath(new URL("../../apps/desktop/node_modules/vite/bin/vite.js", import.meta.url));
  const output = fileURLToPath(new URL("../dist/", import.meta.url));
  const child = spawn(process.execPath, [vite, "preview", "--outDir", output, "--base", "/holdmap/", "--host", "127.0.0.1", "--port", String(port), "--strictPort"], {
    cwd: desktop,
    stdio: ["ignore", "pipe", "pipe"],
  });
  let logs = "";
  let startError;
  child.on("error", (error) => { startError = error; });
  for (const pipe of [child.stdout, child.stderr]) {
    pipe.on("data", (chunk) => { logs = (logs + chunk).slice(-8192); });
  }
  const alive = () => child.exitCode === null && child.signalCode === null;
  const close = async () => {
    if (!child.pid || !alive()) return;
    child.kill("SIGTERM");
    const deadline = Date.now() + 2000;
    while (alive() && Date.now() < deadline) await delay(25);
    if (alive()) child.kill("SIGKILL");
    const forcedDeadline = Date.now() + 1000;
    while (alive() && Date.now() < forcedDeadline) await delay(25);
    if (alive()) throw new Error("The website preview did not exit after cleanup.");
  };
  const url = `http://127.0.0.1:${port}/holdmap/`;
  try {
    const deadline = Date.now() + 15_000;
    while (Date.now() < deadline) {
      if (startError) throw startError;
      if (!alive()) throw new Error(`Website preview exited before startup.\n${logs}`);
      try {
        const response = await fetch(url, { signal: AbortSignal.timeout(1000) });
        await response.arrayBuffer();
        if (response.ok) return { url, close };
      } catch { /* wait for the server to bind */ }
      await delay(100);
    }
    throw new Error(`Website preview did not start. Build site/ first.\n${logs}`);
  } catch (error) {
    await close();
    throw error;
  }
}

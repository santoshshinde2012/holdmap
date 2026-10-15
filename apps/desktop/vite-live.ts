/** Dev-server bridge: `npm run dev` reads this machine through the holdmap CLI. Never spawned in production builds. */
import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import type { Connect, Plugin } from "vite";

const execFileAsync = promisify(execFile);
const PREFIX = "/__holdmap/";
const TIMEOUT_MS = 20_000;
const MAX_BUFFER = 32 * 1024 * 1024;
const EMPTY_ACTIVITY = {
  spans: [],
  received: 0,
  dropped: 0,
  limits: ["No local recorder is running. Start recording under Agents → Flow → Calls in the desktop app, or run holdmap activity serve."],
};

type Route = (url: URL) => string[];

function portArg(url: URL): string {
  const port = Number(url.searchParams.get("port"));
  if (!Number.isInteger(port) || port < 1 || port > 65_535) throw new Error("Choose a TCP or UDP port between 1 and 65535.");
  return String(port);
}

const routes: Record<string, Route> = {
  scan: (url) => ["list", "--json", ...(url.searchParams.get("all") === "1" ? ["--all"] : [])],
  agents: (url) => ["agents", "--json", ...(url.searchParams.get("capabilities") === "1" ? ["--capabilities"] : [])],
  topology: (url) => ["graph", "--json", ...(url.searchParams.get("all") === "1" ? ["--all"] : [])],
  agent_flow: () => ["agents", "--flow", "--json"],
  explain: (url) => ["explain", portArg(url), "--json"],
  inspect: (url) => ["inspect", portArg(url), "--json"],
};

function repoRoot(): string {
  return path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
}

function holdmapBin(): string {
  if (process.env.HOLDMAP) return process.env.HOLDMAP;
  const root = repoRoot();
  const name = process.platform === "win32" ? "holdmap.exe" : "holdmap";
  for (const dir of ["target/debug", "target/release"]) {
    const candidate = path.join(root, dir, name);
    if (existsSync(candidate)) return candidate;
  }
  return name;
}

async function runCli(args: string[]): Promise<unknown> {
  const bin = holdmapBin();
  try {
    const { stdout } = await execFileAsync(bin, args, { timeout: TIMEOUT_MS, maxBuffer: MAX_BUFFER, windowsHide: true });
    return JSON.parse(stdout);
  } catch (cause) {
    const failure = cause as { stdout?: string; stderr?: string; code?: string | number; message?: string };
    if (failure.stdout) {
      try {
        return JSON.parse(failure.stdout);
      } catch {
        /* fall through to a readable error */
      }
    }
    if (failure.code === "ENOENT") throw new Error(`holdmap CLI not found (${bin}). Build it with cargo build -p holdmap or set HOLDMAP.`);
    const detail = (failure.stderr || failure.message || "holdmap CLI failed").trim().split("\n").at(-1) ?? "holdmap CLI failed";
    throw new Error(detail);
  }
}

function send(res: { statusCode: number; setHeader(name: string, value: string): void; end(body: string): void }, status: number, body: unknown) {
  res.statusCode = status;
  res.setHeader("Content-Type", "application/json; charset=utf-8");
  res.setHeader("Cache-Control", "no-store");
  res.end(JSON.stringify(body));
}

export function liveCollector(): Plugin {
  const middleware: Connect.NextHandleFunction = (req, res, next) => {
    const raw = req.url ?? "";
    if (!raw.startsWith(PREFIX)) return next();
    if (req.method !== "GET") {
      send(res, 405, { error: "Live collector is read-only." });
      return;
    }
    void (async () => {
      try {
        const url = new URL(raw, "http://127.0.0.1");
        const name = url.pathname.slice(PREFIX.length);
        if (name === "activity") {
          try {
            send(res, 200, await runCli(["activity", "snapshot", "--json"]));
          } catch {
            send(res, 200, EMPTY_ACTIVITY);
          }
          return;
        }
        const route = routes[name];
        if (!route) {
          send(res, 404, { error: `Unknown live collector route: ${name}` });
          return;
        }
        const args = route(url);
        if (url.searchParams.get("docker") === "0") args.unshift("--no-docker");
        send(res, 200, await runCli(args));
      } catch (cause) {
        send(res, 503, { error: cause instanceof Error ? cause.message : "Live holdmap scan failed." });
      }
    })();
  };
  return {
    name: "holdmap-live",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use(middleware);
    },
  };
}

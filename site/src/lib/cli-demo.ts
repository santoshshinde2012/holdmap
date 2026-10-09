// Scripted CLI sessions for the terminal player. The output mirrors the real CLI's layout and
// colours (see crates/portwise-cli), with the same sample machine as the desktop demo.
// Markup is a tiny subset: <b> bold, <d> dim, <c> port cyan, <g> green, <r> red, <y> amber, <v> violet.

export type Chapter = { id: string; label: string; cmd: string; caption: string; out: string[] };

const pad = (s: string, n: number, right = false) => (right ? s.padStart(n) : s.padEnd(n));

type Row = [port: number, addr: string, pid: number, proc: string, what: string, dev: boolean];
const rows: Row[] = [
  [3000, "127.0.0.1, ::1", 43000, "node", "Next.js · shop-web (feat/checkout)", true],
  [3001, "127.0.0.1, ::1", 43001, "node", "Express · shop-api (main)", true],
  [5000, "0.0.0.0, [::]", 522, "ControlCenter", "AirPlay Receiver [protected]", false],
  [5173, "::1", 45173, "node", "Vite · docs (main)", true],
  [5432, "0.0.0.0, [::]", 1290, "com.docker.backend", "PostgreSQL · container shop-db-1", true],
  [6379, "127.0.0.1", 611, "redis-server", "Redis", false],
  [8000, "0.0.0.0", 48000, "Python", "Django · ml-service (exp/embeddings)", true],
  [11434, "127.0.0.1", 835, "ollama", "Ollama", false],
];
const W = { port: 5, addr: 14, pid: 6, proc: 18 };
const list = [
  `<d>${pad("PORT", W.port, true)}</d>  <d>PROTO</d>  <d>${pad("ADDRESS", W.addr)}</d>  <d>${pad("PID", W.pid, true)}</d>  <d>${pad("PROCESS", W.proc)}</d>  <d>WHAT</d>`,
  ...rows.map(([port, addr, pid, proc, what, dev]) =>
    `<c>${pad(String(port), W.port, true)}</c>  <d>TCP</d>    <d>${pad(addr, W.addr)}</d>  ${pad(String(pid), W.pid, true)}  ${pad(proc, W.proc)}  ${dev ? `<g>${what}</g>` : `<d>${what}</d>`}`),
  "",
  "<d>8 ports · 5 dev · 3 exposed to the network · 38 ms</d>",
];

const kv = (k: string, v: string) => `<d>${pad(k, 13, true)}</d>  ${v}`;

export const plan = [
  "<b>Plan for :3000</b> <d>(<g>low risk</g>)</d>",
  "  1. Send SIGTERM to 2 processes: npm (42999), node (43000); SIGKILL any still running after 5s",
  "  2. Verify TCP port 3000 is free (wait up to 3s)",
];
export const held = "<r>●</r> <b>Port 3000 is held by a Next.js dev server (node, PID 43000) in ~/code/shop-web (branch feat/checkout), running for 2h 3m.</b>";

export const chapters: Chapter[] = [
  {
    id: "list", label: "list", cmd: "portwise list",
    caption: "Every listening port with its process, project, branch and framework.",
    out: list,
  },
  {
    id: "inspect", label: "inspect", cmd: "portwise inspect 3000",
    caption: "Everything about one port: owner, tree, memory, connections and bind risk.",
    out: [
      kv("Port", "<c>3000</c> / TCP"),
      kv("State", "LISTEN"),
      kv("Addresses", "127.0.0.1, ::1"),
      kv("Exposure", "this machine only"),
      kv("PID", "43000"),
      kv("Process", "node"),
      kv("Command", "node ~/code/shop-web/node_modules/.bin/next dev"),
      kv("Working dir", "~/code/shop-web"),
      kv("Uptime", "2h 3m"),
      kv("Memory", "1.2 GB · 3 helpers"),
      kv("Project", "shop-web (~/code/shop-web, package.json) · branch <v>feat/checkout</v>"),
      kv("Framework", "Next.js (DevServer)"),
      kv("Ancestry", "node (43000) ← npm (42999) ← zsh (42960) ← launchd (1)"),
      kv("Connections", "3 <d>(3 established)</d> · Google Chrome ×3"),
      kv("Bind risk", "<g>low</g> · Only this computer can connect"),
      kv("HTTP", '200 OK · "shop-web" (3 ms)'),
      "",
      "<v>→</v> Gracefully stop npm run dev (2 processes: SIGTERM, then SIGKILL after 5s) and verify port 3000 is free.",
    ],
  },
  {
    id: "stop", label: "stop", cmd: "portwise stop 3000",
    caption: "Stops the whole dev-server tree gracefully, then checks the port is really free.",
    out: [
      held,
      ...plan,
      "<y>?</y> Proceed? [y/N] y",
      "  <d>·</d> <d>sent SIGTERM to npm (42999)</d>",
      "  <d>·</d> <d>sent SIGTERM to node (43000)</d>",
      "  <d>·</d> <d>TCP port 3000 is free</d>",
      "<g>✔</g> <b>:3000 is free (stopped 2 processes)</b> <d>in 412 ms</d>",
    ],
  },
  {
    id: "run", label: "run", cmd: "portwise run -p 3000 -- npm run dev",
    caption: "Frees the port safely, then starts your server on it with PORT set.",
    out: [
      held,
      ...plan,
      "<y>?</y> Proceed? [y/N] y",
      "<g>✔</g> <b>:3000 is free (stopped 2 processes)</b> <d>in 398 ms</d>",
      "<v>→</v> npm run dev <d>(PORT=3000)</d>",
      "",
      "  <b>▲ Next.js</b>",
      "  - Local:   http://localhost:3000",
      "  <g>✓</g> Ready in 1.4s",
    ],
  },
  {
    id: "up", label: "up", cmd: "portwise up",
    caption: "Starts a project's services from .portwise.toml, dependencies first.",
    out: [
      "<v>→</v> <b>shop</b> <d>~/code/shop/.portwise.toml</d>",
      "  <g>✔</g> <b>db</b> <c>:5432</c> <d>already running (PostgreSQL · container shop-db-1)</d>",
      "  <g>✔</g> <b>api</b> <c>:4000</c> up after 0.8 s <d>PID 51210 · log ~/.config/portwise/logs/shop-api.log</d>",
      "  <g>✔</g> <b>web</b> <c>:3000</c> up after 1.4 s <d>PID 51244 · log ~/.config/portwise/logs/shop-web.log</d>",
      "<g>✔</g> <b>shop is up</b>",
      "<d>`portwise status` shows the services, `portwise down` stops them.</d>",
    ],
  },
];

/** Our markup → HTML spans (content is ours, but escape anything that isn't a known tag). */
export function toHtml(line: string): string {
  return line
    .replace(/&/g, "&amp;")
    .replace(/<(\/?)([bdcgryv])>/g, "\u0001$1$2\u0002")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\u0001\/[bdcgryv]\u0002/g, "</span>")
    .replace(/\u0001([bdcgryv])\u0002/g, '<span class="$1">');
}

/** A short static session for the Interfaces section (real output formats, sample data). */
export const cliSession: string[] = [
  "<v>$</v> portwise agents",
  "<v>Claude Code</v>  pid 51200 · 214 MB · 3% CPU · 1 stoppable",
  "  folders   shop-api  ~/code/shop-api",
  "  ports     <c>:3001</c>  shop-api (Vite)  · stoppable",
  "  access    sandbox unknown · network :3001 only",
  "",
  "<v>$</v> portwise explain 3000",
  held,
  "  <v>→</v> Gracefully stop npm run dev (2 processes) and verify port 3000 is free.",
  "",
  "<v>$</v> portwise free-port --near 3000",
  "3002",
];
export const plain = (line: string) => line.replace(/<\/?[bdcgryv]>/g, "");

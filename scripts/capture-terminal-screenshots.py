#!/usr/bin/env python3
"""Refresh the published CLI/TUI screenshots from real terminal output.

Optional maintainer tooling for macOS/Linux; this is not a production dependency.
Create an isolated venv, install scripts/requirements-terminal-screenshots.txt,
build `cargo build -p holdmap`, then run this script with that venv's Python.
Use --binary, --output-dir or --font to override the defaults.

The fixtures use built-in HTTP/UDP servers and small project manifests that
exercise Holdmap's framework classification. They need no npm install and make
no external connections. Ports 3000/5173/8000/8125 are preferred; occupied ports
fall back to OS-assigned ports. The Agents captures use an owned `gemini.js` process
with TCP/UDP services on the same number and a minimal stdio MCP child. That
product has no recent-project reader, so account history never enters the capture.
Stop previews are always cancelled; no reviewed plan is executed. Cleanup
terminates only the child process groups this script created. App state lives in
a temp directory. Screenshots retain the actual ports, PIDs, timings, memory and plans.

The TUI is captured only after filtering to the unique fixture path or exact owned
agent PID. The Graph capture verifies every matching node and connection belongs
to the four fixture listeners before publishing the filtered frame. ANSI output
is held in memory, not saved as an unfiltered transcript. For published images,
the current account name is replaced with `devuser`, truncated/padded to its
original width; all other terminal cells and styles are retained. Each screen is rendered with
fixed dark and light terminal palettes and font; it does not reconstruct or edit app output.
"""

from __future__ import annotations

import argparse
import codecs
import copy
import errno
import getpass
import hashlib
import json
import os
import re
import pwd
from pathlib import Path
import select
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time

try:
    import fcntl
    import pty
    import termios
    import pyte
    from PIL import Image, ImageDraw, ImageFont
except ImportError as error:
    raise SystemExit(
        "Use macOS/Linux and install the optional capture dependencies from "
        "scripts/requirements-terminal-screenshots.txt in an isolated venv. "
        f"Missing dependency: {error.name}"
    ) from error


ROOT = Path(__file__).resolve().parent.parent
FAINT = "788a9b"  # SGR 2 marker; each theme renders it as its muted foreground.
# pyte reports 256-colour indices 0-15 (how ratatui writes named colours) as xterm hex values.
XTERM16 = dict(zip(
    ["000000", "cd0000", "00cd00", "cdcd00", "0000ee", "cd00cd", "00cdcd", "e5e5e5",
     "7f7f7f", "ff0000", "00ff00", "ffff00", "5c5cff", "ff00ff", "00ffff", "ffffff"],
    ["black", "red", "green", "brown", "blue", "magenta", "cyan", "white", "brightblack",
     "brightred", "brightgreen", "brightbrown", "brightblue", "brightmagenta", "brightcyan", "brightwhite"],
))
THEMES = {
    "dark": {
        "background": "#0c1117", "foreground": "#c6d3de", "faint": "#788a9b", "on_color": None, "greys": {}, "named_xterm": False,
        "palette": {
            "black": "#151b23", "red": "#ff757f", "green": "#9ece6a",
            "brown": "#e0af68", "blue": "#7aa2f7", "magenta": "#bb9af7",
            "cyan": "#7dcfff", "white": "#c6d3de", "brightblack": "#788a9b",
            "brightred": "#ff9eab", "brightgreen": "#b9f27c", "brightbrown": "#ffdb8e",
            "brightblue": "#9ac5ff", "brightmagenta": "#d4b6ff",
            "brightcyan": "#b4f9f8", "brightwhite": "#f4f7fb",
        },
    },
    # A white-background terminal: ANSI colours darkened for contrast, xterm greys 236-238 used
    # as selection backgrounds lightened, and text on a coloured background drawn in white.
    "light": {
        "background": "#ffffff", "foreground": "#1f2328", "faint": "#6e7781", "on_color": "#ffffff", "named_xterm": True,
        "greys": {"303030": "#eef1f4", "3a3a3a": "#e4e8ec", "444444": "#d8dee4"},
        "palette": {
            "black": "#1f2328", "red": "#cf222e", "green": "#1a7f37",
            "brown": "#9a6700", "blue": "#0969da", "magenta": "#8250df",
            "cyan": "#0a6e8a", "white": "#1f2328", "brightblack": "#59636e",
            "brightred": "#a40e26", "brightgreen": "#116329", "brightbrown": "#7d4e00",
            "brightblue": "#0550ae", "brightmagenta": "#6639ba",
            "brightcyan": "#0a5c73", "brightwhite": "#1f2328",
        },
    },
}


class CaptureScreen(pyte.Screen):
    def select_graphic_rendition(self, *attrs: int) -> None:
        super().select_graphic_rendition(*attrs)
        # pyte has no faint field; use the dark terminal's muted foreground for SGR 2.
        index, faint = 0, False
        while index < len(attrs):
            attr = attrs[index]
            if attr == 2:
                faint = True
            elif attr in (38, 48) and index + 1 < len(attrs):
                # Palette index 2 and true-color mode 2 are not the faint attribute.
                mode = attrs[index + 1]
                index += 2 if mode == 5 else 4 if mode == 2 else 0
            index += 1
        if faint:
            self.cursor.attrs = self.cursor.attrs._replace(fg=FAINT)


class Terminal:
    """Bounded PTY lifecycle, with terminal cells parsed by a VT emulator."""

    def __init__(self, command: list[str], cwd: Path, env: dict[str, str], columns: int, rows: int):
        self.screen = CaptureScreen(columns, rows)
        self.stream = pyte.Stream(self.screen)
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
        try:
            self.child = subprocess.Popen(
                command, cwd=cwd, env=env, stdin=slave, stdout=slave, stderr=slave,
                start_new_session=True,
            )
        except BaseException:
            os.close(self.master)
            raise
        finally:
            os.close(slave)
        self.closed = False

    def pump(self, duration: float = 0.1) -> None:
        if self.closed:
            return
        ready, _, _ = select.select([self.master], [], [], duration)
        if not ready:
            return
        try:
            data = os.read(self.master, 65536)
        except OSError as error:
            if error.errno != errno.EIO:
                raise
            data = b""
        if data:
            self.stream.feed(self.decoder.decode(data))
        else:
            self.closed = True

    def wait_for(self, predicate, description: str, timeout: float = 15) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            self.pump()
            if predicate("\n".join(self.screen.display)):
                return
            if self.child.poll() is not None or self.closed:
                break
        raise RuntimeError(f"Timed out waiting for {description}; no unfiltered transcript was saved")

    def write(self, text: str) -> None:
        os.write(self.master, text.encode())

    def finish(self, timeout: float = 10) -> int:
        deadline = time.monotonic() + timeout
        while self.child.poll() is None or not self.closed:
            self.pump()
            if time.monotonic() >= deadline:
                raise RuntimeError("Terminal child did not exit within the capture timeout")
        return self.child.returncode

    def close(self) -> None:
        terminate_owned(self.child)
        os.close(self.master)


def terminate_owned(child: subprocess.Popen) -> None:
    if child.poll() is None:
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            child.wait(timeout=3)
            return
        try:
            child.wait(timeout=3)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait(timeout=3)


def preferred_port(port: int, udp: bool = False) -> int:
    with socket.socket(type=socket.SOCK_DGRAM if udp else socket.SOCK_STREAM) as candidate:
        try:
            candidate.bind(("127.0.0.1", port))
        except OSError:
            return 0
    return port


def fixture(children: list[subprocess.Popen], cwd: Path, command: list[str]) -> int:
    child = subprocess.Popen(
        command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    children.append(child)
    assert child.stdout is not None
    ready, _, _ = select.select([child.stdout], [], [], 8)
    if not ready:
        raise RuntimeError("Fixture listener did not report its bound port")
    line = child.stdout.readline()
    if not line.startswith(b"READY "):
        raise RuntimeError("Fixture listener failed to start")
    port = int(line.split()[1])
    child.stdout.close()
    return port


def create_fixtures(base: Path, children: list[subprocess.Popen]) -> tuple[int, list[int]]:
    node = shutil.which("node")
    if node is None:
        raise RuntimeError("Node.js is required for the optional HTTP fixtures")
    ports = []
    for name, preferred, framework in [("shop-web", 3000, "next"), ("docs-site", 5173, "vite")]:
        directory = base / name
        directory.mkdir()
        (directory / "package.json").write_text(json.dumps({
            "name": name, "private": True, "scripts": {"dev": "node server.js"},
            "dependencies": {framework: "*"},
        }))
        (directory / "server.js").write_text("""const http = require('http');
const server = http.createServer((_, res) => {
  res.setHeader('Content-Type', 'text/html');
  res.end('<title>Holdmap local demo</title>Fixture listener');
});
server.on('error', error => {
  if (error.code === 'EADDRINUSE') server.listen(0, '127.0.0.1');
  else process.exit(1);
});
server.on('listening', () => console.log('READY ' + server.address().port));
server.listen(Number(process.argv[2]), '127.0.0.1');
""")
        subprocess.run(["git", "init", "-q", "-b", "feat/checkout" if name == "shop-web" else "main"],
                       cwd=directory, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5)
        # `explain` previews both protocols; reserve the first fixture number against
        # UDP too so its published explanation cannot include an unrelated service.
        candidate = shared_listener_port(preferred) if name == "shop-web" else preferred_port(preferred)
        ports.append(fixture(children, directory, [node, "server.js", str(candidate)]))

    api = base / "orders-api"
    api.mkdir()
    (api / "pyproject.toml").write_text('[project]\nname = "orders-api"\ndependencies = ["fastapi"]\n')
    (api / "serve.py").write_text("""from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import sys
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200); self.end_headers(); self.wfile.write(b'Fixture API')
    def log_message(self, *args): pass
try: server = ThreadingHTTPServer(('127.0.0.1', int(sys.argv[1])), Handler)
except OSError: server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
print('READY', server.server_port, flush=True)
server.serve_forever()
""")
    ports.append(fixture(children, api, [sys.executable, "serve.py", str(preferred_port(8000))]))
    metrics = base / "statsd"
    metrics.mkdir()
    (metrics / "statsd.py").write_text("""import socket, sys
sock = socket.socket(type=socket.SOCK_DGRAM)
try: sock.bind(('127.0.0.1', int(sys.argv[1])))
except OSError: sock.bind(('127.0.0.1', 0))
print('READY', sock.getsockname()[1], flush=True)
while True: sock.recvfrom(4096)
""")
    # The absolute command path keeps this project-less UDP row inside the same exact fixture filter.
    ports.append(fixture(children, metrics, [sys.executable, str(metrics / "statsd.py"), str(preferred_port(8125, True))]))
    return ports[0], ports


def shared_listener_port(preferred: int) -> int:
    """Find one number available for both protocols before starting owned listeners."""
    for attempt in range(8):
        with socket.socket() as tcp, socket.socket(type=socket.SOCK_DGRAM) as udp:
            try:
                tcp.bind(("127.0.0.1", preferred if attempt == 0 else 0))
                port = tcp.getsockname()[1]
                udp.bind(("127.0.0.1", port))
                return port
            except OSError:
                continue
    raise RuntimeError("No fixture port was available for both TCP and UDP")


def create_agent_fixture(base: Path, children: list[subprocess.Popen]) -> dict[str, int]:
    """Only process metadata identifies this fixture; no real agent state is created."""
    node = shutil.which("node")
    if node is None:
        raise RuntimeError("Node.js is required for the optional agent fixture")
    workspace = base / "agent-workspace"
    workspace.mkdir()
    (workspace / "package.json").write_text(json.dumps({
        "name": "agent-workspace", "private": True, "dependencies": {"vite": "*"},
    }))
    (workspace / "web.js").write_text("""const http = require('http');
const server = http.createServer((_, res) => res.end('Owned agent HTTP fixture'));
server.listen(Number(process.argv[2]), '127.0.0.1', () => {
  console.log('READY ' + server.address().port);
});
""")
    (workspace / "metrics.js").write_text("""const socket = require('dgram').createSocket('udp4');
socket.on('message', (_, remote) => socket.send('fixture-alive', remote.port, remote.address));
socket.bind(Number(process.argv[2]), '127.0.0.1', () => {
  console.log('READY ' + socket.address().port);
});
""")
    mcp = workspace / "@modelcontextprotocol" / "server-filesystem"
    mcp.mkdir(parents=True)
    # Implements a bounded stdio handshake, with no tools or file-system access.
    (mcp / "index.js").write_text("""const readline = require('readline');
readline.createInterface({input: process.stdin}).on('line', line => {
  if (line.length > 4096) return;
  const request = JSON.parse(line);
  if (request.id === undefined) return;
  const result = request.method === 'initialize'
    ? {protocolVersion: '2024-11-05', capabilities: {},
       serverInfo: {name: 'owned-fixture', version: '1.0.0'}}
    : request.method === 'tools/list' ? {tools: []} : {};
  process.stdout.write(JSON.stringify({jsonrpc: '2.0', id: request.id, result}) + '\\n');
});
""")
    (workspace / "gemini.js").write_text("""const {spawn} = require('child_process');
const readline = require('readline');
const path = require('path');
const children = [];
function start(entry, args = []) {
  const child = spawn(process.execPath, [path.join(__dirname, entry), ...args],
    {stdio: ['pipe', 'pipe', 'ignore']});
  children.push(child);
  return child;
}
function firstLine(child) {
  return new Promise((resolve, reject) => {
    const lines = readline.createInterface({input: child.stdout});
    lines.once('line', line => { lines.close(); resolve(line); });
    child.once('error', reject);
    child.once('exit', () => reject(new Error('Fixture exited before readiness')));
  });
}
process.on('exit', () => {
  for (const child of children) if (child.exitCode === null) child.kill('SIGTERM');
});
(async () => {
  const port = Number(process.argv[2]);
  const tcp = start('web.js', [String(port)]);
  if (await firstLine(tcp) !== 'READY ' + port) throw new Error('TCP fixture failed');
  const udp = start('metrics.js', [String(port)]);
  if (await firstLine(udp) !== 'READY ' + port) throw new Error('UDP fixture failed');
  const mcp = start('@modelcontextprotocol/server-filesystem/index.js');
  const initialized = firstLine(mcp);
  mcp.stdin.write(JSON.stringify({jsonrpc: '2.0', id: 1, method: 'initialize'}) + '\\n');
  const response = JSON.parse(await initialized);
  if (response.result.serverInfo.name !== 'owned-fixture') throw new Error('MCP fixture failed');
  console.log('READY ' + JSON.stringify({root: process.pid, tcp: tcp.pid,
    udp: udp.pid, mcp: mcp.pid, port}));
})().catch(() => process.exit(1));
""")
    child = subprocess.Popen(
        [node, str(workspace / "gemini.js"), str(shared_listener_port(3001))],
        cwd=workspace, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    children.append(child)
    assert child.stdout is not None
    if not select.select([child.stdout], [], [], 8)[0]:
        raise RuntimeError("The owned agent fixture did not report readiness")
    line = child.stdout.readline()
    child.stdout.close()
    if not line.startswith(b"READY "):
        raise RuntimeError("The owned agent fixture failed to start")
    metadata = json.loads(line[6:])
    if set(metadata) != {"root", "tcp", "udp", "mcp", "port"} or metadata["root"] != child.pid:
        raise RuntimeError("Unexpected owned agent fixture metadata")
    if any(not isinstance(pid, int) or pid <= 1 for pid in metadata.values()):
        raise RuntimeError("Invalid owned agent fixture identities")
    return metadata


def verify_agent_fixture(binary: Path, base: Path, env: dict[str, str], owned: dict[str, int]) -> None:
    """Inspect only the owned agent and preview protocol-scoped plans, never execute them."""
    def inspect(*args: str):
        result = subprocess.run(
            [str(binary), "--no-docker", "agents", str(owned["root"]), *args, "--json"],
            cwd=base, env=env, capture_output=True, text=True, timeout=30,
        )
        if result.returncode != 0:
            raise RuntimeError("The owned agent inspection failed; no transcript was saved")
        return json.loads(result.stdout)

    report = inspect()
    if len(report["agents"]) != 1:
        raise RuntimeError("The agent PID query did not isolate the owned fixture")
    agent = report["agents"][0]
    if agent["pid"] != owned["root"] or agent["product"] != "gemini":
        raise RuntimeError("The owned process was not identified as the expected agent fixture")
    if not {owned[k] for k in ("tcp", "udp", "mcp")}.issubset(agent["process_ids"]):
        raise RuntimeError("The agent report lost one of its owned fixture children")
    if {(p["port"], p["protocol"]) for p in agent["ports"]} != {
        (owned["port"], "tcp"), (owned["port"], "udp"),
    }:
        raise RuntimeError("The fixture listeners were not attributed with their exact protocols")
    if not any(t["pid"] == owned["mcp"] and t["kind"] == "mcp_server"
               and t["evidence"] == "inferred" and not t["ports"] for t in agent["tools"]):
        raise RuntimeError("The owned stdio MCP child is missing its conservative evidence label")
    if any(f["source"] == "recent" or not Path(f["path"]).resolve().is_relative_to(base.resolve())
           for f in agent["folders"]):
        raise RuntimeError("An agent screenshot could contain unrelated folder metadata")
    plans = inspect("--stop-ports", "--dry-run")
    if len(plans) != 2 or any(plan["blocked"] for plan in plans):
        raise RuntimeError("The fixture did not yield two executable reviewed service previews")
    protocols = []
    for plan in plans:
        verifies = [s["protocol"] for s in plan["steps"] if s["action"] == "verify_free"]
        if len(verifies) != 1:
            raise RuntimeError("A fixture preview broadened into another protocol")
        protocols.extend(verifies)
        refs = [p["pid"] for s in plan["steps"] if s["action"] == "signal_processes"
                for p in s["processes"]]
        if not refs or not set(refs).issubset({owned["tcp"], owned["udp"]}):
            raise RuntimeError("A service preview included the agent session or a non-service child")
    if sorted(protocols) != ["tcp", "udp"]:
        raise RuntimeError("The fixture previews did not preserve both exact protocols")


def verify_owned_alive(owned: dict[str, int]) -> None:
    for key in ("root", "tcp", "udp", "mcp"):
        os.kill(owned[key], 0)
    with socket.create_connection(("127.0.0.1", owned["port"]), timeout=2) as tcp:
        tcp.sendall(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n")
        if b"HTTP/1.1 200" not in tcp.recv(4096):
            raise RuntimeError("The owned TCP listener changed after a cancelled review")
    with socket.socket(type=socket.SOCK_DGRAM) as udp:
        udp.settimeout(2)
        udp.sendto(b"check", ("127.0.0.1", owned["port"]))
        if udp.recv(64) != b"fixture-alive":
            raise RuntimeError("The owned UDP listener changed after a cancelled review")


def verify_graph_fixture(binary: Path, base: Path, env: dict[str, str],
                         children: list[subprocess.Popen], ports: list[int]) -> None:
    """Verify the Graph search and selected-node neighbourhood cannot reveal other processes."""
    result = subprocess.run(
        [str(binary), "--no-docker", "graph", "--all", "--json"],
        cwd=base, env=env, capture_output=True, text=True, timeout=30,
    )
    if result.returncode != 0:
        raise RuntimeError("The Graph fixture inspection failed; no transcript was saved")
    graph = json.loads(result.stdout)
    clusters = {cluster["id"]: cluster["name"] for cluster in graph["clusters"]}
    query = base.name.lower()
    matches = []
    for node in graph["nodes"]:
        fields = [node.get(key) for key in ("label", "subtitle", "project", "project_root")]
        fields.extend([node.get("framework", {}).get("name") if node.get("framework") else None,
                       node.get("container", {}).get("name") if node.get("container") else None,
                       clusters.get(node.get("cluster"))])
        if any(query in value.lower() for value in fields if isinstance(value, str)):
            matches.append(node)
    owned_pids = {child.pid for child in children[:4]}
    if len(matches) != 4 or {node.get("root_pid") for node in matches} != owned_pids:
        raise RuntimeError("The Graph search did not isolate exactly four owned fixture services")
    expected = {(port, "udp" if index == 3 else "tcp") for index, port in enumerate(ports)}
    listeners = {(listener["port"], listener["protocol"])
                 for node in matches for listener in node["ports"]}
    if listeners != expected or any(not set(node["pids"]).issubset(owned_pids) for node in matches):
        raise RuntimeError("The Graph capture includes a listener or process outside its fixtures")
    if any(not node.get("project_root")
           or not Path(node["project_root"]).resolve().is_relative_to(base.resolve())
           for node in matches):
        raise RuntimeError("The Graph capture includes unrelated project metadata")
    owned_ids = {node["id"] for node in matches}
    if any((edge["from"] in owned_ids or edge["to"] in owned_ids)
           and not {edge["from"], edge["to"]}.issubset(owned_ids) for edge in graph["edges"]):
        raise RuntimeError("The Graph fixture neighbourhood includes an unrelated process")


def capture_graph_view(binary: Path, base: Path, env: dict[str, str],
                       children: list[subprocess.Popen], ports: list[int],
                       terminals: list[Terminal]) -> CaptureScreen:
    # Give the project-less UDP fixture its own metadata only after the original
    # CLI/Ports captures. Graph search deliberately never inspects process commands.
    (base / "statsd" / "pyproject.toml").write_text('[project]\nname = "statsd"\n')
    verify_graph_fixture(binary, base, env, children, ports)
    terminal = Terminal([str(binary), "--no-docker", "tui"], base, env, 180, 36)
    terminals.append(terminal)
    terminal.wait_for(lambda text: "◉ holdmap" in text, "the Graph capture initial frame")
    terminal.write("\t/" + base.name + "\r")

    def owned_frame(text: str) -> bool:
        lines = text.splitlines()
        return (base.name in lines[1] and "Esc to clear" in lines[1]
                and "4 services" in lines[0] and "Graph · 4 of" in text
                and all(f":{port}" in text for port in ports)
                and all(name in text for name in ("shop-web", "docs-site", "orders-api", "statsd")))

    terminal.wait_for(owned_frame, "the four owned services in the Graph search")
    terminal.write("p")
    terminal.wait_for(lambda text: owned_frame(text) and "paused" in text,
                      "the paused, isolated Graph search")
    verify_graph_fixture(binary, base, env, children, ports)
    for _ in range(4):
        terminal.pump()
    screen = copy.deepcopy(terminal.screen)
    terminal.write("q")
    if terminal.finish() != 0:
        raise RuntimeError("The Graph TUI did not exit cleanly")
    return screen


def capture_agent_views(binary: Path, base: Path, env: dict[str, str], children: list[subprocess.Popen],
                        terminals: list[Terminal]) -> tuple[dict[str, object], dict[str, int]]:
    owned = create_agent_fixture(base, children)
    verify_agent_fixture(binary, base, env, owned)
    screens = {}
    cli = Terminal([str(binary), "--no-docker", "agents", str(owned["root"]), "--wide"],
                   base, env, 180, 100)
    terminals.append(cli)
    cli.stream.feed(f"$ holdmap --no-docker agents {owned['root']} --wide\r\n")
    if cli.finish() != 0 or "server-filesystem" not in "\n".join(cli.screen.display):
        raise RuntimeError("The CLI agent capture did not describe the owned stdio MCP child")
    screens["cli-agents-dark.png"] = copy.deepcopy(cli.screen)
    for columns, rows, name in [(180, 36, "tui-agents-dark.png"), (80, 24, "tui-agents-narrow-dark.png")]:
        terminal = Terminal([str(binary), "--no-docker", "tui"], base, env, columns, rows)
        terminals.append(terminal)
        terminal.wait_for(lambda text: "◉ holdmap" in text, "the Agents capture initial frame")
        terminal.write("\x1b[Z")  # Shift+Tab opens Agents directly from Ports.
        terminal.wait_for(lambda text: "Agent details" in text, "the Agents tab")
        query = "pid:" + str(owned["root"])

        def owned_frame(text: str) -> bool:
            lines = text.splitlines()
            return (query in lines[1] and "/ edit" in lines[1]
                    and re.search(r"\b1(?:/\d+)? agents\b", lines[0]) is not None
                    and f"Google · PID {owned['root']}" in text and "Gemini CLI" in text)

        # Folder searches can include a real parent agent that owns the capture TUI.
        # An exact PID is the ownership boundary for every published agent frame.
        terminal.write("/" + query + "\r")
        terminal.wait_for(lambda text: owned_frame(text) and "2 services" in text,
                          "the exact owned agent and its two services")
        terminal.write("p")
        terminal.wait_for(lambda text: owned_frame(text) and "paused" in text,
                          "the paused owned agent report")
        terminal.write("/\x15pid:0\r")
        terminal.wait_for(lambda text: "No agents match this search" in text,
                          "the empty agent query")
        terminal.write("/\x15" + query + "\rjk\x1b[H\x1b[F")
        terminal.wait_for(owned_frame, "the agent navigation and restored exact PID query")
        # Scroll to the inferred, port-less tool in the narrow layout while keeping identity pinned.
        if columns < 100:
            terminal.write("\x1b[6~")
            terminal.wait_for(lambda text: "server-filesystem" in text and "Gemini CLI" in text,
                              "the narrow running-tool details")
        else:
            terminal.wait_for(lambda text: "server-filesystem" in text,
                              "the wide running-tool details")
        for _ in range(4):
            terminal.pump()
        screens[name] = copy.deepcopy(terminal.screen)
        terminal.write("x")
        terminal.wait_for(lambda text: "Review service stops" in text and "cancel" in text,
                          "the reviewed service-stop dialog")
        if columns >= 100:
            terminal.wait_for(lambda text: f"{owned['port']}/tcp" in text
                              and f"{owned['port']}/udp" in text and "SIGTERM" in text,
                              "both retained protocol-specific stop plans")
            screens["tui-agent-stop-confirm-dark.png"] = copy.deepcopy(terminal.screen)
        # Explicit default cancellation, then reopen and cancel with the other supported key.
        terminal.write("\r")
        terminal.wait_for(lambda text: "Review service stops" not in text and "Agent details" in text,
                          "Enter cancelling the review")
        verify_owned_alive(owned)
        terminal.write("x")
        terminal.wait_for(lambda text: "Review service stops" in text, "the second service review")
        terminal.write("n")
        terminal.wait_for(lambda text: "Review service stops" not in text and "Agent details" in text,
                          "n cancelling the review")
        verify_owned_alive(owned)
        terminal.write("q")
        if terminal.finish() != 0:
            raise RuntimeError("The Agents TUI did not exit cleanly")
    verify_agent_fixture(binary, base, env, owned)
    verify_owned_alive(owned)
    return screens, owned


def mask_account(screen) -> None:
    accounts = {getpass.getuser(), pwd.getpwuid(os.getuid()).pw_name}
    for account in accounts - {"", "devuser"}:
        replacement = "devuser"[:len(account)].ljust(len(account))
        # Only account contexts: home-path segments and the displayed User field.
        # A user named `node` or `root` must not rename a process or alter a plan.
        pattern = re.compile(rf"(?:/(?:Users|home)/|\b(?:User|user)\s+|you \()({re.escape(account)})(?=$|[^A-Za-z0-9_.-])")
        for y, row in enumerate(screen.display):
            for match in pattern.finditer(row):
                for offset, char in enumerate(replacement):
                    x = match.start(1) + offset
                    screen.buffer[y][x] = screen.buffer[y][x]._replace(data=char)


def find_font(requested: str | None) -> Path:
    candidates = [Path(requested)] if requested else [
        Path("/System/Library/Fonts/Menlo.ttc"),
        Path("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
        Path("/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf"),
    ]
    for path in candidates:
        if path.is_file():
            return path
    raise RuntimeError("No supported monospace font found; pass --font /path/to/font.ttf")


def named(value: str, theme: dict) -> str:
    return XTERM16.get(value.lower(), value) if theme["named_xterm"] else value


def color(value: str, default: str, theme: dict) -> str:
    value = named(value, theme)
    if value == "default":
        return default
    if value == FAINT:
        return theme["faint"]
    if value in theme["palette"]:
        return theme["palette"][value]
    if value.lower() in theme["greys"]:
        return theme["greys"][value.lower()]
    if len(value) == 6 and all(char in "0123456789abcdef" for char in value.lower()):
        return "#" + value
    return default


def render(screen, path: Path, font_path: Path, size: int, crop: bool = False) -> None:
    """Render one captured screen as `<name>-dark.png` and its `<name>-light.png` twin."""
    for name, theme in THEMES.items():
        render_theme(screen, path.with_name(path.name.replace("-dark.png", f"-{name}.png")), font_path, size, crop, theme)


def render_theme(screen, path: Path, font_path: Path, size: int, crop: bool, theme: dict) -> None:
    background, foreground = theme["background"], theme["foreground"]
    # Supersampling improves small text while preserving the terminal's cell geometry.
    scale, padding = 2, 18
    font = ImageFont.truetype(str(font_path), size * scale)
    if font_path.suffix == ".ttc":
        bold = ImageFont.truetype(str(font_path), size * scale, index=1)
    else:
        bold_path = font_path.with_name(font_path.name.replace("-Regular", "-Bold") if "-Regular" in font_path.name else font_path.stem + "-Bold" + font_path.suffix)
        bold = ImageFont.truetype(str(bold_path), size * scale) if bold_path.exists() else font
    symbol_path = Path("/System/Library/Fonts/Apple Symbols.ttf")
    symbols = ImageFont.truetype(str(symbol_path), size * scale) if symbol_path.exists() else font
    missing_glyph = bytes(font.getmask("\uffff"))
    cell_width = round(font.getlength("M"))
    line_height = round(size * 1.5 * scale)
    rows = screen.lines
    if crop:
        rows = max(index + 1 for index, line in enumerate(screen.display) if line.strip()) + 1
    width = screen.columns * cell_width + padding * 2 * scale
    height = rows * line_height + padding * 2 * scale
    output = Image.new("RGB", (width, height), background)
    draw = ImageDraw.Draw(output)
    for y in range(rows):
        for x in range(screen.columns):
            cell = screen.buffer[y][x]
            fg, bg = color(cell.fg, foreground, theme), color(cell.bg, background, theme)
            if cell.reverse:
                fg, bg = bg, fg
            if theme["on_color"] and bg != background and named(cell.bg, theme) in theme["palette"]:
                fg = theme["on_color"]
            left, top = padding * scale + x * cell_width, padding * scale + y * line_height
            if bg != background:
                draw.rectangle((left, top, left + cell_width - 1, top + line_height - 1), fill=bg)
            if cell.data.strip():
                selected_font = bold if cell.bold else font
                if bytes(font.getmask(cell.data)) == missing_glyph:
                    selected_font = symbols
                # Terminals commonly draw box glyphs on the cell grid so borders connect
                # even when the selected font's line-height exceeds its glyph bounds.
                cx, cy = left + cell_width / 2, top + line_height / 2
                box_connections = {"─": "lr", "│": "ud", "┌": "rd", "┐": "ld", "└": "ru", "┘": "lu", "├": "rud", "┤": "lud", "┬": "lrd", "┴": "lru", "┼": "lrud"}
                rounded = {"╭": (cx, cy, cx + cell_width, cy + line_height, 180, 270), "╮": (cx - cell_width, cy, cx, cy + line_height, 270, 360), "╰": (cx, cy - line_height, cx + cell_width, cy, 90, 180), "╯": (cx - cell_width, cy - line_height, cx, cy, 0, 90)}
                if cell.data in box_connections:
                    ends = {"l": (left, cy), "r": (left + cell_width, cy), "u": (cx, top), "d": (cx, top + line_height)}
                    for direction in box_connections[cell.data]:
                        draw.line((cx, cy, *ends[direction]), fill=fg, width=scale)
                elif cell.data in rounded:
                    *bounds, start, end = rounded[cell.data]
                    draw.arc(bounds, start, end, fill=fg, width=scale)
                elif cell.data == "⏸":
                    for offset in (0.25, 0.65):
                        draw.rectangle((left + cell_width * offset, top + line_height * 0.25,
                                        left + cell_width * (offset + 0.15), top + line_height * 0.75), fill=fg)
                else:
                    draw.text((left, top), cell.data, font=selected_font, fill=fg)
            if cell.underscore:
                draw.line((left, top + line_height - 3, left + cell_width, top + line_height - 3), fill=fg)
    output.resize((width // scale, height // scale), Image.Resampling.LANCZOS).save(path)


def capture(binary: Path, output: Path, font: Path) -> None:
    binary_digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    children: list[subprocess.Popen] = []
    terminals: list[Terminal] = []
    with tempfile.TemporaryDirectory(prefix="holdmap-capture-", dir="/tmp") as temporary:
        base = Path(temporary)
        state = base / "state"
        state.mkdir()
        env = dict(os.environ, TERM="xterm-256color", HOLDMAP_HOME=str(state), HOLDMAP_COLOR="always")
        env.pop("NO_COLOR", None)
        try:
            port, ports = create_fixtures(base, children)
            inspection = subprocess.run(
                [str(binary), "--no-docker", "explain", str(port), "--json"],
                cwd=base, env=env, capture_output=True, text=True, timeout=30,
            )
            if inspection.returncode not in (0, 1):
                raise RuntimeError("The CLI explanation ownership check failed")
            entries = json.loads(inspection.stdout)["entries"]
            if not entries or any(entry["protocol"] != "tcp"
                                  or entry["pid"] != children[0].pid
                                  or not set(entry["pids"]).issubset({children[0].pid})
                                  for entry in entries):
                raise RuntimeError("The CLI explanation includes a listener outside its owned fixture")
            cli = Terminal([str(binary), "--no-docker", "explain", str(port)], base, env, 104, 100)
            terminals.append(cli)
            # This command header names the command actually invoked; body cells come from the PTY.
            cli.stream.feed(f"$ holdmap --no-docker explain {port}\r\n")
            if cli.finish() not in (0, 1):
                raise RuntimeError("The CLI explain capture failed")
            if f"Port {port} is held by" not in "\n".join(cli.screen.display):
                raise RuntimeError("The CLI capture did not explain the fixture listener")
            cli_screen = copy.deepcopy(cli.screen)
            tui = Terminal([str(binary), "--no-docker", "tui"], base, env, 180, 36)
            terminals.append(tui)
            tui.wait_for(lambda text: "◉ holdmap" in text, "the TUI initial frame")
            tui.write("/" + base.name + "\r")
            tui.wait_for(lambda text: f"Ports 4" in text and all(f":{p}" in text for p in ports), "the four filtered fixture rows")
            tui.write("p")
            tui.wait_for(lambda text: "paused" in text and base.name in text, "the filtered paused frame")
            # Drain pending diffs while preserving the paused screen before the app restores the terminal.
            for _ in range(4):
                tui.pump()
            tui_screen = copy.deepcopy(tui.screen)
            tui.write("q")
            if tui.finish() != 0:
                raise RuntimeError("The TUI did not exit cleanly")
            graph_screen = capture_graph_view(binary, base, env, children, ports, terminals)
            for screen in [cli_screen, tui_screen, graph_screen]:
                mask_account(screen)
            output.mkdir(parents=True, exist_ok=True)
            render(cli_screen, output / "cli-explain-dark.png", font, 16, crop=True)
            render(tui_screen, output / "tui-list-dark.png", font, 14)
            render(graph_screen, output / "tui-graph-dark.png", font, 14)
            # Start agent listeners after the original four-row captures are complete.
            agent_screens, owned = capture_agent_views(binary, base, env, children, terminals)
            if hashlib.sha256(binary.read_bytes()).hexdigest() != binary_digest:
                raise RuntimeError("The CLI binary changed during capture; rebuild and capture again")
            for name, screen in agent_screens.items():
                mask_account(screen)
                render(screen, output / name, font, 14, crop=name == "cli-agents-dark.png")
            print(f"Captured real CLI/TUI output: fixture ports {', '.join(map(str, ports))}")
            print("Verified Graph search: four owned fixture listeners and no unrelated neighbourhood processes")
            print(f"Verified owned agent {owned['root']}: {owned['port']}/tcp + {owned['port']}/udp, "
                  "inferred stdio MCP, Enter/n cancellation, all four fixture processes alive")
            print(f"Captured CLI binary SHA-256: {binary_digest}")
            print(f"Saved canonical screenshots in {output}")
        finally:
            for terminal in reversed(terminals):
                terminal.close()
            for child in reversed(children):
                terminate_owned(child)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/holdmap")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "docs/screenshots")
    parser.add_argument("--font")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"Build the CLI first (`cargo build -p holdmap`): {binary} does not exist")
    capture(binary, args.output_dir.resolve(), find_font(args.font))


if __name__ == "__main__":
    main()

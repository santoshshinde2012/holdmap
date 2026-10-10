#!/usr/bin/env python3
"""Refresh the published CLI/TUI screenshots from real terminal output.

Optional maintainer tooling for macOS/Linux; this is not a production dependency.
Create an isolated venv, install scripts/requirements-terminal-screenshots.txt,
build `cargo build -p holdmap`, then run this script with that venv's Python.
Use --binary, --output-dir or --font to override the defaults.

The fixtures use built-in HTTP/UDP servers and small project manifests that
exercise Holdmap's framework classification. They need no npm install and make
no external connections. Ports 3000/5173/8000/8125 are preferred; occupied ports
fall back to OS-assigned ports. This script never calls a stop command: cleanup
terminates only the child process groups it created. App state lives in a temp
directory. Screenshots retain the actual ports, PIDs, timings, memory and plans.

The TUI is captured only after filtering to the unique fixture path. ANSI output
is held in memory, not saved as an unfiltered transcript. For published images,
the current account name is replaced with `devuser`, truncated/padded to its
original width; all other terminal cells and styles are retained. The renderer uses a fixed dark
terminal palette and font; it does not reconstruct or manually edit app output.
"""

from __future__ import annotations

import argparse
import codecs
import copy
import errno
import getpass
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
BACKGROUND = "#0c1117"
FOREGROUND = "#c6d3de"
PALETTE = {
    "black": "#151b23", "red": "#ff757f", "green": "#9ece6a",
    "brown": "#e0af68", "blue": "#7aa2f7", "magenta": "#bb9af7",
    "cyan": "#7dcfff", "white": "#c6d3de", "brightblack": "#788a9b",
    "brightred": "#ff9eab", "brightgreen": "#b9f27c", "brightbrown": "#ffdb8e",
    "brightblue": "#9ac5ff", "brightmagenta": "#d4b6ff",
    "brightcyan": "#b4f9f8", "brightwhite": "#f4f7fb",
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
            self.cursor.attrs = self.cursor.attrs._replace(fg="788a9b")


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
        ports.append(fixture(children, directory, [node, "server.js", str(preferred_port(preferred))]))

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


def color(value: str, default: str) -> str:
    if value == "default":
        return default
    if value in PALETTE:
        return PALETTE[value]
    if len(value) == 6 and all(char in "0123456789abcdef" for char in value.lower()):
        return "#" + value
    return default


def render(screen, path: Path, font_path: Path, size: int, crop: bool = False) -> None:
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
    output = Image.new("RGB", (width, height), BACKGROUND)
    draw = ImageDraw.Draw(output)
    for y in range(rows):
        for x in range(screen.columns):
            cell = screen.buffer[y][x]
            fg, bg = color(cell.fg, FOREGROUND), color(cell.bg, BACKGROUND)
            if cell.reverse:
                fg, bg = bg, fg
            left, top = padding * scale + x * cell_width, padding * scale + y * line_height
            if bg != BACKGROUND:
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
            for screen in [cli_screen, tui_screen]:
                mask_account(screen)
            output.mkdir(parents=True, exist_ok=True)
            render(cli_screen, output / "cli-explain-dark.png", font, 16, crop=True)
            render(tui_screen, output / "tui-list-dark.png", font, 14)
            print(f"Captured real CLI/TUI output: fixture ports {', '.join(map(str, ports))}")
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

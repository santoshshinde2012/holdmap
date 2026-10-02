#!/usr/bin/env bash
# Start a handful of realistic dev servers for trying portwise (and for screenshots).
# Usage: scripts/demo-servers.sh [start|stop]
set -euo pipefail
DEMO="${PORTWISE_DEMO_DIR:-${TMPDIR:-/tmp}/portwise-demo}"
PIDS="$DEMO/pids"

start() {
  mkdir -p "$DEMO"/{shop-web,orders-api,docs-site,ml-service}
  : > "$PIDS"

  # 1. "Next.js" app started through npm (npm → sh → node tree) on 3000
  cat > "$DEMO/shop-web/package.json" <<'JSON'
{ "name": "@acme/shop-web", "private": true,
  "scripts": { "dev": "node server.js" },
  "dependencies": { "next": "15.0.0", "react": "19.0.0" } }
JSON
  cat > "$DEMO/shop-web/server.js" <<'JS'
const http = require('http');
const port = process.env.PORT || 3000;
http.createServer((_, res) => res.end('<title>shop-web</title>hello')).listen(port, '127.0.0.1');
process.on('SIGTERM', () => process.exit(0));
JS
  (cd "$DEMO/shop-web" && git init -q -b feat/checkout 2>/dev/null || true)
  (cd "$DEMO/shop-web" && PORT=3000 nohup npm run dev >/dev/null 2>&1 & echo $! >> "$PIDS")

  # 2. A Vite-style dev server on 5173 (IPv4 + IPv6)
  cat > "$DEMO/docs-site/package.json" <<'JSON'
{ "name": "docs-site", "scripts": { "dev": "vite" }, "devDependencies": { "vite": "6.0.0" } }
JSON
  cat > "$DEMO/docs-site/vite.js" <<'JS'
const http = require('http');
const h = (_, r) => r.end('vite');
http.createServer(h).listen(5173, '127.0.0.1');
http.createServer(h).listen(5173, '::1');
JS
  (cd "$DEMO/docs-site" && nohup node vite.js >/dev/null 2>&1 & echo $! >> "$PIDS")

  # 3. A Python API exposed on all interfaces (0.0.0.0:8000)
  cat > "$DEMO/ml-service/pyproject.toml" <<'TOML'
[project]
name = "ml-service"
dependencies = ["fastapi>=0.110", "uvicorn"]
TOML
  (cd "$DEMO/ml-service" && nohup python3 -m http.server 8000 --bind 0.0.0.0 >/dev/null 2>&1 & echo $! >> "$PIDS")

  # 4. A FastAPI-style API on 8080 that ignores SIGTERM (shows SIGKILL escalation)
  cat > "$DEMO/orders-api/pyproject.toml" <<'TOML'
[project]
name = "orders-api"
dependencies = ["fastapi>=0.110"]
TOML
  cat > "$DEMO/orders-api/serve.py" <<'PY'
import signal, socket, time
signal.signal(signal.SIGTERM, signal.SIG_IGN)
s = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
s.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 0)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("::", 8080)); s.listen()
while True: time.sleep(60)
PY
  (cd "$DEMO/orders-api" && nohup python3 serve.py >/dev/null 2>&1 & echo $! >> "$PIDS")

  # 5. A UDP service (statsd-like) on 8125
  (cd "$DEMO" && nohup python3 -c 'import socket,time; s=socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.bind(("127.0.0.1",8125)); time.sleep(10**9)' >/dev/null 2>&1 & echo $! >> "$PIDS")
  sleep 1
  echo "demo servers started in $DEMO (ports 3000 5173 8000 8080 8125/udp)"
}

killtree() {
  local pid=$1 child
  for child in $(pgrep -P "$pid" 2>/dev/null); do killtree "$child"; done
  kill -9 "$pid" 2>/dev/null || true
}

stop() {
  [ -f "$PIDS" ] || { echo "no demo running"; return; }
  while read -r pid; do killtree "$pid"; done < "$PIDS"
  rm -f "$PIDS"; echo "demo servers stopped"
}

case "${1:-start}" in start) start ;; stop) stop ;; *) echo "usage: $0 [start|stop]"; exit 2 ;; esac

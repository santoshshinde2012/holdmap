#!/usr/bin/env bash
# Start a handful of realistic dev servers for trying portwise (and for screenshots).
# Usage: scripts/demo-servers.sh [start|stop]
set -euo pipefail
DEMO="${PORTWISE_DEMO_DIR:-${TMPDIR:-/tmp}/portwise-demo}"
PIDS="$DEMO/pids"   # one "name pid" line per server we started; only ever appended to

alive() { [ -n "$1" ] && kill -0 "$1" 2>/dev/null; }

# True if a server called $1 that we started earlier is still running (makes start idempotent).
running() {
  [ -f "$PIDS" ] || return 1
  local name pid
  while read -r name pid; do
    [ "$name" = "$1" ] && alive "$pid" && return 0
  done < "$PIDS"
  return 1
}

# launch NAME DIR CMD...: start CMD in DIR in the background unless NAME is already running,
# and append its pid to the pid file.
launch() {
  local name=$1 dir=$2; shift 2
  if running "$name"; then echo "  $name already running"; return; fi
  (cd "$dir" || exit 1; nohup "$@" </dev/null >/dev/null 2>&1 & echo "$name $!" >> "$PIDS")
}

start() {
  mkdir -p "$DEMO"/{shop-web,orders-api,docs-site,ml-service}
  touch "$PIDS"

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
  PORT=3000 launch shop-web "$DEMO/shop-web" npm run dev

  # 2. A Vite-style dev server on 5173 (IPv4 + IPv6)
  cat > "$DEMO/docs-site/package.json" <<'JSON'
{ "name": "docs-site", "scripts": { "dev": "vite" }, "devDependencies": { "vite": "6.0.0" } }
JSON
  cat > "$DEMO/docs-site/vite.js" <<'JS'
const http = require('http');
const h = (_, r) => r.end('vite');
http.createServer(h).listen(5173, '127.0.0.1');
http.createServer(h).listen(5173, '::1');
// Like Vite's `server.proxy`: keep a connection to the orders API on 8080.
(function dial() {
  const s = require('net').connect(8080, '127.0.0.1');
  s.on('error', () => {}); s.on('close', () => setTimeout(dial, 1000));
})();
JS
  launch docs-site "$DEMO/docs-site" node vite.js

  # 3. A Python API exposed on all interfaces (0.0.0.0:8000)
  cat > "$DEMO/ml-service/pyproject.toml" <<'TOML'
[project]
name = "ml-service"
dependencies = ["fastapi>=0.110", "uvicorn"]
TOML
  launch ml-service "$DEMO/ml-service" python3 -m http.server 8000 --bind 0.0.0.0

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
  launch orders-api "$DEMO/orders-api" python3 serve.py

  # 5. A UDP service (statsd-like) on 8125
  launch statsd "$DEMO" python3 -c 'import socket,time; s=socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.bind(("127.0.0.1",8125)); time.sleep(10**9)'

  # 6. acme-shop: a pnpm monorepo stack with live connections, for `portwise graph`:
  #    concurrently → { web :3100 (Next.js), worker }   web → api :4000 (FastAPI) → db :5432, cache :6379
  #    worker → api, cache;  api → one external host.  (docs-site's vite proxy → orders-api :8080)
  acme
  sleep 1.5
  echo "demo servers started in $DEMO (ports 3000 3100 4000 5173 5432 6379 8000 8080 8125/udp)"
}

acme() {
  local A="$DEMO/acme-shop"
  mkdir -p "$A"/{apps/web,services/api,services/db,services/cache,services/worker,node_modules/concurrently/bin}
  echo '{ "name": "acme-shop", "private": true, "scripts": { "dev": "concurrently npm:dev:*" } }' > "$A/package.json"
  printf "packages:\n  - apps/*\n  - services/*\n" > "$A/pnpm-workspace.yaml"
  printf "services:\n  db: { image: postgres:16, ports: ['5432:5432'] }\n  cache: { image: redis:7, ports: ['6379:6379'] }\n" > "$A/compose.yaml"
  (cd "$A" && git init -q -b main 2>/dev/null || true)
  # A tiny TCP "mesh" helper: listen on PORT (accept + hold), keep connections to upstreams open.
  cat > "$A/tcpmesh.py" <<'PY'
import socket, sys, threading, time
def hold(conn):
    try:
        while conn.recv(4096): pass
    except OSError: pass
def serve(port):
    s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(("127.0.0.1", port)); s.listen(64)
    while True:
        c, _ = s.accept(); threading.Thread(target=hold, args=(c,), daemon=True).start()
def upstream(host, port, n=1):
    while True:
        try:
            conns = [socket.create_connection((host, port), timeout=3) for _ in range(n)]
            while True:
                for c in conns: c.sendall(b"."); 
                time.sleep(2)
        except OSError:
            time.sleep(2)
def run(port, ups=()):
    for u in ups: threading.Thread(target=upstream, args=u, daemon=True).start()
    if port: serve(port)
    else:
        while True: time.sleep(60)
PY
  : > "$A/services/db/requirements.txt"; : > "$A/services/cache/requirements.txt"
  echo 'import sys; sys.path.insert(0, "../.."); from tcpmesh import run; run(5432)' > "$A/services/db/postgres.py"
  echo 'import sys; sys.path.insert(0, "../.."); from tcpmesh import run; run(6379)' > "$A/services/cache/redis-server.py"
  printf '[project]\nname = "api"\ndependencies = ["fastapi>=0.110", "sqlalchemy", "redis"]\n' > "$A/services/api/pyproject.toml"
  cat > "$A/services/api/main.py" <<'PY'
import sys; sys.path.insert(0, "../..")
from tcpmesh import run
run(4000, [("127.0.0.1", 5432, 2), ("127.0.0.1", 6379, 1), ("api.github.com", 443, 1)])
PY
  echo '{ "name": "web", "scripts": { "dev": "next dev -p 3100" }, "dependencies": { "next": "15.0.0", "react": "19.0.0" } }' > "$A/apps/web/package.json"
  echo '{ "name": "worker", "scripts": { "dev": "node worker.js" }, "dependencies": { "bullmq": "5" } }' > "$A/services/worker/package.json"
  cat > "$A/node_modules/concurrently/bin/mesh.js" <<'JS'
// Keep N connections to host:port open, reconnecting if they drop.
const net = require('net');
module.exports = (port, n = 1) => {
  for (let i = 0; i < n; i++) (function dial() {
    const s = net.connect(port, '127.0.0.1');
    const t = setInterval(() => s.write('.'), 2000);
    s.on('error', () => {}); s.on('close', () => { clearInterval(t); setTimeout(dial, 1000); });
  })();
};
JS
  cat > "$A/apps/web/server.js" <<'JS'
const http = require('http');
require('../../node_modules/concurrently/bin/mesh.js')(4000, 2);
http.createServer((_, res) => res.end('<title>acme web</title>')).listen(3100, '127.0.0.1');
JS
  cat > "$A/services/worker/worker.js" <<'JS'
const mesh = require('../../node_modules/concurrently/bin/mesh.js');
mesh(6379, 1); mesh(4000, 1);
setInterval(() => {}, 1 << 30);
JS
  # A supervisor like `concurrently` that runs web + worker side by side.
  cat > "$A/node_modules/concurrently/bin/concurrently.js" <<'JS'
const { spawn } = require('child_process');
const path = require('path');
const root = path.resolve(__dirname, '../../..');
const kids = [['apps/web', 'server.js'], ['services/worker', 'worker.js']].map(([dir, file]) =>
  spawn(process.execPath, [file], { cwd: path.join(root, dir), stdio: 'ignore' }));
const stop = () => { kids.forEach(k => k.kill('SIGTERM')); process.exit(0); };
process.on('SIGTERM', stop); process.on('SIGINT', stop);
setInterval(() => {}, 1 << 30);
JS
  launch acme-db "$A/services/db" python3 postgres.py
  launch acme-cache "$A/services/cache" python3 redis-server.py
  sleep 0.3
  launch acme-api "$A/services/api" python3 main.py
  sleep 0.3
  launch acme-dev "$A" node node_modules/concurrently/bin/concurrently.js "npm:dev:*"
}

killtree() {
  local pid=$1 child
  for child in $(pgrep -P "$pid" 2>/dev/null); do killtree "$child"; done
  kill -9 "$pid" 2>/dev/null || true
}

stop() {
  [ -s "$PIDS" ] || { echo "no demo running"; return; }
  local name pid n=0
  while read -r name pid; do
    [ -n "$pid" ] || { pid=$name; name=?; }   # tolerate old pid-only files
    if alive "$pid"; then killtree "$pid"; n=$((n + 1)); fi
  done < "$PIDS"
  rm -f "$PIDS"; echo "demo servers stopped ($n)"
}

case "${1:-start}" in start) start ;; stop) stop ;; *) echo "usage: $0 [start|stop]"; exit 2 ;; esac

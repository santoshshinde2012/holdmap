# portwise user guide

This guide walks through the everyday jobs portwise does, first on the command line and then in
the TUI and the desktop app. Every command and flag is listed in the [CLI reference](cli.md), and
AI agents are covered in the [MCP guide](mcp.md).

## Contents

1. [Find out what's on a port](#1-find-out-whats-on-a-port)
2. [Understand why it's busy](#2-understand-why-its-busy)
3. [Stop it safely](#3-stop-it-safely)
4. [Free a port and start your server](#4-free-a-port-and-start-your-server)
5. [Search and filter](#5-search-and-filter)
6. [See which services depend on which](#6-see-which-services-depend-on-which)
7. [Pins, watching, history and restart](#7-pins-watching-history-and-restart)
8. [Look at another machine](#8-look-at-another-machine)
9. [The TUI](#9-the-tui)
10. [The desktop app](#10-the-desktop-app)
11. [Scripts and CI](#11-scripts-and-ci)

## 1. Find out what's on a port

```sh
portwise list            # every listening port (alias: portwise ls)
portwise list --dev      # only likely dev servers, databases and containers
portwise inspect 3000    # everything about one port
```

`list` prints one row per port: protocol, address, PID, process and a description built from the
project (`package.json`, `Cargo.toml`, `pyproject.toml`…), the framework and the git branch. Ports
bound to `0.0.0.0` or `::` are reachable from your network and are marked as exposed. If some
sockets belong to other users, the footer says how many are hidden; run with `sudo` to see them.

![portwise list --dev](screenshots/cli-list-dark.png)

## 2. Understand why it's busy

```sh
portwise explain 3000    # alias: portwise why 3000
```

`explain` names the owner and says what kind of thing it is:

- **A dev-server tree** such as `npm run dev → sh → node`. Stopping only `node` would let `npm`
  start it again, so portwise plans to stop the whole tree.
- **A container.** The port is published by Docker, OrbStack, Colima, Rancher Desktop or Podman.
  portwise stops the container through the runtime.
- **A supervised service** (systemd, pm2, `brew services`). portwise uses the supervisor, because
  killing the process would just make it restart.
- **An OS feature**: AirPlay Receiver on macOS (5000/7000), HTTP.sys or an excluded port range on
  Windows, the WSL relay. portwise explains how to turn it off instead of killing anything.
- **Another user's process, or one you can't see.** You need elevated rights.
- **`TIME_WAIT`**: a connection that just closed. It clears by itself within a minute or two.

It ends with the recommended action, the exact plan, and the equivalent commands.

![portwise explain 3000](screenshots/cli-explain-dark.png)

## 3. Stop it safely

```sh
portwise stop 3000 --dry-run   # show the plan, change nothing
portwise stop 3000             # ask, then stop gracefully and check the port is free
portwise stop 3000 --yes       # don't ask
portwise stop 3000 --timeout 15s   # a longer grace period before SIGKILL
portwise kill 3000             # SIGKILL / TerminateProcess straight away
```

A stop sends SIGTERM (or `WM_CLOSE` on Windows) to the whole tree, waits for the grace period
(5 seconds by default), sends SIGKILL to anything still running, and then checks the port is
really free. If something brings it back, portwise tells you what.

You can also stop by PID or process name: `portwise stop pid:1234`, `portwise stop vite`, or
`--pid` and `--name`. `--no-tree` stops only the socket holder, and `--udp` targets UDP sockets.

Some processes are **protected**. portwise refuses to stop your editor's backend, a terminal, an
AI agent host, system services or itself, and exits with code 3. If you're sure, add
`--allow-protected` (it doesn't apply to PID 1, the OS core or portwise's own tree). Ports owned
by another user exit with code 4: run the command again with `sudo`.

![portwise stop 8080 escalating to SIGKILL](screenshots/cli-stop-dark.png)

## 4. Free a port and start your server

```sh
portwise run -p 3000 -- npm run dev    # free 3000 (safely), then run npm with PORT=3000
portwise run -p 3000 --fallback -- npm run dev   # use the next free port if 3000 can't be freed
portwise run -p 8000 --env APP_PORT -- python app.py   # set APP_PORT instead of PORT
portwise free-port --near 3000         # first free port at or after 3000
portwise free-port --range 8000-8999 --count 3
portwise wait 5432 --timeout 30s       # wait until Postgres accepts connections
portwise wait 3000 --free              # wait until 3000 is free
```

`run` asks before stopping the current owner unless you pass `--yes`, and it follows the same
safety rules as `stop`.

## 5. Search and filter

The query language works in `list`, the TUI search (`/`) and the desktop search box:

| Query | Matches |
|---|---|
| `:3000` | exactly port 3000 |
| `3000-3999` | a port range |
| `proto:udp` | UDP only (`proto:tcp` for TCP) |
| `pid:1234` | one process |
| `user:me` | your own processes |
| `next`, `shop-web`, `feat/login` | free text: process, project, framework, branch, command |

`list` adds flags for the common filters: `--dev`, `--mine`, `--exposed`, `--tcp`, `--udp`,
`--range`, `--all` (every socket, not only listeners), `--sort` (port, pid, name, proto, memory,
uptime) and `--wide` (full command and user columns).

## 6. See which services depend on which

```sh
portwise graph                    # a tree of services and their connections (alias: mesh)
portwise graph --cluster shop     # one cluster
portwise graph --mermaid          # paste into a GitHub comment or Markdown file
portwise graph --dot | dot -Tsvg > graph.svg
portwise stop --cluster shop --dry-run   # the dependency-ordered stop plan
```

portwise builds the graph from live local TCP connections: `web → api` means web has an open
connection to api, so web depends on it. Services are grouped into **clusters** by Docker Compose
project, Kubernetes namespace (for `kubectl port-forward`), supervisor (pm2, turbo, nx,
concurrently), monorepo workspace or git repository. Connections to the outside world collapse into
one "External hosts" node (`--no-external` hides it).

A cluster stop goes **dependents first** (web, then api, then db), so nothing reconnects to a
dependency that's shutting down. Services outside the cluster that still depend on it show up as
warnings.

![portwise graph](screenshots/cli-graph-dark.png)

## 7. Pins, watching, history and restart

```sh
portwise pin 3000 --label web   # pinned ports are listed first and watched even when free
portwise pins                   # pinned ports and whether they're in use
portwise unpin 3000
portwise watch                  # print new, closed and conflicting listeners as they happen
portwise watch --json           # the same as NDJSON, one event per line
portwise history                # what portwise stopped, with the command and directory
portwise restart 3000           # stop the current owner and run the same command again
```

Every stop is recorded with its command line and working directory, so `restart` can bring a
server back after you stopped it, even when the port is free. Restarted servers write their output
to the `logs/` folder in the configuration directory (see the [README](../README.md#settings-and-files)).

## 8. Look at another machine

```sh
portwise ssh devbox             # the remote machine's ports, read-only
portwise ssh deploy@10.0.0.5 graph
portwise ssh devbox -- list --dev   # run a remotely installed portwise with these arguments
portwise ssh devbox --agentless     # never use a remote portwise; read ss and ps
```

`ssh` uses your normal `ssh` configuration, keys and aliases. If portwise is installed on the other
machine it runs that; otherwise it reads `ss` and `ps` output. It never stops anything remotely.
Host names are validated, so a value starting with `-` can't be used to pass options to `ssh`.

## 9. The TUI

Run `portwise` with no arguments (or `portwise tui`). The list refreshes every 2 seconds. Move with
the arrow keys or `j`/`k`, press `Enter` to explain, `x` to stop (you'll see the plan first), `/` to
search and `?` for every key. `Tab` switches to the graph, where `C` stops the selected service's
cluster. The [README](../README.md#tui) has the full key table.

| Ports | Graph |
|---|---|
| ![TUI port list](screenshots/tui-list-dark.png) | ![TUI graph tab](screenshots/tui-graph-dark.png) |

## 10. The desktop app

The desktop app shows the same information with a mouse-friendly layout and a tray icon.

- **Find a port.** Type in the search box (`/`). Searching for a free port number shows "Port N
  is free" with a ready `portwise run` command.
- **Read the details.** Select a row (or press `Enter`). The Overview tab explains the owner and
  what Stop will do; Connections, Process, Network and Commands have the rest.
- **Stop it.** Press `⌫` or click Stop. The confirmation lists every step. Then you see progress,
  "Port N is free" and a toast with *Copy restart command*. `⇧⌫` force-kills.
- **Protected processes** show *"I understand — stop it anyway"*, which you have to tick before
  the button works.
- **Graph.** Press `g`. Hover a service to highlight its neighbours, select it to sync with the
  list, and press `s` to stop its cluster.
- **Everything else** is in the command palette (`⌘K` / `Ctrl K`): sorting, filters, theme, pins,
  history (`h`), the remote view and settings (`⌘,`).
- **In the background.** Closing the window keeps portwise in the tray or menu bar. It notifies
  you about new and conflicting dev servers and brings the window back with the global shortcut
  (`⌘⌥P` / `Ctrl+Alt+P` by default). Settings → General turns on launch at login.

| Details | Stop confirmation |
|---|---|
| ![Desktop details pane](screenshots/desktop-overview-dark.png) | ![Desktop stop confirmation](screenshots/desktop-stop-confirm-light.png) |

## 11. Scripts and CI

- Every read command has `--json`; `watch --json` streams NDJSON.
- Exit codes: `0` ok, `1` busy, not found or timed out, `2` error, `3` blocked by the safety
  policy, `4` needs elevation.
- `portwise stop --json` needs `--yes` (or `--dry-run`), because JSON output never prompts.
- `--color never` (or `NO_COLOR=1`) turns colours off; `--no-docker` skips container runtimes.

```sh
# Start the database, wait for it, then run the tests
docker compose up -d db && portwise wait 5432 --timeout 60s && npm test

# Fail a CI step if something is already listening on 8080
portwise wait 8080 --free --timeout 1s --quiet || { portwise explain 8080; exit 1; }

# The PID on port 3000, if any
portwise list :3000 --json | jq '.entries[0].pid'
```

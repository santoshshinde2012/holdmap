---
title: Project stacks
description: Describe a project's services once in .portwise.toml and start them in dependency order with portwise up.
order: 3
---

A `.portwise.toml` at the root of a project names its services, their ports and how to start them.
Then `portwise up` starts them in dependency order, `portwise status` shows them and `portwise down`
stops them, dependents first.

## Write one

`portwise init` writes a starter file from the dev servers running under the current folder.
Add `--print` to see it first. A typical file:

```toml
name = "shop"            # optional; defaults to the folder name
protect = [5432]         # ports portwise must never stop

[services.api]
port = 4000
command = "npm run dev"  # run through your shell, with PORT set
cwd = "apps/api"         # relative to this file
health = "/health"       # HTTP path checked by `status` (optional)

[services.web]
port = 3000
command = "npm run dev"
cwd = "apps/web"
depends_on = ["api"]     # started after api accepts connections
env = { API_URL = "http://localhost:4000" }

[services.db]
port = 5432              # no command: started elsewhere (docker compose), only checked
```

| Key | Meaning |
|---|---|
| `port` | The TCP port the service listens on. |
| `command` | How to start it. Runs through your shell with `PORT` set. Leave it out for services something else starts. |
| `cwd` | Working directory, relative to the file. |
| `depends_on` | Services that must be accepting connections first. |
| `env` | Extra environment variables. |
| `health` | HTTP path `status` probes. |
| `ready_timeout_s` | How long `up` waits for the port. |

## Run it

```sh
portwise up        # dependencies first; waits until each port accepts connections
portwise status    # running, stopped or held by something else, with HTTP status
portwise down      # dependents first, through the usual safety checks
```

```text
→ shop ~/code/shop/.portwise.toml
  ✔ db :5432 already running (PostgreSQL · container shop-db-1)
  ✔ api :4000 up after 0.8 s PID 51210 · log ~/.config/portwise/logs/shop-api.log
  ✔ web :3000 up after 1.4 s PID 51244 · log ~/.config/portwise/logs/shop-web.log
✔ shop is up
```

If a port is held by something that isn't part of the stack, `up` stops and tells you who it is.
`portwise up --replace` stops it first (with the normal plan and confirmation); ports listed in
`protect` are never touched. `down` leaves services without a `command` (such as a database
container) alone unless you pass `--all`, and `--dry-run` shows what `up` or `down` would do.

## Safety

The file runs commands through your shell, so portwise only trusts one that you (or root) own and
that other users can't write to. A `.portwise.toml` dropped into a shared folder such as `/tmp` is
refused.

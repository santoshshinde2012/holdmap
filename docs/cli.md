# portwise CLI reference

<!-- Generated from the clap definitions in crates/portwise-cli. Do not edit by hand:
     PORTWISE_BLESS=1 cargo test -p portwise cli_reference -->

This page is the `--help` output of every command of portwise 0.1.9. `cargo test` checks it
against the code, so it never drifts. For an overview see the [README](../README.md).

Global options (accepted by every command): `--color <auto|always|never>` (env `PORTWISE_COLOR`; `NO_COLOR` and `CLICOLOR_FORCE` are honoured too) and `--no-docker`.

| Command | What it does |
|---|---|
| [`portwise list`](#portwise-list) (alias `ls`) | List ports in use (listening sockets by default) |
| [`portwise inspect`](#portwise-inspect) | Show everything about a port: owner, process tree, project, plan |
| [`portwise explain`](#portwise-explain) (alias `why`) | Explain in plain English why a port is busy and what to do |
| [`portwise stop`](#portwise-stop) | Gracefully stop whatever holds a port (SIGTERM → SIGKILL), then verify it is free |
| [`portwise kill`](#portwise-kill) | Like `stop --force`: kill immediately (SIGKILL / TerminateProcess) |
| [`portwise free-port`](#portwise-free-port) | Print a free TCP port |
| [`portwise wait`](#portwise-wait) | Wait until a port is accepting connections (or free, with --free) |
| [`portwise run`](#portwise-run) | Free a port (safely) and run a command on it, with PORT set |
| [`portwise graph`](#portwise-graph) (alias `mesh`) | Show which services talk to which (dependencies, clusters) as a tree, JSON, DOT or Mermaid |
| [`portwise agents`](#portwise-agents) | Show the AI coding agents running here: their folders, access, ports and connections |
| [`portwise watch`](#portwise-watch) | Stream port events: new listeners, closed listeners, conflicts |
| [`portwise pin`](#portwise-pin) | Pin a port (favourite): shown first and watched even when free |
| [`portwise unpin`](#portwise-unpin) | Remove a pin |
| [`portwise pins`](#portwise-pins) | List pinned ports and whether they are in use |
| [`portwise history`](#portwise-history) | Ports portwise stopped recently, with the command that ran there |
| [`portwise restart`](#portwise-restart) | Stop what holds a port and start the same command again (or re-run it from history) |
| [`portwise open`](#portwise-open) | Open http://localhost:PORT in the browser |
| [`portwise up`](#portwise-up) | Start a project's services from its .portwise.toml, dependencies first |
| [`portwise down`](#portwise-down) | Stop a project's services (dependents first), through the usual safety checks |
| [`portwise status`](#portwise-status) | Show a project's services: running, stopped or held by something else, with HTTP status |
| [`portwise init`](#portwise-init) | Print a shell hook that explains "port already in use" errors, or write a .portwise.toml |
| [`portwise ssh`](#portwise-ssh) | Inspect another machine's ports over SSH (read-only, nothing to install remotely) |
| [`portwise tui`](#portwise-tui) | Open the interactive terminal UI |
| [`portwise mcp`](#portwise-mcp) | Run the MCP (Model Context Protocol) server on stdio for AI coding assistants |
| [`portwise completions`](#portwise-completions) | Generate shell completions |
| [`portwise man`](#portwise-man) | Print the man page (roff), or write one page per command with --out-dir |

## portwise

```text
portwise shows every listening port with its owning process, project and framework, explains in
plain English why a port is busy (dev-server tree, Docker container, systemd/pm2/brew service, OS
feature, TIME_WAIT, another user) and stops the correct thing gracefully, verifying the port is free
afterwards.

Run without arguments in a terminal to open the interactive TUI.

Usage: portwise [OPTIONS] [COMMAND]

Commands:
  list         List ports in use (listening sockets by default) [alias: ls]
  inspect      Show everything about a port: owner, process tree, project, plan
  explain      Explain in plain English why a port is busy and what to do [alias: why]
  stop         Gracefully stop whatever holds a port (SIGTERM → SIGKILL), then verify it is free
  kill         Like `stop --force`: kill immediately (SIGKILL / TerminateProcess)
  free-port    Print a free TCP port
  wait         Wait until a port is accepting connections (or free, with --free)
  run          Free a port (safely) and run a command on it, with PORT set
  graph        Show which services talk to which (dependencies, clusters) as a tree, JSON, DOT or
               Mermaid [alias: mesh]
  agents       Show the AI coding agents running here: their folders, access, ports and connections
  watch        Stream port events: new listeners, closed listeners, conflicts
  pin          Pin a port (favourite): shown first and watched even when free
  unpin        Remove a pin
  pins         List pinned ports and whether they are in use
  history      Ports portwise stopped recently, with the command that ran there
  restart      Stop what holds a port and start the same command again (or re-run it from history)
  open         Open http://localhost:PORT in the browser
  up           Start a project's services from its .portwise.toml, dependencies first
  down         Stop a project's services (dependents first), through the usual safety checks
  status       Show a project's services: running, stopped or held by something else, with HTTP
               status
  init         Print a shell hook that explains "port already in use" errors, or write a
               .portwise.toml
  ssh          Inspect another machine's ports over SSH (read-only, nothing to install remotely)
  tui          Open the interactive terminal UI
  mcp          Run the MCP (Model Context Protocol) server on stdio for AI coding assistants
  completions  Generate shell completions
  man          Print the man page (roff), or write one page per command with --out-dir
  help         Print this message or the help of the given subcommand(s)

Options:
      --color <COLOR>
          When to use colours

          [env: PORTWISE_COLOR=]
          [default: auto]
          [possible values: auto, always, never]

      --no-docker
          Don't query Docker/Podman/OrbStack/Colima

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

EXAMPLES:
  portwise                      Open the interactive TUI
  portwise list --dev           Only dev servers
  portwise explain 3000         Why is 3000 busy?
  portwise stop 3000            Gracefully stop whatever holds 3000
  portwise stop 3000 --dry-run  Show the plan only
  portwise run -p 3000 -- npm run dev
  portwise free-port --near 3000
  portwise wait 5432 --timeout 30s
  portwise graph                Which services depend on which
  portwise agents               AI coding agents: folders, access, ports
  portwise stop --cluster shop  Stop a whole stack, dependents first
  portwise up                   Start the services in .portwise.toml
  eval "$(portwise init zsh)"   Explain port-in-use errors in your shell
  portwise watch                Stream new/closed/conflicting listeners

EXIT CODES: 0 ok · 1 busy/not found/timeout · 2 error · 3 blocked by safety policy · 4 needs
elevation
```

## portwise list

```text
List ports in use (listening sockets by default)

Usage: portwise list [OPTIONS] [QUERY]...

Arguments:
  [QUERY]...  Filter: words, `:3000`, `3000-3999`, `proto:udp`, `pid:123`, `user:me`

Options:
  -a, --all            Include non-listening sockets (ESTABLISHED, TIME_WAIT, …)
      --tcp            TCP only
      --udp            UDP only
  -d, --dev            Only likely development servers (frameworks, projects, databases, containers)
  -m, --mine           Only processes owned by you
  -x, --exposed        Only ports reachable from the network (bound to 0.0.0.0 / ::)
  -r, --range <RANGE>  Port range, e.g. 3000-3999
  -s, --sort <SORT>    Sort order [default: port] [possible values: port, pid, name, proto, memory,
                       uptime]
  -w, --wide           Show the full command and user columns
      --http           Ask each TCP listener for its HTTP status and page title (a short `GET /`)
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise inspect

```text
Show everything about a port: owner, process tree, project, plan

Usage: portwise inspect [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
      --udp            Only consider UDP
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise explain

```text
Explain in plain English why a port is busy and what to do

Usage: portwise explain [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
      --udp            Only consider UDP
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise stop

```text
Gracefully stop whatever holds a port (SIGTERM → SIGKILL), then verify it is free

Usage: portwise stop [OPTIONS] [TARGETS]...

Arguments:
  [TARGETS]...  Ports, `pid:<n>` or process names. Bare numbers are ports

Options:
      --all-dev            Stop every dev server you own (what `portwise list --dev --mine` shows)
      --pid <PID>          Stop a process by PID (repeatable)
      --name <NAME>        Stop processes by exact name (repeatable)
      --cluster <CLUSTER>  Stop every service in a cluster (see `portwise graph`), dependents first
  -f, --force              Skip SIGTERM: SIGKILL / TerminateProcess immediately
  -y, --yes                Don't ask for confirmation
  -n, --dry-run            Show the plan but don't do anything
  -t, --timeout <TIMEOUT>  Grace period before escalating to SIGKILL [default: 5s]
      --allow-protected    Also allow stopping protected processes (system services, editors,
                           terminals)
      --no-tree            Only stop the socket holder, not its dev-server tree
      --udp                Only consider UDP sockets for port targets
      --json               Machine-readable JSON output (implies no prompt; requires --yes unless
                           --dry-run)
      --color <COLOR>      When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## portwise kill

```text
Like `stop --force`: kill immediately (SIGKILL / TerminateProcess)

Usage: portwise kill [OPTIONS] [TARGETS]...

Arguments:
  [TARGETS]...  Ports, `pid:<n>` or process names. Bare numbers are ports

Options:
      --all-dev            Stop every dev server you own (what `portwise list --dev --mine` shows)
      --pid <PID>          Stop a process by PID (repeatable)
      --name <NAME>        Stop processes by exact name (repeatable)
      --cluster <CLUSTER>  Stop every service in a cluster (see `portwise graph`), dependents first
  -f, --force              Skip SIGTERM: SIGKILL / TerminateProcess immediately
  -y, --yes                Don't ask for confirmation
  -n, --dry-run            Show the plan but don't do anything
  -t, --timeout <TIMEOUT>  Grace period before escalating to SIGKILL [default: 5s]
      --allow-protected    Also allow stopping protected processes (system services, editors,
                           terminals)
      --no-tree            Only stop the socket holder, not its dev-server tree
      --udp                Only consider UDP sockets for port targets
      --json               Machine-readable JSON output (implies no prompt; requires --yes unless
                           --dry-run)
      --color <COLOR>      When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## portwise free-port

```text
Print a free TCP port

Usage: portwise free-port [OPTIONS]

Options:
      --near <NEAR>    Return the first free port at or after this one
      --range <RANGE>  Search within a range, e.g. 3000-3999
  -c, --count <COUNT>  How many ports to return [default: 1]
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise wait

```text
Wait until a port is accepting connections (or free, with --free)

Usage: portwise wait [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
  -t, --timeout <TIMEOUT>    Give up after this long (exit code 1) [default: 30s]
      --free                 Wait until the port is free instead
      --interval <INTERVAL>  Polling interval [default: 200ms]
  -q, --quiet                Print nothing; just use the exit code
      --json                 Machine-readable JSON output
      --color <COLOR>        When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible
                             values: auto, always, never]
      --no-docker            Don't query Docker/Podman/OrbStack/Colima
  -h, --help                 Print help
```

## portwise run

```text
Free a port (safely) and run a command on it, with PORT set

Usage: portwise run [OPTIONS] --port <PORT> -- <COMMAND>...

Arguments:
  <COMMAND>...  The command to run

Options:
  -p, --port <PORT>        Port the command needs
  -y, --yes                Stop the current owner without asking (still subject to the safety
                           policy)
  -f, --force              Kill the owner immediately instead of gracefully
      --fallback           If the port can't be freed safely, use the next free port instead
  -t, --timeout <TIMEOUT>  Grace period before escalating to SIGKILL [default: 5s]
      --env <ENV>          Name of the environment variable to set (default PORT) [default: PORT]
      --color <COLOR>      When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## portwise graph

```text
Show which services talk to which (dependencies, clusters) as a tree, JSON, DOT or Mermaid

Usage: portwise graph [OPTIONS]

Options:
  -a, --all                Include everything (system services, apps), not just dev services and
                           their peers
  -c, --cluster <CLUSTER>  Only show one cluster (id or name)
      --no-external        Hide the collapsed "external hosts" node
  -f, --format <FORMAT>    Output format [default: tree] [possible values: tree, json, dot, mermaid]
      --json               Shorthand for --format json
      --dot                Shorthand for --format dot (Graphviz)
      --mermaid            Shorthand for --format mermaid
      --ascii              ASCII-only tree (no box-drawing characters)
      --color <COLOR>      When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## portwise agents

```text
Show the AI coding agents running here: their folders, access, ports and connections

Usage: portwise agents [OPTIONS] [AGENT]

Arguments:
  [AGENT]  Only agents matching this (product such as `claude` or `cursor`, name, or PID)

Options:
  -w, --wide           List every process of each agent, with its command line (secrets hidden)
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise watch

```text
Stream port events: new listeners, closed listeners, conflicts

Usage: portwise watch [OPTIONS]

Options:
  -a, --all                  Report every listener, not only dev servers and pinned ports
  -i, --interval <INTERVAL>  Polling interval [default: 1s]
      --json                 One JSON object per event (NDJSON)
      --color <COLOR>        When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible
                             values: auto, always, never]
      --no-docker            Don't query Docker/Podman/OrbStack/Colima
  -h, --help                 Print help
```

## portwise pin

```text
Pin a port (favourite): shown first and watched even when free

Usage: portwise pin [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
  -l, --label <LABEL>  A note shown next to the port
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise unpin

```text
Remove a pin

Usage: portwise unpin [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise pins

```text
List pinned ports and whether they are in use

Usage: portwise pins [OPTIONS]

Options:
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise history

```text
Ports portwise stopped recently, with the command that ran there

Usage: portwise history [OPTIONS]

Options:
  -n, --limit <LIMIT>  How many entries to show [default: 20]
      --clear          Forget the history
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise restart

```text
Stop what holds a port and start the same command again (or re-run it from history)

Usage: portwise restart [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
  -y, --yes                Don't ask before stopping the current owner
  -t, --timeout <TIMEOUT>  How long to wait for the port to accept connections again [default: 30s]
      --color <COLOR>      When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## portwise open

```text
Open http://localhost:PORT in the browser

Usage: portwise open [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
      --print          Print the URL instead of opening it
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise up

```text
Start a project's services from its .portwise.toml, dependencies first

Usage: portwise up [OPTIONS] [SERVICES]...

Arguments:
  [SERVICES]...  Services to start (default: all). Their dependencies are started too

Options:
      --file <PATH>    Use this project file instead of the nearest `.portwise.toml`
      --replace        If a port is held by something outside the project, stop it (after
                       confirmation)
  -y, --yes            Don't ask before stopping a conflicting owner (with --replace)
  -n, --dry-run        Show what would be started without starting anything
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise down

```text
Stop a project's services (dependents first), through the usual safety checks

Usage: portwise down [OPTIONS] [SERVICES]...

Arguments:
  [SERVICES]...  Services to stop (default: all). Services that depend on them are stopped first

Options:
      --file <PATH>        Use this project file instead of the nearest `.portwise.toml`
  -a, --all                Also stop services without a `command` (for example a database container
                           of this project)
  -y, --yes                Don't ask for confirmation
  -n, --dry-run            Show the plan but don't do anything
  -f, --force              Skip SIGTERM: SIGKILL / TerminateProcess immediately
  -t, --timeout <TIMEOUT>  Grace period before escalating to SIGKILL [default: 5s]
      --color <COLOR>      When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## portwise status

```text
Show a project's services: running, stopped or held by something else, with HTTP status

Usage: portwise status [OPTIONS]

Options:
      --file <PATH>    Use this project file instead of the nearest `.portwise.toml`
      --no-http        Don't send HTTP requests to the services
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise init

```text
Print a shell hook that explains "port already in use" errors, or write a .portwise.toml

Usage: portwise init [OPTIONS] [SHELL]

Arguments:
  [SHELL]  Print the integration script for this shell. Without a shell, write a starter
           .portwise.toml from the dev servers running under the current directory [possible values:
           zsh, bash, fish, powershell]

Options:
      --force          Overwrite an existing .portwise.toml
      --print          Print the project file instead of writing it
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise ssh

```text
Inspect another machine's ports over SSH (read-only, nothing to install remotely)

Usage: portwise ssh [OPTIONS] <HOST> [COMMAND]...

Arguments:
  <HOST>        `[user@]host` (anything `ssh` accepts, including ~/.ssh/config aliases)
  [COMMAND]...  What to show: `list` (default), `graph`, or any portwise command line after `--` to
                run a remotely installed portwise

Options:
      --json           JSON output
      --agentless      Never use a remote portwise; always read ss/ps
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise tui

```text
Open the interactive terminal UI

Usage: portwise tui [OPTIONS]

Options:
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise mcp

```text
Run the MCP (Model Context Protocol) server on stdio for AI coding assistants

Usage: portwise mcp [OPTIONS]

Options:
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise completions

```text
Generate shell completions

Usage: portwise completions [OPTIONS] <SHELL>

Arguments:
  <SHELL>  Shell to generate completions for [possible values: bash, elvish, fish, powershell, zsh]

Options:
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## portwise man

```text
Print the man page (roff), or write one page per command with --out-dir

Usage: portwise man [OPTIONS]

Options:
      --out-dir <DIR>  Write `portwise.1` and `portwise-<command>.1` into this directory instead of
                       printing
      --color <COLOR>  When to use colours [env: PORTWISE_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

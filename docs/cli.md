# holdmap CLI reference

<!-- Generated from the clap definitions in crates/holdmap-cli. Do not edit by hand:
     HOLDMAP_BLESS=1 cargo test -p holdmap cli_reference -->

This page is the `--help` output of every command of holdmap 0.3.0. `cargo test` checks it
against the code, so it never drifts. For an overview see the [README](../README.md).

Global options (accepted by every command): `--color <auto|always|never>` (env `HOLDMAP_COLOR`; `NO_COLOR` and `CLICOLOR_FORCE` are honoured too) and `--no-docker`.

| Command | What it does |
|---|---|
| [`holdmap list`](#holdmap-list) (alias `ls`) | List ports in use (listening sockets by default) |
| [`holdmap inspect`](#holdmap-inspect) | Show everything about a port: owner, process tree, project, plan |
| [`holdmap explain`](#holdmap-explain) (alias `why`) | Explain in plain English why a port is busy and what to do |
| [`holdmap stop`](#holdmap-stop) | Gracefully stop whatever holds a port (SIGTERM → SIGKILL), then verify it is free |
| [`holdmap kill`](#holdmap-kill) | Like `stop --force`: kill immediately (SIGKILL / TerminateProcess) |
| [`holdmap free-port`](#holdmap-free-port) | Print a free TCP port |
| [`holdmap wait`](#holdmap-wait) | Wait until a port is accepting connections (or free, with --free) |
| [`holdmap run`](#holdmap-run) | Free a port (safely) and run a command on it, with PORT set |
| [`holdmap graph`](#holdmap-graph) (alias `mesh`) | Show which services talk to which (dependencies, clusters) as a tree, JSON, DOT or Mermaid |
| [`holdmap agents`](#holdmap-agents) | Show AI coding agents and developer tools: folders, access, ports and connections |
| [`holdmap watch`](#holdmap-watch) | Stream port events: new listeners, closed listeners, conflicts |
| [`holdmap pin`](#holdmap-pin) | Pin a port (favourite): shown first and watched even when free |
| [`holdmap unpin`](#holdmap-unpin) | Remove a pin |
| [`holdmap pins`](#holdmap-pins) | List pinned ports and whether they are in use |
| [`holdmap history`](#holdmap-history) | Ports holdmap stopped recently, with the command that ran there |
| [`holdmap restart`](#holdmap-restart) | Stop what holds a port and start the same command again (or re-run it from history) |
| [`holdmap open`](#holdmap-open) | Open http://localhost:PORT in the browser |
| [`holdmap up`](#holdmap-up) | Start a project's services from its .holdmap.toml, dependencies first |
| [`holdmap down`](#holdmap-down) | Stop a project's services (dependents first), through the usual safety checks |
| [`holdmap status`](#holdmap-status) | Show a project's services: running, stopped or held by something else, with HTTP status |
| [`holdmap init`](#holdmap-init) | Print a shell hook that explains "port already in use" errors, or write a .holdmap.toml |
| [`holdmap ssh`](#holdmap-ssh) | Inspect another machine's ports over SSH (read-only, nothing to install remotely) |
| [`holdmap tui`](#holdmap-tui) | Open the interactive terminal UI |
| [`holdmap mcp`](#holdmap-mcp) | Run the MCP (Model Context Protocol) server on stdio for AI coding assistants |
| [`holdmap completions`](#holdmap-completions) | Generate shell completions |
| [`holdmap man`](#holdmap-man) | Print the man page (roff), or write one page per command with --out-dir |

## holdmap

```text
holdmap shows every listening port with its owning process, project and framework, maps AI coding
agents and developer tools (Claude Code, Cursor, Docker Desktop…) to the folders and ports they
hold, explains in plain English why a port is busy (dev-server tree, Docker container,
systemd/pm2/brew service, OS feature, TIME_WAIT, another user) and stops the correct thing
gracefully, verifying the port is free afterwards.

Run without arguments in a terminal to open the interactive TUI (Tab cycles Ports → Graph → Agents).

Usage: holdmap [OPTIONS] [COMMAND]

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
  agents       Show AI coding agents and developer tools: folders, access, ports and connections
  watch        Stream port events: new listeners, closed listeners, conflicts
  pin          Pin a port (favourite): shown first and watched even when free
  unpin        Remove a pin
  pins         List pinned ports and whether they are in use
  history      Ports holdmap stopped recently, with the command that ran there
  restart      Stop what holds a port and start the same command again (or re-run it from history)
  open         Open http://localhost:PORT in the browser
  up           Start a project's services from its .holdmap.toml, dependencies first
  down         Stop a project's services (dependents first), through the usual safety checks
  status       Show a project's services: running, stopped or held by something else, with HTTP
               status
  init         Print a shell hook that explains "port already in use" errors, or write a
               .holdmap.toml
  ssh          Inspect another machine's ports over SSH (read-only, nothing to install remotely)
  tui          Open the interactive terminal UI
  mcp          Run the MCP (Model Context Protocol) server on stdio for AI coding assistants
  completions  Generate shell completions
  man          Print the man page (roff), or write one page per command with --out-dir
  help         Print this message or the help of the given subcommand(s)

Options:
      --color <COLOR>
          When to use colours

          [env: HOLDMAP_COLOR=]
          [default: auto]
          [possible values: auto, always, never]

      --no-docker
          Don't query Docker/Podman/OrbStack/Colima

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

EXAMPLES:
  holdmap                      Open the interactive TUI
  holdmap list --dev           Only dev servers
  holdmap explain 3000         Why is 3000 busy?
  holdmap stop 3000            Gracefully stop whatever holds 3000
  holdmap stop 3000 --dry-run  Show the plan only
  holdmap run -p 3000 -- npm run dev
  holdmap free-port --near 3000
  holdmap wait 5432 --timeout 30s
  holdmap graph                Which services depend on which
  holdmap agents               Agents & tools: folders, access, ports
  holdmap agents --stop-ports  Stop the ports they started
  holdmap stop --cluster shop  Stop a whole stack, dependents first
  holdmap up                   Start the services in .holdmap.toml
  eval "$(holdmap init zsh)"   Explain port-in-use errors in your shell
  holdmap watch                Stream new/closed/conflicting listeners

EXIT CODES: 0 ok · 1 busy/not found/timeout · 2 error · 3 blocked by safety policy · 4 needs
elevation
```

## holdmap list

```text
List ports in use (listening sockets by default)

Usage: holdmap list [OPTIONS] [QUERY]...

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
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap inspect

```text
Show everything about a port: owner, process tree, project, plan

Usage: holdmap inspect [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
      --udp            Only consider UDP
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap explain

```text
Explain in plain English why a port is busy and what to do

Usage: holdmap explain [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
      --udp            Only consider UDP
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap stop

```text
Gracefully stop whatever holds a port (SIGTERM → SIGKILL), then verify it is free

Usage: holdmap stop [OPTIONS] [TARGETS]...

Arguments:
  [TARGETS]...  Ports, `pid:<n>` or process names. Bare numbers are ports

Options:
      --all-dev            Stop every dev server you own (what `holdmap list --dev --mine` shows)
      --pid <PID>          Stop a process by PID (repeatable)
      --name <NAME>        Stop processes by exact name (repeatable)
      --cluster <CLUSTER>  Stop every service in a cluster (see `holdmap graph`), dependents first
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
      --color <COLOR>      When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## holdmap kill

```text
Like `stop --force`: kill immediately (SIGKILL / TerminateProcess)

Usage: holdmap kill [OPTIONS] [TARGETS]...

Arguments:
  [TARGETS]...  Ports, `pid:<n>` or process names. Bare numbers are ports

Options:
      --all-dev            Stop every dev server you own (what `holdmap list --dev --mine` shows)
      --pid <PID>          Stop a process by PID (repeatable)
      --name <NAME>        Stop processes by exact name (repeatable)
      --cluster <CLUSTER>  Stop every service in a cluster (see `holdmap graph`), dependents first
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
      --color <COLOR>      When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## holdmap free-port

```text
Print a free TCP port

Usage: holdmap free-port [OPTIONS]

Options:
      --near <NEAR>    Return the first free port at or after this one
      --range <RANGE>  Search within a range, e.g. 3000-3999
  -c, --count <COUNT>  How many ports to return [default: 1]
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap wait

```text
Wait until a port is accepting connections (or free, with --free)

Usage: holdmap wait [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
  -t, --timeout <TIMEOUT>    Give up after this long (exit code 1) [default: 30s]
      --free                 Wait until the port is free instead
      --interval <INTERVAL>  Polling interval [default: 200ms]
  -q, --quiet                Print nothing; just use the exit code
      --json                 Machine-readable JSON output
      --color <COLOR>        When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible
                             values: auto, always, never]
      --no-docker            Don't query Docker/Podman/OrbStack/Colima
  -h, --help                 Print help
```

## holdmap run

```text
Free a port (safely) and run a command on it, with PORT set

Usage: holdmap run [OPTIONS] --port <PORT> -- <COMMAND>...

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
      --color <COLOR>      When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## holdmap graph

```text
Show which services talk to which (dependencies, clusters) as a tree, JSON, DOT or Mermaid

Usage: holdmap graph [OPTIONS]

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
      --color <COLOR>      When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## holdmap agents

```text
Show AI coding agents and developer tools: folders, access, ports and connections

Usage: holdmap agents [OPTIONS] [AGENT]

Arguments:
  [AGENT]  Only agents matching this (product such as `claude` or `cursor`, name, or PID)

Options:
  -w, --wide           List every process of each agent, with its command line (secrets hidden)
      --stop-ports     Stop the unprotected ports each matching agent started (dev servers and
                       services)
      --dry-run        Show the stop plan only; don't send any signal
      --yes            Don't ask for confirmation before stopping
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap watch

```text
Stream port events: new listeners, closed listeners, conflicts

Usage: holdmap watch [OPTIONS]

Options:
  -a, --all                  Report every listener, not only dev servers and pinned ports
  -i, --interval <INTERVAL>  Polling interval [default: 1s]
      --json                 One JSON object per event (NDJSON)
      --color <COLOR>        When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible
                             values: auto, always, never]
      --no-docker            Don't query Docker/Podman/OrbStack/Colima
  -h, --help                 Print help
```

## holdmap pin

```text
Pin a port (favourite): shown first and watched even when free

Usage: holdmap pin [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
  -l, --label <LABEL>  A note shown next to the port
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap unpin

```text
Remove a pin

Usage: holdmap unpin [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap pins

```text
List pinned ports and whether they are in use

Usage: holdmap pins [OPTIONS]

Options:
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap history

```text
Ports holdmap stopped recently, with the command that ran there

Usage: holdmap history [OPTIONS]

Options:
  -n, --limit <LIMIT>  How many entries to show [default: 20]
      --clear          Forget the history
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap restart

```text
Stop what holds a port and start the same command again (or re-run it from history)

Usage: holdmap restart [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
  -y, --yes                Don't ask before stopping the current owner
  -t, --timeout <TIMEOUT>  How long to wait for the port to accept connections again [default: 30s]
      --color <COLOR>      When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## holdmap open

```text
Open http://localhost:PORT in the browser

Usage: holdmap open [OPTIONS] <PORT>

Arguments:
  <PORT>  Port number (e.g. 3000 or :3000)

Options:
      --print          Print the URL instead of opening it
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap up

```text
Start a project's services from its .holdmap.toml, dependencies first

Usage: holdmap up [OPTIONS] [SERVICES]...

Arguments:
  [SERVICES]...  Services to start (default: all). Their dependencies are started too

Options:
      --file <PATH>    Use this project file instead of the nearest `.holdmap.toml`
      --replace        If a port is held by something outside the project, stop it (after
                       confirmation)
  -y, --yes            Don't ask before stopping a conflicting owner (with --replace)
  -n, --dry-run        Show what would be started without starting anything
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap down

```text
Stop a project's services (dependents first), through the usual safety checks

Usage: holdmap down [OPTIONS] [SERVICES]...

Arguments:
  [SERVICES]...  Services to stop (default: all). Services that depend on them are stopped first

Options:
      --file <PATH>        Use this project file instead of the nearest `.holdmap.toml`
  -a, --all                Also stop services without a `command` (for example a database container
                           of this project)
  -y, --yes                Don't ask for confirmation
  -n, --dry-run            Show the plan but don't do anything
  -f, --force              Skip SIGTERM: SIGKILL / TerminateProcess immediately
  -t, --timeout <TIMEOUT>  Grace period before escalating to SIGKILL [default: 5s]
      --color <COLOR>      When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible
                           values: auto, always, never]
      --no-docker          Don't query Docker/Podman/OrbStack/Colima
  -h, --help               Print help
```

## holdmap status

```text
Show a project's services: running, stopped or held by something else, with HTTP status

Usage: holdmap status [OPTIONS]

Options:
      --file <PATH>    Use this project file instead of the nearest `.holdmap.toml`
      --no-http        Don't send HTTP requests to the services
      --json           Machine-readable JSON output
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap init

```text
Print a shell hook that explains "port already in use" errors, or write a .holdmap.toml

Usage: holdmap init [OPTIONS] [SHELL]

Arguments:
  [SHELL]  Print the integration script for this shell. Without a shell, write a starter
           .holdmap.toml from the dev servers running under the current directory [possible values:
           zsh, bash, fish, powershell]

Options:
      --force          Overwrite an existing .holdmap.toml
      --print          Print the project file instead of writing it
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap ssh

```text
Inspect another machine's ports over SSH (read-only, nothing to install remotely)

Usage: holdmap ssh [OPTIONS] <HOST> [COMMAND]...

Arguments:
  <HOST>        `[user@]host` (anything `ssh` accepts, including ~/.ssh/config aliases)
  [COMMAND]...  What to show: `list` (default), `graph`, or any holdmap command line after `--` to
                run a remotely installed holdmap

Options:
      --json           JSON output
      --agentless      Never use a remote holdmap; always read ss/ps
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap tui

```text
Open the interactive terminal UI

Usage: holdmap tui [OPTIONS]

Options:
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap mcp

```text
Run the MCP (Model Context Protocol) server on stdio for AI coding assistants

Usage: holdmap mcp [OPTIONS]

Options:
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap completions

```text
Generate shell completions

Usage: holdmap completions [OPTIONS] <SHELL>

Arguments:
  <SHELL>  Shell to generate completions for [possible values: bash, elvish, fish, powershell, zsh]

Options:
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

## holdmap man

```text
Print the man page (roff), or write one page per command with --out-dir

Usage: holdmap man [OPTIONS]

Options:
      --out-dir <DIR>  Write `holdmap.1` and `holdmap-<command>.1` into this directory instead of
                       printing
      --color <COLOR>  When to use colours [env: HOLDMAP_COLOR=] [default: auto] [possible values:
                       auto, always, never]
      --no-docker      Don't query Docker/Podman/OrbStack/Colima
  -h, --help           Print help
```

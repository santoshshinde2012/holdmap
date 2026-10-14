<p align="center">
  <img src="apps/desktop/assets/icon.png" width="88" alt="holdmap routed H logo" />
</p>

<h1 align="center">holdmap</h1>

<p align="center">
  <b>See which ports, agents and tools are running — and stop the right thing safely.</b><br/>
  CLI · TUI · desktop and tray app · MCP server, all on one Rust core.
</p>

<p align="center">
  <a href="https://santoshshinde2012.github.io/holdmap/"><b>Website</b></a> ·
  <a href="https://santoshshinde2012.github.io/holdmap/#demo">Live demo</a> ·
  <a href="https://santoshshinde2012.github.io/holdmap/docs/">Guide</a>
</p>

<p align="center">
  <a href="https://github.com/santoshshinde2012/holdmap/actions/workflows/ci.yml"><img src="https://github.com/santoshshinde2012/holdmap/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI" /></a>
  <a href="https://github.com/santoshshinde2012/holdmap/releases/latest"><img src="docs/badges/version.svg" alt="Holdmap version; open the latest release" /></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue" alt="License: MIT OR Apache-2.0" /></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey" alt="Platforms: macOS, Linux, Windows" />
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/desktop-overview-dark.png" />
    <img src="docs/screenshots/desktop-overview-light.png" width="860" alt="The holdmap desktop app: ports grouped by kind, port 3000 selected with project, branch, memory, connections and what Stop will signal" />
  </picture>
</p>

`EADDRINUSE: address already in use :::3000`? Most tools hand you a PID and a `kill -9`.
holdmap tells you what's really there — which agent or tool started it — and stops only what
it should.

Version **0.7.0** adds local screen recording, Computer / Footprint / Project agent-map
zoom, and live local scans during desktop development. Read
[what's new](https://santoshshinde2012.github.io/holdmap/docs/whats-new/), check the
[latest published release](https://github.com/santoshshinde2012/holdmap/releases/latest) for
current installers and see the [changelog](CHANGELOG.md) for release notes. Desktop
screenshots use the shared browser UI with sample data.

## Features

- **Who owns the port.** Process, user, command, folder, plus the project, git branch and
  framework (Next.js, Vite, Django, Postgres…) or the Docker / Podman / OrbStack container.
- **Why it's busy, in plain English.** A dev-server tree, a container, a systemd / pm2 / Homebrew
  service, an OS feature like AirPlay Receiver, `TIME_WAIT`, or another user.
- **A safe stop.** Preview the plan, stop the whole dev-server tree gracefully (SIGTERM, then
  SIGKILL), and check the port is free afterwards. System processes and IDEs are protected.
- **What's behind it.** Who is connected right now, the full process tree, uptime, CPU and
  memory (the whole app, helpers included), the HTTP status and title, and what the bind address
  means for your safety.
- **The service graph.** Which local services talk to which, grouped into clusters (Compose,
  Kubernetes, workspaces). Stop a whole stack in dependency order.
- **Your agents, tools and apps.** Claude Code, Codex, Cursor, Copilot, Gemini CLI, Windsurf,
  Aider, Docker Desktop, OrbStack and more: memory, CPU, stoppable ports, parent process, the
  folders each one works in, the ports and apps it started (children grouped as tools & apps),
  the services and hosts it talks to, and access (account, sandbox, approval flags, network
  exposure including debug and model-server ports), each fact marked seen, inferred or unknown.
  Agent settings files are not read. Reveal a folder, open it in your editor,
  or stop only the services that agent started, through the normal safety policy.
- **Three levels of agent maps.** Computer shows this machine and outside systems;
  Footprint shows agents' folders, tools, ports and hosts; Project focuses on one source tree.
- **Local screen recording.** Choose a screen or window, capture up to two minutes without
  audio, then preview and save the available MP4/GIF exports. Nothing is uploaded.
- **Everywhere you work.** A scriptable CLI (`--json`), a TUI, a desktop and tray app, and an MCP
  server for AI coding assistants. macOS, Linux and Windows. No product analytics.
- **Agent flow.** Follow observed process/socket relationships, or opt in to
  local recording of reported tools, model calls and delegated agents. Inspect parallel work,
  failures, duration and reported serialized sizes without storing prompts or tool arguments.

## Install

**Linux desktop 0.3.0 downloads predate the GLib security fixes.** Use version **0.4.0 or newer**,
or build the desktop from current source. CLI, TUI, MCP and macOS/Windows
desktop are unaffected by this Linux dependency issue. See the
[security policy](SECURITY.md#linux-glib-dependency-backports).

**macOS and Linux**

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/santoshshinde2012/holdmap/releases/latest/download/holdmap-installer.sh | sh
```

**Windows (PowerShell)**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/santoshshinde2012/holdmap/releases/latest/download/holdmap-installer.ps1 | iex"
```

The binary goes to `~/.local/bin` (`%USERPROFILE%\.local\bin` on Windows). If that folder is
already on your `PATH`, `holdmap` works right away. If not, the installer adds it and tells
you: open a new terminal, or run the PATH command it prints.

**Desktop app**

Choose your platform and installer format in the [website's download section](https://santoshshinde2012.github.io/holdmap/#install).
It follows the latest published release, including while a newer source version is being built.

| Platform | Download |
|---|---|
| macOS, Apple silicon | [holdmap_aarch64.dmg][dmg-arm64] |
| macOS, Intel | [holdmap_x64.dmg][dmg-x64] |
| Windows, x64 | [.msi][msi] or [setup .exe][nsis] |
| Linux, x64 | [.AppImage][appimage], [.deb][deb] or [.rpm][rpm] |

Linux ARM64 has a CLI archive, but no prebuilt desktop installer.

On a Mac with Apple silicon (M1 or newer), choose the **Apple silicon** download.
It runs natively and does not require Rosetta. Choose **Intel** for an Intel Mac;
Apple menu → **About This Mac** shows either a chip name or an Intel processor.
See [Apple's guidance on native apps and Rosetta](https://support.apple.com/en-gb/102527).

<!-- release-please updates this source-version marker; download links follow published releases. -->
<!-- x-release-please-start-version -->
Source version: **0.7.0**.
<!-- x-release-please-end -->

[dmg-arm64]: https://santoshshinde2012.github.io/holdmap/#install
[dmg-x64]: https://santoshshinde2012.github.io/holdmap/#install
[msi]: https://santoshshinde2012.github.io/holdmap/#install
[nsis]: https://santoshshinde2012.github.io/holdmap/#install
[appimage]: https://santoshshinde2012.github.io/holdmap/#install
[deb]: https://santoshshinde2012.github.io/holdmap/#install
[rpm]: https://santoshshinde2012.github.io/holdmap/#install

**From source** (Rust 1.95+): use the versioned Cargo command in
[Install](https://santoshshinde2012.github.io/holdmap/#install) to build a published release.
To install your checked-out source, run `cargo install --locked --path crates/holdmap-cli`
from the repository root. Homebrew and npm-registry installation are not available;
the release's npm tarball is a downloadable asset.

Every release has SHA-256 checksums and build provenance (`gh attestation verify FILE --repo santoshshinde2012/holdmap`).

## Environment

| Variable | Purpose |
|---|---|
| `HOLDMAP_HOME` | Config/history directory (overrides the platform default) |
| `HOLDMAP_COLOR` | `auto` / `always` / `never` (also via `--color`) |
| `HOLDMAP_TRACE` | Diagnostics topics, e.g. `scan` or `all` |
| `HOLDMAP_ACTIVITY_ENDPOINT` | Active local collector URL for CLI hooks, snapshots and MCP recording |
| `HOLDMAP_ACTIVITY_TOKEN` | Temporary bearer token printed by `activity serve`; keep private |
| `HOLDMAP_EDITOR` | Editor executable for folder actions; when unset, a known installed editor |

## Quick start

```sh
holdmap list                 # every listening port, grouped by kind
holdmap agents               # compact overview of running agents and tools
holdmap agents codex         # detailed tools, folders, ports and access
holdmap agents claude --stop-ports --dry-run   # plan stopping what that agent started
holdmap inspect 3000         # owner, process tree, connections, bind risk and the stop plan
holdmap list --sort memory   # heaviest apps first
holdmap stop 3000 --dry-run  # show exactly what stop would do
holdmap stop 3000            # stop it gracefully, then check the port is free
holdmap run -p 3000 -- npm run dev   # free 3000 safely, then start your server on it
holdmap                      # interactive TUI (Shift+Tab opens Agents)
```

<img src="docs/screenshots/cli-explain-dark.png" width="720" alt="holdmap explain: fixture listener ownership and the recommended plan" />

**Project stacks.** Describe a project's services once in `.holdmap.toml` (`holdmap init` writes
one from what's running) and start them in dependency order with `holdmap up`:

```toml
name = "shop"

[services.api]
port = 4000
command = "npm run dev"   # runs with PORT set
depends_on = ["db"]

[services.db]
port = 5432               # no command: started elsewhere, up just waits for it
```

`holdmap status` shows the stack and `holdmap down` stops it, dependents first.
Stack files support up to 512 services within 1 MiB. Set `ready_timeout_s` to 1–3,600 seconds
per service (default 60); `holdmap status` runs at most 32 HTTP checks concurrently.

## CLI

| Purpose | Commands |
|---|---|
| Look | `holdmap list`, `holdmap inspect`, `holdmap explain`, `holdmap graph`, `holdmap agents`, `holdmap watch`, `holdmap ssh HOST` (read-only, nothing to install remotely) |
| Act | `holdmap stop`, `holdmap kill`, `holdmap restart`, `holdmap run`, `holdmap open`, `holdmap agents AGENT --stop-ports` |
| Ports | `holdmap free-port --near 3000`, `holdmap wait 5432 --timeout 30s` |
| Projects | `holdmap up`, `holdmap down`, `holdmap status`, `holdmap init` |
| Remember | `holdmap pin`, `holdmap unpin`, `holdmap pins`, `holdmap history` |
| Integrate | `holdmap activity serve`, `holdmap activity setup`, `holdmap activity snapshot`, `holdmap activity hook`, `holdmap tui`, `holdmap mcp`, `holdmap completions zsh`, `holdmap man` |

Every read command takes `--json`. Exit codes: `0` ok, `1` busy / not found / timed out, `2`
error, `3` blocked by the safety policy, `4` needs elevation. All flags are in the
**[CLI reference](docs/cli.md)**.

Current source keeps every matching process in a name-based stop with `--no-tree` and
verifies TCP/UDP listeners held by every included descendant in a PID/name tree stop.
Supervisor commands must match the planned service or app; systemd service stops recheck
protection for other members of the same service before running.

**Agent visibility in 0.4.0.** `holdmap agents` includes child MCP candidates even when
no port listens, resource totals for all owned processes, and counts for omitted display rows.
Search by product, vendor, exact owned PID (`pid:1234`), working folder or tool name:
`holdmap agents claude` or `holdmap agents filesystem`. `--wide` shows listed processes and
redacted commands; `--json` preserves evidence, timestamps and collection limits. Command text
is not searched. Recent folders belong to the current account's history and are attached only
to confirmed matching accounts; they do not prove current work by that running instance.

**Terminal agents in 0.5.0.** Start `holdmap` and press **Shift+Tab**
to open Agents. Use **↑/↓** to choose an agent, **/** to search, and **PgUp/PgDn** to read its
tools, folders, connections and access. Wide terminals show the list beside the details;
small terminals stack them. **p** pauses refreshes, **r** refreshes, and **?** shows help.
**x** opens every service-stop plan, including its TCP/UDP protocol; **Enter**, **n** or
**Esc** cancels and only **y** confirms. The agent session remains protected. Failed or
changed targets require a new review; completed stops and history-write errors stay visible.
`holdmap agents` now gives a compact overview; a query expands matching footprints and
`--wide` adds redacted process commands. Version 0.4.0 downloads predate these terminal changes.
The service graph now searches its visible services and keeps selection attached to the same
node across refreshes. In terminals that support bracketed paste, pasted text is accepted only
inside search; pasted action keys cannot open or confirm a stop. Mouse-wheel scrolling is enabled
while the TUI runs, and terminal input modes are restored on exit.

<details>
<summary>Current terminal agent overview, small-screen details and service review</summary>

![CLI agent details with inferred child-tool evidence](docs/screenshots/cli-agents-dark.png)
![Terminal agent list and selected details](docs/screenshots/tui-agents-dark.png)
![Terminal agents in an 80-column window](docs/screenshots/tui-agents-narrow-dark.png)
![Terminal service review keeps explicit confirmation and cancellation visible](docs/screenshots/tui-agent-stop-confirm-dark.png)

![Terminal graph search keeps the selected service and observed connection evidence visible](docs/screenshots/tui-graph-dark.png)

</details>

**Agent flow in 0.5.0.** Open **Agents → Flow**. **Connections** shows
observed processes, listeners and sockets, with evidence for each relationship. **Calls** shows
reported tool/model work and agent handoffs after you enable local recording. Only reported
trace parents create call edges; an ordered list of session events does not prove communication.
Parallel calls remain separate. Unknown results, missing parents, dropped data and exporter
limitations stay visible. Selecting a call shows status, duration and available serialized sizes.
Use **All**, **Failed** or **No completion** to narrow the timeline, and sort by time or longest
reported duration. Search matches individual records; the graph retains their reported parents
and the inspected record. A finished call with an unknown outcome stays distinct from a call
without completion data.
For OTLP spans, an explicit final status takes precedence over a conflicting custom
`success` attribute. An optional MCP observer that panics is disabled for that session,
and protocol responses continue; blocking or aborting callbacks remain unsupported.

Version 0.5.0 also has **Settings → General → Single-key shortcuts**. Turn it off to use
speech input or reserve letter keys; arrow navigation and modifier shortcuts remain available.
Port filters and selected-port commands apply to the port views, and native controls keep their
own keyboard behavior. The command palette names its search and keeps its active result valid
as live commands change.

Recording is off by default, local-only and memory-only. Start it in the local desktop, review
**Setup**, then explicitly configure a supported agent. Pause freezes the view; **Stop recording**
closes the collector, and **Clear** removes retained metadata. A new recording session creates
a fresh token and an OS-assigned loopback endpoint; reconfigure producers after every start. Full app quit loses the recording. The website demonstrates these views
with labelled sample data. [Agent flow guide](docs/agent-flow.md) explains provider coverage,
privacy, metadata contracts and the CLI alternative:

```sh
holdmap agents --flow --json        # observed relationships; no recording required
holdmap activity serve              # keep running in this terminal
```

In a second terminal, set `HOLDMAP_ACTIVITY_ENDPOINT` and `HOLDMAP_ACTIVITY_TOKEN` to the
exact values printed by that collector, then run the commands below. Keep the token private.
`setup` prints a reviewable configuration example; it does not change the agent's settings.
Apply that example to the supported agent, or enable recording for this MCP server:

```sh
holdmap activity setup --provider codex   # prints configuration for the active collector
holdmap activity snapshot --json    # retained metadata from that collector
holdmap mcp --record-activity        # optional recording of this server's known tools
```

<details>
<summary>Observed connections, reported calls and a narrow-screen inspector</summary>

![Observed agent connections in dark theme](docs/screenshots/desktop-agent-connections-dark.png)
![Observed agent connections in light theme](docs/screenshots/desktop-agent-connections-light.png)
![Reported tools, a delegated agent and a failed execution in dark theme](docs/screenshots/desktop-agent-flow-dark.png)
![Reported tools, a delegated agent and a failed execution in light theme](docs/screenshots/desktop-agent-flow-light.png)
![Narrow dark flow with a readable inspector](docs/screenshots/desktop-agent-flow-narrow-dark.png)
![Narrow light flow with a readable inspector](docs/screenshots/desktop-agent-flow-narrow-light.png)

</details>

**Shell hook.** Add `eval "$(holdmap init zsh)"` to `~/.zshrc` (also `bash`, `fish`, `powershell`).
When a command fails with "port in use", it prints who holds the port and how to free it.

## Desktop app

Open **Settings → Power → Shut down now…** to shut down this computer. Review the local
computer, save work, and explicitly acknowledge that all apps, agents and local services
will stop. Confirmation expires after 60 seconds; this is not a scheduled shutdown.
Operating-system permissions and inhibitors still apply. The browser demo only simulates
the action. Local shutdown was introduced in 0.4.0; version 0.3.0 binaries predate it.

<details>
<summary>Shutdown confirmation in dark and light themes</summary>

![Dark shutdown confirmation](docs/screenshots/desktop-shutdown-dark.png)
![Light shutdown confirmation](docs/screenshots/desktop-shutdown-light.png)

</details>

**Scheduling:** Power settings separate **Shut down now…** from **Schedule
shutdown…**. The scheduled review, **Schedule computer shutdown?**, starts at **In 1 hour**
and excludes **Now**; immediate shutdown has no timing controls. In the browser, the entries
are **Simulate shutdown now…** and **Schedule simulation…**, with **Schedule shutdown
simulation?** clearly identifying the scheduled review. For a local port, **Schedule stop…**
opens the one-hour review separately from **Stop…**. Choose another preset, a custom delay
or **At a time** as needed; agent service reviews also support delays. **Scheduled actions**
shows each deadline, countdown, reviewed port and cancellation result. When nothing is
scheduled, **0.6.0** can start a shutdown or a reviewed service stop from that dialog.
Schedules run while Holdmap stays open, including in the tray; quitting clears them. Keep the computer awake.
Missed deadlines or clock changes require a fresh review. Agent schedules stop only the
reviewed services and ports, preserving the agent session. Use **0.6.0 or newer** (scheduling
itself landed in 0.5.0).

<details>
<summary>Scheduled shutdown, agent services and cancellation in dark and light themes</summary>

![Dark scheduled shutdown](docs/screenshots/desktop-shutdown-schedule-dark.png)
![Light scheduled shutdown](docs/screenshots/desktop-shutdown-schedule-light.png)
![Dark scheduled agent services](docs/screenshots/desktop-agent-schedule-dark.png)
![Light scheduled agent services](docs/screenshots/desktop-agent-schedule-light.png)
![Dark scheduled action manager](docs/screenshots/desktop-schedules-dark.png)
![Light scheduled action manager](docs/screenshots/desktop-schedules-light.png)

</details>

The screen recorder and Computer / Footprint / Project map zoom described below require
**0.7.0 or newer**. Version **0.6.0** installers predate these features.
To add them to an existing **0.6.0** installation, download and install the new desktop build.

A tray and menu-bar app with the port list, details, the service graph (`G`), the agents map
(`⇧A`), pins, history with one-click restart, local screen recording (`⇧V`), remote hosts over
SSH and a command palette (`⌘K` /
`Ctrl+K`). `⌘⌥P` (`Ctrl+Alt+P`) brings it up from any app; press `?` for every shortcut. `B`
hides the details pane so Graph uses the full window; Agents and Flow have matching Hide
cards / Hide inspector / Hide limits / Hide recording controls. On the
Agents map, Computer / Footprint / Project zoom over the same observed processes (C4-style:
this machine, folders/tools/ports/hosts across agents, then one source tree). Expand a card
to reveal a folder, open it in your editor, or stop the unprotected
ports that agent started. From a port’s details pane you can open it in a browser, restart a
dev server, open its folder (`HOLDMAP_EDITOR`, else Cursor, VS Code, Zed…) or Finder, and copy
its URL, a `curl` or the kill command. Version 0.4.0 also adds agent search, expanded child-tool
details, inferred MCP nodes and explicit omission counts. Native reports refresh independently
of the window's poll; the browser [live demo](https://santoshshinde2012.github.io/holdmap/#demo)
uses the same UI with sample data.

Current source preserves composing and modified keys in settings, menus and resize controls.
Dialog Tab navigation includes only enabled, visible controls in the active tab order.

**Close and reopen.** With a working menu-bar or tray icon, closing the main window keeps
Holdmap running. Click its icon or use your configured global shortcut to reopen; on macOS,
the Dock icon also restores it. **0.6.0** hides the application through AppKit so a fullscreen
close returns to the previous app and retains the window's state for reopening. Choose
**Quit** to exit. If the tray is unavailable, closing the main window exits normally.

**Folder actions.** The chooser was added in **0.5.0**. In the **0.7.0** map, click a folder
on Footprint to zoom into that source tree; click it
again (or its name/path in a card) for folder actions. **Enter** or **Space** activates a focused folder. The shared chooser shows
the full path and offers **Open in Finder** (macOS), **Open in Explorer** (Windows) or
**Open in file manager**, **Open in editor**, and **Copy path**. **Esc** closes it and returns
focus to the folder. Clipboard copying preserves the original path.

Editor opening uses `HOLDMAP_EDITOR`, then a known installed editor. Set an executable name or absolute path, without shell arguments. A missing
explicitly configured editor shows an error instead of opening another app. Native actions
recheck the chosen agent and exact folder against a fresh scan; stale folders remain copyable
but cannot be opened. Recent folders remain account history, not proof of current work.
Browser file-manager/editor actions are labelled simulations. Version 0.4.0 downloads predate
the chooser; use 0.5.0 or newer.

<details>
<summary>Current folder actions with sample data</summary>

![Folder actions show the full path, file-manager, editor and clipboard choices in dark theme](docs/screenshots/desktop-folder-actions-dark.png)
![Folder actions with clear browser simulation guidance in light theme](docs/screenshots/desktop-folder-actions-light.png)
![Folder actions retain readable paths and controls in a narrow window](docs/screenshots/desktop-folder-actions-narrow-light.png)

</details>

**Screen recording.** Choose the camera button, **Record screen…** in the command palette,
or `⇧V`, then select a screen or window in the system chooser. Capture lasts up to two
minutes with no microphone or system audio. Stopping ends screen sharing before preparing
the available MP4/GIF exports; format availability depends on encoding support.
Save through the native file dialog, or the browser's download manager in the demo.
Closing the preview keeps it available through **Review recording** for this session;
**Discard recording**, reload or quitting clears it. Save a file to keep it.
This is separate from optional agent tool-call metadata recording. See the
[screen recording guide](https://santoshshinde2012.github.io/holdmap/docs/desktop/#record-the-screen)
for permissions, platform requirements and export limits.

**First open.** The **0.7.0** macOS app is ad-hoc signed and sealed without Developer-ID
signing or notarisation; its Windows installers are unsigned. Verify the checksums and
build provenance before opening a download. Follow the
[first-open guide](https://santoshshinde2012.github.io/holdmap/docs/desktop/#first-open)
for OS-specific instructions and policy restrictions.

Update an existing desktop installation by downloading the latest installer above that
matches your operating system and architecture. **0.7.0 requires manual desktop installation**;
its published installers have no configured signed updater feed. Run the CLI installer again
to update the CLI, TUI and MCP server; it does not update the separate desktop app.

<details>
<summary>More screenshots</summary>

| Service graph | Stop confirmation |
|---|---|
| ![The service graph: a Compose cluster with shop-web, shop-api, db and redis, dark theme](docs/screenshots/desktop-graph-dark.png) | ![The stop confirmation lists every step before anything is sent](docs/screenshots/desktop-stop-confirm-light.png) |
| **Command palette** | **Settings** |
| ![The command palette searching "sto"](docs/screenshots/desktop-command-palette-light.png) | ![Settings: startup and the global shortcut, dark theme](docs/screenshots/desktop-settings-dark.png) |
| **TUI** | **Agents** |
| ![The holdmap TUI: port table with the details pane](docs/screenshots/tui-list-dark.png) | ![The agents view: coding agents and developer tools with resource use, folders, ports and access, dark theme](docs/screenshots/desktop-agents-dark.png) |
| **Agents in light theme** | **Child tools and MCP evidence** |
| ![The agents map in light theme with sample data](docs/screenshots/desktop-agents-light.png) | ![Expanded child-tool details show an inferred Filesystem MCP process with no listening port](docs/screenshots/desktop-agent-tools-dark.png) |
| **Agent bulk stop, dark theme** | **Agent bulk stop, light theme** |
| ![A bulk stop previews all four steps for the agent's two ports, dark theme](docs/screenshots/desktop-agent-stop-confirm-dark.png) | ![A bulk stop previews all four steps for the agent's two ports, light theme](docs/screenshots/desktop-agent-stop-confirm-light.png) |
| **Compact port details, dark theme** | **Compact port details, light theme** |
| ![A 340-pixel details pane keeps tabs, activity and separate Stop and Schedule stop actions readable](docs/screenshots/desktop-detail-compact-dark.png) | ![Compact details retain clear port scope and actions in a 1100-pixel window](docs/screenshots/desktop-detail-compact-light.png) |
| **Separate Power actions, dark theme** | **Separate Power actions, light theme** |
| ![Power settings distinguish immediate shutdown from scheduled shutdown simulations](docs/screenshots/desktop-settings-power-dark.png) | ![Power settings keep the local scope, save-work warning and open-app schedule requirement visible](docs/screenshots/desktop-settings-power-light.png) |
| **Sort menu at the right edge, dark theme** | **Sort menu at the right edge, light theme** |
| ![The Sort menu stays inside the viewport with readable grouping descriptions](docs/screenshots/desktop-sort-right-edge-dark.png) | ![The right-edge Sort menu leaves all choices visible in a compact window](docs/screenshots/desktop-sort-right-edge-light.png) |

</details>

## MCP server

`holdmap mcp` lets AI coding assistants list ports, explain them, find a free port, wait for a
server, read the service graph, list agents and developer tools (`list_agents`), stop the ports
an agent started (`stop_agent_ports`, dry-run by default), and stop their own dev servers
(never protected processes).
For Claude Desktop or Cursor (`~/.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "holdmap": { "command": "holdmap", "args": ["mcp"] }
  }
}
```

VS Code (`.vscode/mcp.json`) uses `"servers"` with `"type": "stdio"`. If the client can't find
`holdmap`, use the full path, for example `/Users/you/.local/bin/holdmap`.

Version 0.4.0 also exposes a bundled `holdmap://guide` resource and three workflow prompts:

| Prompt | Purpose |
|---|---|
| `diagnose_port` | Explain ownership, inspect topology with `all: true`, then choose another port or preview an authorized stop. |
| `prepare_dev_server` | Find a candidate port and check TCP readiness after the project's authorized launch. |
| `inspect_agents` | Summarize child tools, resource use, folders and access with their evidence and collection limits. |

Retrieving a prompt performs no actions. The guide and templates help clients interpret results;
MCP identities remain inferred from process metadata and do not prove tool calls or transport.
Tool input/output schemas preserve structured data, and malformed arguments are rejected before
acting, including incorrectly typed stop controls. Both stop tools preview by default; execution
requires the one-use `confirmation_id` returned by a preview in the same session, with unchanged
target/options and fresh matching effects. The MCP client must show the plans and obtain user
authorization for their effects; the handle binds the preview but does not establish consent.
The session keeps at most 32 previews for five minutes.
Frames are capped at 1 MiB and legacy batches at 64 messages, with oversized input rejected before
dispatch. `list_ports` reports collection warnings even
for empty results. See the [MCP guide](site/src/content/docs/mcp.md) and
[0.4.0 changes](CHANGELOG.md#040---2026-10-10). Clients upgrading from 0.3.0 must adopt
the preview and confirmation flow before executing stops.

## Safety model

holdmap stops processes, so one set of rules in `holdmap-core` applies to every surface:

**In 0.6.0:** state-read/write failures stay visible and preserve existing files.
If a stop completes but its history cannot be saved, the desktop preserves the actual result
and blocks automatic restart or later queued stops. CLI `stop --json` includes `history_error`
and exits with code 2; additional requested targets are not attempted. See the
[0.6.0 changes](CHANGELOG.md#060---2026-10-14).

- **Graceful first:** SIGTERM (`taskkill` on Windows), then a force kill after `--timeout` (5 s).
- **The right target:** the dev-server tree root (`npm`, `nodemon`, `uvicorn --reload`…) so nothing
  respawns; containers through their runtime, services through their supervisor.
- **Protected processes:** policy blocks core OS processes, holdmap and its ancestors. Shells,
  terminals, IDEs and AI-assistant hosts are protected by default; the CLI's `--allow-protected`
  or the desktop's explicit review can authorize soft protection. The executor rechecks identity
  and current protection before signalling and escalation. MCP has no protected-owner override.
- **Reviewed effects:** desktop stops retain the original plan behind a one-use handle, limited
  to 128 pending previews for five minutes, and reject changes found by a fresh semantic check.
  Agent bulk dialogs show every queued plan. MCP uses the same effect comparison with its own
  session previews. Plans support `--dry-run`, and execution checks whether ports are free.
- **Credential handling:** recognized secrets in command lines, errors and exported results are
  conservatively redacted, including headers and nested JSON values. Human terminal output
  neutralizes untrusted control sequences. Agent visibility uses process/socket metadata and
  bounded project-history fields. Optional recording discards prompt, argument and result
  fields after accepting explicitly configured local exporter requests; it does not inspect chats
  or credential stores. Redaction
  is heuristic; arbitrary positional secrets may be missed.
- **State privacy:** restart history retains raw commands. Unix history/config/log files use
  `0600`, state directories `0700`, with ownership and link checks. Windows uses inherited
  account/profile-directory ACLs and does not impose an account-exclusive DACL; keep a custom
  `HOLDMAP_HOME` private. Product analytics are absent. Optional agent recording accepts local
  allowlisted metadata into a bounded in-memory store; it never persists trace payloads.
- **Locked down:** desktop actions identify ports, history entries or known agent folders.
  The backend checks targets, constructs localhost URLs and resolves folders through a fresh
  scan. The webview has a strict CSP and local-only capabilities. Stack files are checked and
  read through one handle; Unix permits current-account/root ownership and rejects group/world
  writes, while Windows relies on checkout ACLs. State and CLI output writes reject link
  redirection. See [SECURITY.md](SECURITY.md) for OS limits and reporting.

## Architecture

`holdmap-core` owns collection, attribution, protection and stop planning. The CLI/TUI, native
backend and MCP adapter coordinate requests and present its types. The website builds the same
Svelte UI. The public website demo uses sample data; `npm run dev` and the desktop app read live local scans.

![Holdmap system architecture](docs/diagrams/system-architecture.svg)

<details>
<summary>Mermaid source</summary>

```mermaid
---
config:
  theme: base
  look: classic
  htmlLabels: false
  themeVariables:
    darkMode: false
    background: "#ffffff"
    primaryColor: "#DBEAFE"
    primaryTextColor: "#172554"
    primaryBorderColor: "#2563EB"
    secondaryColor: "#CCFBF1"
    tertiaryColor: "#ffffff"
    textColor: "#0F172A"
    lineColor: "#475569"
    edgeLabelBackground: "#ffffff"
    clusterBkg: "#ffffff"
    clusterBorder: "#94A3B8"
  flowchart:
    wrappingWidth: 240
---
flowchart TB
  accTitle: Holdmap system architecture
  accDescr: The CLI, MCP server and desktop share a Rust core. A separate website demo uses sample data. Only the native executor performs reviewed OS actions.

  subgraph CLIENTS["Application interfaces"]
    CLI["CLI / TUI<br/>Commands + terminal views"]
    MCP["MCP server<br/>Validated tools + preview handles"]
    DESKTOP["Desktop app<br/>Svelte UI + Tauri commands"]
  end

  subgraph CORE["holdmap-core · shared domain"]
    SCAN["Scanner<br/>Injected collection providers"]
    ENGINE["Engine<br/>Ownership + protection policy"]
    AGENTS["Agent visibility<br/>Folders, tools, network + evidence"]
    GRAPH["Topology<br/>Services, clusters + dependencies"]
    PLAN["ActionPlan<br/>Reviewed effects + safeguards"]
    EXEC["Executor<br/>Fresh identity checks + verification"]
    ENGINE --> SCAN & AGENTS & GRAPH & PLAN
    PLAN --> EXEC
  end

  subgraph RECORDING["holdmap-activity · optional local recording"]
    INPUT["Provider adapters<br/>Hooks, OTLP JSON + MCP observer"]
    COLLECTOR["Authenticated loopback collector<br/>Bounded parsing + normalization"]
    ACTIVITY[("In-memory metadata<br/>Reported identities + outcomes")]
    INPUT --> COLLECTOR --> ACTIVITY
  end

  subgraph LOCAL["Operating system + local state"]
    OS["macOS / Linux / Windows<br/>Sockets, processes + supervisors"]
    STORE[("Private state<br/>Config, pins + restart history")]
  end

  subgraph PUBLIC["Public website · browser only"]
    SITE["Astro site<br/>Guides + installation"]
    DEMO["Shared Svelte demo<br/>Sample data API"]
    SITE --> DEMO
  end

  CLI & MCP & DESKTOP --> ENGINE
  SCAN -->|reads metadata| OS
  EXEC -->|authorized actions| OS
  CLI & DESKTOP --> STORE
  CLI & DESKTOP -->|explicit recording session| COLLECTOR
  MCP -.->|optional observer| INPUT
  ACTIVITY -->|read-only flow| DESKTOP

  style CLIENTS fill:#ffffff,stroke:#94A3B8,stroke-width:1px
  style CORE fill:#ffffff,stroke:#94A3B8,stroke-width:1px
  style RECORDING fill:#ffffff,stroke:#94A3B8,stroke-width:1px
  style LOCAL fill:#ffffff,stroke:#94A3B8,stroke-width:1px
  style PUBLIC fill:#ffffff,stroke:#94A3B8,stroke-width:1px

  classDef surface fill:#DBEAFE,stroke:#2563EB,color:#172554,stroke-width:1.5px
  classDef domain fill:#EDE9FE,stroke:#7C3AED,color:#2E1065,stroke-width:1.5px
  classDef safety fill:#FEF3C7,stroke:#D97706,color:#451A03,stroke-width:1.5px
  classDef data fill:#CCFBF1,stroke:#0F766E,color:#134E4A,stroke-width:1.5px
  classDef web fill:#F1F5F9,stroke:#64748B,color:#0F172A,stroke-width:1.5px
  class CLI,MCP,DESKTOP surface
  class ENGINE,AGENTS,GRAPH domain
  class PLAN,EXEC safety
  class SCAN,OS,STORE,INPUT,ACTIVITY data
  class COLLECTOR safety
  class SITE,DEMO web
```

</details>

Blue groups interfaces, violet shows domain logic, amber marks action safeguards, teal marks
collection and state, and slate identifies the public website. Labels and boundaries carry the
same meaning independently of color. The browser demo uses sample data and has no native OS access.

SOLID guides these boundaries: collection, domain policy, execution and presentation have
separate owners; registries extend detection and planning; narrow provider, detector and
classifier traits accept system implementations or fixtures. Composition points choose
implementations. Activity adapters share metadata validation, while recording controls,
graph presentation and MCP delivery remain separate. The
[design notes](docs/architecture.md#7-design-notes) map all five principles to concrete
contracts and regression coverage. [Contributor review guidance](CONTRIBUTING.md#design-review)
explains how to preserve those contracts when extending the app.

Action authority stays at the adapters and executor: desktop and MCP retain immutable previews,
compare fresh effects before execution, and pass the original plan to the shared identity and
protection checks. Persistence, credential redaction and terminal-safe rendering have separate
boundaries. CI combines all-severity npm audits, cargo-deny, full-history/source Gitleaks scans
and CodeQL for JavaScript/TypeScript, Rust, Python and Actions; daily dependency checks and
immutable action pins keep those gates reviewable. The Linux GTK dependency uses
[audited GLib source fixes](vendor/README.md), verified by source hashes and optimized regressions.

The folder layout keeps each surface's code and tooling together:

```text
crates/holdmap-core/       Scanning, agents, topology, planning and execution
crates/holdmap-cli/        CLI, TUI and integration tests
crates/holdmap-mcp/        Protocol, schemas, tools, resources, prompts and observer contract
crates/holdmap-activity/   Metadata contracts, provider adapters, bounded local collector and client
apps/desktop/src/         Shared Svelte UI and pure TypeScript view logic
apps/desktop/src-tauri/   Native commands, cache, watcher and tray
apps/desktop/e2e/         Browser tests using sample data
site/                     Astro website, guides and demo build
docs/                    Architecture, generated CLI reference and published screenshots
scripts/                  Workspace checks, docs generation and development helpers
vendor/                   Audited third-party security backport and upstream provenance
```

Source assets and published screenshots are kept with their consumers. Build output,
dependencies and temporary captures are regenerated; Git history holds earlier versions.

## Troubleshooting

- **`command not found: holdmap`** right after installing: open a new terminal or use the
  PATH command printed by the installer. The default macOS/Linux shell setup uses
  `source ~/.config/holdmap/env.sh`; custom install/config locations can change that path.
  On Windows, open a new PowerShell window and check `Get-Command holdmap`.
- **An old copy runs instead of the new one:** check `command -v holdmap` (macOS/Linux) or
  `Get-Command holdmap` (Windows), then `holdmap --version`. A Cargo-installed copy in
  `~/.cargo/bin` can shadow the installer's copy in `~/.local/bin`; update or remove only the
  old copy you identified. Very old installations used the separate name `portwise`.
- **`Permission denied` on a shell rc file:** an old `sudo` left it owned by root. Run
  `sudo chown "$USER" ~/.bash_profile` and install again. Never run the installer with `sudo`.
- **Other users' ports are hidden:** run with `sudo` (an elevated terminal on Windows).
- **Slow or stale list:** `HOLDMAP_TRACE=scan holdmap list` prints how long each part of a scan
  took (sockets, processes, containers).
- **Uninstall:** remove the binary at the path reported above, remove the installer's PATH
  setup, and uninstall the desktop app through your OS. Settings and restart history remain in
  `~/.config/holdmap` (macOS: `~/Library/Application Support/holdmap`, Windows: `%APPDATA%\holdmap`),
  or your custom `HOLDMAP_HOME`. Delete that directory separately to clear them. History contains
  raw launch commands; inspect it before sharing diagnostics.

## Learn more

- [Website and guide](https://santoshshinde2012.github.io/holdmap/): the live demo and short how-tos.
- [What's new in 0.7.0](https://santoshshinde2012.github.io/holdmap/docs/whats-new/): recording, maps, fixes and upgrade instructions.
- [Website maintenance](site/README.md): SEO checks, deployment and current website snapshots.
- [Agents, tools and apps](https://santoshshinde2012.github.io/holdmap/docs/agents/): folders, access and stopping what an agent started.
- [CLI reference](docs/cli.md): every command and flag.
- [Architecture](docs/architecture.md): how the core, CLI, TUI, desktop app and MCP server fit together.
- [Brand assets](apps/desktop/assets/README.md): the vector master and icon regeneration guide.
- [Changelog](CHANGELOG.md) and [security policy](SECURITY.md).

## Contributing

Use Rust 1.95+ and Node.js 22.12+ (`nvm use`). [CONTRIBUTING.md](CONTRIBUTING.md) covers native
system dependencies, Chromium, actionlint and cargo-deny setup.

For browser development with live local data, build the CLI first, then start the desktop UI:

```sh
cargo build --locked -p holdmap
cd apps/desktop
npm ci
npm run dev
```

Add `?demo=1` to the development URL for sample data. The live development collector is
read-only; `npm run tauri dev` from `apps/desktop` runs the native app with real actions.
The public website always uses sample scan data. See the
[development guide](https://santoshshinde2012.github.io/holdmap/docs/desktop/#develop-with-local-data).

Run these checks from the repository root:

```sh
cargo test --locked                                  # core, CLI/TUI, MCP and activity
(cd apps/desktop && npm ci && npm run check && npm test && npm run test:e2e)
(cd site && npm ci && npm run check && HOLDMAP_SITE_OFFLINE=1 npm run build && npm test && npm run test:demo)
scripts/check-all.sh                                  # complete workspace and native checks
```

The full check script builds the UI before native checks and covers formatting, clippy,
Rust/native tests, browser tests, docs, website links/budgets, SEO and dependency audits.

## License

Dual-licensed under [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option. Unless
you state otherwise, any contribution you submit is dual-licensed the same way. The desktop app
bundles Inter and JetBrains Mono under the SIL Open Font License 1.1.

---
title: Desktop app
description: Explore the holdmap desktop app, agent visibility, keyboard shortcuts and confirmed local computer shutdown and scheduled actions.
order: 6
---

A tray and menu-bar app with the port list, details, the service graph, the agents map, pins, history with
one-click restart, remote hosts over SSH and a command palette. It runs on the same core as the
CLI, so the plans and protections are identical. [Try it in the browser](../../#demo).

## Shut down this computer

Version 0.4.0 adds **Settings → Power → Shut down this computer**. Current source labels
this immediate entry **Shut down now…**, separately from **Schedule shutdown…**; its review
has no timing controls. Check the computer
name, save your work and acknowledge that all apps, agents and local services will stop.
The confirmation expires after 60 seconds; it does not schedule shutdown. Cancel or Escape
closes the review before submission. After submission, follow any OS permission prompts;
closing the dialog does not cancel the request. If the OS cancels later, quit holdmap
completely and reopen it before trying again.

macOS uses System Events and may ask for Automation permission. Windows requests shutdown
without forced termination. Linux requires systemd 248 or newer with explicit inhibitor
checks. OS policy or permissions may refuse the action. The browser demo only simulates
shutdown and cannot power off a computer. Version 0.3.0 binaries predate this feature.

## Schedule shutdown or a service stop

**Current source / Unreleased:** scheduling is available in the current desktop source and
browser demo. Version 0.4.0 downloads include immediate shutdown and stop confirmations,
but do not include scheduling yet.

1. For this computer, open **Settings → Power → Schedule shutdown…**. The review is titled
   **Schedule computer shutdown?**. For a local port, use **Schedule stop…** in its details
   instead of **Stop…**. For an agent's services, open its card and choose **Stop all** or a service's **Stop**.
2. Dedicated schedule reviews start at **In 1 hour** and exclude **Now**. Choose another preset,
   **Custom delay**, or **At a time**. Delays range from
   one minute to 24 hours. The preview shows the local date, time and timezone.
3. Read the reviewed effects. For shutdown, acknowledge that all apps and unsaved work are
   affected, then choose **Schedule shutdown**. For services, choose **Schedule stop**.
4. Open the clock control, the active schedule banner, or **Settings → Power → View scheduled
   actions** to check the countdown and cancel. Cancellation succeeds only before execution
   starts; the UI waits for the backend's acknowledgement.

Keep Holdmap running, including in its tray, and keep this computer awake. Completely quitting
Holdmap or restarting the computer clears every schedule; the app does not wake the machine.
A deadline missed by more than one minute, or a wall/monotonic clock discrepancy greater than
30 seconds, requires a fresh review. Jobs run once and never retry automatically.

The backend retains the reviewed services and process identities. If a process restarts, a port
changes owner, agent ownership changes, or current protections block the action, Holdmap
refuses the scheduled stop instead of targeting a replacement. An agent schedule stops its
reviewed services and ports; it leaves the agent application running. Stop plans retain normal
grace periods and any escalation displayed during review. Force-stop requests and protected
process overrides cannot be scheduled.

Power controls always target the local desktop computer, including when viewing remote hosts.
Scheduling does not expose an SSH, CLI or MCP shutdown endpoint. Browser schedules simulate
status changes only; they cannot shut down a machine or stop a process. Browser Power entries
are **Simulate shutdown now…** and **Schedule simulation…**; the scheduled dialog is titled
**Schedule shutdown simulation?**.

These current UI screenshots show the browser simulation:

![Power settings offer separate immediate and scheduled simulations with clear local scope](../../../../docs/screenshots/desktop-settings-power-light.png)

![Schedule a shutdown one hour from now, with a separate acknowledgement](../../../../docs/screenshots/desktop-shutdown-schedule-light.png)

![Review the countdown and cancel a scheduled action](../../../../docs/screenshots/desktop-schedules-light.png)

## First open

**Linux desktop 0.3.0:** published AppImage, Debian and RPM downloads predate the GLib
security fixes. Use version **0.4.0 or newer**, or build the desktop from current source.
This Linux GTK dependency issue does not affect the CLI, TUI or
MCP server, or the macOS and Windows desktop builds. See the
[security policy](https://github.com/santoshshinde2012/holdmap/blob/main/SECURITY.md#linux-glib-dependency-backports).

The app isn't notarised or code-signed yet, so the OS asks once.

- **macOS:** open it once, then go to **System Settings › Privacy & Security › Open Anyway**
  (macOS 14 and earlier: right-click the app, then **Open**). Or run
  `xattr -dr com.apple.quarantine /Applications/holdmap.app`.
- **Windows:** in "Windows protected your PC", click **More info**, then **Run anyway**.
- **Linux:** `chmod +x` the AppImage, or install the .deb or .rpm.

## Keyboard

`⌘⌥P` (`Ctrl+Alt+P`) brings the app up from anywhere; change it in Settings. Inside the app
(`Ctrl` instead of `⌘` on Windows and Linux):

| Keys | Action |
|---|---|
| `⌘K` | Command palette |
| `/` or `⌘F` | Search ports, projects and processes |
| `↑` `↓` or `j` `k` | Move through the list |
| `Esc` | Clear the search, the filters or the selection |
| `⌫` | Stop the selected port (`⇧⌫` to force) |
| `o` · `c` | Open in the browser · copy its URL |
| `p` | Pin or unpin the port |
| `g` | Switch between the list and the service graph |
| `⇧A` | Agents, tools and apps: folders, ports they started, connections and access |
| `h` | Recently stopped, with one-click restart |
| `d` · `m` · `e` · `t` | Filter: dev servers · mine · exposed · TCP/UDP |
| `⌘R` | Rescan now |
| `⇧L` | Cycle the theme |
| `⌘,` | Settings |
| `?` | Every shortcut |

In current source / Unreleased, **Settings → General → Single-key shortcuts** lets you disable
letter and symbol shortcuts, including `/` and `j`/`k`. Arrow navigation and `⌘`/`Ctrl` shortcuts
remain available. This is useful with speech input or when you want to reserve letter keys.
Shortcuts respect text composition and native controls; port filters apply only in port views.
The command palette keeps its active result valid as running services change.

## Agents map

**Agents** (`⇧A`) shows each AI coding agent and developer tool that is running (Claude Code,
Codex, Cursor, Copilot, Gemini CLI, Windsurf, Aider, Docker Desktop, OrbStack and others). Cards
skim memory, CPU, process count, stoppable ports and parent; expand for folders (with source
labels), ports and apps it started, child processes as tools & apps, connections, and access: the
account it runs as, sandbox and approval flags, network listeners, and macOS-protected folders.
Each fact is marked seen, inferred or unknown. From a card you can reveal a folder, open it in your
editor, or stop the unprotected ports that agent started. Recent projects come from folder names
only. Chats, settings and tokens are never read. Remote hosts show as IP addresses. Click a port to
open its details. `↑` `↓` switch agents. See [Agents, tools and apps](../agents/) for the full guide.
`holdmap agents` and `holdmap agents AGENT --stop-ports` do the same in a terminal.

Version 0.4.0 also lets you search by agent, child tool, folder, exact PID or
`port:3000`. Likely MCP server children appear even without a listening socket; expanded cards
show their evidence, resource use and redacted commands. Omitted counts and collection warnings
show where visibility is incomplete. Version 0.3.0 predates these additions;
the browser demo runs the current UI with sample data.

## From the details pane

In current source, **Open** and **Restart** stay beside **More actions**, which offers folder
opening and copy commands. **Stop…** and **Schedule stop…** have a separate footer labelled
with the selected port; stopping a port keeps the computer running. Tabs wrap and content
remains readable in a 340-pixel pane.
The Sort menu keeps its choices and descriptions within the right edge of the window.

Folder opening uses `HOLDMAP_EDITOR`, then legacy `PORTWISE_EDITOR` when unset, or a known
installed editor. The file manager and URL, `curl` and command-copy actions remain available
through **More actions**.

![Compact port details wrap tabs and keep separate Stop and Schedule stop controls visible](../../../../docs/screenshots/desktop-detail-compact-light.png)

![The Sort menu remains within the right edge with readable grouping descriptions](../../../../docs/screenshots/desktop-sort-right-edge-light.png)

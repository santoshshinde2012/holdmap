---
title: Agents, tools and apps
description: Use holdmap to inspect AI coding agents, see which ports they started, and stop only those services.
order: 2
---

holdmap finds the **AI coding agents** and **developer tools** on your machine and shows, for
each one, the folders it works in, the ports and apps it started, the services and hosts it talks
to, and what the OS says about its access. Every process fact is marked seen, inferred or unknown.
Default scans do not inspect chats, agent settings or credential stores. Recognized credentials
in observed process arguments are redacted for display; redaction is heuristic. Optional recording
accepts a separate, strictly limited metadata projection from integrations you configure.

Recognized products include Claude Code, Codex, Cursor, Copilot, Gemini CLI, Windsurf, Aider,
Cline, Goose, Docker Desktop, OrbStack, Podman Desktop and others.

The Computer / Footprint / Project map, live local scans during `npm run dev`, and separate
screen recorder described below require **0.7.0 or newer**. Check the
[latest published release](https://github.com/santoshshinde2012/holdmap/releases/latest) for
current installers. Version **0.6.0** added editor-extension identity and Hide cards / Hide
inspector. Flow, folder actions and the updated CLI and terminal navigation landed in
0.5.0. Child-tool classifications, expanded metadata and desktop agent search were introduced
in 0.4.0. The live demo uses the same interface with sample data.

## From the CLI

```sh
holdmap agents                         # compact overview
holdmap agents claude                  # matching agents with detailed footprints
holdmap agents codex --wide             # include redacted process commands
holdmap agents claude --stop-ports --dry-run   # plan only
holdmap agents claude --stop-ports --yes       # stop unprotected ports that agent started
holdmap agents --json                  # machine-readable report
```

Current source also supports `holdmap agents --capabilities --json` for explicitly requested
configuration/skill metadata. This addition is **Unreleased** and is not in the published
**0.7.1** binaries; ordinary `agents` scans keep that inspection off.

Each entry shows memory, CPU, process count, how many ports are stoppable, the parent process,
child processes grouped as tools & apps it started, folder source labels (working dir / child cwd /
recent), ports with roles, connections, and access facts.

Child tools include likely MCP servers even when they have no listening port. The report
includes evidence and omitted counts; an executable signature does not establish which tools
an agent called. Recent folders are current-account history, attached only to agents with a
matching known account, and do not establish current work by a particular running instance.

## Desktop and TUI

- **Desktop:** `⇧A` opens the Agents map. Expand a card to reveal a folder, open it in your editor,
  or stop the unprotected ports that agent started. Paths are verified against the current report
  before anything opens.
  Search by agent, folder, tool, exact PID (`pid:51200`) or explicit port (`port:3001`). A port
  search includes both the listener owner and agents connected to it.
  Choosing **Agents** reveals **Overview** and **Flow** as action pills on the same toolbar row.
  **Overview** then reveals **Computer**, **Footprint** and **Project**; **Flow** reveals
  **Connections** and **Calls**. Current source also offers the explicit **MCP & skills**
  inspection described below. Computer is this machine, the agents running on it, and observed
  outside systems, with parent `started` edges. **Footprint** shows folders, tools, ports and hosts
  across every agent. Click a folder to zoom into **Project** — only agents, ports, tools and hosts
  observed for that source tree — never sample or invented peers. **Hide cards** and **Legend** give the map the full width; both persist.
  The installed app and `npm run dev` read the live process table; the website demo uses labelled sample data.
- **TUI:** start `holdmap`, then press **Shift+Tab** to open Agents directly. **Tab** cycles
  Ports → Graph → Agents. Use **↑/↓** to select an agent, **/** to search, and **PgUp/PgDn** to
  scroll its details. Wide windows show both panes side by side; small windows stack them.
  Mouse-wheel scrolling follows the hovered pane. **p** pauses, **r** refreshes and **?** opens
  help. Selection follows the same agent across refreshes. Failed scans clearly mark cached data.
- **Review a service stop:** press **x** to review every affected service and its exact TCP/UDP
  protocol. Scroll the full plan with **PgUp/PgDn**. **Enter**, **n** or **Esc** cancels; only
  **y** confirms. The batch is freshly checked before any action. Changed targets require a new
  review; failures stop remaining plans and show what completed. Agent sessions stay running.

These browser captures show **Computer** and one selected **Project** using sample data:

![Computer view shows this machine, its running agents and observed remote systems](../../../../docs/screenshots/desktop-agents-computer-dark.png)

![Project view focuses the agents, tools and services observed for shop-web](../../../../docs/screenshots/desktop-agents-project-dark.png)

The compact CLI overview and updated terminal navigation are included in **0.5.0**;
version 0.4.0 downloads predate these changes. `--json` retains
its structured report format. No terminal shell attachment or private conversation content is
needed to inspect the observed processes.

These terminal captures use isolated local fixtures:

![Terminal Agents list beside selected tools, services and access details](../../../../docs/screenshots/tui-agents-dark.png)

![An 80-column terminal keeps agent identity visible while tool details scroll](../../../../docs/screenshots/tui-agents-narrow-dark.png)

## MCP and skills (current source)

These additions are [Unreleased](https://github.com/santoshshinde2012/holdmap/blob/main/CHANGELOG.md#unreleased),
not part of the published **0.7.1** installers. Open **Agents → MCP & skills** to explicitly
inspect supported local configuration for the running current-account agents. The CLI equivalent
is `holdmap agents --capabilities`, optionally with `--json`; MCP clients can call
`list_agents` with `include_capabilities: true`. Ordinary scans do not inspect those files.

### Four kinds of evidence

| Evidence | Where to look | What it establishes | What stays unknown |
| --- | --- | --- | --- |
| **Configured MCP server** | MCP & skills | A server alias was found in a supported config file, with declared transport, scope, source path and any explicit enabled flag. | Whether this running client loaded it, approved it, connected to it or called a tool. |
| **Skill file** | MCP & skills | A `SKILL.md` file exists in a supported skill directory. Its folder is the identity; optional declared name and purpose describe the file. | Whether the skill is valid, trusted, enabled, offered to the model or activated. |
| **Running child tool** | Overview / Connections | The OS observed a process and its parent. A known executable signature can suggest an MCP server. | Its configured alias, negotiated transport and individual calls. |
| **Recorded usage** | Flow → Calls | A configured integration explicitly reported a tool event, MCP alias or Claude skill activation. | Uninstrumented usage, authenticated process ownership and completion of an activated skill. |

Nothing is launched, connected, installed or enabled by inventory inspection. Calls never joins
an alias or skill name to an inventory row or process automatically. Matching names across two
sources are insufficient evidence of ownership or use.

These current-source browser captures use labelled sample definitions and process observations:

| MCP and skills map | Selected skill details |
| --- | --- |
| ![Capability map places declared MCP sources left of agents and skill sources to the right, with runtime candidates separate below](../../../../docs/screenshots/desktop-capabilities-dark.png) | ![Selecting a skill card reveals folder identity, optional declared purpose and source in the left inspector](../../../../docs/screenshots/desktop-skills-dark.png) |

| Light theme | Narrow window |
| --- | --- |
| ![The light-theme capability map preserves declared-source relationships and separates runtime observations](../../../../docs/screenshots/desktop-capabilities-light.png) | ![A narrow capability map opens selected details in a dismissible inspector](../../../../docs/screenshots/desktop-capabilities-narrow-light.png) |

### Inspect supported definitions

Open **MCP & skills** for a map with the same visual style as Overview. The left rail starts
with **All agents** and the running agents. Configured MCP definitions sit to the left of the
agents; skill files sit to the right. Lines associate an agent's inspected source layout with
a definition. They do not mean the agent loaded, trusted, connected to or used it. Running MCP
candidates occupy a separate lane below the agents, with no inferred links to definitions.

Select an agent card to focus its map and open agent details. Select an MCP, skill or runtime
card to replace the left rail with that item's inspector. Nothing is selected automatically,
and there is no empty details column. **Agents**, the close button or **Esc** returns to the
agent rail and restores focus, while retaining the selected agent scope; **Show all** returns
to the combined map. Keyboard users can focus a map card and press **Enter** or **Space**.
On narrow windows, **Browse agents** opens a dismissible agent drawer. Fit, zoom and the minimap
help explore larger maps. A bounded map reports omitted definitions and relationships.
The narrow canvas opens at a readable zoom around an agent; pan the map or choose **Fit all
capabilities in view** to see the wider layout.

Use **All**, **MCP servers**, **Skills** or **Running** to choose the evidence on the map.
**Filters** reveals scope and MCP configuration state. Running candidates have independent
evidence: definition scope and enablement do not filter them, and **Running** resets/disables
those selectors. Search matches names, retained purposes, source paths and reported PIDs.
Filtering or a refresh that removes the inspected item closes its details rather than choosing
another item. An unchanged definition stays selected across refreshes.

**Coverage** opens the collection notes on request, grouping common notes across agents. An
empty or unsupported selected agent remains on the map with a compact explanation and actions
to view all agents or coverage; it does not expand a long list of notes automatically. Source
paths in the inspector identify where each definition was found.
An explicit enabled/disabled value is a declaration; a missing value stays **Unknown**. Client
precedence, project trust, approval policy and effective loading are not resolved. Repeated
aliases from different files remain separate definitions; Holdmap does not choose a winner.

Skill identity stays the folder name even when the optional YAML header declares another name.
Inspection reads a bounded prefix of `SKILL.md` to project only a sanitized declared name and
one-line purpose, up to 300 characters. Missing, malformed or unsafe metadata is omitted; file
presence can still be shown. The purpose is author-supplied text, not an instruction for Holdmap
or proof that the skill behaves as described. Instruction bodies are not retained or executed.
Only the scalar YAML `name` and `description` fields are projected as `declared_name` and
`description`; other header fields are discarded. A strict opening/closing `---` header is
required. Unsupported complex values for these two fields, duplicate keys, aliases, anchors,
tags, malformed YAML or parser limits leave header metadata absent. Inspection neither expands
aliases nor resolves includes. It does not validate the whole skill package.

These are the supported default locations; `~` means the current account's home. Project paths
mean observed working folders and known project roots, not arbitrary parent folders or recent
folder history.

| Client | MCP definitions | Skill directories |
| --- | --- | --- |
| Codex / Codex desktop | `~/.codex/config.toml` and `<project>/.codex/config.toml` | `.agents/skills` and `.codex/skills` under home and observed projects |
| Claude Code | `~/.claude.json` user entries and matching private project entries; `<project>/.mcp.json` | `.claude/skills` under home and observed projects |
| Cursor / Cursor agent | `.cursor/mcp.json` under home and observed projects | `.cursor/skills`, `.agents/skills`, `.claude/skills` and `.codex/skills` under those locations |
| Claude Desktop | Manual `Claude/claude_desktop_config.json` under `~/Library/Application Support` on macOS or `%APPDATA%` on Windows | Not inspected |

Skill directories are shallow, with one extra category or `.system` level. Custom config homes,
other ancestor directories, managed settings, plugins, desktop extensions, connectors and cloud
inventories are not inspected. Unsupported products/platforms, other accounts, symbolic links,
reparse points and macOS privacy-protected locations are skipped. File, byte, entry, project,
context and time limits bound inspection; the report preserves omissions and coverage limits.
An empty list does not prove that no capabilities exist.

The current collection bounds are explicit:

| Bound | Scope and behavior |
| --- | --- |
| **16 agent contexts** | Per report; repeated matching contexts share their inventory, and additional contexts remain unknown. |
| **8 observed projects** | Per context, alongside the account's supported default locations. |
| **24 existing sources** | Per context, counting attempted reads of existing configuration/header files and skill directories. Missing default paths do not consume this allowance. |
| **1 MiB total bytes** | Per context across configuration files and skill-header prefixes. |
| **256 candidate entries** | Per context across MCP definitions and skill-directory entries; additional entries may remain unexamined. |
| **64 MCP definitions and 64 skills retained** | Per context, with explicit omitted counts for additional definitions that were observed. Those counts do not estimate unexamined locations. |
| **256 KiB per config; 8 KiB per skill prefix** | Oversized configs are skipped. Only a complete header within the skill prefix can contribute optional metadata. |
| **256 YAML parser events; depth 8** | A more complex header contributes no metadata; the skill file can still appear. |
| **100 ms cooperative deadline** | Per context, checked between filesystem/parser operations. It is not a hard OS timeout and cannot interrupt a blocked filesystem call. |

The total-byte budget includes the bounded prefix read, even though bytes beyond its YAML
delimiter are not parsed or retained. No background scan expands these limits or searches
additional locations on your behalf.

Inspection parses bounded JSON/TOML files, which may contain credential bytes, then projects
only the allowed metadata. Command strings, arguments, URLs and environment/header values are
excluded from the returned inventory. The skill prefix can contain additional text, but only
allowed YAML-header metadata is retained. Source paths remain visible for manual inspection.
This inventory does not audit a server or skill for safety or authorize its command to run.

The same explicit inspection is available without the desktop:

```sh
holdmap agents --capabilities             # detailed current-account inventory
holdmap agents claude --capabilities --json
```

For MCP, send a `tools/call` request to a current-source Holdmap server:

```json
{"name":"list_agents","arguments":{"include_capabilities":true}}
```

The default `include_capabilities` is false. Inventory does not run in ordinary scans, and
`--capabilities` cannot be combined with `--flow` or `--stop-ports`.

### Understand explicit usage

The built-in hook/OTLP adapters report sanitized MCP server aliases from explicit Claude hook
metadata or an unambiguous `mcp__server__tool` label. Explicit normalized integrations can
also supply the optional `ActivitySpan.mcp_server` field on tool records through the authenticated
`/v1/activity` route; the collector sanitizes that identifier and validates the record. These
are unverified source labels, never an automatic inventory/process join or action authority.
The standard MCP protocol does not require this naming convention. Claude skill activation
logs are separate from tool-call counts. Provider-redacted
custom skill names stay **name not reported**, and activation never establishes completion or
success. See the [recording contract and optional log-only setup](https://github.com/santoshshinde2012/holdmap/blob/main/docs/agent-flow.md#record-skill-activations-current-source).

Use matching current-source CLI/desktop and collector builds for these unreleased usage
fields. The new decoder accepts older snapshots with no MCP alias, but the published 0.7.1
decoder rejects new fields or the new skill kind. An older collector cannot report this new
skill event; absent records do not prove that no skill was used.

### Troubleshoot missing capabilities or usage

- **Not inspected:** open MCP & skills or pass the explicit CLI/MCP option. Only confirmed
  current-account agents are eligible; process visibility alone does not grant config access.
- **Empty or partial inventory:** read the displayed limits and check the supported paths
  above. Custom homes, inherited/plugin definitions, unreadable or linked files and bounded
  scans can omit definitions. Check the client's own config without changing file permissions
  or enabling an unknown server just to make it appear.
- **Unknown enablement or purpose:** the file may omit an explicit switch or a safe, supported
  YAML header. Holdmap does not resolve client policy or validate skill packages.
- **Configured server with no Calls:** inventory is independent of recording. Configure a
  supported producer, start a fresh recording and restart that producer with its new endpoint
  and token. Keep private setup credentials out of bug reports.
- **Skill name not reported or no activation:** producer versions and telemetry policies vary;
  default custom names can be redacted. Use the documented log-only option if needed, without
  enabling detailed tool logging. Do not infer absence or a successful workflow from silence.

The boundaries follow the [MCP tools specification](https://modelcontextprotocol.io/specification/2025-11-25/server/tools),
[Agent Skills specification](https://agentskills.io/specification) and
[Claude skill telemetry](https://code.claude.com/docs/en/monitoring-usage#skill-activated-event).
Local inventory and observed activation answer different questions.

## Open an agent folder

Folder actions are available from **0.5.0**: click a folder on the map or its name/path in an
expanded card. From **0.7.0**, the first click on Footprint zooms into that source tree;
click the folder again (or **Enter** / **Space** on Project) for **Folder actions**. **Enter** or **Space** activates a focused folder. **Folder actions** shows the full path and
offers **Open in Finder** on macOS, **Open in Explorer** on Windows, or **Open in file manager**,
plus **Open in editor** and **Copy path**. **Esc** or **Close** returns focus to the folder.
Copying uses the original path, even when a card shortens its display.

The editor preference is `HOLDMAP_EDITOR`. Use an installed executable name or absolute path, without shell arguments. Without a
preference, Holdmap finds a known installed editor. A missing explicit preference shows an
error instead of silently opening another app.

The chosen agent and exact path are rechecked against the current report and a fresh native
scan before opening. If that folder disappears, open buttons become unavailable while the
path remains copyable. Recent folders describe account history; they do not prove that the
agent is currently working there. Browser file-manager/editor actions are labelled simulations;
copying uses the displayed sample path.

This chooser is included in **0.5.0**. Version 0.4.0 downloads predate it.
Existing card **Reveal** and **Editor** shortcuts remain available.

## Desktop Flow: connections and calls

In **Agents**, choose **Flow**. The two data sources answer different questions:

| View | What it shows | What it cannot establish |
| --- | --- | --- |
| **Connections** | Observed parent processes, listener ownership and live TCP socket relationships, including the source process IDs. Tool roles may be inferred. | Individual tool invocations, messages, data direction or transferred bytes. |
| **Calls** | Tool, model, agent, handoff and execution-step metadata supplied by a configured integration, with status, timing and reported parent spans. Current source also shows explicit skill activations separately. | Uninstrumented activity, private message content or verified OS process ownership. |

The graph has zoom, fit and a minimap. Participants, relationships and the activity timeline
provide keyboard-accessible ways to inspect the same evidence. **Hide inspector**, **Hide
limits** and **Hide recording** put those chrome sections away so the graph can stay in
focus; **Show** brings each back. **Pause view** stops automatic
updates; **Refresh** still fetches an explicit update. It does not stop recording or an agent.
The selected run and record remain stable during refreshes. Small windows stack the readable
timeline below the graph. Choices persist.

Use **All**, **Failed** or **No completion** to find the records you need. Search matches
individual metadata records, and **Order** switches between chronological order and longest
reported duration. Records without a duration stay last in duration order. The graph retains
matching records, the inspected record and their reported parents; filtering does not change
the recorded run. A finished record with an unknown outcome is not treated as missing completion.
The shown/total count and **Reset activity view** make active filters easy to see and clear.

Connections and Calls are included in **0.5.0**. Version 0.4.0 downloads predate Flow.
These captures use clearly labelled browser example metadata:

![Connections shows observed process parenting, listeners and live socket relationships](../../assets/agent-flow/connections-dark.webp)

![Calls shows a reported multi-agent trace with parallel tools and an activity timeline](../../assets/agent-flow/calls-dark.webp)

### Set up optional recording

This records tool-call metadata from configured agents. Pixel capture of a display is a
separate [screen recording](../desktop/#record-the-screen) that downloads MP4 and GIF locally.

1. Open **Flow → Calls → Start recording** in the local desktop app.
2. Open **Setup**, copy the Codex or Claude Code settings, merge them into your existing
   configuration, and restart that agent. Settings are never edited automatically.
3. Run a task in the configured agent, then inspect the received metadata in Calls.

Recording status is checked before controls become available. If its status cannot be read,
the view says so; use **Refresh** to check again. An unavailable status does not mean recording
has stopped.

The copied configuration contains a private local bearer token. The collector binds to loopback,
requires that token, rejects browser-origin requests and retains metadata in memory on this
computer. **Stop recording** ends collection and keeps the current snapshot. **Clear records**
removes it from memory; fully quitting Holdmap also discards it. When the tray or menu bar is
available, closing the desktop window keeps Holdmap and any active recording running there.
Recording does not survive a full quit.

Every new recording session creates a fresh token and an OS-assigned loopback endpoint.
After stopping and starting recording again, copy the new provider settings, merge them into
your configuration and restart its producers. Settings from the previous session will no
longer send activity to the new collector.

The default setup uses Codex telemetry logs and Claude Code telemetry traces; Claude Code
tracing is a beta capability. Standard OpenTelemetry JSON traces and explicit hook or MCP
integrations can also supply metadata. Choose one recording integration per provider to avoid
duplicate counts. Recording requires instrumentation and only includes subsequently reported
activity; a running agent alone does not expose its internal calls.

Payload, prompt, tool-argument, result, error-message, URL and path fields are excluded by
the metadata projection. Source labels are sanitized and may be redacted. Sanitization cannot
prove that a custom integration has not embedded sensitive information in a valid identifier.
Reported process IDs are labelled unverified and never authorize stop or file actions.

### Read the evidence correctly

- A **Requested** hook record may arrive before permission or execution. An absent completion
  stays incomplete; stopping recording marks pending requests unknown.
- Completed telemetry spans usually arrive after export. A tool wrapper with **Status unknown**
  stays unknown even when a separate execution step reports a failure.
- A **Recorded trace** draws parent links only from the parent span IDs the integration supplied.
  A **Recorded session** groups hook or log events without inventing a call hierarchy. Event order
  and session membership are not proof that two tools communicated.
- **Serialized input/output** sizes are optional reported payload sizes. They describe neither
  network traffic nor the contents of a message. Unknown differs from a reported zero-byte size.
- At most **1,000 spans** are retained. Large runs show up to **128 spans** in the graph, with an
  explicit count; all retained records stay available in the timeline. Selecting a later record
  shows it and its reported parent context. Sampling, export delays, collector limits and missing
  instrumentation can leave the view incomplete.

## Access and evidence

Access covers the account the process runs as, sandbox and approval flags, network listeners, and
(on macOS) protected folders. Each fact carries evidence: **seen**, **inferred** or **unknown**.
Unknown is labelled on cards so you never mistake a guess for a measurement.

Access facts come from the live process table, command line and sockets. Default scans do not
read agent settings, so YOLO or sandbox modes that exist only there stay **unknown**. The
separate, explicit [MCP & skills inventory](#mcp-and-skills-current-source) does not interpret
those files as permission grants, change process protection or replace a dedicated security audit.

Network facts name well-known debug and model-server ports (Node inspector, Chrome DevTools,
Ollama). Names taken from the port number are **inferred**; names taken from a detected
framework are **seen**. A loopback debugger is still this machine only; the same port on every
interface is elevated. MCP children started with unpinned `npx` (no numeric version) are
labelled on the tool, not treated as a completed tool call.

Editor-extension services that inherit the open workspace as their working directory are labelled
as that editor's extension, protected, and skipped as stoppable dev servers. They are not given
the project's framework (a database tools process in a Next.js folder is not Next.js).

## Safety

Stopping from the Agents surface only targets unprotected ports the agent started (dev servers and
services). The agent’s own IDE and auth listeners (`PortRole::Agent`) are skipped. Protected
processes with hard protection are never stopped. The CLI's `--allow-protected` flag can authorize
soft-protected targets only; MCP refuses all protected targets.

## Related

- [Desktop app](../desktop/): shortcuts and the Agents map.
- [MCP for AI agents](../mcp/): `list_agents` and `stop_agent_ports`.
- [CLI reference](../cli/): every flag for `holdmap agents`.

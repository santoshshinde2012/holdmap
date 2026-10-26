---
title: What's new
description: "What's new in holdmap: 0.12.0 adds guided analyzer installs, new-since-last-run results, skill trust, risk scores and scheduled rescans."
order: 1.5
---

The [changelog](https://github.com/santoshshinde2012/holdmap/blob/main/CHANGELOG.md) records
every change, and [releases](https://github.com/santoshshinde2012/holdmap/releases) has the
notes for each version.

## 0.12.0

**holdmap 0.12.0** makes skill analyzers a full workflow: install, run, compare between runs, and
know when a skill changed since you trusted it.

- **Guided analyzer installs.** **Install…** shows the pinned package, its source, the folder, the
  hosts contacted and what the analyzer reads, and installs only after you consent. The analyzer
  stays off until you approve it. [Analyzers](../analyzers/).
- **Run progress, results and exports.** A run dock with the current step and **Cancel**; a
  sortable **Results** table with **Show in folder**; JSON, SARIF, CSV and Markdown exports of
  what's on screen, with review reasons.
- **New and fixed between runs.** **New since last run** and **What was fixed** after each rescan.
  `holdmap insights --new --fail-on high` fails CI only on new findings.
- **Skill trust.** Each skill gets a fingerprint. Skills that changed since you trusted them are
  flagged on Home, on agents and in Security → **Skills**, with what changed by file name and size.
- **Risk scores.** Security → **Risks** gives an explained score per agent and skill folder from
  open findings and changed skills, with CSV and Markdown export.
- **Scheduled rescans.** Off by default. Only offline analyzers run, only while the app is open,
  with throttled notifications and quiet hours.
- **Custom snoozes** up to 90 days, with a reason.
- **MCP:** `list_skill_trust` and `list_risks` (sixteen read-only tools).
- Builds are unsigned; see [first open](../desktop/#first-open).

## 0.11.0

**holdmap 0.11.0** records only its own window, adds recording presets, and fixes what a pass over
every screen found:

- **Window-only recording.** Other apps, your desktop and notifications are never captured. macOS
  records the holdmap window directly, with no picker. Browsers are asked for this tab and the
  choice is checked; a screen or another window is refused before a frame is read. Browsers that
  don't report what they share (Firefox, Safari) and the Linux screen-sharing portal are refused too,
  because window-only can't be guaranteed there: use the macOS app or a Chromium tab.
  [Record the screen](../desktop/#record-the-screen).
- **Recording presets.** *Share a clip* (GIF, 15 s, up to 10 MB) or *Full session* (MP4, up to 30
  minutes), with a live size estimate, pause, and the saved path with Copy and Show in folder.
- **Every screen checked.** Contrast, target sizes, keyboard access, empty states and diagram text
  fixed across all views in light and dark.
- **MCP:** all read-only tools declare their four annotation hints explicitly.
- Builds are unsigned; see [first open](../desktop/#first-open).

## 0.10.0

**holdmap 0.10.0** brings opt-in analyzers for your agents' skills and MCP packages, a guided
way to add any MCP server as one, and the Security workspace:

- **Analyzers (opt-in).** Scan your agents' skills and MCP packages with SkillSpector, the
  built-in OSV.dev lookup, presets for Cisco Skill Scanner, OSV-Scanner, Trivy and Bandit, or
  any MCP server. Each one is off until you approve exactly what it gets, and runs on a copy in
  a sandbox. [Analyzers](../analyzers/).
- **Needs you everywhere.** `holdmap needs` and the MCP tool `list_needs` show the same list as
  Home, including analyzer findings. **Agents → Security** collects every agent's findings and lists the analyzers.
- **Add your own analyzer in three steps.** Source → Tools → Review: each tool's full
  description, hidden characters and the server's own hints before you choose, and it's added
  off until you approve it.
- **One look everywhere.** Summary tiles on Home, the port list, the service graph, MCP & skills
  and Security; Settings can turn reading agent settings off again.
- **MCP:** `list_analyzers`, `list_insights` and `list_needs` bring the server to fourteen tools.

## 0.9.0

**holdmap 0.9.0** brings Home, a richer agent inventory and port stops from Home, plus PNG
snapshots, a readable service graph and the **Explore / Map / Compare** capability explorer.

### Home, agent inventory and port stops

- **Home** shows every agent as **Cards** or a **Map** with a **Timeline** of uptime, ports
  and MCP, plus a **Needs you** list. Press `V` to switch. [Home](../desktop/#home-and-first-open).
- **Agent inventory** adds how each MCP server launches (runner, package, pinned or not), its
  remote host, env/header **names**, whether it runs now, permission rules, safety settings,
  instruction files and built-in tools: `holdmap agents --capabilities`, MCP `list_agents`
  with `include_capabilities`, and the desktop cards. Values, prompts and file bodies are never
  read. [Launch, run state and permissions](../agents/#launch-run-state-and-permissions).
- **Stop a port from Home:** click a card chip, map node or timeline pin for **Details** or
  **Stop…**; the stop plan opens first. Narrow windows list each agent's ports with the same menu.

### PNG snapshots alongside video

Use the camera button, **Take snapshot…** or `⇧S` to choose what to capture:

- **Holdmap view:** the visible app viewport, without screen-sharing permission.
- **Holdmap window:** the system captures only holdmap's own window, then sharing ends
  after the still is captured.
- **Recording window:** while video is active, take a still from the window being recorded
  without stopping the video.

Review and save the PNG locally. Images preserve their aspect ratio within **3840×2160**
and **32 MiB**. Closing the preview or cancelling a save retains it for retry; a new capture
replaces it only on success. **Discard snapshot**, reload or quit clears the retained image.
The desktop opens a native save dialog; the browser starts a download. Nothing is uploaded.

App-view snapshots work separately from display-capture support. The native window path
requires **macOS 14 or later**; other platforms use browser screen-sharing support and refuse
anything that is not the holdmap tab or window. [Snapshot sources and limits](../desktop/#take-a-png-snapshot).
Video recording retains its two-minute, no-audio MP4/GIF workflow and now uses the video icon.

### A readable service graph

Graph opens with a full canvas until you select a service. Both layouts pack disconnected
services and separate clusters compactly. Cards start at a readable size; **Fit all services
in view** offers an overview and **Reset to readable size** restores readable cards.

Search names, projects, exact `port:3000` / `:3000` or `pid:43000`, with every query term
required to match. Immediate observed neighbors keep connection context without becoming
extra matches. List filters stay separate. **Options** holds Traffic, External and Legend;
keyboard activation and focus return work through the selected service's details.
Layout proximity never invents a connection. [Service graph guide](../desktop/#service-graph-and-cluster-grouping).

### Explore, map and compare capabilities

Open **Agents → MCP & skills** to explicitly inspect the existing supported local sources.
**Explore** leads with declared skill purposes, **Map** retains source relationships, and
**Compare** shows exact observed definitions across two contexts. Matching process observations
group while retaining their original PIDs; a group is not a verified conversation or session.

Select a card for **Purpose**, **Source**, **Evidence** and **Activity**. File presence does not
establish loading, approval or use. **Tools** explains **Catalog not reported** instead of
claiming zero tools. No new client catalog, recording adapter or name-based activity join is
introduced; coverage and privacy limits are unchanged.

![Explore skill cards lead with declared purpose while preserving source paths, scope and file-presence evidence](../../../../docs/screenshots/desktop-capability-explore-dark.png)

The workspace loads only after explicit navigation. Search, scope/state filters, comparison
selectors and dismissible narrow-window details support the same observed inventory.
[Capability explorer, supported sources and limits](../agents/#mcp-and-skills).

## Upgrade

Update the CLI and desktop separately:

1. Run the current CLI installer from [Install](../../#install), then check `holdmap --version`.
   This updates the CLI, TUI and MCP server.
2. Download the published desktop installer for your OS and architecture. Quit Holdmap,
   install it, reopen the installed copy and check **Settings → About**, including its
   running-app location. Updating the CLI does not replace the desktop app.

Published installers have no signed updater feed, so desktop updates are manual. macOS apps
are ad-hoc signed without Developer-ID signing or notarization, and Windows installers are
unsigned. Check the release's SHA-256 checksums and build provenance before opening a new
download. [First open](../desktop/#first-open) explains the platform controls.

Optional activity recording keeps its metadata contract; use matching clients and collectors
for reported MCP aliases and skill activations. Every fresh recording session uses a new local endpoint and token; recopy its producer
settings and restart the producer. [Recording setup](../agents/#set-up-optional-recording).

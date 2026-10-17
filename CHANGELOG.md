# Changelog

All notable changes to holdmap are documented here. The project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Once release-please has its token, entries are generated
by [release-please](https://github.com/googleapis/release-please) from
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/); see
[Releasing](CONTRIBUTING.md#releasing).

## [Unreleased]

### Added

- **Home** in the desktop app, as **Cards** or **Map** (toggle at the top right, or press V;
  the choice is remembered). Cards show each agent's projects, ports, MCP servers and skills
  with an attention badge; Map shows the same footprint around this computer with a
  collapsible **Timeline** of uptime, ports and MCP. A compact **Needs you** list sits beside
  both. Selection is shared across views. Keyboard: ←/→ between agents, j/k in the timeline,
  Esc to clear. Click any port on Home for **Details** or **Stop…**, which opens the usual stop plan.
- Agent inventory now shows how each MCP server launches (runner, package, pinned or not),
  its remote host, env/header **names**, and whether it is running in that agent's processes;
  plus permission rules, safety settings, instruction files and documented built-in tools.
  Available in `holdmap agents --capabilities`, MCP `list_agents` and the desktop agent cards.
  Opt in from the welcome or Home. Values, prompts and file bodies are never read.
- A first-open welcome to choose the start view and review notifications, launch at login,
  single-key shortcuts and the agent inventory. Skipping changes nothing.
- On a narrow window the Home Map lists each agent's ports, with the same **Details** /
  **Stop…** menu as the wide map.

### Changed

- Screen recording and snapshots moved from the title bar to the status bar; the recording
  timer and **Stop screen recording** stay in the title bar.
- Evidence caveats show once, as a chip with a tooltip, instead of repeated notes.
- Flow keeps source, refresh and pause in its toolbar; inspector, limits and recording
  controls moved to **View options**.
- Agent cards wrap PIDs and counts whole instead of cutting them off.
- `holdmap agents --capabilities`, `stop` and `kill` help describe the inventory, the stop
  plan and protection; MCP docs list the full `include_capabilities` metadata.
- The website and README lead with Home, the per-agent inventory and safe port stops, and show
  the latest published release rather than the source version. The README badge now reads the
  latest release; the source-version SVG badge is gone.
- The desktop shell is split into preferences, commands, shortcuts and dialogs, with no
  behaviour change.

### Fixed

- On narrow windows the Home summary and layout toggle no longer run under **Needs you**.

### Security

- cargo-deny ignores the three `zip` advisories (RUSTSEC-2026-0367/0368/0369), reached only
  through `tauri-plugin-updater` on already signature-verified updates; see CONTRIBUTING.

## [0.9.0] - 2026-10-16

### Added

- A purpose-led MCP & skills explorer with **Explore**, **Map** and **Compare** views over
  the existing opt-in inventory. Matching observed process contexts group without losing
  their original identities; comparisons keep exact source definitions and coverage limits.
- Capability inspector sections for **Purpose**, **Source**, **Evidence** and **Activity**,
  with explicit unknowns for loading, approval and use. **Tools** shows **Catalog not reported**;
  configuration inspection does not enumerate tools or establish an empty catalog. Supported
  client coverage is unchanged; no client catalog adapter or inventory-to-call join is added.
- Local PNG snapshots from the camera button or `⇧S`: choose the visible Holdmap view
  without screen-sharing permission, or a screen/window through its system picker.
  During video recording, snapshot the existing source without another picker or stopping it.
- Review and save PNGs up to 3840×2160, scaled with their aspect ratio preserved, and
  32 MiB. Closing the preview or cancelling/failing a save retains the latest PNG for retry;
  explicit discard, reload or quit clears it. A new capture replaces it only on success.
- Separate snapshot providers, cancellation/result ownership and native save validation;
  app-view rendering embeds only allowlisted bundled assets. The native selected-source
  fallback requires macOS 14 or later.

### Changed

- Load the capability explorer on demand, with loading and retry states when it is opened.
- Use the video icon for screen recording and the camera icon for PNG snapshots.
- Give the service graph the full canvas until a service is selected. Details open beside
  the graph when space allows, or in a dismissible drawer, without changing List preferences.
- Keep service cards readable by default, with a zoom percentage/reset control and an explicit
  Fit all overview. Move secondary display settings into Options and keep the legend outside
  the graph viewport.
- Give Graph its own service/port/PID search with observed neighbour context; keep List-only
  filters and sorting out of Graph. Native card buttons support keyboard activation and focus
  returns after details close.

### Fixed

- Require published state when release asset verification explicitly requests
  `CHECK_RELEASE_DRAFT=false`; a draft can no longer pass that post-publication check.
- Pack disconnected graph components and cluster hulls compactly instead of repelling
  unrelated services across a large Force canvas. Preserve viewport choices during refreshes.
- Wait for fresh scope data before positioning the graph; centre a readable service card in
  narrow windows. Clear hidden selections so unrelated port actions cannot remain active.
- Dismiss Graph Options with Escape before clearing service details, including WebKit pointer
  activation that leaves focus on the canvas. Modal dialogs retain their own Escape handling.
- Make browser sample All services include the other listening TCP entries from the same
  sample snapshot, with matching IDs and no fabricated links.

### Upgrade

- Install desktop 0.9.0 to get PNG snapshots, the readable service graph and the capability
  explorer. Published 0.8.0 installers do not contain these changes. Update the CLI/MCP server
  separately if you use them; the capability and activity contracts introduced in 0.8.0 are
  unchanged. Older 0.7.x collectors and decoders still lack those 0.8.0 contracts.
- Desktop installation remains manual when a signed updater is not configured. Quit Holdmap,
  install the matching OS/architecture package, reopen it and verify Settings → About shows
  0.9.0. Use the latest published download until the complete 0.9.0 release is available.
- Snapshot screen/window capture uses the system picker and its permission flow. The
  Holdmap-view choice does not need screen-sharing permission. Native selected-source capture
  requires macOS 14 or later; app-view snapshots remain available without that fallback.

## [0.8.0] - 2026-10-16

### Added

- Opt-in MCP configuration and skill-file inventory for supported local agents through
  Agents → MCP & skills, `agents --capabilities` and MCP `list_agents(include_capabilities:true)`.
  It includes optional bounded skill-header names/purposes. Default scans stay off; inventory
  never executes definitions or treats presence as use.
- An interactive capability map places MCP definitions and skills around their agents,
  groups shared source definitions, and opens source/purpose details on selection. Runtime
  candidates remain separate. Search, filters, keyboard controls and a compact narrow-window
  inspector support exploration; coverage and rendering limits stay explicit.
- Capability inventory covers supported default and compatibility locations for legacy Windsurf, Gemini CLI,
  GitHub Copilot, Copilot CLI and Cline in addition to Codex, Claude Code, Cursor and
  Claude Desktop. Custom homes, Aider `--mcp-servers-file` paths, VS Code profile MCP
  files and editor globalStorage are not followed. Presence still does not prove loading
  or use.
- MCP `inspect_port` returns the same structured inspection as `explain_port`. MCP
  `get_agent_flow` projects observed process ancestry and live TCP relationships without
  command lines, prompts or recorded tool calls.
- The terminal Agents pane toggles inventory with **c**; leaving Agents revokes inspection
  and clears retained metadata. Ordinary TUI refreshes stay on the process table.
- Calls labels sanitized reported MCP aliases and explicit Claude skill activations separately
  from tool-call counts. Provider-redacted skill names and unreported outcomes remain unknown;
  documented log-only setup keeps tool-detail/content logging disabled.
- Settings → About identifies the running desktop copy and architecture, explains manual
  updates when automatic updates are unavailable, and links to the official desktop
  downloads and installation guide. The command palette also exposes desktop downloads.
- Browser About identifies the web preview and omits desktop settings and app paths.

### Fixed

- Parse supported provider formats without inventing transport or enablement: handle VS Code
  JSONC, Gemini comments, Copilot CLI local/bare project maps and Cline transport declarations.
- Reject malformed MCP argument envelopes as protocol errors and publish complete typed
  observed-flow schemas.
- Keep interactive graph text readable, dismiss nonmodal overlays when focus leaves, and
  exercise the actual light theme in browser tests.
- Retain native recording ownership across startup acknowledgement and cancellation, so
  cancelling during that handoff discards the acknowledged session instead of leaving
  its picker or capture active without a frontend owner.
- Coordinate native recording cleanup with in-flight operations so temporary files remain
  available until those operations finish.

### Upgrade

- Update the CLI/MCP server and desktop app separately to use the new capabilities and
  usage records. Use matching 0.8.0 clients and collectors; older 0.7.1 decoders reject
  the new usage fields and skill kind.
- Desktop updates require manual installation of the matching OS/architecture installer.
  Quit Holdmap, install it, reopen it and check Settings → About. The macOS installers are
  ad-hoc signed without Developer-ID signing or notarization; Windows installers are unsigned.
  This release does not include a configured signed updater feed. Verify published checksums
  and build provenance before opening downloads.
- Refresh the public website, README and guides for the MCP/skills map, supported sources,
  reported usage, MCP tools and desktop update instructions.

## [0.7.1] - 2026-10-15

### Documentation

- Refresh the README's 0.7.0 features, install/update guidance and development workflow;
  clarify SOLID review expectations and the authorization required for new supervisor strategies.
- Refresh the public website for 0.7.0 with release highlights, a release guide, current
  screenshots and product tours, and corrected install, upgrade, recording and privacy details.

### Fixed

- Use Apple's native screen picker on macOS 14 or later when the installed desktop
  window lacks browser screen capture. Native capture keeps the two-minute, video-only
  limit, local MP4/GIF exports, cancellable preparation and private temporary-file cleanup.
- Check the actual screen-capture API in hardened, packaged macOS runtime tests before
  exercising the synthetic encoder, and verify the real app's recording confirmation.
- Keep credential-redaction regression failures from printing tested secrets or raw HTTP
  metadata in test logs, including JSON, summaries and redirect assertions.
- Discover draft releases through authenticated release listing and numeric IDs instead
  of the published-only tag endpoint. Release recovery can verify and publish existing
  complete draft assets from `main` without rebuilding or moving their tag.
- Release website deployments run trusted workflow code from `main` while building the
  exact published tag's source, preserving source revision metadata and avoiding stale
  tag-context Pages deployments.

## [0.7.0] - 2026-10-14

### Added

- Local screen recording from the title bar or command palette (`⇧V`). After you pick a screen
  or window, Holdmap encodes **MP4** and **GIF** on this computer — nothing is uploaded, and
  microphone and system audio are not captured. Download either file when the recording stops
  (two minutes maximum).
- The Agents map zooms like a C4 diagram over the same observed report: **Computer** (this machine,
  the agents on it, and systems they talk to), **Footprint** (folders, tools, ports and hosts), and
  **Project** (one source tree). Edges stay honest — parent, host, folder, port and remote links are
  collected, never invented. Click a folder on Footprint to open that project; click it again for
  Reveal / Editor.

### Changed

- Closing a recording preview keeps the latest files available through **Review recording**
  for this session, including after cancelled or failed saves. **Discard recording** clears
  them before recording again; reloading or quitting still clears in-memory previews.
- README and the public Desktop, Agents and MCP guides identify **0.7.0** as the release
  adding screen recording, C4-style map zoom and live development scans, while preserving
  historical feature availability.
- Desktop `npm run dev` reads this machine through the holdmap CLI (ports, agents, C4 map, flow connections). Sample shop-web data stays on the public website demo, Playwright, and `?demo=1`. Stopping processes still needs the desktop app.
- Exclusive view actions use hinged action pills instead of nested segmented tracks: List / Graph /
  Agents, then Overview/Flow when Agents is selected, Computer/Footprint/Project when Overview is
  selected, Connections/Calls when Flow is selected, and Listening / protocol when List or Graph is
  selected. Independent toolbar and canvas toggles share the same pill shape with a soft fill. Hide
  cards lives on the map with Legend.
- The Agents map keeps Hide cards, counts and the legend in one compact overlay so the graph is not
  covered by a full-width bar. Sort lists a short reason for every order. The empty scheduled-actions
  dialog uses the same action rows as the rest of the app.
- Flow uses one toolbar: the source line, recording controls, Pause, Refresh and hide controls share
  that row, and Visibility limits start collapsed. Hide inspector, Hide limits and Hide recording
  put those sections away so the graph can stay in focus; each choice persists.
- The list details pane keeps tabs on one scrolling row. Identity and badges share the header;
  Open, Restart, Schedule and Stop sit in one footer bar without repeating the port.
- Folder actions wrap long paths at directory separators instead of mid-token.

### Fixed

- Name-based stops with `--no-tree` retain all matching processes, including children of
  another match. PID/name tree stops verify TCP and UDP listeners held by included descendants.
- Supervisor execution requires the planned command, unit or app ID. Ordinary systemd
  service stops recheck protected cgroup members outside the main process's descendant tree;
  hard protections remain enforced even with a soft-protection override.
- Optional MCP observers that panic are disabled for the session without interrupting tool
  dispatch or protocol responses. Explicit final OTLP span status takes precedence over a
  conflicting custom `success` attribute.
- Desktop settings, action menus and resize controls preserve composing and modified keys.
  Dialog focus traps exclude controls outside the active tab order and disabled or hidden
  controls, and preserve composing, modified and already handled Tab events.
- Cross-platform checks run against a temporary copy of current source, preserve existing
  vendored Cargo patches and leave the checkout's manifest and lockfile untouched on success
  or failure. The checks also include the activity crate.
- Keep distribution action pins synchronized with the release workflow, with a regression
  check that rejects future drift.
- Screen recording cancellation releases late streams and prevents stale results from reopening.
  Stopping ends screen sharing before preparing exports; preparation can be discarded.
- Linux desktop screen sharing handles WebKitGTK's display permission for the trusted main
  view, then requires the system ScreenCast portal's source selection. Camera and microphone
  permissions are not granted by this handler.
- GIF exports cover the complete recording with bounded adaptive sampling and background
  encoding. MP4 uses resolution-aware H.264 checks with a MediaRecorder fallback. Encoder
  failures preserve any other successful format.
- Desktop recording downloads use a native save dialog with atomic writes, cancellation and
  retryable errors. Recording encoders load only when needed; the public demo checks initial
  and optional JavaScript budgets separately and follows the current map and Flow controls.
- The public demo reveals dialog actions in the website's viewport, including after focus,
  animation and content-size changes, so scaled folder and stop dialogs remain usable.
- Release-triggered website builds use the same source commit as the installers and report
  that revision in `build-info.json`, even when development has advanced meanwhile.
- Releases remain drafts until every CLI archive, desktop installer and checksum passes the
  final asset check, so the latest download cannot advance to an incomplete release.

### Security

- Restrict GIF encoding to the owning dedicated worker's trusted messages and validate
  bounded frame data. Release website builds run on the published stable version tag without
  executing a `workflow_run` checkout or populating shared dependency caches. README downloads
  stay valid while a release is built.
- Update rustls to 0.23.46 for RUSTSEC-2026-0346 through RUSTSEC-2026-0349. Native recording
  saves accept bounded MP4/GIF bytes only from the trusted main view and require a user-chosen
  destination without granting general filesystem access to the webview.

## [0.6.0] - 2026-10-14

### Fixed

- Editor-extension services (VS Code, Cursor, Windsurf and similar, including remote
  `*.vscode-server` / `*.cursor-server` trees) that inherit an open workspace as their
  working directory are labelled as that editor's extension, not as the project's
  framework — in the list and on the service graph, even if a project leaked onto the
  entry. A Postgres tools process in a Next.js checkout is no longer
  "Next.js · seller-agent". Those listeners are protected and are not treated as stoppable
  dev servers. Installed Cargo helpers such as rust-analyzer are not labelled as the open
  Rust project's framework on Windows.

### Changed

- The graph view explains when services on the canvas aren't connected, offers All
  services from that empty state, and leaves External idle until there are outbound
  hosts. Service cards no longer repeat the title as a subtitle. Framework tiles include
  Ollama, Cursor and Windsurf.

- Details, agent cards, the Flow inspector, and graph legends can be hidden so Graph and
  Flow use the full canvas. `B` toggles the details pane; each view keeps a control to
  bring the panel or legend back. Choices persist.

- The desktop title bar groups commands, scan, history, schedules, remote, theme and
  settings. View tooltips sit below the control so they no longer cover the logo. Scheduled
  actions can start a shutdown or a reviewed service stop from the empty manager, instead of
  only explaining that those reviews exist elsewhere.

- Agent network facts name well-known debug and model-server ports (Node inspector, Chrome)
  DevTools, Ollama) from the port number as inferred hints, and from a detected framework as
  seen. Copilot CLI `--yolo`/`--allow-all`, Cursor Agent `--force`/`--approve-mcps`/`--trust`
  and Gemini `--approval-mode auto_edit` are reported from the process command line. MCP
  children started with unpinned `npx` are labelled as such. Agent settings files are still
  not read.

- README, the public guide and website metadata use Holdmap names. Page titles and
  descriptions are unique, include holdmap, and stay in the search snippet range. The
  homepage schema describes the free app without inventing ratings or a release version.

### Security

- Every native app command passes the trusted-main-webview policy before dispatch,
  including restart, settings, history and update installation.
- Activity metadata encoding rejects excess record counts before serialization and stops
  at the request byte limit, including escaped JSON, before opening a connection.
- HTTP status, headers and titles redact recognized credentials before clipping or export.
  URL fragments and namespaced recording labels also hide recognized credential forms.
- Settings updates and history appends preserve existing files when validation or reads
  fail, and checked restart lookups report those errors explicitly.

### Fixed

- Windows MSVC native test executables embed the Common Controls v6 manifest required
  by Tauri's mocked IPC tests, preventing a loader failure before the test harness starts.
- Closing the macOS main window hides the application through AppKit, returning from
  fullscreen to the previous app instead of leaving a black Space. Tray, shortcut and Dock
  reopening restore the same window and webview, including minimized windows. First-open
  guidance explains closing, reopening and Quit when the native tray is available.
- Desktop scan failures preserve the last successful rows while showing the failure and
  an accessible retry action. Searched-port availability is rechecked against updated scans
  and cannot keep showing a stale free result after a failed refresh. Socket-mode changes
  during an in-flight scan trigger a fresh request for the selected mode.
- Narrow browser windows keep the search field, every header action and scan-retry button
  within the viewport, including when macOS platform detection reserves titlebar space.
- HTTP health checks require complete headers and a final response, handle bounded body
  framing and stop reading as soon as a length-delimited response completes.
- Stops report process outcomes and history-save failures separately. CLI, desktop and MCP
  batches halt after recording failures; automatic restart requires a successful stop and
  saved history. Failed stop steps cannot appear successful merely because the port is free.
- Pending desktop stop/restart reviews show cancellable progress, discard cancelled or
  superseded replies and require another review when a selected listener changes identity.
- Unavailable settings show an explicit retry flow while platform information and scans
  remain usable.
- Agent service batches validate every reviewed listener against one fresh native scan
  before stopping anything. Retained plans correctly verify multiple ports released by a
  shared child process, and partial results preserve the actual attempted/completed counts.
  Identical successfully completed supervisor commands run once per batch while each
  listener is still verified and recorded separately. Immediate, scheduled and MCP batches
  share this rule in the core, retaining original reviewed plans for history.

### Performance

- Native settings, history and OS integration work run off the UI thread, including
  cross-process file-lock waits and durable writes.
- Numeric port checks keep at most one request in flight and one latest queued request,
  preventing slow checks from accumulating during live scans.

## [0.5.1] - 2026-10-13

### Security

- CLI and terminal process controls enforce project protection against every known
  process-tree, container and service effect, reject untrusted configuration, and refresh
  policy and ownership after confirmation.
- Restart requests retain protocol and process identity. TCP restart preserves unrelated
  UDP owners; ambiguous multi-owner recordings and unsafe legacy Windows shell recordings
  require the normal launcher instead of replaying a different command.
- Windows CMD syntax is retained privately for faithful restart and omitted from exports.
  Shell scripts, PowerShell encoded payloads and credential option variants are redacted
  without modifying the private execution arguments.
- Runtime framing, connection and I/O deadlines, configuration/history/log reads and worker
  concurrency are bounded. Private state transactions reject unsafe files and preserve
  existing data on failed writes.
- Configured signed updater artifacts and their authenticated release versions are verified
  before upload. One publisher produces the complete platform index after every build passes.

### Fixed

- Linux global shortcuts release every X11 key reservation immediately when disabled or
  changed. Failed registration rolls back its partial reservations, and unavailable X11
  workers report errors instead of success.
- Windows detached and quiet launches release caller output pipes while preserving normal
  inherited standard I/O. Project discovery resolves DOS short-name working directories
  within bounded shared worker limits.
- Docker responses buffered before peer closure are drained correctly on macOS without
  exposing the caller to broken-pipe signals.
- Desktop live scans and details preserve current owners, queued updates and listener cleanup.
  History supports duplicate rows, exact restart selection and recoverable action failures.
- Settings, pin, clipboard and history actions report failures and prevent duplicate pending
  work. Global shortcut changes preserve the previous working registration after a failure.
- Compact desktop controls and the website's mobile headline fit narrow viewports. Keyboard
  focus returns correctly after history confirmation and cancellation.
- Unix process fixtures use immutable executable aliases and static SSH scripts, eliminating
  writable-executable races while retaining real process-control and injection checks.

### Performance

- Service dependency ordering, graph relationships and highlighting use bounded or linear
  work. Large graphs use layered layout instead of expensive force layout.
- CLI log tails read only the bounded suffix, health probes limit parallel work, and the MCP
  tool catalog is initialized once.

## [0.5.0] - 2026-10-11

### Added

- Desktop folder actions from the Agents map and card paths, with mouse and
  Enter/Space activation, a full path, platform file-manager/editor choices, clipboard copy
  and focus restoration. Browser openers are explicitly labelled simulations.
- Calls diagnosis filters for failed records and missing completion, individual metadata search,
  chronological or longest-reported-duration ordering and retained reported parent context.
- A persisted desktop single-key shortcut setting, with modifier and arrow navigation preserved.

- Agent Flow separates observed process/socket relationships from opt-in local reported calls.
  Parallel tools, model calls, delegated agents, failures and missing data have a graph, timeline
  and inspector. Codex/Claude adapters and an optional MCP observer retain bounded metadata in
  memory; CLI setup, hook and snapshot commands use a token-authenticated loopback collector.

- Terminal Agents list with independent search, stable identity-based selection, responsive
  details, keyboard and mouse scrolling, explicit collection limits and stale-scan warnings.
  CLI agent output starts with a compact overview; filtered queries expand details.

- Desktop scheduling for local shutdown, individual ports and an agent's reviewed services,
  with presets, custom delays, local date/time selection, countdowns and confirmed cancellation.
  A native queue keeps schedules active in the tray while the app runs; quitting clears them.
  Delayed actions retain reviewed identities, reject changed targets and require fresh review
  after missed deadlines or significant clock changes. Browser schedules are simulations.

### Changed

- A routed H identity replaces the original socket icon across the desktop,
  website, favicons, native app icons and refreshed screenshots/videos. One vector master
  generates platform assets, including a monochrome macOS menu-bar template. Website
  tour chapters use the timings measured from the new recordings.
- Power settings separate **Shut down now** from **Schedule shutdown**;
  scheduled reviews start at one hour and exclude immediate execution. Local port details
  similarly offer separate **Stop** and **Schedule stop** actions. Browser labels identify
  simulations, and immediate reviews omit timing controls.
- Port details wrap tabs and long content at compact widths, with a clear port-scoped action
  footer. Shared select menus fit the viewport and retain readable option descriptions.
- Agent recording separates hook/OTLP parsing, shared metadata and credential policy, typed
  HTTP routing, MCP delivery and UI controls. Native commands share authorized session access;
  unknown recording status stays explicit until a successful refresh.
- The CLI reference includes nested activity commands and identifies version 0.5.0 availability.
  Public guides clarify opt-in recording, tray/quit behavior and process-protection limits;
  the homepage links local shutdown and scheduling guidance.

### Fixed

- Release notes use the attached npm package when registry publishing is disabled, and retain
  that accurate installation guidance after desktop installers are uploaded.
- File-manager actions open an existing folder's contents, including in Finder on macOS,
  and reject files or missing directories before launching an application.
- A missing explicitly configured `HOLDMAP_EDITOR` (or legacy `PORTWISE_EDITOR`) now returns
  an error instead of silently choosing another editor. Folder openers retain the chosen owner
  and exact path through UI polling and native fresh-scan validation.
- The website's macOS download button opens the Apple silicon and Intel choices instead
  of selecting an architecture from OS detection alone. Native Apple silicon guidance
  makes clear that Rosetta is unnecessary for that build.
- Terminal graph search filters and navigates the displayed graph, retaining selected node
  identity across refreshes. Mouse input and bracketed paste use paired terminal mode cleanup;
  recognized paste is bounded search text and never an action or confirmation.
- Desktop shortcuts respect composition, editable content and native controls. Port filters and
  stale selected-port palette commands no longer affect hidden port views while inspecting agents.
  Palette search has an accessible name and maintains a valid active result during live updates.
- Public website snapshot capture waits for visible images, so offscreen lazy images cannot
  stall mobile guide captures.

- Public guide screenshots render through Astro's content component with responsive sizing
  and descriptive alt text. Website checks reject unresolved image placeholders and verify
  that guide screenshots load at desktop and mobile widths.

### Security

- CLI and TUI agent service stops share retained, protocol-specific reviewed plans. All targets
  require fresh identity, ownership and protection checks before any effect. Confirmation
  displays every service, Enter cancels, and failures halt remaining steps without retries.
  Stop-history errors preserve completed-effect status. Agent sessions remain protected.

- Desktop release publishing uses the verified installer manifest for builds without a signed
  updater. Configured updater archives and signatures receive platform checksum and build
  provenance coverage. Matching signed updater assets retain their IDs so retry uploads
  preserve existing updater URLs.
- Global release artifacts, including source and npm archives, installers and SBOMs, receive
  build-provenance attestations alongside the platform archives and desktop installers.

## [0.4.0] - 2026-10-10

### Added

- Desktop Power settings with local computer identification, an explicit save-work
  acknowledgement, expiring one-use shutdown confirmations and a browser-only simulation.
  Native adapters preserve OS permissions and inhibitors without forced termination.

- Agent tool visibility across desktop, CLI and MCP: likely MCP server processes, development
  servers, shells and commands, including processes without listening ports. Tool identities
  carry evidence and commands remain redacted.
- Agent search by name, owned process PID, folder and tool, with CPU, memory and process totals
  on desktop cards and explicit omitted counts in reports.
- MCP discovery includes a bundled agent guide and prompts for port diagnosis, development
  server preparation and agent inspection. Tools publish structured output contracts.
- Repeatable browser and CLI/MCP integration suites, with CI coverage and a full local
  validation command at `scripts/check-all.sh`.

### Changed

- Agent discovery composes small detector and classifier interfaces; process ownership and
  network attribution have separate modules. CLI and MCP share the core query predicate.
- MCP routing, tool metadata, argument validation, live handlers and discovery content have
  separate responsibilities.
- CI checks both npm dependency trees, runs native desktop tests on all three platforms and
  keeps website dependencies up to date through Dependabot.
- MCP stop tools now preview by default and require the session-local `confirmation_id` from
  that preview for execution. IDs expire after five minutes, are one-use and bind the original
  target/options and effects. This breaking MCP protocol change requires clients upgrading
  from 0.3.0 to preview before executing; sessions retain at most 32 pending previews.
- Security gates now audit both npm lockfiles at all severities, scan full history/current
  source with Gitleaks, run CodeQL for JavaScript/TypeScript, Rust, Python and Actions, and
  recheck dependency advisories daily. Workflow actions use immutable commit pins.

### Fixed

- Agent identification no longer treats an ordinary command's file arguments as agent entry
  points. Process displays prioritize children over large groups of helpers.
- Agent network reports distinguish specific-interface binds, match local listeners by address
  and port, count remote hosts before clipping, and surface collection warnings.
- Desktop agent reports preserve refresh order, distinguish unknown access from standard
  access, and retain different relationships to the same graph node.
- Changing desktop views dismisses a previous port-details drawer while retaining the
  listener selection for List and Graph.
- MCP rejects malformed tool arguments before execution, including string values for stop
  flags, and formats discovery and results for the negotiated protocol version.
- MCP port results expose scan warnings, collection time and platform, including partial scans
  that return no listening ports.
- Native agent, topology and explanation commands refresh expired scans and invalidate the
  cache when Docker collection changes. Failed refreshes return an error instead of stale data.
- Agent helper ownership distinguishes Windows user accounts and keeps roots with unknown
  account identity separate. Process protection uses the same runtime entry detection as
  agent discovery.
- Current-account recent-project history is attached only to agents with a matching known
  account; other users' working directories still appear from live process metadata.
- Account descriptions and ownership now use the same UID-first match as recent history.
- Agent memory formatting retains fractional gigabytes without overflowing its size threshold.
- The website demo can return from Agents to List before inspecting or planning a simulated stop.
- Website release-triggered deployment ignores pull requests that only plan release artifacts.

### Security

- The Linux GTK3 dependency uses verified GLib 0.18.5 backports of upstream's
  `VariantStrIter` soundness fix (RUSTSEC-2024-0429) and boxed-inline slice allocation fix
  (upstream issue #2040), plus a separate local zero-initialization fix for `Value` slice
  copies. The original version and license remain; source/lockfile verification and optimized
  Linux regressions cover the local override.
  Cargo-deny now checks transitive soundness advisories, with no advisory ignores.

- Desktop stops retain the exact reviewed plan behind a one-use confirmation handle, with a
  five-minute lifetime and 128-preview bound. A fresh semantic check rejects changed owners,
  protocols or effects before execution. Agent bulk dialogs display every queued plan.
- MCP executing stops compare fresh effects with retained original plans; agent bulk stops
  validate all plans and the matching agent selection before any action. Agent port plans retain
  their TCP/UDP protocol, and pinned process effects require verified current-account ownership.
  Unknown, foreign or protected owners and unpinned systemd socket plans are refused.
- The shared executor rechecks process identity and hard/soft protection before first signal
  and escalation, retains explicit soft authorization, and guards supervisor owner identity.
- Supervisor targets cannot broaden an individual stop: PM2 requires a verified unique numeric
  app ID, Homebrew formula names reject option injection and systemd units reject glob patterns.
- Native folder actions force a fresh authorization scan; TCP/UDP row targets cannot fall
  through to process-name dispatch. Local-only capabilities and CSP remain narrowly scoped.
- State and CLI output operations reject symlink/reparse/hard-link redirection where applicable
  before reads or truncation. Unix state uses checked ownership and `0600`/`0700`; Windows
  privacy relies on inherited account/profile-directory ACLs, without a blanket owner-only claim.
  Stack files are checked and read through the same handle.
- Credential redaction covers authorization/cookie headers, quoted and whitespace-bearing
  values, encoded query keys and nested JSON. Human CLI output neutralizes external terminal
  controls and row injection while JSON metadata stays structured.
- MCP rejects frames larger than 1 MiB and legacy batches larger than 64 messages before
  dispatch; an oversized frame is drained completely so the next request remains separate.
- HTTP health paths reject request/header injection. Command output is capped at 4 MiB per
  stream with inherited-pipe deadlines, and SSH destinations/arguments use validated, quoted
  command boundaries.
- Release tags are validated as literal versions before planning; release planning and build
  jobs use read-only permissions, with publishing authority scoped to publication jobs.

### Documentation

- README and architecture diagrams describe the shared native/browser UI, adapter responsibilities,
  concrete SOLID interfaces and feature availability by version.
- Desktop, CLI/TUI and website screenshots have repeatable capture tooling; README images,
  website crops, hero posters and the social card are refreshed from current source.

## [0.3.0] - 2026-10-09

### Changed

- **Renamed from portwise to holdmap.** The product maps agents, tools and apps — not only ports.
  Binary, crates, config dir (`~/.config/holdmap`), project file (`.holdmap.toml`), env vars
  (`HOLDMAP_*`), MCP server name, desktop bundle id (`dev.holdmap.app`) and site base path all
  use the new name. Legacy env aliases (`PORTWISE_HOME`, `PORTWISE_TRACE`, `PORTWISE_EDITOR`,
  `PORTWISE_COLOR`), a leftover `portwise` config directory (migrated once), and
  `.portwise.toml` project files are still accepted. Installers and release artifacts are named
  `holdmap-*`.

### Added

- **Control from the Agents surface.** Reveal a folder, open it in your editor, or stop the
  unprotected ports an agent started (desktop cards, `holdmap agents --stop-ports`, MCP
  `stop_agent_ports`). Paths are verified against the current agents report before opening.
- **Developer tools in the catalog.** Docker Desktop, OrbStack and Podman Desktop appear beside
  AI coding agents, with the same folders / ports / access footprint.
- **TUI Agents tab.** Cycle Ports → Graph → Agents with Tab; browse each agent's footprint.
- **Site and docs** reframe the problem around agents, tools and apps — not only ports. Marketing
  pages, SEO copy, Interfaces, Features (agents screenshot), Footer and a new
  [Agents guide](https://santoshshinde2012.github.io/holdmap/docs/agents/) cover control, richer
  skim and developer tools.

### Changed

- Agents empty states, onboarding, shortcuts and MCP copy talk about tools and apps as well as
  coding agents. Desktop hides port filters while the Agents tab is open and prefetches the badge
  count.
- **Richer agent skim surfaces:** memory, CPU, process count, stoppable count and parent on cards /
  graph nodes / CLI / TUI / MCP text; child processes grouped as “tools & apps it started”; folder
  source labels (working dir / child cwd / recent); TUI `x` stops an agent’s ports; demo includes
  Docker Desktop.
- Product copy (CLI `--help`, desktop store metadata, README architecture, site CLI demo) aligns
  with agents and tools; removed a duplicate desktop `icon.svg`.

## [0.2.0] - 2026-10-09

### Added

- **AI coding agents, end to end.** holdmap now finds the coding agents running on your machine
  (Claude Code, Codex, Cursor, Copilot, Gemini CLI, Windsurf, Aider, Cline and others) and shows,
  for each one, the folders it works in, the ports it serves, the services and hosts it talks to,
  and its access: account, sandbox, approval flags, network listeners and macOS-protected
  folders. Each fact is marked seen, inferred or unknown. Chats, settings and tokens are never
  read.
  - **Desktop: Agents view** (`⇧A`): a card per agent next to a map of their shared footprint.
    Click a port to open its details.
  - **CLI: `holdmap agents [AGENT] [-w] [--json]`.**
  - **MCP: `list_agents`**, read-only.
- **Website** at <https://santoshshinde2012.github.io/holdmap/> with a live, in-browser demo of the
  desktop app, including the Agents view with sample agents, install steps for every platform and
  a short guide. It is built from `site/` and deployed by the Pages workflow.

## [0.1.9] - 2026-10-08

### Fixed

- **Desktop: Force layout no longer looks empty** when services have no links between them. Unlinked
  nodes were pushed thousands of pixels apart; a gentle pull now keeps them in view.
- **Release notes no longer show a `brew install` line** that didn't work yet. Homebrew returns
  once the tap is published.

## [0.1.8] - 2026-10-08

### Fixed

- **Desktop: Graph view no longer rebuilds the topology in a loop.** It re-fetched the graph as soon
  as the previous one arrived, costing CPU while Graph was open.

### Changed

- README and release screenshots show the current UI (light and dark).

## [0.1.7] - 2026-10-05

### Fixed

- **macOS desktop release checksums:** installer re-upload no longer uses `mapfile` (bash 3.2
  on macOS runners), so `holdmap-desktop-macos-*.sha256` can be published after the DMG.
- **Windows desktop release:** NSIS `setup.exe` and `holdmap-desktop-windows-x64.sha256` no longer drop
  when the upload flaked after MSI. Collect runs whenever Tauri produced output, and installer/
  checksum uploads retry with longer timeouts.

### Performance

- Watch path borrows the snapshot (one fewer clone per scan).
- Desktop skips Live pushes while minimized and uses the slow cadence when minimized/occluded.
- Graph / Svelte Flow loads only when you open Graph (~188 KB JS deferred from startup).
- Live list updates coalesce onto `requestAnimationFrame`.

### Measured (before → after where applicable)

- CLI `list` / scan already ~40 ms / ~32 ms on Mac (unchanged; no scan rewrite).
- Vite production: GraphView split into its own chunk (`GraphView-*.js` ~188 KB / ~60 KB gzip);
  initial `index-*.js` no longer embeds the graph.
- Slimming IPC by dropping cmdline/exe skipped (would empty Process details without a larger API).
- List virtualization and font subsetting skipped (short lists; font risk).

## [0.1.6] - 2026-10-05

### Fixed

- **Desktop: blank white screen after long runs.** An uncaught error could leave the WebView
  empty (only the native title bar). The app now remounts and tells you why. Explain/HTTP/
  details caches and sparkline samples have hard size caps, history is dropped while the
  window is hidden, and the service graph is only built in Graph view.

### Added

- **Accurate memory.** macOS uses Activity Monitor's `phys_footprint`, Linux `VmRSS`, Windows
  Working set. Each port shows the app total (process + helpers) with a helper count in the
  list, details, graph, CLI, JSON and MCP. Heavy apps get a memory badge; sort-by-memory uses
  the app total; a copy-only quit-helpers command appears when safe.

## [0.1.5] - 2026-10-04

### Fixed

- **Desktop (macOS): the list no longer freezes behind the Documents privacy prompt.** Project
  detection read files in each process's folder during the scan, and macOS holds the first read
  under `~/Documents`, `~/Desktop` or `~/Downloads` until the prompt is answered, so the header
  counted up instead of staying Live. Project lookups now run in the background with a short
  budget and a per-folder cache; project and branch fill in when ready and stay put while they
  refresh. If access is denied or a read hangs, the row just has no project.
- **Desktop (macOS)**: the privacy prompt now says why holdmap reads those folders.

## [0.1.4] - 2026-10-04

### Added

- **Details pane**: who is connected (local apps by name, remote peers, counts), the full
  process tree, start time, CPU and memory trend lines, and the bind address explained in plain
  words with a fix. Loaded lazily for the selected port; nothing flashes while it refreshes.
- **Quick actions**: Restart (a confirmed stop, then the same command in the same folder), open
  the folder in your editor or Finder, copy the URL, a `curl` or the kill command. Force kill moved
  into this menu (`⇧⌫` still works).
- `holdmap inspect` and MCP `explain_port` show the same connections, process tree, start time,
  bind risk and HTTP status (`details` and `http` in the JSON).

### Fixed

- **Desktop**: a service restarted from the app (or its history) was listed as "Protected" and
  couldn't be stopped again, because it counted as part of holdmap's own process tree.

### Security

- Secrets in command lines are hidden everywhere: CLI, TUI, JSON, MCP, the app and history views.
  The history file keeps the real command (so restart works) and is now `0600`; logs too.
- Control characters from processes, manifests and local web pages are stripped before printing,
  so they can't inject terminal escape sequences.
- Desktop: the webview may only listen for events and drag the window, under a stricter CSP
  (`script-src 'self'`; `object-src`, `base-uri`, `frame-ancestors` and `form-action` `'none'`).
  Restart names a history entry and the backend reads the command from its own file; the opener
  plugin is gone (URLs and folders are looked up by port from the scan).
- URLs are opened without `cmd.exe` on Windows and only when they're plain `http(s)`.
- A `.holdmap.toml` owned by another user or writable by anyone is refused.
- MCP: `stop_port` says to confirm with the user first; protected processes stay refused even if
  an agent passes extra arguments (tested).

## [0.1.3] - 2026-10-04

### Fixed

- **Desktop: the header no longer drops to "12s ago" on macOS.** The window's own poll timer
  can be throttled or paused by macOS (WebKit, App Nap, covered windows), and the age counted
  from the last snapshot even while a scan was running. The app's watcher now pushes every
  scan to the window, the window catches up as soon as it's focused or uncovered, and "Live"
  holds while a scan is in flight.
- **Desktop**: scans never overlap; the window, the watcher and the tray share one scan per
  interval (previously up to two). The scan no longer waits for the tray menu to update.

### Changed

- **Core**: the user list (a directory-service query on macOS) is cached for a minute instead
  of being re-read on every scan.
- `HOLDMAP_TRACE=scan` prints how long each part of a scan took (sockets, processes,
  containers), for the CLI and the desktop app.

## [0.1.2] - 2026-10-04

### Fixed

- **Desktop: the details pane no longer blinks.** Every scan (every 3 s) threw away the selected
  port's explanation, so the Overview dropped to its loading skeleton, the stop plan vanished and
  the tab counts and footer reset until it reloaded. Details now stay on screen and refresh in
  the background; the skeleton only shows the first time a port is opened.
- **Desktop**: polls reuse unchanged data, so rows, the graph and sparklines only update what
  changed (about half the idle CPU with 400 ports); the pane keeps its scroll position across
  scans; background scans don't spin the refresh button; the "Live" label no longer flickers to
  "4s ago" between scans; the HTTP status is re-checked every 15 s instead of never; the tray
  menu is only rebuilt when its content changes, so an open menu isn't closed by a scan.

## [0.1.1] - 2026-10-04

### Fixed

- **macOS desktop app "is damaged and can't be opened"**: the app is now ad-hoc signed as a whole
  bundle (v0.1.0 carried only the linker's signature on the binary, which Gatekeeper rejects once
  a download is quarantined). macOS now shows the usual one-time "Open Anyway" prompt; every
  release checks the app inside each .dmg.
- **Installer**: the CLI installs to `~/.local/bin` (was `~/.cargo/bin`), so it works at once
  where that folder is already on `PATH`. Coming from 0.1.0, delete `~/.cargo/bin/holdmap`.
- **CLI**: `stop`/`kill` reject port 0 and ports above 65535; `up` keeps starting services that
  don't depend on one that failed; `unpin` of an unpinned port says so; `run` explains why it
  didn't stop the holder; hidden-listener counts match the rows shown; `ssh` only asks for a
  remote TTY when interactive, so piped and `--json` output stay clean.
- **TUI**: no panic without a terminal (clear message, exit 2); section rules fit the panel
  width; header stats drop whole items on narrow terminals; "1 port", not "1 ports".
- **Core**: sockets with no local port are hidden; full Linux process names past 15 bytes;
  retitled processes no longer show their environment as the command line; multi-line `sh -c`
  scripts aren't dev-server launchers; container runtimes' own ports aren't dev servers; command
  output can't deadlock; correct plurals everywhere.
- **MCP**: `stop_port` is recorded in the history; `list_ports` counts hidden listeners properly.
- **Release**: bare file names in the Windows desktop checksum file; the desktop build checks out
  the release tag through dist's `workflow_call` (CodeQL alert fixed).

### Changed

- **Desktop design pass**: an "At a glance" summary in the empty details pane; details facts in
  one divided card; the command palette keeps its sections while searching, with quieter
  destructive icons; the stop confirmation names its target ("Stop :3000"); WCAG AA contrast for
  faint text on selected rows; keyboard focus stays in the list after the welcome banner; a
  distinct "match system" theme icon; the Graph badge reads "6 links" to screen readers.
- **Desktop graph**: readable faded nodes and motion-token transitions; edge labels fit between
  their nodes; the graph fits below its toolbar and legend; the minimap only appears when part of
  the graph is off-screen, translucent until hovered.

## [0.1.0] - 2026-10-03

The first public release.

### Added

- **Core** (`holdmap-core`): socket → PID enumeration for Linux (`/proc`), macOS (libproc) and
  Windows (IP Helper); process trees; project, framework and git-branch detection; Docker,
  Podman, OrbStack and Colima container mapping; plain-English `explain`; previewable stop plans
  with graceful tree stop, SIGKILL escalation, a PID-reuse guard and port-free verification;
  supervisor awareness (systemd, pm2, Homebrew services); OS-feature explanations (AirPlay
  Receiver, HTTP.sys, excluded port ranges); one protection rule for every surface (holdmap's own
  tree, shells, terminals, IDEs and AI-agent hosts).
- **Service graph**: who talks to whom over local connections, grouped into clusters (Compose,
  Kubernetes namespace, pm2/turbo/nx, workspace, git repo), with tunnels and port-forwards, and a
  dependency-ordered cluster stop. Exported as a tree, JSON, DOT or Mermaid.
- **Project stacks**: a `.holdmap.toml` with `holdmap up`, `down`, `status` and `init`. Services
  start in dependency order and are waited on; a different program on a project port is reported
  as a conflict (`--replace` stops it after confirmation); `protect` ports are never stopped
  without `--allow-protected`.
- **HTTP probe**: `list --http`, `inspect`, `status` and the desktop details pane show each web
  port's status code and page title.
- **Stop all dev servers**: `stop --all-dev`, the tray menu and the desktop command palette stop
  every dev server of yours in one confirmed plan.
- **Shell hook**: `holdmap init zsh|bash|fish|pwsh` explains a port-in-use failure after a
  dev-server command and prints the command that frees it.
- **CLI**: `list`, `inspect`, `explain`, `stop`, `kill`, `free-port`, `wait`, `run -p`, `graph`,
  `watch`, `pin`/`unpin`/`pins`, `history`, `restart`, `open`, `ssh HOST` (read-only remote
  inspection), `completions` and `man`, with `--json` everywhere and documented exit codes.
- **TUI**: search, filters, sorting, a details pane, a graph tab, confirmed stop and cluster stop.
- **MCP server**: `list_ports`, `explain_port`, `find_free_port`, `wait_for_port`, `stop_port`,
  `get_topology` and `plan_cluster_stop`.
- **Desktop and tray app** (Tauri v2, Svelte 5): a grouped live list, details pane, stop flow with
  live progress, service graph, pins, history with restart, remote hosts over SSH, a command
  palette, settings, notifications for new and conflicting listeners, launch at login and a
  global shortcut.

### Performance

- A warm scan takes about 19 ms on Linux (was 60 ms): process refresh no longer walks every
  thread's `/proc/<pid>/task` entry.

[Unreleased]: https://github.com/santoshshinde2012/holdmap/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/santoshshinde2012/holdmap/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/santoshshinde2012/holdmap/compare/v0.7.1...v0.8.0
[0.7.1]: https://github.com/santoshshinde2012/holdmap/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/santoshshinde2012/holdmap/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/santoshshinde2012/holdmap/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/santoshshinde2012/holdmap/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/santoshshinde2012/holdmap/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/santoshshinde2012/holdmap/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/santoshshinde2012/holdmap/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.9...v0.2.0
[0.1.9]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/santoshshinde2012/holdmap/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/santoshshinde2012/holdmap/releases/tag/v0.1.0

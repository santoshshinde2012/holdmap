---
title: Troubleshooting
description: Fix holdmap install, PATH, hidden ports, desktop first-open and uninstall issues on macOS, Linux and Windows.
order: 7
---

**`command not found: holdmap` right after installing.** Open a new terminal, or use the PATH
command printed by the installer. The default macOS/Linux shell setup uses
`source ~/.config/holdmap/env.sh` (a custom `XDG_CONFIG_HOME` or install location can change it).
On Windows, open a new PowerShell window and check `Get-Command holdmap`; `source` is a Unix-shell command.

**An old copy runs instead of the new one.** Check `command -v holdmap` on macOS/Linux or
`Get-Command holdmap` on Windows, then `holdmap --version`. A Cargo-installed copy in `~/.cargo/bin`
can shadow the installer's `~/.local/bin` copy. Update or remove the old copy at the path you
identified. Very old installations used the name `portwise`; those are separate binaries.

**`Permission denied` on a shell startup file.** An old `sudo` left it owned by root. Run
`sudo chown "$USER" ~/.bash_profile` and install again. Never run the installer with `sudo`.

**Other users' ports show as hidden.** Run with `sudo` (an elevated terminal on Windows) to see
their owners.

**The list is slow or looks stale.** `HOLDMAP_TRACE=scan holdmap list` prints how long each part
of a scan took (sockets, processes, containers). `--no-docker` skips container lookups.

**The desktop app won't open.** It isn't notarised yet; see [first open](../desktop/#first-open).

**The download has the wrong architecture.** Apple silicon Macs use `aarch64`; Intel Macs use
`x64`. Current Windows and Linux desktop installers are x64. Linux ARM64 has a CLI archive,
but no prebuilt desktop installer. Choose from [the platform downloads](../../#install).

**The desktop still shows an older version.** Check **Settings → About** in that app,
rather than `holdmap --version`. About also shows a copyable running-app location and
architecture, with actions for the official desktop downloads and installation guide.
If automatic updates are unavailable, quit Holdmap, install the current release for the same
OS and architecture, then reopen the installed copy.
An app launched from an older disk image or another folder can still be the old version.
The CLI installer updates the CLI, TUI and MCP server; it does not update the separate desktop app.
Browser About identifies the web preview separately and does not show a desktop settings folder.

**Record screen says “Screen capture unavailable” in the macOS app.** The native window
capture needs **macOS 14 or later**. On older macOS versions, try a browser with screen
sharing or the system screen recorder. If capture is denied, check this app's Screen Recording
permission in System Settings. “Holdmap can only record its own window” means the window is
minimized or hidden: bring it forward and try again. Holdmap never records anything else. See [screen recording](../desktop/#record-the-screen).

**The MCP client can't start holdmap.** Use the full path in its config, see
[MCP setup](../mcp/#set-it-up).

**MCP stops now require `confirmation_id`.** Current servers preview before executing.
Keep the same server session, show every plan, then send the returned one-use handle with
`dry_run: false` and identical target/options. See [stop confirmation](../mcp/#preview-and-execute-a-stop).

**Calls recording stays empty.** A running agent or socket connection does not expose calls.
Start recording, configure a supported exporter or `holdmap mcp --record-activity`, and use
the endpoint and token for that same active collector. New recording sessions have new tokens.
See [recording setup](../agents/#set-up-optional-recording).

**MCP or skills are missing.** Explicitly open
**Agents → MCP & skills**, pass `agents --capabilities`, press **c** in the terminal Agents pane,
or call MCP `list_agents` with `include_capabilities: true`. Read the coverage limits; custom
homes and client policy are not resolved. Use matching collector and CLI/desktop builds when
reading skill activations or reported MCP aliases.
See [supported definitions and troubleshooting](../agents/#mcp-and-skills).

**Environment variables.**

| Variable | Purpose |
|---|---|
| `HOLDMAP_HOME` | Config and history folder (overrides the platform default) |
| `HOLDMAP_COLOR` | `auto`, `always` or `never` (also `--color`) |
| `HOLDMAP_TRACE` | Diagnostics topics, such as `scan` or `all` |
| `HOLDMAP_EDITOR` | Editor executable for folder actions |
| `HOLDMAP_ACTIVITY_ENDPOINT`, `HOLDMAP_ACTIVITY_TOKEN` | The active local recording collector; keep the token private |

Project stacks use `.holdmap.toml`.

**Uninstall.** Remove the binary at the path reported above (the default is `~/.local/bin/holdmap`
on macOS/Linux or `%USERPROFILE%\.local\bin\holdmap.exe` on Windows), remove the installer's
PATH setup, and uninstall the desktop app through your OS. Settings and restart history remain
in `~/.config/holdmap` (macOS: `~/Library/Application Support/holdmap`, Windows: `%APPDATA%\holdmap`),
or your custom `HOLDMAP_HOME`. Delete that directory separately if you want to clear them.
History contains raw launch commands; inspect it before sharing diagnostics.

Still stuck? [Open an issue](https://github.com/santoshshinde2012/holdmap/issues) with the output
of `holdmap --version` and the command you ran.

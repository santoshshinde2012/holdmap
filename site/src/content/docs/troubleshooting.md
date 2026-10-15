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

**The desktop still shows an older version.** Version **0.7.1** requires a manual update.
Quit Holdmap, install the current release for the same OS and architecture, then reopen
the installed copy. Check **Settings → About** in that app, rather than `holdmap --version`.
An app launched from an older disk image or another folder can still be the old version.
The CLI installer updates the CLI, TUI and MCP server; it does not update the separate desktop app.
No signed updater feed is included in 0.7.1.

Current source adds a copyable running-app location, architecture and desktop download
actions to **Settings → About**. Browser About identifies the web preview separately and does
not show a desktop settings folder.

**Record screen says “Screen capture unavailable” in the macOS app.** Version **0.7.1**
adds Apple's native picker when the desktop webview lacks screen sharing, requiring
**macOS 14 or later**. The 0.7.0 installer predates that fallback. Download the **0.7.1 or
newer desktop installer** from [Install](../../#install), install it and reopen the app;
updating the CLI does not update it. On older macOS versions,
try a browser with screen sharing or the system screen recorder. If the picker opens but
capture is denied, check this app's Screen Recording permission in System Settings.
See [screen recording](../desktop/#record-the-screen).

**The MCP client can't start holdmap.** Use the full path in its config, see
[MCP setup](../mcp/#set-it-up).

**MCP stops now require `confirmation_id`.** Current servers preview before executing.
Keep the same server session, show every plan, then send the returned one-use handle with
`dry_run: false` and identical target/options. Version 0.3.0 used the earlier protocol; follow
the installed server's discovered schema. See [stop confirmation](../mcp/#preview-and-execute-a-stop).

**Calls recording stays empty.** A running agent or socket connection does not expose calls.
Start recording, configure a supported exporter or `holdmap mcp --record-activity`, and use
the endpoint and token for that same active collector. New recording sessions have new tokens.
See [recording setup](../agents/#set-up-optional-recording).

**Environment variables.** `HOLDMAP_HOME` (config dir), `HOLDMAP_COLOR`, `HOLDMAP_TRACE`,
`HOLDMAP_EDITOR`, plus `HOLDMAP_ACTIVITY_ENDPOINT` and private `HOLDMAP_ACTIVITY_TOKEN` for
explicit local recording integrations. Project stacks use `.holdmap.toml`.

**Uninstall.** Remove the binary at the path reported above (the default is `~/.local/bin/holdmap`
on macOS/Linux or `%USERPROFILE%\.local\bin\holdmap.exe` on Windows), remove the installer's
PATH setup, and uninstall the desktop app through your OS. Settings and restart history remain
in `~/.config/holdmap` (macOS: `~/Library/Application Support/holdmap`, Windows: `%APPDATA%\holdmap`),
or your custom `HOLDMAP_HOME`. Delete that directory separately if you want to clear them.
History contains raw launch commands; inspect it before sharing diagnostics.

Still stuck? [Open an issue](https://github.com/santoshshinde2012/holdmap/issues) with the output
of `holdmap --version` and the command you ran.

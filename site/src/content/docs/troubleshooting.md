---
title: Troubleshooting
description: Fixes for the common snags, from "command not found" to hidden ports and uninstalling.
order: 7
---

**`command not found: holdmap` right after installing.** Open a new terminal, or run
`source ~/.config/holdmap/env.sh`.

**An old copy runs instead of the new one.** Copies from v0.1.0 live in `~/.cargo/bin`. Remove it
with `rm ~/.cargo/bin/holdmap`; `command -v holdmap` shows which one runs.

**`Permission denied` on a shell startup file.** An old `sudo` left it owned by root. Run
`sudo chown "$USER" ~/.bash_profile` and install again. Never run the installer with `sudo`.

**Other users' ports show as hidden.** Run with `sudo` (an elevated terminal on Windows) to see
their owners.

**The list is slow or looks stale.** `HOLDMAP_TRACE=scan holdmap list` prints how long each part
of a scan took (sockets, processes, containers). `--no-docker` skips container lookups.

**The desktop app won't open.** It isn't notarised yet; see [first open](../desktop/#first-open).

**The MCP client can't start holdmap.** Use the full path in its config, see
[MCP setup](../mcp/#set-it-up).

**Uninstall.** `rm ~/.local/bin/holdmap`, remove the `env.sh` line the installer added to your
shell startup files, and delete the app. Settings and history live in `~/.config/holdmap` (macOS:
`~/Library/Application Support/holdmap`, Windows: `%APPDATA%\holdmap`).

Still stuck? [Open an issue](https://github.com/santoshshinde2012/holdmap/issues) with the output
of `holdmap --version` and the command you ran.

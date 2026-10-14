---
title: Troubleshooting
description: Fix holdmap install, PATH, hidden ports, desktop first-open and uninstall issues on macOS, Linux and Windows.
order: 7
---

**`command not found: holdmap` right after installing.** Open a new terminal, or run
`source ~/.config/holdmap/env.sh`.

**An old copy runs instead of the new one.** Copies from v0.1.0 live in `~/.cargo/bin`. If
`command -v` still names an older `portwise` binary, remove `~/.local/bin/portwise` and
`~/.cargo/bin/portwise`. Then `command -v holdmap` shows which binary runs.

**`Permission denied` on a shell startup file.** An old `sudo` left it owned by root. Run
`sudo chown "$USER" ~/.bash_profile` and install again. Never run the installer with `sudo`.

**Other users' ports show as hidden.** Run with `sudo` (an elevated terminal on Windows) to see
their owners.

**The list is slow or looks stale.** `HOLDMAP_TRACE=scan holdmap list` prints how long each part
of a scan took (sockets, processes, containers). `--no-docker` skips container lookups.

**The desktop app won't open.** It isn't notarised yet; see [first open](../desktop/#first-open).

**The MCP client can't start holdmap.** Use the full path in its config, see
[MCP setup](../mcp/#set-it-up).

**Environment variables.** `HOLDMAP_HOME` (config dir), `HOLDMAP_COLOR`, `HOLDMAP_TRACE`,
`HOLDMAP_EDITOR`. Project stacks use `.holdmap.toml`.

**Uninstall.** `rm ~/.local/bin/holdmap`, remove the `env.sh` line the installer added to your
shell startup files, and delete the app. Settings and history live in `~/.config/holdmap` (macOS:
`~/Library/Application Support/holdmap`, Windows: `%APPDATA%\holdmap`).

Still stuck? [Open an issue](https://github.com/santoshshinde2012/holdmap/issues) with the output
of `holdmap --version` and the command you ran.

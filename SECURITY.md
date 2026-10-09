# Security policy

holdmap inspects other processes and can stop them, so security reports are handled first.

## Reporting a vulnerability

**Please don't open a public issue.** Report it privately through GitHub's
[private vulnerability reporting](https://github.com/santoshshinde2012/holdmap/security/advisories/new)
(the **Security** tab → **Report a vulnerability**). Include the holdmap version
(`holdmap --version`), your OS, the affected surface (CLI, TUI, desktop app or MCP server), the
steps to reproduce and the impact. Fixes for the latest release and `main` are published as soon
as they're ready, with credit unless you'd rather stay anonymous.

## Scope

Anything that breaks the [safety model](README.md#safety-model) is in scope: holdmap signalling a
process it shouldn't (a protected process, another user's process, the wrong process after PID
reuse, or anything outside the confirmed plan), the MCP server stopping something it must refuse,
command injection (for example through `holdmap ssh` host names or restart commands from history),
leaking data beyond what the user asked to see (for example a password from a command line
showing up unredacted), or the desktop webview reaching anything beyond its two permissions
(listening to events, dragging the window). What a user can already do with their own
permissions is out of scope.

## What holdmap does to stay safe

- No telemetry. Network use is limited to localhost probes, `holdmap ssh` to hosts you name,
  and the desktop update check against GitHub Releases.
- Command lines are redacted for display and export; the stop history keeps the real command
  for restarts in a file only you can read (`0600`, directory `0700`).
- Commands are spawned with argument arrays, never through a shell, except the `command =` you
  write in your own `.holdmap.toml`, which is only read when you own it and others can't write it.
- Releases are built by GitHub Actions with pinned action SHAs and least-privilege tokens, with
  SHA-256 checksums and build-provenance attestations for every file
  (`gh attestation verify <file> -R santoshshinde2012/holdmap`).

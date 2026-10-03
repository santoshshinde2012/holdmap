# Security policy

portwise inspects other processes and can stop them, so security reports are handled first.

## Reporting a vulnerability

**Please don't open a public issue.** Report it privately through GitHub's
[private vulnerability reporting](https://github.com/santoshshinde2012/portwise/security/advisories/new)
(the **Security** tab → **Report a vulnerability**). Include the portwise version
(`portwise --version`), your OS, the affected surface (CLI, TUI, desktop app or MCP server), the
steps to reproduce and the impact. Fixes for the latest release and `main` are published as soon
as they're ready, with credit unless you'd rather stay anonymous.

## Scope

Anything that breaks the [safety model](README.md#safety-model) is in scope: portwise signalling a
process it shouldn't (a protected process, another user's process, the wrong process after PID
reuse, or anything outside the confirmed plan), the MCP server stopping something it must refuse,
command injection (for example through `portwise ssh` host names or restart commands from history),
or leaking data beyond what the user asked to see. What a user can already do with their own
permissions is out of scope.

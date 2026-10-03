# Security policy

portwise inspects other processes and can stop them, so security reports are taken seriously and
handled before anything else.

## Supported versions

| Version | Supported |
|---|---|
| 0.1.x (latest release and `main`) | ✅ |
| Older | ❌ |

## Reporting a vulnerability

**Please don't open a public issue.** Report it privately through GitHub's
[private vulnerability reporting](https://github.com/santoshshinde/portwise/security/advisories/new)
(the repository's **Security** tab → **Report a vulnerability**).

Include what you can of:

- the portwise version (`portwise --version`), the OS and how portwise was installed;
- which surface is affected (CLI, TUI, desktop app, MCP server);
- the steps to reproduce, and what you expected to happen;
- the impact as you understand it.

You can expect an acknowledgement within 3 working days and an assessment within 10. Fixes are
released as soon as they're ready, with credit in the changelog and the advisory unless you'd
rather stay anonymous.

## What counts

Anything that breaks the [safety model](README.md#safety-model) is in scope, for example:

- portwise signalling a process it shouldn't: a protected process, another user's process, the
  wrong process after PID reuse, or anything outside the confirmed plan;
- the MCP server stopping something its rules say it must refuse, or a tool argument that changes
  what gets executed;
- command or option injection, for example through `portwise ssh` host names, restart commands
  from history, or values read from the system;
- the desktop app's IPC commands being reachable from content they shouldn't be;
- reading or leaking data beyond what the user asked portwise to show.

Out of scope: what a user can already do with their own permissions (portwise run as root can stop
root's processes; that's expected), and denial of service from a process that floods the system
with sockets.

## How portwise limits the damage

- It runs unprivileged and never elevates itself.
- Every stop is a previewable plan, re-checked against each process's start time just before
  signalling (with pidfd on Linux).
- Protection rules live in one place in `portwise-core` and apply to every surface.
- It has no telemetry and no network access beyond the local container runtime socket and SSH
  connections the user starts.

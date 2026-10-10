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

Local machine shutdown in current desktop source is also in scope. It requires a trusted
main webview and a one-use confirmation that expires after 60 seconds. The backend uses
fixed OS commands without shell interpolation, force flags, elevation or remote targets.
Linux requires systemd support for explicit inhibitor checks (248 or newer). macOS may
request Automation permission. Windows uses the OS system directory and a zero timeout
without the force flag. Unsaved work can still be lost; users must save before confirming.
An accepted or uncertain request prevents further shutdown requests until the app fully
quits and reopens. Closing the dialog does not cancel an accepted OS request. The public
browser demo has no native shutdown capability.

Anything that breaks the [safety model](README.md#safety-model) is in scope: holdmap signalling a
process it shouldn't (a protected process, another user's process, the wrong process after PID
reuse, or anything outside the confirmed plan), the MCP server stopping something it must refuse,
command injection (for example through `holdmap ssh` host names or restart commands from history),
leaking data beyond what the user asked to see (for example a password from a command line
showing up unredacted), or the desktop webview bypassing native target checks or its local-only
capabilities. The main window has three capability permissions: event listen/unlisten and
window dragging. Its own native commands separately validate operations such as stops,
restarts and opening known folders. What a user can already do with their own permissions is
out of scope.

## What holdmap does to stay safe

- Stop plans retain their original effects. Desktop confirmation handles are one-use, expire
  after five minutes and are limited to 128 pending previews. Execution binds the target and
  options to that preview, compares a fresh plan with `ActionPlan::same_effects`, and rejects
  changed owners or effects. Agent bulk dialogs preview every queued plan.
- The shared executor checks process identity and current hard/soft protection before the
  initial signal and escalation. Soft protection requires an explicit authorization carried
  in the plan; hard protection cannot be overridden. Supervisor commands recheck current
  owner protection too, and targets use verified PM2 IDs or validated exact service names.
- MCP executing stops require a session-local, one-use `confirmation_id` from a dry-run preview
  and unchanged target/options and effects. Stop tools preview by default; the session retains
  at most 32 previews for five minutes. Bulk agent stops validate all captured plans and the
  matching agent selection before any action. Agent ports are scoped to their TCP/UDP protocol.
  Process and supervisor effects require verified current-account ownership; unknown or foreign
  accounts are refused even when a supervisor plan otherwise appears low risk. Handles bind
  effects rather than prove human
  consent; the MCP client remains responsible for obtaining authorization. MCP has no
  protected-process override.
- Command and error displays conservatively redact recognized credentials, including URL
  credentials, encoded query keys, authorization/cookie headers and nested JSON values. Raw
  commands remain available only where execution needs them, including restart history.
  Human terminal output makes untrusted escape/control sequences inert; JSON retains its
  structured metadata.
- History/config reads and writes check regular files and reject link redirection. Unix state
  directories/files use `0700`/`0600`; reads require account ownership and reject files writable
  by other accounts. Writes use exclusive temporary files and directory-relative operations.
  Windows rejects final reparse points and extra hard links, but inherits the chosen directory's
  ACLs: it does not install or verify an account-exclusive DACL. Use an account-controlled
  profile directory or a private `HOLDMAP_HOME`.
- Commands use argument arrays except explicitly configured project shell commands. Stack
  files are checked and read through the same handle. On Unix they must belong to the current
  account or root and must not be group/world writable; Windows relies on the checkout's ACLs.
  CLI project-file creation and forced exports reject link redirection before truncation.
- The desktop's strict CSP and local-only capabilities constrain the webview. Native openers
  construct localhost URLs or resolve folders through a fresh scan, and agent paths must belong
  to the current report. Untrusted process and HTTP text renders as text, not HTML. The website
  demo uses sample data through the same UI and has no native process access.
- MCP stdio frames are limited to 1 MiB excluding the newline; legacy batches are limited to
  64 messages. Oversized input is rejected before dispatch, and an oversized frame is drained
  completely so it cannot become a later valid request. HTTP probe paths reject request/header
  injection; captured command output is bounded to 4 MiB per stream with a deadline.
- No telemetry is implemented. Holdmap's own network operations are localhost probes, SSH to
  named hosts, and desktop update checks against GitHub Releases. Project commands and SSH
  configuration retain the user's own authority.
- Releases use immutable action SHAs and scoped tokens, SHA-256 checksums and build provenance
  (`gh attestation verify <file> -R santoshshinde2012/holdmap`).

## Linux GLib dependency backports (Unreleased)

The desktop GTK3 bindings require GLib 0.18. The published 0.18.5 contains
[RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html): optimized string-variant
iteration can crash. Its safe boxed-inline slice conversion also allocates only one element
before copying the whole slice, causing a heap overflow as documented in [upstream issue
#2040](https://github.com/gtk-rs/gtk-rs-core/issues/2040). The current source uses
[verified local backports](vendor/README.md) of both upstream fixes, retaining the truthful
0.18.5 version and license. A separate local `g_malloc0` initialization fix gives `Value`
copy callbacks the zero-filled destination required by GObject; the original allocation can
crash even with one element. Existing releases and older lockfiles retain the original
dependency until rebuilt with these fixes.

Cargo-deny now checks transitive unsoundness advisories explicitly. Registry scanning does not
cover local path packages; CI therefore also verifies every vendored file, both exact backports,
the local initialization change, lockfile and GTK resolution, and runs optimized Linux
regressions against the actual GLib
dependency. No advisory is ignored. A clean dependency scan alone does not establish that a
local patch is safe, and the original advisory is not a false positive.

## Verification and limits

[CI](.github/workflows/ci.yml) runs Rust/native regressions on macOS, Linux and Windows,
desktop browser tests, cargo-deny, workflow validation and `npm audit` at all severities for
both npm lockfiles. [Secret scanning](scripts/check-secrets.sh) checks full Git history and
current source with Gitleaks. [CodeQL](.github/workflows/codeql.yml) covers JavaScript/TypeScript,
Rust, Python and GitHub Actions. A [daily dependency workflow](.github/workflows/security.yml)
rechecks advisories without waiting for a code change. Dependabot vulnerability alerts and
automated security updates are enabled. [Architecture notes](docs/architecture.md)
describe the boundaries and fixtures behind these checks.

These checks reduce risk; they do not prove the absence of vulnerabilities. Credential
redaction is heuristic and may miss arbitrary positional secrets. Windows privacy depends on
filesystem ACLs, including any custom state directory. Process visibility depends on OS
permissions, and macOS retains a check-to-signal race; Linux pidfds and Windows process
handles bind signalling more directly to process identity. Review plans and avoid sharing raw
history/logs without inspecting them. Security hardening described here is in current
[Unreleased source](CHANGELOG.md#unreleased), not the downloadable 0.3.0 release.

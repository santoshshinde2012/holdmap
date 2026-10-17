---
title: Analyzers
description: Opt-in holdmap analyzers scan agent skills and MCP packages. Nothing runs until you approve exactly what each one gets.
order: 5
---

Analyzers are scanners you choose. They look at the skills and pinned MCP packages of your
running agents and add short findings to Home, the map, **Needs you** and each agent's
**Insights**. Every analyzer is off until you turn it on.

| Analyzer | What it checks | Network |
| --- | --- | --- |
| **SkillSpector** (NVIDIA, install it yourself) | Skills, statically, for prompt injection, exfiltration, dangerous code and vulnerable dependencies. Runs with `--no-llm`. | `api.osv.dev` for dependency lookups |
| **OSV lookup** (built in) | Pinned npm and PyPI packages your MCP servers launch, against OSV.dev. Unpinned ones are listed as skipped. | `api.osv.dev`: ecosystem, name and version only |
| **Your own** | A local command-line or MCP scanner you register from a manifest. | Only what its manifest declares |

## Turning one on

Open **Settings → Analyzers** and choose **Review and turn on**. The permissions sheet is built
from your real inventory. It shows the folders or packages the analyzer gets, the exact command,
the network hosts and what is sent, the exact OSV request body, the environment variable names,
the sandbox and when it runs. Approving pins the manifest and the program's path, hash and
version. If any of them change, the analyzer stops and asks you to review it again.

```sh
holdmap analyzers                       # list and state
holdmap analyzers show holdmap.osv      # the permissions sheet
holdmap analyzers approve holdmap.osv
holdmap analyzers run nvidia.skillspector
holdmap analyzers auto holdmap.osv on   # also run when items change
holdmap analyzers key set ID NAME       # an API key, stored in the OS keychain
holdmap insights --min medium           # findings; --format sarif for other tools
```

## When it runs

An analyzer runs when you press **Run now**. One that uses no network also runs while holdmap
is open, whenever the analysed items change. An analyzer that uses the network runs on change
only if you tick **Also run when items change** (`holdmap analyzers auto ID on`). MCP analyzers
run only when you press Run.

## Sandbox

Every run gets a copy of each folder, a cleared environment, a private temporary home and a time
limit. On top of that, holdmap uses what the operating system offers. The permissions sheet
names the one in use.

| System | Sandbox | It adds |
| --- | --- | --- |
| Linux | bubblewrap if installed, otherwise Basic | Read-only system, writes only to its own folder, credential folders hidden, no network unless declared |
| macOS | Seatbelt (`sandbox-exec`, best effort) | Writes only to its own folder, credential folders unreadable, no network unless declared |
| Windows | Job Object | Whole process tree stopped at the time limit, memory capped |
| Other | Basic | Nothing more |

No sandbox stops an analyzer from reading your other files. Windows doesn't block file reads or
network access. Only approve analyzers you trust.

## API keys

If an analyzer declares a key, set it with **Set key** or `holdmap analyzers key set`. The key
is stored in the OS keychain and passed only to that analyzer's run, under the declared name.
It is never shown again, logged or sent anywhere else.

## Signed catalog

The SkillSpector and OSV manifests come from a catalog that ships signed with holdmap. **Check for
a catalog update** (`holdmap analyzers catalog fetch`) downloads a newer catalog and checks its
signature. Nothing changes until you review the field-by-field diff and accept it
(`holdmap analyzers catalog accept`, or `reject`). An analyzer whose manifest changed asks you
to approve it again.

## Your own analyzers

Paste a manifest under **Add your own analyzer…**, or run `holdmap analyzers add scanner.toml`.
holdmap validates it, and it stays off until you approve it like any other analyzer. To remove
it, along with its approval, results and keys, run `holdmap analyzers remove ID`.

An MCP analyzer names a local stdio server and one tool that holdmap calls once per skill:

```toml
schema = 1
id = "example.mcp-scan"
name = "Example MCP Scan"
publisher = "Example"
homepage = "https://example.com"
license = "MIT"
summary = "Scans a skill folder through a local MCP server."

[runner]
kind = "mcp"
command = "example-mcp"
args = ["--stdio"]
per_subject = "skill"
tool = "scan_skill"
timeout_secs = 30

[runner.tool_args]
path = "{subject.dir}"

[permissions]
read = ["{subject.dir}"]
spawns_mcp_servers = true

[output]
format = "sarif"
```

holdmap starts that server inside the sandbox only for a run you start, passes it a copy of
the folder, and stops it when the run ends. If the server looks like one an agent is configured
to use, the permissions sheet warns you first.

## What it never does

- Starts MCP servers on its own, or connects to your agents' servers.
- Passes your environment variables, tokens or secrets. Only a key you set for that analyzer is
  passed.
- Keeps matched text. Only a short title, location, fix hint and severity are stored, locally.

MCP clients can read results with `list_analyzers` and `list_insights`. They can't run, approve
or add analyzers.

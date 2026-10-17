---
title: Analyzers
description: Opt-in scanners for agent skills and MCP packages. Nothing runs until you approve exactly what each one gets.
order: 5
---

Analyzers are scanners you choose. They look at the skills and pinned MCP packages of your
running agents and add short findings to Home, the map, **Needs you** and each agent's
**Insights**. Every analyzer is off until you turn it on.

| Analyzer | What it checks | Network |
| --- | --- | --- |
| **OSV lookup** (built in) | Pinned npm and PyPI packages your MCP servers launch, against OSV.dev. Unpinned ones are listed as skipped. | `api.osv.dev`: ecosystem, name and version only |
| **SkillSpector** (NVIDIA, install it yourself) | Skills, statically, for prompt injection, exfiltration, dangerous code and vulnerable dependencies. Runs with `--no-llm`. | `api.osv.dev` for dependency lookups |

## Turning one on

Open **Settings → Analyzers** and choose **Review and turn on**. The permissions sheet shows,
built from your real inventory: the folders or packages it gets, the exact command, network
hosts and what is sent, the exact request body (OSV), the environment variable names it receives,
the sandbox and when it runs. Approving pins the manifest, the program's path, hash and version;
if any of them change, the analyzer stops and asks you to review again.

From a terminal:

```sh
holdmap analyzers                 # list and state
holdmap analyzers show holdmap.osv
holdmap analyzers approve holdmap.osv
holdmap analyzers run nvidia.skillspector
holdmap insights --min medium     # findings; --format sarif for other tools
holdmap insights review <fingerprint>
```

## What it never does

- Starts MCP servers or connects to them.
- Passes your environment variables, tokens or secrets. Runs get a cleared environment, a
  private temporary home and a copy of each skill folder.
- Keeps matched text. Only a short title, location, fix hint and severity are stored, locally.
- Runs on a schedule. It runs on **Run now**, and while holdmap is open when the analysed items
  change.

The v1 sandbox is basic: the operating system does not stop an analyzer from reading other files
or using the network. Only approve analyzers you trust. MCP clients can read results with
`list_analyzers` and `list_insights`; they cannot run or approve analyzers.

---
title: Analyzers
description: Opt-in holdmap analyzers scan agent skills and MCP packages. Nothing runs until you approve exactly what each one gets.
order: 5
---

Analyzers are scanners you choose. They look at the skills and pinned MCP packages of your
running agents and add short findings to Home, the map, **Needs you**, skill cards and **Agents → Security**.
Every analyzer is off until you turn it on.

Want the whole flow with screenshots, step by step? See
[Analyzers, end to end](https://github.com/santoshshinde2012/holdmap/blob/main/docs/analyzers.md).

**Agents → Security → Analyzers** lists the analyzers you set up and the ready-made presets,
each marked on/off or installed. A skill or MCP card's details have **Scan with…**: approved
analyzers run on request; the rest link to their permissions sheet in Settings.

Analyzers are new in 0.10.0.

| Analyzer | What it checks | Network |
| --- | --- | --- |
| **SkillSpector** (NVIDIA; install it from holdmap or yourself) | Skills, statically, for prompt injection, exfiltration, dangerous code and vulnerable dependencies. Runs with `--no-llm`. | `api.osv.dev` for dependency lookups |
| **OSV lookup** (built in) | Pinned npm and PyPI packages your MCP servers launch, against OSV.dev. Unpinned ones are listed as skipped. | `api.osv.dev`: ecosystem, name and version only |
| **Presets** (Cisco, Google, Aqua Security, PyCQA; install from holdmap or yourself) | One click adds a ready-made manifest for a well-known local scanner. See [Presets](#presets). | Only OSV-Scanner: `api.osv.dev` |
| **Your own** | A local command-line or MCP scanner you register from a manifest. | Only what its manifest declares |

## What a skill analyzer is for

A skill is a folder of instructions and scripts your coding agents load and follow. A risky skill
acts with your agent's access: it can tell the agent to run commands, read files or send data out.
A skill analyzer reads a copy of each skill folder and reports what looks risky. For every finding
holdmap shows **which agents load the skill**, its **scope** (global, project or local, or
"unknown" when holdmap can't tell), the **folder** it scanned and the **source file and line**,
with suggested next steps. Findings are the analyzer's word; holdmap hasn't verified them.

## Installing one

In the desktop app, **Install…** on an analyzer card, in **Settings → Analyzers**, in **Scan
with…** or in **Presets** opens a guided install. Before anything runs it shows the exact pinned
package and version, where it comes from, the folder it installs into (holdmap's own
`analyzers/tools/ID` folder; no admin rights, nothing system-wide, your shell config untouched),
the hosts the package manager contacts and what the analyzer reads once approved. You tick a
consent box, then **Install**. Progress and the package manager's output are shown live, with
**Cancel install**; a failure says why and offers **Retry**. holdmap then checks the version the
program reports and records its SHA-256.

Installs use package managers you already have (`uv` for Python tools, `go` for OSV-Scanner) with
a cleared environment and a 10-minute limit. If one is missing, the dialog says so and links to
its official install page. Trivy ships as a prebuilt program, so holdmap shows the safest way to
install it yourself. **Prefer to run it yourself?** keeps the publisher's command to copy. An
installed analyzer is still **off** until you review its permissions and turn it on.

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

### Progress and results

A run keeps going in the background. A dock above the status bar shows each analyzer's step
(preparing, copying, scanning which skill, collecting), elapsed time and **Cancel**; the status
bar and the analyzer's card show it too, across tabs and dialogs. When it ends, the dock
summarises the findings and **View results** opens **Security → Results**.

**Results** is a table of severity, rule, message, agent, skill, folder and path:line. Sort by any
column, group by severity, skill, agent, folder or analyzer, filter by severity or agent and
search. A row expands to the agents, scope, folder and source path, with **Copy path**, **Show in
folder** (desktop; only folders holdmap scanned; the browser copies the path instead) and
suggested next steps. **Run history** lists recent runs with time, analyzer version, result and
counts.

**Copy summary**, **Download JSON**, **Download SARIF 2.1.0**, **Download CSV** and **Download
Markdown** carry **only what's on screen** (the current filter), with a note saying what was
included. They have the rule, severity, title, analyzer, skill or package, agents, scope, folder,
path and fix, with your home folder shown as `~`. CSV cells can't start a spreadsheet formula. The
raw evidence behind a finding is left out.

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
| Linux | bubblewrap, if installed | Read-only system, writes only to its own folder, credential folders hidden, no network unless declared |
| Linux without bubblewrap | Landlock (Linux 5.13+) | Writes only to its own folder, credential folders unreadable, can't signal other programs. On Linux 6.7+, no TCP unless declared (UDP isn't blocked) |
| macOS | Seatbelt (`sandbox-exec`, best effort) | Writes only to its own folder, credential folders unreadable, no network unless declared |
| Windows | Job Object | Whole process tree stopped at the time limit, memory capped |
| Other | Basic | Nothing more |

No sandbox stops an analyzer from reading your other files. Windows doesn't block file reads or
network access. holdmap doesn't use a Windows AppContainer. Rust can't start a process in one
without unstable APIs. An analyzer in an AppContainer also couldn't read its own program files
unless holdmap changed the permissions on them, and holdmap never changes files it doesn't own.
Only approve analyzers you trust.

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

## Presets

**Add your own analyzer… → Presets** (`holdmap analyzers presets`, then
`holdmap analyzers add --preset ID`) lists scanners from established publishers that run on this
computer with no LLM and no API key. Install the tool from holdmap (see
[Installing one](#installing-one)) or yourself. holdmap adds its manifest as your own
analyzer: off, with the same permissions sheet, sandbox and pin as any other.

| Preset | Id | What it checks | Network |
| --- | --- | --- | --- |
| [Cisco Skill Scanner](https://github.com/cisco-ai-defense/skill-scanner) | `cisco.skill-scanner` | Skill instructions and scripts: prompt injection, exfiltration, command injection. Rules only (YARA, patterns, AST); its LLM judge, VirusTotal and AI Defense options stay off. | None |
| [OSV-Scanner](https://github.com/google/osv-scanner) (Google) | `google.osv-scanner` | Lockfiles and manifests in a skill, in many ecosystems, for known vulnerabilities. A skill without packages shows "nothing to check". | `api.osv.dev`: ecosystem, name and version |
| [Trivy](https://github.com/aquasecurity/trivy) secret scan (Aqua Security) | `aquasecurity.trivy-secrets` | Hard-coded credentials: cloud keys, tokens, private keys. Telemetry and version checks are off. | None |
| [Bandit](https://github.com/PyCQA/bandit) (PyCQA) | `pycqa.bandit` | Risky Python in a skill's scripts: `shell=True`, `eval`, unsafe deserialization, weak crypto. | None |

The list shows whether each tool is **Installed** (found on your `PATH` or a usual install
folder), with **Install…** and the install command for those that aren't. Rules-only scanners miss things an LLM
review might catch; treat no findings as "nothing matched", not "safe". How each preset and SkillSpector did on a
small labelled set of skills, with how to rerun it, is in
[Benchmarks and evaluation](https://github.com/santoshshinde2012/holdmap/blob/main/docs/benchmarks.md). Presets ship with
holdmap but are not part of the signed catalog.

### Settings files a skill ships

A skill could ship a scanner's own settings to switch checks off or hide its findings. Every
analyzer, presets and your own, runs on a copy that leaves these files out, at any depth and in
any letter case: `.bandit`, `osv-scanner.toml`, `.trivyignore`, `.trivyignore.yaml`,
`trivy.yaml`, `trivy-secret.yaml`, `.gitleaks.toml`, `.gitleaksignore`, `.semgrepignore`,
`.secrets.baseline`, `.snyk` and `.skillspector-baseline.yaml`. The run notes how many were left
out. The Bandit and OSV-Scanner presets also point their settings option at an empty file, so
nothing else is read either.

## Your own analyzers

**Add your own analyzer…** (in Settings → Analyzers, or the **Add your own** card in Agents → Security → Analyzers) takes any MCP server, not only ones written for holdmap. It walks through
**Source → Tools → Review** and checks each field as you type. Before you choose tools you see each
tool's full description, any hidden characters in it, and what the server says about it (read-only,
may delete, reaches outside). Those are hints from the server, and holdmap doesn't rely on them. Choose how
to reach it:

- **Command**: a local stdio server, for example `npx -y some-mcp-scanner`
  (`holdmap analyzers add --mcp --id local.lint --name "Lint" --command npx --arg -y --arg some-mcp-scanner`).
- **Local URL**: a Streamable HTTP server already running on this computer
  (`--url http://127.0.0.1:8765/mcp`).
- **From your agents**: copy a server an agent is configured to use (`--from AGENT/NAME`,
  `holdmap analyzers servers`). holdmap warns that it's the same program your agent uses.
- **Manifest**: paste a TOML manifest, or run `holdmap analyzers add scanner.toml`. A
  command-line manifest can list `exit_no_findings` codes: with no report written, that exit
  means "nothing to check" rather than a failure. Its arguments can use `{subject.dir}` (the
  private copy), `{output}` (the report file) and `{empty.file}` (an empty file holdmap creates
  for the run: use it instead of `/dev/null`, which doesn't exist on Windows, for options such as
  `--config`).

The browser demo does all of this with sample tools, and installs and starts nothing.

### Choosing tools

**List tools** (`holdmap analyzers tools ID`) starts or contacts the server once, asks for its
tools (every page) and stops it. No tool is called. holdmap then:

- pins the server name, version, endpoint and every tool's name, description and schemas as a
  SHA-256 hash;
- flags descriptions that look like tool poisoning: hidden-instruction markup, asking to ignore
  earlier instructions, keeping something from the user, credential or config files, sending
  data elsewhere;
- suggests an argument mapping from each tool's input schema.

Pick the tools to call once per skill and map each argument to one inventory field, a fixed
value or nothing. The fields are the path of a private copy of the skill folder, the text of
its `SKILL.md`, its file names, its name and the agents that use it. The sheet shows the exact
arguments for one real skill and checks them against the schema
(`holdmap analyzers map ID --auto`, or `--tool NAME --arg path={subject.dir}`).

### Reading the output

SARIF and holdmap JSON are recognised on their own. From SARIF, holdmap takes the rule's
short description as the title (or a readable form of its name), and its help as the fix,
turned into plain text. Help that only repeats the title and the match is dropped. The result
message is never read, because scanners often quote the matched code or secret there. For other JSON, say where the findings are,
in JSONPath style: `--items '$.issues[*]' --rule-id code --severity level --title msg --file file`,
or `--auto-output` to take the suggestion from the tool's output schema. `structuredContent` is
read first. Anything else is kept only as a short excerpt, never as findings.

### Pins and re-approval

Every run lists the tools again before calling any. If the pinned hash changed (a renamed tool,
a new description, a changed schema), the run stops before any call and the analyzer shows
**Changed** with a diff until you approve it again. Changing the mapping also needs approval.

### Remote servers

A URL that isn't on this computer needs **Allow this server on another machine** on the
permissions sheet (`holdmap analyzers approve ID --allow-remote`). A remote server only gets
metadata: skill names, file names and agents, never paths or file contents. A bearer token is
stored in the OS keychain (`--auth KEY_NAME`, then `holdmap analyzers key set ID KEY_NAME`).
OAuth isn't supported. holdmap connects to the URL directly: it follows no redirects and
ignores `HTTP_PROXY` / `HTTPS_PROXY`, so what it sends reaches only that server.

### Limits

holdmap declares no client capabilities, so servers can't ask for elicitation, sampling or
roots. Each call has a time limit and a response size cap; **Stop** (or the run timing out)
sends `notifications/cancelled` and stops the server. A stdio server runs in the sandbox with a
cleared environment. An HTTP server is already running, so holdmap can't sandbox it.

A manifest for the same thing looks like this:

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
timeout_secs = 30

[[runner.tools]]
name = "list_files"
args = { files = "{subject.files}", skill_name = "{subject.name}" }

[permissions]
read = ["{subject.dir}"]
spawns_mcp_servers = true

[output]
format = "auto"
items = "$.issues[*]"
rule_id = "code"
severity = "level"
title = "msg"
file = "file"
```

To remove an analyzer, along with its approval, pin, results and keys, run
`holdmap analyzers remove ID`.

## How it works, as a graph

**How it works** on an analyzer, or **Agents → Security → Pipeline**, draws its pipeline:
inventory sources, the exact fields it gets, the sandbox and hosts, the program or MCP tools,
the parser, the findings by severity, and where they appear (Home, Needs you, Security). Each
step shows the last run's counts, duration and status, plus the approval and pin state. Click
or press Enter on a step for details, and use the arrow keys to move between steps. The overview
shows which analyzer covers which agent. Narrow windows get the same steps as a list.

## What it never does

- Starts MCP servers on its own, or connects to your agents' running servers.
- Calls a tool you didn't choose, or keeps running after a server's tools change.
- Passes your environment variables, tokens or secrets. Only a key you set for that analyzer is
  passed.
- Keeps matched text. Only a short title, location, fix hint and severity are stored, locally.

MCP clients can read results with `list_analyzers` and `list_insights`. They can't run, approve
or add analyzers.

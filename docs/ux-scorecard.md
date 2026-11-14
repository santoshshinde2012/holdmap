# UX scorecard

A short audit of the desktop app, the tray panel and the CLI on a real macOS machine
(7 agents and tools, 40+ ports) and on the browser sample data. Each flow lists the steps
before and after this pass. A step is one click, one key press or one typed query.

## The 10-second test

Can someone new tell what is running, what is risky and what to stop?

| Question | Before | After |
| --- | --- | --- |
| What is running? | Home cards name each agent, but two "Codex CLI" cards and two "Docker Desktop" cards look the same | Each card names its folder, or its PID when the name repeats |
| How much does it use? | Home shows no RAM or CPU. You must open Agents | Each Home card shows RAM and CPU |
| What is risky? | "Needs you" is clear | Same |
| What can I stop? | "Stop…" on cards and on "Needs you" rows | Same |
| Is the data current? | The header says "Live" (or "starting…" in green) on Home even when every scan fails | The header says "Scan failed" in amber on Home too |

## Task flows

| Flow | Before | After |
| --- | --- | --- |
| Find what is on port X and stop it | `/`, type the port, Enter, Stop…, Enter (4 steps). Undo means "Copy restart command", then a terminal | Same 4 steps. The success toast has **Start again**, which starts the same command again (1 step). If holdmap can't tell which process to start, it falls back to "Copy restart command" |
| Which agent uses the most RAM, and why | Agents tab, read the side bar, click the card, read RAM (2 clicks) | Read it on the Home card (0 clicks). Open, then read RAM (1 click) |
| Where an agent keeps skills, rules, memory and MCP config | Open, Config, Read local config, scroll, Copy path, then go to Finder (4 clicks and a context switch) | Open, Config, Read local config (first time only), **Show in folder** (3 clicks after the first time, no context switch) |
| Review a security finding and act on it | Expand the "Needs you" row, ›, expand the finding, Mark reviewed (4 clicks) | Same 4 clicks. Kept: each step shows what happens next |
| Any of the above from the keyboard | ⌘K finds ports and stops, but not agents, Home, Security, MCP & skills or Flow | ⌘K also opens Home, MCP & skills, Flow and Security, opens or stops each agent, and has "Most RAM" (2 steps: ⌘K, type, Enter) |

## Issues found

| # | Area | Issue | Fixed in |
| --- | --- | --- | --- |
| 1 | Data | Processes the OS won't let holdmap read (root daemons such as Docker's vmnetd) show **0.0% CPU**, which looks measured | d108a5a |
| 2 | Data | The same processes have no readable start time, so Home shows **"up 20771d 9h"** | d108a5a |
| 3 | Home | Cards show no RAM or CPU, so "who uses the most" needs another view | b23f627 |
| 4 | Home | Agents with the same name can't be told apart | b23f627 |
| 5 | States | When scans fail, the header still says "Live" or a green "starting…" on Home and Agents | 24158f2 |
| 6 | States | Agents keeps saying "Looking for agents…" when the agents read fails, and the same error toast stacks twice | 24158f2 |
| 7 | States | Error text repeats "Error:" in front of every message | 24158f2 |
| 8 | Stop flow | The success toast offers "Copy restart command" instead of restarting | 08796af |
| 9 | Keyboard | ⌘K can't open Home, MCP & skills, Flow or Security, or a named agent | 7f382e1 |
| 10 | Config | Locations offer only "Copy path"; nothing opens the folder | 65e31f7 |
| 11 | Config | The Config tab shows two identical "Read local config" buttons | 65e31f7 |
| 12 | Tray | In a browser the panel always shows sample data, so real names and numbers were never checked | 857087f |
| 13 | Tray | When the first read fails, the status says "Checking…" and the lists say "Looking for agents…" forever | 857087f |
| 14 | Copy | "total memory" on Agents cards, "RAM" on Home, the inspector and the tray | af41998, 4e36ad2 |
| 15 | Copy | The browser sample data says "Live" in the header | 24158f2 |
| 16 | Keyboard | Opening an agent from Home left focus on the page, so Esc and Tab started from the top | 73b78f7, 8fac87a |
| 17 | Copy | The CLI, the TUI, the sort menu and the 3D view said "MEM" or "Memory"; the rest of the app says RAM | 4e36ad2 |
| 18 | Copy | "Needs you" said "Network: wider than usual" for agents started with flags that skip approvals | af41998 |
| 19 | States | If the very first scan failed, Home cards showed "…" forever | 8e725f8 |
| 20 | Tray | Offline, the Dev servers section was blank | 8e725f8 |
| 21 | Agents | The access badge on agent cards cut off ("Network: wider than usu…") | 0858adb |

## Checked and kept

Measured on the real machine after the fixes (Chromium, 1280 × 800):

* Layout shift on load is 0.003 on Home, 0.031 on List, 0.001 on Agents and 0.001 on Graph, and 0 during live refresh (good is under 0.1).
* Home cards kept the same order over six live scans in 24 seconds. List rows that stay between scans keep their order.
* With reduced motion on, no animation runs after Home loads or an agent opens.
* Esc closes the palette, dialogs and the agent drawer. Closing the drawer puts focus back on the card that opened it (end-to-end test).
* The empty machine (`?mockPorts=0`) has a clear message on every view.
* The tray handles long names, many agents (top 5, then "more"), zero agents and an offline scanner (unit tests and screenshots).

## Left as is

* While the first scan runs, numbers show "…" instead of grey skeleton blocks. They hold their space, so nothing jumps.
* "Show in folder" is in the inspector's Config tab. Other lists of locations still offer "Copy path" only.
* The tray in a browser uses real scan data but does not stop processes. The native tray was not photographed from a signed build.
* In the tray, a process whose CPU can't be read shows "…" without the reason. The app and the CLI give the reason.

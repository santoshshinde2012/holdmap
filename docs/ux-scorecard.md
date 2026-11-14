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
| Find what is on port X and stop it | `/`, type the port, Enter, Stop…, Enter (4 steps). Undo means "Copy restart command", then a terminal | Same 4 steps. The success toast has **Restart**, which starts the same command again (1 step) |
| Which agent uses the most RAM, and why | Agents tab, read the side bar, click the card, read RAM (2 clicks) | Read it on the Home card (0 clicks). Open, then read RAM (1 click) |
| Where an agent keeps skills, rules, memory and MCP config | Open, Config, Read local config, scroll, Copy path, then go to Finder (4 clicks and a context switch) | Open, Config, Read local config, **Reveal** (4 clicks, no context switch) |
| Review a security finding and act on it | Expand the "Needs you" row, ›, expand the finding, Mark reviewed (4 clicks) | Same. Kept: each step shows what happens next |
| Any of the above from the keyboard | ⌘K finds ports and stops, but not agents, Home, Security, MCP & skills or Flow | ⌘K also opens every view and section, each agent, and "Agent using the most RAM" |

## Issues found

| # | Area | Issue |
| --- | --- | --- |
| 1 | Data | Processes the OS won't let holdmap read (root daemons such as Docker's vmnetd) show **0.0% CPU**, which looks measured |
| 2 | Data | The same processes have no readable start time, so Home shows **"up 20771d 9h"** |
| 3 | Home | Cards show no RAM or CPU, so "who uses the most" needs another view |
| 4 | Home | Agents with the same name can't be told apart |
| 5 | States | When scans fail, the header still says "Live" or a green "starting…" on Home and Agents |
| 6 | States | Agents keeps saying "Looking for agents…" when the agents read fails, and the same error toast stacks twice |
| 7 | States | Error text repeats "Error:" in front of every message |
| 8 | Stop flow | The success toast offers "Copy restart command" instead of restarting |
| 9 | Keyboard | ⌘K can't open Home, MCP & skills, Flow or Security, or a named agent |
| 10 | Config | Locations offer only "Copy path"; nothing opens the folder |
| 11 | Config | The Config tab shows two identical "Read local config" buttons |
| 12 | Tray | In a browser the panel always shows sample data, so real names and numbers were never checked |
| 13 | Tray | When the first read fails, the status says "Checking…" and the lists say "Looking for agents…" forever |
| 14 | Copy | "total memory" on Agents cards, "RAM" on Home, the inspector and the tray |
| 15 | Copy | The browser sample data says "Live" in the header |

## Checked and kept

* Layout shift on load and during live refresh stays under 0.04 on every view (good is under 0.1).
* Rows keep their order between scans; nothing re-sorts under the cursor.
* Dialogs, drawers and the palette close with Esc and return focus.
* Transitions respect reduced motion.
* The empty machine (`?mockPorts=0`) has a clear message on every view.

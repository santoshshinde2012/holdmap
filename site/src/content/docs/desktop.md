---
title: Desktop app
description: The portwise tray and menu-bar app, its keyboard shortcuts and how to open it the first time.
order: 5
---

A tray and menu-bar app with the port list, details, the service graph, pins, history with
one-click restart, remote hosts over SSH and a command palette. It runs on the same core as the
CLI, so the plans and protections are identical. [Try it in the browser](../../#demo).

## First open

The app isn't notarised or code-signed yet, so the OS asks once.

- **macOS:** open it once, then go to **System Settings › Privacy & Security › Open Anyway**
  (macOS 14 and earlier: right-click the app, then **Open**). Or run
  `xattr -dr com.apple.quarantine /Applications/portwise.app`.
- **Windows:** in "Windows protected your PC", click **More info**, then **Run anyway**.
- **Linux:** `chmod +x` the AppImage, or install the .deb or .rpm.

## Keyboard

`⌘⌥P` (`Ctrl+Alt+P`) brings the app up from anywhere; change it in Settings. Inside the app
(`Ctrl` instead of `⌘` on Windows and Linux):

| Keys | Action |
|---|---|
| `⌘K` | Command palette |
| `/` or `⌘F` | Search ports, projects and processes |
| `↑` `↓` or `j` `k` | Move through the list |
| `Esc` | Clear the search, the filters or the selection |
| `⌫` | Stop the selected port (`⇧⌫` to force) |
| `o` · `c` | Open in the browser · copy its URL |
| `p` | Pin or unpin the port |
| `g` | Switch between the list and the service graph |
| `h` | Recently stopped, with one-click restart |
| `d` · `m` · `e` · `t` | Filter: dev servers · mine · exposed · TCP/UDP |
| `⌘R` | Rescan now |
| `⇧L` | Cycle the theme |
| `⌘,` | Settings |
| `?` | Every shortcut |

## From the details pane

Open the port in a browser, restart a dev server, open its folder in your editor
(`PORTWISE_EDITOR`, else Cursor, VS Code, Zed…) or the file manager, and copy its URL, a `curl` or
the kill command.

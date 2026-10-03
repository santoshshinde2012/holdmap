# Developing portwise

How to build, run and test every part of the repository. The rules for changes (safety, naming,
commits) are in [CONTRIBUTING.md](../CONTRIBUTING.md), and the design is in
[architecture.md](architecture.md).

## Repository layout

```text
crates/
  portwise-core/     the engine: scanning, explanations, plans, safety, stop, topology (library)
  portwise-cli/      the `portwise` binary: clap CLI and the ratatui TUI (src/tui/)
  portwise-mcp/      the MCP stdio server (library, run by `portwise mcp`)
apps/desktop/
  src/               Svelte 5 UI: App.svelte, components/ (PascalCase.svelte), lib/ (kebab-case.ts)
  src-tauri/         the Tauri v2 backend: commands, state, tray, watcher, global shortcuts
docs/                guides and reference (kebab-case.md); screenshots/ for images
scripts/             demo servers, cross-checks, doc generation, screenshot capture
.github/             CI, release workflows, issue and PR templates
```

## Prerequisites

| For | You need |
|---|---|
| Core, CLI, TUI, MCP | Rust stable (pinned by `rust-toolchain.toml`, minimum 1.95). macOS: Xcode Command Line Tools. Windows: the MSVC build tools |
| Desktop UI | Node.js 20 or newer (CI uses 22) |
| Desktop app on Linux | `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev` |
| Cross-checking macOS and Windows from Linux | `rustup target add x86_64-pc-windows-gnu aarch64-apple-darwin` (the script adds them) |

## Build and run

```sh
cargo build                          # core, CLI and MCP (the default workspace members)
cargo run -p portwise -- list --dev  # run the CLI from source
cargo run -p portwise                # the TUI
cargo install --locked --path crates/portwise-cli   # install your build as `portwise`

cd apps/desktop
npm install
npm run dev          # the UI alone in a browser (http://localhost:1420) with mock data
npm run tauri dev    # the real app with hot reload
npm run tauri build  # release build and installers for this OS
```

The browser preview (`npm run dev`) uses `src/lib/mock.ts`, so you can work on the UI without the
Rust backend. Open `http://localhost:1420/#ui-gallery` to see every UI-kit control and the type
specimen in both themes.

## Demo servers

`scripts/demo-servers.sh start` starts a realistic set of listeners under `$TMPDIR/portwise-demo`
(override with `PORTWISE_DEMO_DIR`): an npm → sh → node tree on 3000, a Vite-style server on 5173
(IPv4 and IPv6), a Python API exposed on `0.0.0.0:8000`, an API on 8080 that ignores SIGTERM (to
show SIGKILL escalation), a UDP service on 8125, and the `acme-shop` pnpm monorepo with live
connections between its services for `portwise graph`. `start` is idempotent;
`scripts/demo-servers.sh stop` stops everything any run started. It needs Node.js and Python 3.

## Tests and checks

Run these before opening a pull request. CI runs the same on Linux, macOS and Windows.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test                                   # unit, proptest, CLI, MCP, docs and Linux end-to-end tests
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p portwise-core -p portwise-mcp -p portwise
scripts/check-cross.sh                       # clippy for the macOS and Windows backends, from Linux

cd apps/desktop
npm run check                                # svelte-check (types and Svelte diagnostics)
npm test                                     # vitest: logic, components, typography and contrast lints
npm run build                                # production build
```

What the test suites cover:

| Suite | Where | Notes |
|---|---|---|
| Core unit tests | `crates/portwise-core/src/**` (`#[cfg(test)]`) | Parsers run against recorded fixtures in `crates/portwise-core/tests/fixtures/` |
| Property tests | `crates/portwise-core/src/topology/proptests.rs` | Stop order is a valid DAG order and deterministic; parsers never panic |
| End-to-end (Linux) | `crates/portwise-core/tests/e2e_linux.rs` | Real listeners, process trees, SIGTERM-ignoring children, connected services. The test binary re-executes itself to spawn them |
| CLI and TUI | `crates/portwise-cli/tests/cli.rs`, unit tests in `src/` | `assert_cmd` against the real binary; TUI state, graph and styles |
| Docs and conventions | `crates/portwise-cli/src/cli_docs.rs`, `crates/portwise-cli/tests/repo_conventions.rs` | See below |
| MCP | `crates/portwise-mcp/src/lib.rs` | JSON-RPC handshake, tools, safety refusals |
| Desktop backend | `apps/desktop/src-tauri/src/{watch,shortcuts}.rs` | Notification rules and shortcut presets |
| Desktop UI | `apps/desktop/src/**/*.test.ts` | vitest; component tests run in jsdom with Testing Library |

### Docs checks

`cargo test` keeps the documentation honest:

- **`docs/cli.md` matches the code.** It's generated from the clap definitions. After changing a
  command or flag, regenerate it with `PORTWISE_BLESS=1 cargo test -p portwise cli_reference` (or
  `scripts/gen-docs.sh`) and commit the result.
- **The README mentions every command** and uses only flags that exist in its CLI section.
- **Links resolve.** Every relative link and image in every Markdown file must point at a file that
  exists, and every `#anchor` at a real heading.
- **No orphaned screenshots.** Every image in `docs/screenshots/` must be used by a document.
- **File names follow the conventions** in [CONTRIBUTING.md](../CONTRIBUTING.md#naming-conventions).

### Desktop lints

- **Typography** (`apps/desktop/src/typography.test.ts`): font sizes, weights, line heights,
  letter spacing and families must come from the semantic tokens in `app.css`. Exempt a line
  with `/* type-exempt: reason */`.
- **Contrast** (`apps/desktop/src/contrast.test.ts`): the text and button colour tokens must meet
  WCAG AA (4.5:1) in both themes.

### Benchmarks

```sh
cargo bench -p portwise-core      # criterion: scan, topology build, explain, ss parsing
```

On an 8-core Linux machine with about 260 processes: a warm scan takes about 58 ms, building the
topology about 0.23 ms, explaining one port about 12 µs, and parsing 2 000 `ss` lines about 0.76 ms.

## Reference material

```sh
scripts/gen-docs.sh
```

writes the man pages (`target/docs/man/portwise.1` and one page per command), shell completions
for bash, zsh, fish, PowerShell and Elvish (`target/docs/completions/`), and regenerates
`docs/cli.md`. The same output is available at runtime with `portwise man --out-dir DIR` and
`portwise completions SHELL`.

## Screenshots

Screenshots live in `docs/screenshots/` and are named `<surface>-<view>-<theme>.png`, where the
surface is `desktop`, `cli` or `tui` and the theme is `light` or `dark`.

- **Desktop**: start the browser preview (`npm run dev` in `apps/desktop`), then run
  `node scripts/capture-desktop-screenshots.mjs docs/screenshots` (it needs `puppeteer-core` and
  Chrome; see the header of the script). Pass a comma-separated list of names to capture only some.
- **Terminal**: start the demo servers and an X display, then use
  `scripts/capture-terminal-screenshot.sh NAME COLS ROWS "command" [keys…]`.

Delete a screenshot when no document uses it any more; `cargo test` fails on orphans.

## Debugging tips

- `PORTWISE_HOME=/tmp/pw portwise …` uses a throwaway configuration and history.
- `portwise list --all --json` shows every socket with all fields, which is useful for bug reports.
- `--no-docker` rules out the container runtime when a scan is slow or wrong.
- In the desktop app, the web inspector opens with the usual shortcut in `npm run tauri dev`.

# Contributing to portwise

Thanks for helping. portwise stops processes on people's machines, so correctness and safety come
before features. Please read the [Code of Conduct](CODE_OF_CONDUCT.md), and report security
problems privately as described in [SECURITY.md](SECURITY.md).

## Contents

- [Setup](#setup)
- [Checks to run before a pull request](#checks-to-run-before-a-pull-request)
- [Ground rules](#ground-rules)
- [Naming conventions](#naming-conventions)
- [Documentation](#documentation)
- [Commits and pull requests](#commits-and-pull-requests)
- [Releases](#releases)
- [Licence](#licence)

## Setup

- **Rust** stable, pinned by `rust-toolchain.toml` (minimum 1.95). macOS needs the Xcode Command
  Line Tools; Windows needs the MSVC build tools.
- **Node.js** 20 or newer for the desktop app (CI uses 22).
- **Linux desktop builds** also need
  `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev`.

```sh
git clone https://github.com/santoshshinde/portwise && cd portwise
cargo build && cargo test
scripts/demo-servers.sh start      # realistic listeners to try things on (stop with `stop`)
cargo run -p portwise -- list --dev
cd apps/desktop && npm install && npm run tauri dev
```

[docs/development.md](docs/development.md) explains the layout, the test suites, benchmarks,
screenshots and debugging in more detail.

## Checks to run before a pull request

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p portwise-core -p portwise-mcp -p portwise
scripts/check-cross.sh                       # type-checks the macOS and Windows backends from Linux
(cd apps/desktop && npm run check && npm test && npm run build)
```

CI runs all of these on Linux, macOS and Windows. Some of them check more than code:

- **The typography lint** (`apps/desktop/src/typography.test.ts`, part of `npm test`) fails on any
  raw `font-size`, `font-weight`, `line-height`, `letter-spacing` or `font-family` outside the
  `:root` tokens in `app.css`. Use the semantic roles (`--fs-body`, `--fw-medium`…). If a value
  really has to be raw, mark the line `/* type-exempt: reason */`.
- **The contrast lint** (`apps/desktop/src/contrast.test.ts`) keeps text and button colours at
  WCAG AA in both themes.
- **The docs checks** (part of `cargo test`) compare `docs/cli.md` with the clap definitions, check
  that the README mentions every command and only real flags, that every Markdown link, image and
  anchor resolves, that no screenshot is orphaned, and that file names follow the conventions
  below. After changing a command or flag, run `PORTWISE_BLESS=1 cargo test -p portwise cli_reference`
  (or `scripts/gen-docs.sh`) and commit the regenerated `docs/cli.md`.
- **Rust docs:** `portwise-core` and `portwise-mcp` use `#![warn(missing_docs)]`, so every public
  item needs a doc comment, and intra-doc links must resolve.

## Ground rules

- **All logic lives in `portwise-core`.** The CLI, TUI, desktop app and MCP server only render
  `Snapshot`, `Explanation`, `ActionPlan` and `StopReport`. They never signal processes themselves.
- **Every destructive path goes through `ActionPlan` and `execute`.** That keeps dry runs,
  confirmations, the PID-reuse guard and the protection re-check consistent everywhere.
- **Platform code stays behind `cfg` in `crates/portwise-core/src/sys/`.** Keep parsers pure and test
  them with fixtures in `crates/portwise-core/tests/fixtures/`.
- **New framework signatures** go in `crates/portwise-core/src/project/framework.rs` with a unit
  test. Matching is on word boundaries: `vite` must not match `invite`.
- **New protected processes** go in `crates/portwise-core/src/safety.rs`. If you're unsure, protect it.
- **New stop strategies** are one `impl StopStrategy` registered in
  `crates/portwise-core/src/engine/strategies/mod.rs`; the engine and the surfaces don't change.
- **Linux end-to-end tests** (`crates/portwise-core/tests/e2e_linux.rs`) spawn real listeners by
  re-executing the test binary. Use that pattern rather than depending on external tools.
- **Desktop UI:** controls come from the UI kit in `apps/desktop/src/components/ui/`; feature
  components compose it and don't restyle inputs. Keep view logic in pure, tested modules in
  `apps/desktop/src/lib/`.

## Naming conventions

Each ecosystem uses its own idiom, applied consistently. `cargo test` enforces these rules
(`crates/portwise-cli/tests/repo_conventions.rs`).

| What | Convention | Examples |
|---|---|---|
| Crate directories and package names | kebab-case | `portwise-core`, `portwise-mcp` |
| Rust modules, files and module directories | snake_case | `process_tree.rs`, `graph_ui.rs`, `engine/strategies/` |
| Rust test fixtures | snake_case | `proc_net_tcp6.txt` |
| Svelte components | PascalCase `.svelte` | `PortRow.svelte`, `CommandPalette.svelte` |
| TypeScript modules | kebab-case `.ts` (all current modules are single words) | `rows.ts`, `api.ts` |
| Unit tests | `<module>.test.ts` next to the module | `lib/rows.test.ts` |
| Component tests | `<folder>.test.ts` in the component folder | `components/list/list.test.ts` |
| Repository-wide lints | named after what they check, in `apps/desktop/src/` | `typography.test.ts`, `contrast.test.ts` |
| Assets (fonts, images) | kebab-case | `inter-variable.woff2` |
| Root documents | conventional UPPERCASE | `README.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `LICENSE-MIT`, `LICENSE-APACHE` |
| Documents under `docs/` | kebab-case | `user-guide.md`, `cli.md`, `architecture.md` |
| Screenshots | `<surface>-<view>-<theme>.png`; surface `desktop`, `cli` or `tui`; theme `light` or `dark` | `desktop-graph-dark.png`, `cli-list-dark.png` |
| Workflows, scripts, issue templates | kebab-case | `desktop-release.yml`, `demo-servers.sh`, `bug-report.md` |

Names fixed by tools are left alone: `Cargo.toml`, `package.json`, `tsconfig.json`,
`svelte.config.js`, `vite.config.ts`, `rust-toolchain.toml`, `src-tauri/`, the Tauri icon set in
`src-tauri/icons/`, and GitHub's `.github/ISSUE_TEMPLATE/` and `PULL_REQUEST_TEMPLATE.md`.
Rename files with `git mv` so their history follows them.

## Documentation

- User-facing changes need docs in the same pull request: the README for anything in the feature
  tour or tables, [docs/user-guide.md](docs/user-guide.md) for workflows, [docs/mcp.md](docs/mcp.md)
  for MCP tools, and [docs/architecture.md](docs/architecture.md) for structural changes.
- Write plainly: short sentences, active voice, real commands that work when pasted.
- UI changes should update the affected screenshots (see
  [docs/development.md](docs/development.md#screenshots)).

## Commits and pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): summary`, in the
  imperative, under about 72 characters.
  - Types: `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`.
  - Scopes: `core`, `cli`, `tui`, `mcp`, `desktop`, `docs`, `ci`, `release`.
  - Examples: `feat(cli): add --wide to list`, `fix(core): re-check start time before SIGKILL`,
    `docs: add the MCP guide`.
  - Breaking changes: `feat(cli)!: …` plus a `BREAKING CHANGE:` footer.
- Keep pull requests focused. Add an entry under `## [Unreleased]` in `CHANGELOG.md` for anything
  users will notice.
- The pull request template has the checklist reviewers look for.

## Releases

Maintainers tag `vX.Y.Z` for the CLI (cargo-dist builds the archives, installers and Homebrew
formula) and `desktop-vX.Y.Z` for the desktop app (tauri-action builds the installers). The full
checklist is in [docs/releasing.md](docs/releasing.md).

## Licence

By contributing, you agree that your contributions are dual-licensed under MIT OR Apache-2.0, as
described in the [README](README.md#licence).

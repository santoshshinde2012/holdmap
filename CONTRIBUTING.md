# Contributing to portwise

Thanks for helping. portwise stops processes on people's machines, so correctness and safety come
before features. Report security problems privately as described in [SECURITY.md](SECURITY.md).

## Setup

You need Rust stable (pinned by `rust-toolchain.toml`, minimum 1.95) and, for the desktop app,
Node.js 20 or newer. Linux desktop builds also need
`libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev`.

```sh
git clone https://github.com/santoshshinde2012/portwise && cd portwise
cargo build && cargo test
scripts/demo-servers.sh start      # realistic listeners to try things on (`stop` to clean up)
cargo run -p portwise -- list --dev
cd apps/desktop && npm install && npm run tauri dev   # or `npm run dev` for the UI with mock data
```

[docs/architecture.md](docs/architecture.md) explains how the crates and the desktop app fit
together.

## Checks

Run these before opening a pull request; CI runs them on Linux, macOS and Windows.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p portwise-core -p portwise-mcp -p portwise
scripts/check-cross.sh                       # type-checks the macOS and Windows backends from Linux
(cd apps/desktop && npm run check && npm test && npm run build)
```

`cargo test` also checks the docs: `docs/cli.md` must match the clap definitions, the README must
mention every command and only real flags, Markdown links and images must resolve, and every
screenshot must be used. After changing a command or flag, regenerate the reference with
`scripts/gen-docs.sh` (or `PORTWISE_BLESS=1 cargo test -p portwise cli_reference`) and commit it.
The desktop tests include a typography lint (use the `--fs-*`/`--fw-*` tokens from `app.css`, not
raw values) and a WCAG AA contrast lint for both themes.

## Ground rules

- All logic lives in `portwise-core`. The CLI, TUI, desktop app and MCP server only render its
  types and never signal processes themselves.
- Every destructive path goes through `ActionPlan` and `execute`, so dry runs, confirmations and
  the PID-reuse guard stay consistent.
- Platform code stays behind `cfg` in `crates/portwise-core/src/sys/`; parsers are pure and tested
  with fixtures.
- New protected processes go in `crates/portwise-core/src/safety.rs`. If you're unsure, protect it.
- User-facing changes update the README (and screenshots if the UI changes) in the same pull
  request, plus an entry under `## [Unreleased]` in `CHANGELOG.md`.

## Naming conventions

Each ecosystem uses its own idiom. `cargo test` enforces these rules
(`crates/portwise-cli/tests/repo_conventions.rs`).

| What | Convention | Examples |
|---|---|---|
| Crate directories and package names | kebab-case | `portwise-core`, `portwise-mcp` |
| Rust modules, files, directories and fixtures | snake_case | `process_tree.rs`, `proc_net_tcp6.txt` |
| Svelte components | PascalCase | `PortRow.svelte` |
| TypeScript modules and tests | kebab-case, tests as `<module>.test.ts` | `rows.ts`, `rows.test.ts` |
| Assets, scripts, workflows, files in `docs/` | kebab-case | `inter-variable.woff2`, `demo-servers.sh`, `architecture.md` |
| Screenshots | `<surface>-<view>-<theme>.png` | `desktop-graph-dark.png` |
| Root documents | conventional UPPERCASE | `README.md`, `CHANGELOG.md`, `LICENSE-MIT` |

Names fixed by tools (`Cargo.toml`, `package.json`, `src-tauri/`, the Tauri icon set) are left
alone. Rename files with `git mv` so their history follows them.

## Commits and releases

Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat(cli): add --wide to list`,
`fix(core): …`, `docs: …`) and keep pull requests focused. Maintainers release the CLI by tagging
`vX.Y.Z` (cargo-dist builds the archives, installers and Homebrew formula) and the desktop app by
tagging `desktop-vX.Y.Z` (tauri-action builds the installers), after updating the versions and the
changelog.

## Licence

By contributing, you agree that your contributions are dual-licensed under MIT OR Apache-2.0, as
described in the [README](README.md#licence).

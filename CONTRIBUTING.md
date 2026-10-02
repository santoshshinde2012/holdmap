# Contributing to portwise

Thanks for helping! portwise kills processes, so correctness and safety come before features.

## Setup

- Rust stable (pinned in `rust-toolchain.toml`, MSRV 1.95). macOS needs the Xcode Command Line Tools.
- Node 20+ for the desktop app.
- Linux desktop builds need: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev`.

```sh
cargo build && cargo test
cd apps/desktop && npm install && npm run tauri dev
scripts/demo-servers.sh start   # demo listeners (Next.js-like tree, Vite, Python, a SIGTERM-ignoring API, UDP)
scripts/demo-servers.sh stop
```

## Before you open a PR

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test
scripts/check-cross.sh           # type-checks the macOS + Windows backends from Linux
(cd apps/desktop && npm run check && npm test && npm run build)
```

CI runs all of this on Linux, macOS and Windows.

## Ground rules

- **All logic lives in `portwise-core`.** The CLI, TUI, desktop app and MCP server only render
  `Snapshot`, `Explanation`, `ActionPlan` and `StopReport`. They must never signal processes themselves.
- **Every destructive path goes through `ActionPlan` + `execute`.** That keeps dry-run, confirmation
  dialogs, the PID-reuse guard and the protection re-check consistent.
- **Platform code stays behind `cfg` in `crates/portwise-core/src/sys/`.** Keep parsers pure and test
  them with fixtures (`crates/portwise-core/tests/fixtures/`).
- **New framework signatures** go in `project.rs` with a unit test. Word-boundary matching: `vite`
  must not match `invite`.
- **New protected processes** go in `safety.rs`. If you're unsure, protect it.
- Linux e2e tests (`tests/e2e_linux.rs`) spawn real listeners by re-executing the test binary. Use
  that pattern rather than depending on external tools.

## Commits & releases

- [Conventional Commits](https://www.conventionalcommits.org/) (`feat(core): …`, `fix(cli): …`).
- CLI releases: bump the workspace version, update `CHANGELOG.md`, tag `vX.Y.Z`. cargo-dist builds
  installers, archives and the Homebrew formula.
- Desktop releases: tag `desktop-vX.Y.Z`. tauri-action builds `.dmg`/`.msi`/`.deb`/`.AppImage`.

## License

By contributing you agree that your contributions are dual-licensed under MIT OR Apache-2.0.

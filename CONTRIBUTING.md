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

The website (`site/`, Astro, Node.js 22) embeds the desktop UI as its live demo, so install both:
`(cd apps/desktop && npm ci) && cd site && npm ci && npm run build && npm run preview`. Guide pages
are Markdown in `site/src/content/docs/`; the CLI reference page is `docs/cli.md` itself.

## Checks

Run these before opening a pull request; CI runs them on Linux, macOS and Windows.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p portwise-core -p portwise-mcp -p portwise
scripts/check-cross.sh                       # type-checks the macOS and Windows backends from Linux
(cd apps/desktop && npm run check && npm test && npm run build)
(cd site && npm run check && npm run build && npm test)  # website: types, links, size budget
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
| Astro components and layouts (`site/`) | PascalCase | `PortRail.astro`, `Docs.astro` |
| TypeScript modules and tests | kebab-case, tests as `<module>.test.ts` | `rows.ts`, `rows.test.ts` |
| Assets, scripts, workflows, files in `docs/` | kebab-case | `inter-variable.woff2`, `demo-servers.sh`, `architecture.md` |
| Screenshots | `<surface>-<view>-<theme>.png` | `desktop-graph-dark.png` |
| Root documents | conventional UPPERCASE | `README.md`, `CHANGELOG.md`, `LICENSE-MIT` |

Names fixed by tools (`Cargo.toml`, `package.json`, `src-tauri/`, the Tauri icon set) are left
alone. Rename files with `git mv` so their history follows them.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat(cli): add --wide to list`,
`fix(core): …`, `docs: …`; `feat!:` or a `BREAKING CHANGE:` footer for breaking changes) and keep
pull requests focused. The commit types drive the version bump and the changelog.

## Releasing

Versions follow [SemVer](https://semver.org/). One `vX.Y.Z` tag releases everything:

- [release-please](https://github.com/googleapis/release-please) keeps a `chore: release X.Y.Z`
  pull request open. It bumps every version (`Cargo.toml`, `Cargo.lock`, the desktop
  `package.json`, `package-lock.json` and `tauri.conf.json`) and prepends `CHANGELOG.md`.
  `scripts/check-versions.sh` fails CI if they ever disagree. It was picked over git-cliff because
  it maintains the release PR itself; it doesn't create tags or releases here, so the tag stays a
  deliberate maintainer step.
- Pushing the tag runs `release.yml` ([dist](https://github.com/axodotdev/cargo-dist)): CLI
  archives for six targets (macOS arm64/x64, Linux gnu arm64/x64, Linux musl x64, Windows x64),
  shell and PowerShell installers, an npm package tarball, a
  CycloneDX SBOM, SHA-256 checksums, binaries built with `cargo auditable`, and GitHub build
  provenance (`gh attestation verify <file> -R santoshshinde2012/portwise`). It creates the GitHub
  Release.
- Once the release is published, `release.yml` calls `desktop-release.yml` (tauri-action), which
  adds the `.dmg`, `.msi`, NSIS `.exe`, `.AppImage`, `.deb` and `.rpm`, with checksums and provenance. Run it by hand with an empty tag
  for a dry run that keeps the bundles as workflow artifacts.

Check a change to the release setup locally with `dist plan`,
`dist build --artifacts=local --target x86_64-unknown-linux-gnu` and `actionlint`.

**Repository setup** (once; everything optional is skipped when absent):

| What | Needed for |
|---|---|
| Secret `HOMEBREW_TAP_TOKEN` (fine-grained, contents: write on `santoshshinde2012/homebrew-tap`), then in `dist-workspace.toml` add `"homebrew"` to `installers`, set `tap` and `publish-jobs = ["homebrew"]`, and run `dist generate` | Homebrew (`brew install santoshshinde2012/tap/portwise`); off until then, so release notes don't advertise it |
| Secret `RELEASE_PLEASE_TOKEN` (fine-grained, contents and pull requests: write) | The release PR; the workflow skips with a notice until it exists |
| Secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Signed, notarised macOS app (optional) |
| Secrets `WINDOWS_CERTIFICATE` (base64 `.pfx`), `WINDOWS_CERTIFICATE_PASSWORD` | Signed Windows installers (optional) |
| `npm run tauri signer generate`, then secrets `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` and variable `TAURI_UPDATER_PUBKEY` | In-app updates with signed `latest.json` (optional; keep the private key backed up) |
| `"npm"` in `publish-jobs` in `dist-workspace.toml`, `dist generate`, secret `NPM_TOKEN` | Publishing the npm package (optional) |

**First release (v0.1.0).** There is no earlier tag, so release-please waits for this one:

1. In `CHANGELOG.md`, change `## [0.1.0] - Unreleased` to today's date and point the `[0.1.0]`
   link at `releases/tag/v0.1.0`. Commit (`chore: release 0.1.0`) and push; wait for CI.
2. Tag and push: `git tag -s v0.1.0 -m "portwise 0.1.0" && git push origin v0.1.0`.
3. Watch `Release`, then `Desktop release`, in the Actions tab. Check the release page, then
   the shell installer.

**Later releases.** Merge the release PR, then tag its merge commit `vX.Y.Z` as in step 2.

## Licence

By contributing, you agree that your contributions are dual-licensed under MIT OR Apache-2.0, as
described in the [README](README.md#license).

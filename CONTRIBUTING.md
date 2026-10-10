# Contributing to holdmap

Thanks for helping. holdmap stops processes on people's machines, so correctness and safety come
before features. Report security problems privately as described in [SECURITY.md](SECURITY.md).

## Setup

You need Rust stable (selected by `rust-toolchain.toml`, minimum 1.95) and Node.js 22.12 or newer for the
desktop app and website, matching CI. Linux desktop builds also need
`libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev`.
If you use nvm, run `nvm use` from the repository root to select the version in `.nvmrc`.

```sh
git clone https://github.com/santoshshinde2012/holdmap && cd holdmap
cargo build && cargo test
scripts/demo-servers.sh start      # realistic listeners to try things on (`stop` to clean up)
cargo run -p holdmap -- list --dev
cd apps/desktop && npm ci && npm run tauri dev   # or `npm run dev` for the UI with mock data
```

[docs/architecture.md](docs/architecture.md) explains how the crates and the desktop app fit
together.

The website (`site/`, Astro, Node.js 22) embeds the desktop UI as its live demo, so install both:
`(cd apps/desktop && npm ci) && cd site && npm ci && npm run build && npm run preview`. Guide pages
are Markdown in `site/src/content/docs/`; the CLI reference page is `docs/cli.md` itself.

## Repository layout

The repository root is the Rust workspace; each surface keeps its source and tooling together.

| Path | Purpose |
|---|---|
| `crates/holdmap-core/` | Shared port scanning, ownership, safety policy and stop execution |
| `crates/holdmap-cli/` | CLI and TUI, with integration tests |
| `crates/holdmap-mcp/` | MCP adapter over the shared core |
| `apps/desktop/src/` | Svelte and TypeScript desktop UI, including browser demo data |
| `apps/desktop/e2e/` | Browser tests against the built desktop UI in demo mode |
| `apps/desktop/src-tauri/` | Native desktop integration, Tauri configuration and icons |
| `apps/desktop/assets/` | Desktop brand assets |
| `site/` | Astro website, Markdown guides, site assets and build tools |
| `docs/` | Architecture, generated CLI reference and screenshots used by the README/site |
| `scripts/` | Workspace checks, CLI documentation generation and development helpers |
| `vendor/` | Audited third-party source backport, unchanged upstream snapshot and provenance |
| `.github/` | CI, release, dependency updates and website deployment |

Keep source assets and their licences with the surface that uses them. Commit generated
`docs/cli.md`, published screenshots and website media because they are consumed directly.
Build output (`target/`, `dist/`, `site/public/demo/`), dependencies (`node_modules/`) and
recording frames (`site/.hero-frames/`) are ignored and regenerated. Use Git history for earlier
versions; keep temporary captures, release downloads and backup copies outside the repository.

## Checks

Run `scripts/check-all.sh` before opening a pull request. It installs dependencies from the
lockfiles and checks the Rust workspace, native desktop, browser UI, website and supply chain.
Install [actionlint](https://github.com/rhysd/actionlint#installation) and
[cargo-deny](https://embarkstudios.github.io/cargo-deny/cli/index.html),
[Gitleaks](https://github.com/gitleaks/gitleaks#installing) and Python 3.11+ first, then install
Chromium once with `(cd apps/desktop && npm ci && npx playwright install chromium)`.
On Linux, use `npx playwright install --with-deps chromium` for its system dependencies.
The browser suite starts and stops its own preview server; failure traces and screenshots go
in the ignored `apps/desktop/test-results/` directory. For an existing Chrome installation,
set `HOLDMAP_E2E_CHROMIUM_EXECUTABLE` to its executable path.

After building the website, `(cd site && npm run test:demo)` starts a temporary preview server
and exercises the embedded demo. Pass a different preview or public URL after `--`.
It covers port details, the graph, searchable MCP candidates, simulated stop confirmation
and reset, and reports browser errors. It uses the desktop's locked Playwright dependency.

To run individual checks:

```sh
cargo fmt --all --check
(cd apps/desktop && npm run check && npm test && npm run test:e2e) # builds the UI before native checks
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked -p holdmap-core -p holdmap-mcp -p holdmap
cargo build --locked -p holdmap-desktop
scripts/check-cross.sh                       # type-checks the macOS and Windows backends from Linux
(cd site && npm run check && npm run build && npm test)  # website: types, links, size budget and SEO
scripts/check-versions.sh
scripts/check-secrets.sh                     # full Git history and tracked/new source; needs a full clone
python3 scripts/test-release-security.py    # isolated release input regressions; never publishes
actionlint
python3 scripts/verify-vendored-glib.py       # source backport, lockfile and GTK/Linux test resolution
cargo deny --all-features check
(cd apps/desktop && npm audit)
(cd site && npm audit)
```

`cargo test` also checks the docs: `docs/cli.md` must match the clap definitions, the README must
mention every command and only real flags, Markdown links and images must resolve, and every
screenshot must be used. After changing a command or flag, regenerate the reference with
`scripts/gen-docs.sh` (or `HOLDMAP_BLESS=1 cargo test -p holdmap cli_reference`) and commit it.
The desktop tests include a typography lint (use the `--fs-*`/`--fw-*` tokens from `app.css`, not
raw values) and a WCAG AA contrast lint for both themes. Browser tests cover agent visibility,
search, navigation and port details using deterministic demo data. The CLI integration tests
launch the actual MCP server, negotiate every supported protocol version and verify discovery,
read tools and malformed stop rejection against isolated local listeners.

CI runs Rust and native desktop tests/builds on Linux, macOS and Windows. Chromium browser
tests run on Linux and gate native builds. Dependency checks include both npm lockfiles and
the full Cargo workspace, and fail on known advisories at every severity. Gitleaks checks the
full fetched Git history and current source with redacted findings. CodeQL checks Rust,
JavaScript/TypeScript, Python and GitHub Actions. A daily workflow checks newly published
dependency advisories; Dependabot maintains desktop, website, Rust and workflow updates.
Repository administrators should enable Dependabot alerts and security updates in GitHub's
security settings; those settings are separate from the committed update configuration.

## Vendored security fixes

The Linux desktop currently uses narrow [GLib 0.18.5 security backports](vendor/README.md).
Its upstream snapshot, version and license are preserved; only the audited iterator and
boxed-inline allocation fixes plus a separate local zero-initialization fix change source
files. Run `python3 scripts/verify-vendored-glib.py` after dependency changes. It requires
Python 3.11+ and Cargo, and resolves locked metadata without compiling GTK. CI also runs the
actual resolved dependency with optimization on Linux:

```sh
G_DEBUG=fatal-warnings MALLOC_PERTURB_=165 cargo test -p holdmap-desktop --locked --config 'profile.test.package.glib.opt-level=3' --test glib_security
```

Cargo-deny's `unsound = "all"` includes transitive registry dependencies, but local path
packages need the source verifier and runtime regressions. Do not add advisory ignores or
relabel the backport as a fixed upstream version. Keep `vendor/glib/` outside the first-party
workspace and preserve upstream filenames. Remove the override and its verification together
when the GTK binding family supports a compatible fixed upstream release.

## Published screenshots

Refresh the desktop screenshots, website crops and hero posters from the production browser
build with `node apps/desktop/scripts/capture-screenshots.mjs`. It uses sample data, its own
temporary build and preview server, and the same Chromium installation as the browser tests.
Pass screenshot filenames after the command to refresh only those flows, for example
`node apps/desktop/scripts/capture-screenshots.mjs desktop-agent-stop-confirm-dark.png`.
Refresh the website social card with `(cd site && node scripts/og.mjs)` after updating the overview.

For CLI/TUI screenshots on macOS or Linux, build `cargo build -p holdmap`, create a virtual
environment outside the repository, install `scripts/requirements-terminal-screenshots.txt`
there, then run `scripts/capture-terminal-screenshots.py` with that Python. It captures actual
terminal output against local fixtures, filters the TUI to those fixtures and masks the account
name. Ports, PIDs, resource readings and plans remain actual. It terminates only fixtures it
started and keeps app state in a temporary directory; its dependencies are optional maintainer
tools.

## Ground rules

- All logic lives in `holdmap-core`. The CLI, TUI, desktop app and MCP server only render its
  types and never signal processes themselves.
- Every destructive path goes through `ActionPlan` and `execute`, so dry runs, confirmations and
  the PID-reuse guard stay consistent.
- Desktop and MCP execution bind to a one-use preview and refuse changed effects. Fresh
  process protection checks apply immediately before execution as well as during planning.
- Use the shared credential redactor for display and serialization, and sanitize untrusted
  terminal text. Private/config writes must reject links before truncating a file.
- Platform code stays behind `cfg` in `crates/holdmap-core/src/sys/`; parsers are pure and tested
  with fixtures.
- New protected processes go in `crates/holdmap-core/src/safety.rs`. If you're unsure, protect it.
- User-facing changes update the README (and screenshots if the UI changes) in the same pull
  request, plus an entry under `## [Unreleased]` in `CHANGELOG.md`.

## Naming conventions

Each ecosystem uses its own idiom. `cargo test` enforces these rules
(`crates/holdmap-cli/tests/repo_conventions.rs`).

| What | Convention | Examples |
|---|---|---|
| Crate directories and package names | kebab-case | `holdmap-core`, `holdmap-mcp` |
| Rust modules, files, directories and fixtures | snake_case | `process_tree.rs`, `proc_net_tcp6.txt` |
| Svelte components | PascalCase | `PortRow.svelte` |
| Astro components and layouts (`site/`) | PascalCase | `Hero.astro`, `Docs.astro` |
| TypeScript modules and tests | kebab-case, tests as `<module>.test.ts` | `rows.ts`, `rows.test.ts` |
| Assets, scripts, workflows, files in `docs/` | kebab-case | `inter-variable.woff2`, `demo-servers.sh`, `architecture.md` |
| Screenshots | `<surface>-<view>-<theme>.png` (surface: `desktop`, `cli`, `tui`, `site`) | `desktop-graph-dark.png`, `site-guide-dark.png` |
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
  provenance (`gh attestation verify <file> -R santoshshinde2012/holdmap`). It creates the GitHub
  Release.
- Once the release is published, `release.yml` calls `desktop-release.yml` (tauri-action), which
  adds the `.dmg`, `.msi`, NSIS `.exe`, `.AppImage`, `.deb` and `.rpm`, with checksums and provenance. Run it by hand with an empty tag
  for a dry run that keeps the bundles as workflow artifacts.

Check a change to the release setup locally with `dist plan`,
`dist build --artifacts=local --target x86_64-unknown-linux-gnu` and `actionlint`.
Run `python3 scripts/test-release-security.py` as well. It uses a stub executable to check
valid releases and reject shell payloads in legal Git refs, without creating a release.

`release.yml` is derived from cargo-dist 0.33.0 and maintained explicitly: it validates
`vX.Y.Z` tags, passes tags as quoted environment values and grants repository writes only to
publishing jobs. `allow-dirty = ["ci"]` preserves this workflow while dist still checks its other
generated metadata. When upgrading dist or changing its config, review the new upstream CI
template against these controls and update the workflow deliberately; `dist generate` does
not regenerate CI with this setting. Keep action pins in the config and workflow in sync.

**Repository setup** (once; everything optional is skipped when absent):

| What | Needed for |
|---|---|
| Secret `HOMEBREW_TAP_TOKEN` (fine-grained, contents: write on `santoshshinde2012/homebrew-tap`), then in `dist-workspace.toml` add `"homebrew"` to `installers`, set `tap` and `publish-jobs = ["homebrew"]`, run `dist generate` and review/add the upstream Homebrew publishing job | Homebrew (`brew install santoshshinde2012/tap/holdmap`); off until then, so release notes don't advertise it |
| Secret `RELEASE_PLEASE_TOKEN` (fine-grained, contents and pull requests: write) | The release PR; the workflow skips with a notice until it exists |
| Secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Signed, notarised macOS app (optional) |
| Secrets `WINDOWS_CERTIFICATE` (base64 `.pfx`), `WINDOWS_CERTIFICATE_PASSWORD` | Signed Windows installers (optional) |
| `npm run tauri signer generate`, then secrets `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` and variable `TAURI_UPDATER_PUBKEY` | In-app updates with signed `latest.json` (optional; keep the private key backed up) |
| `"npm"` in `publish-jobs` in `dist-workspace.toml`, `dist generate`, secret `NPM_TOKEN` | Publishing the npm package (optional) |

**Release steps.** Merging the release PR prepares the version; publishing starts when a
maintainer pushes its tag.

1. Review the release PR's version changes and changelog, then merge it after CI passes.
2. Fetch `main` and check out the release PR's merge commit. Run `scripts/check-versions.sh`
   and confirm that `version.txt` matches the intended `X.Y.Z`.
3. Tag that commit and push the tag: `git tag -s vX.Y.Z -m "holdmap X.Y.Z" && git push origin vX.Y.Z`
   (replace `X.Y.Z` with the release version).
4. Watch `Release`, then `Desktop release`, in the Actions tab. Check the published release's
   CLI and desktop downloads, checksums and installer, then confirm the `Pages` workflow
   updates the website.

## Licence

By contributing, you agree that your contributions are dual-licensed under MIT OR Apache-2.0, as
described in the [README](README.md#license).

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
cd apps/desktop && npm ci && npm run tauri dev   # or `npm run dev` for the UI with a live local scan (`?demo=1` for sample data)
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
| `crates/holdmap-activity/` | Optional local recording, source adapters, metadata policy and bounded transport |
| `apps/desktop/src/` | Svelte and TypeScript desktop UI, including browser demo data |
| `apps/desktop/e2e/` | Browser tests against the built desktop UI in demo mode |
| `apps/desktop/src-tauri/` | Native desktop integration, Tauri configuration and icons |
| `apps/desktop/assets/` | Desktop brand assets |
| `site/` | Astro website, Markdown guides, site assets and build tools |
| `docs/` | Architecture, generated CLI reference and screenshots used by the README/site |
| `scripts/` | Workspace checks, CLI documentation generation and development helpers |
| `vendor/` | Audited third-party fixes, preserved upstream snapshots and provenance |
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
On Linux, use `npx playwright install --with-deps chromium` for its system dependencies,
and install `xvfb xauth` for the isolated native shortcut regressions.
On macOS, install FFmpeg (`brew install ffmpeg`) for the synthetic MP4/GIF runtime check.
Native macOS recording builds also need Xcode's Swift compiler and the macOS 14 or newer SDK.
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
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked -p holdmap-core -p holdmap-mcp -p holdmap-activity -p holdmap
cargo build --locked -p holdmap-desktop
scripts/check-cross.sh                       # type-checks the macOS and Windows backends from Linux
python3 scripts/test-check-cross.py          # cross-check isolation without installing targets
(cd site && npm run check && npm run build && npm test)  # website: types, links, size budget and SEO
scripts/check-versions.sh
scripts/check-secrets.sh                     # full Git history and tracked/new source; needs a full clone
python3 scripts/test-release-security.py    # isolated release input regressions; never publishes
python3 scripts/check-updater-runtime.py    # macOS: real signed scratch install and relaunch
python3 scripts/check-window-lifecycle-runtime.py # interactive macOS: owned close/fullscreen/reopen regression
python3 scripts/check-screen-recording-runtime.py # macOS: synthetic MP4/GIF/PNG, real WKWebView and binary IPC; needs FFmpeg
python3 scripts/check-screen-recording-runtime.py --app-ui # macOS: packaged recorder gates + owned app-view PNG; no screen picker
python3 scripts/check-native-screen-recording.py # macOS: native H.264/GIF/PNG writer and cancellation, synthetic pixels only
actionlint
python3 scripts/verify-vendored-glib.py       # source backport, lockfile and GTK/Linux test resolution
python3 scripts/verify-vendored-global-hotkey.py # X11 fixes, original archive and shortcut dependency resolution
cargo deny --all-features check
(cd apps/desktop && npm audit)
(cd site && npm audit)
```

`cargo test` also checks the docs: `docs/cli.md` must match the clap definitions, the README must
mention every command and only real flags, Markdown links and images must resolve, and every
screenshot must be used. After changing a command or flag, regenerate the reference with
`scripts/gen-docs.sh` (or `HOLDMAP_BLESS=1 cargo test -p holdmap cli_reference`) and commit it.
The generated reference includes nested commands such as `activity serve` and distinguishes
current-source capabilities from published downloads.
Cross-platform checks copy current Rust source into a temporary workspace and reuse build
output. Their macOS type stub extends the existing Cargo patches; the checkout's manifest and
lockfile stay untouched, including when a check fails.
Website checks reject unresolved Markdown image placeholders and missing image sources or
alt attributes; decorative images may use `alt=""`. Public browser checks load guide screenshots
at desktop and mobile widths, as well as exercising the app demo.
Keyboard changes must preserve input-method composition, native control keys and disabled
single-key shortcuts. Calls diagnosis operates on metadata projections and retains only reported
parent context; unknown outcomes and missing completion are distinct. Terminal paste tests must
verify that pasted text cannot reach action or confirmation keys, and mode cleanup must cover
partial setup, exit and unwinding. [Interaction research](docs/agent-flow.md#interaction-standards-and-research)
records the primary standards and the limits of their application here.
The desktop tests include a typography lint (use the `--fs-*`/`--fw-*` tokens from `app.css`, not
raw values) and a WCAG AA contrast lint for both themes. Browser tests cover agent visibility,
search, navigation and port details using deterministic demo data. The CLI integration tests
launch the actual MCP server, negotiate every supported protocol version and verify discovery,
read tools and malformed stop rejection against isolated local listeners.

CI runs Rust and native desktop tests/builds on Linux, macOS and Windows. Chromium browser
tests run on Linux and gate native builds. Dependency checks include both npm lockfiles and
the full Cargo workspace, and fail on known advisories at every severity. Gitleaks checks the
full fetched Git history and current source with redacted findings. CodeQL checks Rust,
JavaScript/TypeScript, Swift, Python and GitHub Actions. Swift analysis compiles the production
native capture adapter on macOS without running it. A daily workflow checks newly published
dependency advisories; Dependabot maintains desktop, website, Rust and workflow updates.
Repository administrators should enable Dependabot alerts and security updates in GitHub's
security settings; those settings are separate from the committed update configuration.

The macOS native job also runs the unchanged updater adapter with a genuine Tauri runtime.
It generates an ephemeral signing key, serves a loopback fixture and installs into a private
scratch app before verifying the actual relaunch. Wrong keys, tampered bytes, mismatched
authenticated versions and escaping result paths must leave the original app untouched.
The runner needs the desktop's locked CLI installed with `npm ci`; it never replaces a host
installation, changes release keys or enables automatic updates for published installers.

The synthetic recording check builds the production encoder and save adapter into an owned
WKWebView with the production CSP. It records a generated canvas, sends both formats through
real binary IPC to a fixture-only destination, and checks hashes, permissions, animation,
duration and cleanup with FFprobe/FFmpeg. Its capture API tripwire rejects any screen-picker
request. It never captures the user's screen or opens the production native save chooser;
save-policy and atomic-write behavior have separate native unit coverage.
On macOS 14 and newer, the generated-canvas fixture requests
[disabled background throttling](https://docs.rs/tauri/2.12.1/tauri/webview/struct.WebviewWindowBuilder.html#method.background_throttling)
for its owned, unfocused window so inactive scheduling does not stall its paint/stop timers.
It reports that policy and timer diagnostics. App UI and capability probes retain default
scheduling; this fixture does not establish background behavior of permissioned display capture.
The app UI check uses a hardened release window, checks the real confirmation dialog and
forces the browser API absent to exercise native capability discovery. The native pixel
fixture compiles the production Swift writer and validates MP4/GIF media and cancellation.
Neither check opens a screen picker, requests Screen Recording permission or captures a display.

Run `python3 scripts/check-window-lifecycle-runtime.py` in an interactive macOS desktop session
for native close changes. It creates a private scratch app, exercises the native fullscreen
close action, and checks application activation, active Space and window/webview identity.
It also verifies secondary and no-tray window destruction and attempts restoration before exit.
The runner force-stops only its owned child if the outer deadline expires;
if macOS denies background activation, the fullscreen case reports an explicit skip.
The fixture does not start scans or plugins, access the user store, replace an installed app,
or request Accessibility or Screen Recording permission. Browser tests cannot cover these
AppKit lifecycle behaviors; this interactive check runs separately from headless CI.

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

The desktop also uses audited [global-hotkey 0.8.0 X11 fixes](vendor/README.md) for immediate
key release, failed-registration rollback and worker error propagation. Its macOS and Windows
sources remain byte-identical to the published archive. The source verifier checks the full
snapshot, reversible local patch and both production/test dependency edges. Normal Linux
tests use a finite child with an invalid display to verify error handling. Real X11 registration,
Caps/Num Lock variants and collision rollback run on an owned virtual display:

```sh
xvfb-run -a env HOLDMAP_X11_TEST=1 cargo test -p holdmap-desktop --locked --test hotkey_x11 -- --include-ignored --test-threads=1
```

These tests require the explicit guard so ordinary workspace tests do not reserve keys on a
developer's display. Remove this override and its verification together when a compatible
upstream release includes these fixes.

## Published screenshots

The canonical logo is `apps/desktop/assets/icon.svg`; its `holdmap-mark` group contains the
glyph separately from the app tile. After editing it, run
`node apps/desktop/scripts/generate-brand-assets.mjs` to refresh web, README and native icons.
The generator uses the desktop's locked Playwright/Tauri dependencies, verifies dimensions
and ICO frames, and derives a transparent monochrome macOS template tray icon from that glyph.
Windows/Linux retain the color tray icon. Refresh screenshots, hero recordings and the social
card afterward so published media uses the same mark.

Refresh the desktop screenshots, website crops and hero posters from the production browser
build with `node apps/desktop/scripts/capture-screenshots.mjs`. It uses sample data, its own
temporary build and preview server, and the same Chromium installation as the browser tests.
The Flow captures also regenerate two optimized website guide derivatives using the site's
`sharp` dependency; install both desktop and website npm dependencies before capturing.
Pass screenshot filenames after the command to refresh only those flows, for example
`node apps/desktop/scripts/capture-screenshots.mjs desktop-agent-stop-confirm-dark.png`.
Refresh the website social card with `(cd site && node scripts/og.mjs)` after updating the overview.

For CLI/TUI screenshots on macOS or Linux, build `cargo build -p holdmap`, create a virtual
environment outside the repository, install `scripts/requirements-terminal-screenshots.txt`
there, then run `scripts/capture-terminal-screenshots.py` with that Python. It captures actual
terminal output against local fixtures, filters the TUI to those fixtures and masks the account
name. Ports, PIDs, resource readings and plans remain actual. It terminates only fixtures it
started and keeps app state in a temporary directory; its dependencies are optional maintainer
tools. Agent captures use an exact owned PID, verify inferred stdio MCP evidence and separate
TCP/UDP plans, and cancel every stop review. Listener and process liveness checks confirm that
capture did not stop an agent or service. Recent-project history is excluded from these fixtures.

## Ground rules

- Process collection, ownership, protection and stop policy live in `holdmap-core`. The CLI,
  TUI, desktop and MCP adapters validate requests, coordinate confirmations and render core
  types; they never signal processes themselves.
- Process stops go through `ActionPlan` and `execute`, so dry runs, confirmations and
  the PID-reuse guard stay consistent.
- Local desktop shutdown uses the separate native Power policy and fixed OS adapters in
  `apps/desktop/src-tauri/src/power.rs`. Preserve trusted-window authorization, one-use
  confirmation, expiry and concurrency guards. Test with injected executors; automated tests
  must never shut down the machine.
- Desktop and MCP execution bind to a one-use preview and refuse changed effects. Fresh
  process protection checks apply immediately before execution as well as during planning.
- Use the shared credential redactor for display and serialization, and sanitize untrusted
  terminal text. Private/config writes must reject links before truncating a file.
- Process platform code stays behind `cfg` in `crates/holdmap-core/src/sys/`; desktop OS
  integrations stay in native adapter modules. Parsers are pure and tested with fixtures.
- New protected processes go in `crates/holdmap-core/src/safety.rs`. If you're unsure, protect it.
- User-facing changes update the README (and screenshots if the UI changes) in the same pull
  request, plus an entry under `## [Unreleased]` in `CHANGELOG.md`.

- Agent communication projection lives in `agents/flow.rs`; recording contracts, pure provider
  normalization, bounded storage and HTTP transport live separately in `holdmap-activity`.
  MCP observers receive catalog names and serialized sizes, never raw request/result values.
  Preserve fail-open agent hooks/MCP, explicit opt-in, loopback authentication, absolute I/O
  deadlines and metadata-only snapshots. Test adapters against provider wire fixtures.
  Keep evidence honest: sockets and chronological events never establish a tool call or handoff.
- Source parsing belongs in the activity hook/OTLP adapters; metadata invariants and
  sanitization remain shared. Add routes through the typed route contract, and reuse the
  credential policy and collector limits in clients and setup generation. Native recording
  commands use the shared session boundary; frontend recording actions stay separate from
  graph projection and selection. Prefer these concrete boundaries over speculative interfaces.

## Design review

Review changes against the concrete [SOLID boundaries](docs/architecture.md#7-design-notes).
Keep collection, decisions, execution, persistence and rendering with their existing owners.
Extend established registries and small interfaces when their contract fits, and select
concrete providers at composition points. Replacement providers and observers must preserve
result, failure and safety semantics; cover changed contracts with fixture regressions.
Introduce an abstraction for a real variation point or test seam. New OS effects still
require explicit executor authorization and identity/protection tests.

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
  Without `RELEASE_PLEASE_TOKEN`, prepare the same version changes manually, including
  `version.txt`, `.release-please-manifest.json`, the README source-version marker and generated CLI help;
  run `scripts/check-versions.sh` and the full validation before opening the release PR.
  README installer links stay on the website's release-resolved download section; they do not
  point at filenames from a release that is still being assembled.
- Pushing the tag runs `release.yml` ([dist](https://github.com/axodotdev/cargo-dist)): CLI
  archives for six targets (macOS arm64/x64, Linux gnu arm64/x64, Linux musl x64, Windows x64),
  shell and PowerShell installers, an npm package tarball, a
  CycloneDX SBOM, SHA-256 checksums, binaries built with `cargo auditable`, and GitHub build
  provenance (`gh attestation verify <file> -R santoshshinde2012/holdmap`). It creates a draft
  GitHub Release. npm registry publication is optional and disabled by default; release notes link
  the attached package tarball when no registry publishing job is configured.
- While the release is still a draft, `release.yml` calls `desktop-release.yml` (tauri-action), which
  adds the `.dmg`, `.msi`, NSIS `.exe`, `.AppImage`, `.deb` and `.rpm`, with checksums and provenance. Run it by hand with an empty tag
  for a dry run that keeps the bundles as workflow artifacts.
  Signed updater artifacts are verified against the public key and signed release version
  before upload. After all platforms succeed, one job uploads the complete `latest.json`.
  A final read-only asset check requires every CLI archive, desktop installer and checksum
  before publishing the release. Failed builds leave the previous public download in place.
- After publishing a stable release, the final job dispatches `Pages` on `main` with that
  version as `source_tag`. The trusted workflow verifies the published stable release,
  resolves its tag to an immutable commit, and checks out that commit for the website build.
  `build-info.json` records the actual checked-out source; the deployment uses the workflow's
  `main` revision. This avoids reusing a tag-context deployment while preserving installer source.
  Prereleases leave the latest stable website in place.

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

**Repository setup** (once; signing and registry integrations are optional):

| What | Needed for |
|---|---|
| Environment `github-pages`: selected deployment branches must allow Branch `main` | Required for both regular website deployments and stable-release dispatches. Published release source is resolved separately through the validated `source_tag` input. |
| Secret `HOMEBREW_TAP_TOKEN` (fine-grained, contents: write on `santoshshinde2012/homebrew-tap`), then in `dist-workspace.toml` add `"homebrew"` to `installers`, set `tap` and `publish-jobs = ["homebrew"]`, run `dist generate` and review/add the upstream Homebrew publishing job | Homebrew (`brew install santoshshinde2012/tap/holdmap`); off until then, so release notes don't advertise it |
| Secret `RELEASE_PLEASE_TOKEN` (fine-grained, contents and pull requests: write) | The release PR; the workflow skips with a notice until it exists |
| Secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Signed, notarised macOS app (optional) |
| Secrets `WINDOWS_CERTIFICATE` (base64 `.pfx`), `WINDOWS_CERTIFICATE_PASSWORD` | Signed Windows installers (optional) |
| `npm run tauri signer generate`, then secrets `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` and variable `TAURI_UPDATER_PUBKEY` | In-app updates with signed artifacts and an authenticated release version (optional; keep the private key backed up) |
| `"npm"` in `publish-jobs` in `dist-workspace.toml`, `dist generate`, secret `NPM_TOKEN`; after restoring the publishing job, pass `--npm-registry-published` to both release-note preparation calls | Publishing the npm package (optional) |

**Release steps.** Merging the release PR prepares the version; publishing starts when a
maintainer pushes its tag.

1. Review the release PR's version changes and changelog, then merge it after CI passes.
2. Fetch `main` and check out the release PR's merge commit. Run `scripts/check-versions.sh`
   and confirm that `version.txt` matches the intended `X.Y.Z`.
3. Create an annotated tag at that commit: `git tag -a vX.Y.Z -m "holdmap X.Y.Z"`, then
   `git push origin vX.Y.Z` (replace `X.Y.Z` with the release version). Use `-s` instead of
   `-a` when the maintainer has a configured signing key. Artifact provenance independently
   binds the builds to the tagged source and release workflow.
4. Wait for the complete `Release` run, including its reusable desktop jobs and final asset
   verification. The release stays a draft until all six CLI archives, seven desktop installers
   and their checksums are uploaded. Check their build provenance, then confirm the
   stable release's dispatched `Pages` deployment uses the tagged commit and new downloads.
   Check `build-info.json` against the tag's commit. Confirm the `github-pages` environment
   allows Branch `main` before publishing; the workflow does not change environment policies.
   Without signing credentials, macOS is ad-hoc signed without notarization and Windows is
   unsigned. Without a configured signed updater, desktop users download the new installer
   manually; do not advertise automatic updates.

If every artifact build succeeded but final publication failed, fix and validate the
publication code on `main`, then run `gh workflow run release.yml --ref main --field tag=vX.Y.Z --field run_id=RUN_ID`
with the existing draft's tag and its original tag-build run ID. This recovery path checks
that the original run completed every required artifact and optional updater job, then skips
builds, verifies every existing installer and checksum, and publishes only a complete draft
before dispatching the tagged website. It runs only from `main`; it never moves the release tag. If publication already
succeeded and only the website dispatch failed, run
`gh workflow run pages.yml --ref main --field source_tag=vX.Y.Z` instead. Dispatches require a
published stable tag and run only from `main`; annotated tags are resolved before checkout,
and the checked-out commit is verified before dependencies or selected release source code execute.

## Licence

By contributing, you agree that your contributions are dual-licensed under MIT OR Apache-2.0, as
described in the [README](README.md#license).

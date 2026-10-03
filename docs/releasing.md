# Releasing portwise

portwise ships two kinds of release from the same repository:

| Release | Tag | Workflow | Produces |
|---|---|---|---|
| CLI (includes the TUI and the MCP server) | `vX.Y.Z` | `.github/workflows/release.yml` (cargo-dist) | Archives for 5 targets, shell and PowerShell installers, a Homebrew formula |
| Desktop app | `desktop-vX.Y.Z` | `.github/workflows/desktop-release.yml` (tauri-action) | `.dmg`/`.app` (Apple silicon and Intel), `.msi`/`.exe`, `.deb`/`.AppImage`/`.rpm` on a draft release |

## Versioning

portwise follows [Semantic Versioning](https://semver.org/). Before 1.0, a minor bump (0.2.0) may
change the CLI, JSON output or MCP tools; a patch bump (0.1.1) only fixes bugs.

The version lives in four places, which must agree:

- `Cargo.toml`: `[workspace.package] version` (the crates inherit it), plus the `version` of
  `portwise-core` and `portwise-mcp` in `[workspace.dependencies]`
- `apps/desktop/src-tauri/tauri.conf.json`: `version`
- `apps/desktop/package.json`: `version` (then run `npm install` to update `package-lock.json`)
- `docs/cli.md`, which shows the version: regenerate it with `scripts/gen-docs.sh`

## Checklist

1. **Start from a green `main`.** CI must pass on Linux, macOS and Windows.
2. **Update the changelog.** Move the entries under `## [Unreleased]` in `CHANGELOG.md` into a new
   `## [X.Y.Z] - YYYY-MM-DD` section, keep an empty `[Unreleased]` section, and update the comparison
   links at the bottom.
3. **Bump the version** in the places listed above, then run the full check:

   ```sh
   cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test
   (cd apps/desktop && npm run check && npm test && npm run build)
   scripts/gen-docs.sh
   ```

4. **Commit** with `chore(release): vX.Y.Z` and merge it to `main`.
5. **Preview the CLI release.** `dist plan` (cargo-dist 0.33) lists what will be built. The release
   workflow also runs its plan step on every pull request.
6. **Tag and push the CLI release:**

   ```sh
   git tag -a vX.Y.Z -m "portwise X.Y.Z"
   git push origin vX.Y.Z
   ```

   cargo-dist builds `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`,
   `aarch64-unknown-linux-gnu` and `x86_64-pc-windows-msvc`, creates the GitHub release with the
   changelog section as its notes, and pushes the formula to `santoshshinde/homebrew-tap`. A tag
   with a pre-release suffix (`v0.2.0-rc.1`) is marked as a pre-release and skips Homebrew.
7. **Tag and push the desktop release** (it can share the version or move on its own):

   ```sh
   git tag -a desktop-vX.Y.Z -m "portwise desktop X.Y.Z"
   git push origin desktop-vX.Y.Z
   ```

   tauri-action uploads the installers to a **draft** release. Check the assets, write the notes
   (link to the changelog), and publish it.
8. **Smoke-test** the published installers on each platform: `portwise --version`,
   `portwise list`, `portwise explain 1`, and opening the desktop app.

## Secrets

| Secret | Used by | Needed for |
|---|---|---|
| `GITHUB_TOKEN` | both workflows | Creating releases (provided by GitHub) |
| `HOMEBREW_TAP_TOKEN` | `release.yml` | Pushing the formula to `santoshshinde/homebrew-tap` |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | `desktop-release.yml` | Optional: signing and notarising the macOS app |

Without the Apple secrets the macOS app is unsigned, and Gatekeeper asks users to open it from the
context menu the first time. Windows builds are unsigned too, so SmartScreen may warn.

## If something goes wrong

- **A build fails after tagging.** Fix it on `main`, delete the tag locally and on GitHub
  (`git tag -d vX.Y.Z && git push origin :refs/tags/vX.Y.Z`), delete the draft release if one was
  created, and tag again. Never reuse a version that was already published.
- **The cargo-dist workflow is out of date.** Run `dist init` with the version pinned in
  `dist-workspace.toml`, review the regenerated `release.yml`, and commit it on its own.

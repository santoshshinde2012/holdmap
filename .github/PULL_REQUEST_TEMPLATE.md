## What and why

<!-- What does this change, and what problem does it solve? Link the issue: "Fixes #123". -->

## How it was tested

<!-- Commands you ran, platforms you tried, screenshots for UI changes. -->

## Checklist

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test` pass
- [ ] Desktop changes: `npm run check`, `npm test` and `npm run build` pass in `apps/desktop`
- [ ] Anything that stops processes goes through `ActionPlan` and `execute`, and has a test
- [ ] New or changed commands and flags: `docs/cli.md` regenerated (`scripts/gen-docs.sh`) and the README updated
- [ ] User-visible changes are in `CHANGELOG.md` under `[Unreleased]`
- [ ] New files follow the [naming conventions](../CONTRIBUTING.md#naming-conventions)
- [ ] Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)

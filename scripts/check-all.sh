#!/usr/bin/env bash
# Reproduce the workspace checks locally. Install the documented prerequisites first.
set -euo pipefail
cd "$(dirname "$0")/.."

for tool in cargo node npm python3 actionlint gitleaks; do
  command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool (see CONTRIBUTING.md)" >&2; exit 1; }
done
cargo deny --version >/dev/null
node --input-type=module -e '
  const [major, minor] = process.versions.node.split(".").map(Number);
  if (major < 22 || (major === 22 && minor < 12)) {
    console.error("Node.js 22.12 or newer is required; run nvm use.");
    process.exit(1);
  }
'

scripts/check-versions.sh
scripts/check-secrets.sh
python3 scripts/test-release-security.py
python3 scripts/verify-vendored-glib.py
actionlint
git diff --check

# Build the frontend before native checks: Tauri embeds these files.
(
  cd apps/desktop
  npm ci
  npm run check
  npm test
  npm run test:e2e
  npm audit
)
(
  cd site
  npm ci
  npm run check
  HOLDMAP_SITE_OFFLINE=1 npm run build
  npm test
  npm run test:demo
  npm audit
)

cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
if [[ "$(uname -s)" == Linux ]]; then
  cargo test -p holdmap-desktop --locked --config 'profile.test.package.glib.opt-level=3' --test glib_security
fi
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked -p holdmap-core -p holdmap-mcp -p holdmap
cargo build --locked -p holdmap-desktop
cargo deny --all-features check

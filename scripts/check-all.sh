#!/usr/bin/env bash
# Reproduce the workspace checks locally. Install the documented prerequisites first.
set -euo pipefail
cd "$(dirname "$0")/.."

for tool in cargo node npm python3 actionlint gitleaks; do
  command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool (see CONTRIBUTING.md)" >&2; exit 1; }
done
cargo deny --version >/dev/null
if [[ "$(uname -s)" == Darwin ]]; then
  for tool in ffmpeg ffprobe; do
    command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool (install ffmpeg; see CONTRIBUTING.md)" >&2; exit 1; }
  done
fi
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
python3 scripts/verify-vendored-global-hotkey.py
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
cargo test --workspace --all-targets --locked --no-fail-fast
cargo test --workspace --doc --locked --no-fail-fast
if [[ "$(uname -s)" == Linux ]]; then
  G_DEBUG=fatal-warnings MALLOC_PERTURB_=165 \
    cargo test -p holdmap-desktop --locked --config 'profile.test.package.glib.opt-level=3' --test glib_security
  xvfb-run -a env HOLDMAP_X11_TEST=1 \
    cargo test -p holdmap-desktop --locked --test hotkey_x11 -- --include-ignored --test-threads=1
elif [[ "$(uname -s)" == Darwin ]]; then
  python3 scripts/check-updater-runtime.py
  python3 scripts/check-screen-recording-runtime.py
fi
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked -p holdmap-core -p holdmap-mcp -p holdmap-activity -p holdmap
# Embed the built frontend, matching the production Tauri build's feature selection.
cargo build --locked -p holdmap-desktop --features tauri/custom-protocol
cargo deny --all-features check

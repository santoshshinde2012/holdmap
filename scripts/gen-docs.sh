#!/usr/bin/env bash
# Generates the reference material that comes from the clap definitions:
#   target/docs/man/          holdmap.1 and holdmap-<command>.1
#   target/docs/completions/  bash, zsh, fish, PowerShell and Elvish completions
#   docs/cli.md               the CLI reference (checked by `cargo test`)
# Usage: scripts/gen-docs.sh
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
OUT=target/docs
cargo build -q -p holdmap
BIN=target/debug/holdmap
rm -rf "$OUT" && mkdir -p "$OUT/man" "$OUT/completions"
"$BIN" man --out-dir "$OUT/man"
"$BIN" completions bash > "$OUT/completions/holdmap.bash"
"$BIN" completions zsh > "$OUT/completions/_holdmap"
"$BIN" completions fish > "$OUT/completions/holdmap.fish"
"$BIN" completions powershell > "$OUT/completions/_holdmap.ps1"
"$BIN" completions elvish > "$OUT/completions/holdmap.elv"
HOLDMAP_BLESS=1 cargo test -q -p holdmap cli_reference >/dev/null
echo "man pages:   $(ls "$OUT/man" | wc -l | tr -d ' ') in $OUT/man"
echo "completions: $(ls "$OUT/completions" | tr '\n' ' ')in $OUT/completions"
echo "docs/cli.md: regenerated"

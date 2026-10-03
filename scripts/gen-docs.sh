#!/usr/bin/env bash
# Generates the reference material that comes from the clap definitions:
#   target/docs/man/          portwise.1 and portwise-<command>.1
#   target/docs/completions/  bash, zsh, fish, PowerShell and Elvish completions
#   docs/cli.md               the CLI reference (checked by `cargo test`)
# Usage: scripts/gen-docs.sh
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
OUT=target/docs
cargo build -q -p portwise
BIN=target/debug/portwise
rm -rf "$OUT" && mkdir -p "$OUT/man" "$OUT/completions"
"$BIN" man --out-dir "$OUT/man"
"$BIN" completions bash > "$OUT/completions/portwise.bash"
"$BIN" completions zsh > "$OUT/completions/_portwise"
"$BIN" completions fish > "$OUT/completions/portwise.fish"
"$BIN" completions powershell > "$OUT/completions/_portwise.ps1"
"$BIN" completions elvish > "$OUT/completions/portwise.elv"
PORTWISE_BLESS=1 cargo test -q -p portwise cli_reference >/dev/null
echo "man pages:   $(ls "$OUT/man" | wc -l | tr -d ' ') in $OUT/man"
echo "completions: $(ls "$OUT/completions" | tr '\n' ' ')in $OUT/completions"
echo "docs/cli.md: regenerated"

#!/usr/bin/env bash
# Type-check (cargo clippy) the macOS and Windows backends from Linux.
#
# Windows: needs only `rustup target add x86_64-pc-windows-gnu`.
# macOS:   netstat2's build script runs bindgen against the macOS SDK, which isn't available on
#          Linux, so we temporarily [patch] it with a type-only stub built from the real crate's types.
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add x86_64-pc-windows-gnu aarch64-apple-darwin >/dev/null
CRATES=(-p holdmap-core -p holdmap -p holdmap-mcp -p holdmap-activity -p holdmap-analyzers)

# Keep the temporary dependency patch and its lockfile out of the working checkout.
# Copy current source, including uncommitted changes, and reuse only build output.
cross_dir=$(mktemp -d "${TMPDIR:-/tmp}/holdmap-cross.XXXXXXXX")
trap 'rm -rf -- "$cross_dir"' EXIT
cp Cargo.toml Cargo.lock rust-toolchain.toml "$cross_dir/"
cp -R crates vendor "$cross_dir/"
mkdir -p "$cross_dir/apps/desktop"
cp -R apps/desktop/src-tauri "$cross_dir/apps/desktop/"
cross_target_dir=${CARGO_TARGET_DIR:-"$PWD/target"}
if [[ "$cross_target_dir" != /* ]]; then cross_target_dir="$PWD/$cross_target_dir"; fi
export CARGO_TARGET_DIR="$cross_target_dir"
cd "$cross_dir"

echo "==> Windows (x86_64-pc-windows-gnu)"
cargo clippy --target x86_64-pc-windows-gnu "${CRATES[@]}" --all-targets --locked -- -D warnings

echo "==> macOS (aarch64-apple-darwin, stubbed netstat2)"
cargo fetch -q --locked
real=$(cargo metadata --locked --format-version 1 | python3 -c '
import json, pathlib, sys
packages = [p for p in json.load(sys.stdin)["packages"] if p["name"] == "netstat2"]
if len(packages) != 1:
    sys.exit("Expected exactly one locked netstat2 dependency")
print(pathlib.Path(packages[0]["manifest_path"]).parent)
')
stub="$cross_dir/netstat2-stub"
mkdir "$stub"
cp -r "$real/src/types" "$stub/types"
mkdir -p "$stub/src" && mv "$stub/types" "$stub/src/types"
cat > "$stub/Cargo.toml" <<TOML
[package]
name = "netstat2"
version = "$(basename "$real" | sed 's/netstat2-//')"
edition = "2021"
[dependencies]
bitflags = "2"
thiserror = "2"
TOML
cat > "$stub/src/lib.rs" <<'RS'
#[macro_use]
extern crate bitflags;
mod types;
pub use types::error::*;
pub use types::*;
pub fn get_sockets_info(_: AddressFamilyFlags, _: ProtocolFlags) -> Result<Vec<SocketInfo>, Error> {
    unimplemented!("type-check stub")
}
RS
# Extend the existing patch table through Cargo's configuration, preserving the
# audited GLib/global-hotkey overrides without creating a duplicate TOML table.
cargo clippy --config "patch.crates-io.netstat2.path=\"$stub\"" --target aarch64-apple-darwin "${CRATES[@]}" --all-targets -- -D warnings
echo "==> cross checks passed"

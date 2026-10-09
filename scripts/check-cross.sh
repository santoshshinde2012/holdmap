#!/usr/bin/env bash
# Type-check (cargo clippy) the macOS and Windows backends from Linux.
#
# Windows: needs only `rustup target add x86_64-pc-windows-gnu`.
# macOS:   netstat2's build script runs bindgen against the macOS SDK, which isn't available on
#          Linux, so we temporarily [patch] it with a type-only stub built from the real crate's types.
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add x86_64-pc-windows-gnu aarch64-apple-darwin >/dev/null
CRATES=(-p holdmap-core -p holdmap -p holdmap-mcp)

echo "==> Windows (x86_64-pc-windows-gnu)"
cargo clippy --target x86_64-pc-windows-gnu "${CRATES[@]}" --all-targets -- -D warnings

echo "==> macOS (aarch64-apple-darwin, stubbed netstat2)"
cargo fetch -q
real=$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/netstat2-0.11.* | sort -V | tail -1)
stub=$(mktemp -d)
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
cp Cargo.toml "$stub/Cargo.toml.orig"; cp Cargo.lock "$stub/Cargo.lock.orig"
restore() { cp "$stub/Cargo.toml.orig" Cargo.toml; cp "$stub/Cargo.lock.orig" Cargo.lock; rm -rf "$stub"; }
trap restore EXIT
printf '\n[patch.crates-io]\nnetstat2 = { path = "%s" }\n' "$stub" >> Cargo.toml
cargo clippy --target aarch64-apple-darwin "${CRATES[@]}" --all-targets -- -D warnings
echo "==> cross checks passed"

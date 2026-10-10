#!/usr/bin/env python3
"""Verify the audited GLib fixes, their unchanged source snapshot and Cargo resolution.

This check is offline except for Cargo's normal locked metadata resolution. It never builds
GTK or launches the app. Requires Python 3.11+ and Cargo; run from any working directory.
"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tomllib


PROVENANCE_SHA256 = "9ab0bd1224628b76f8c7079e857392f2c9e14989bd418e448d408070ae62f1a0"
ARCHIVE_SHA256 = "233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5"
UPSTREAM_COMMIT = "42b9caf98e03ded086362d9653ca58fe94dc8658"
FIX_COMMITS = [
    "b5a4071e439bef2b5eea76c3aa25e5ae84839e34",
    "f54ceb387a2b6d6830753c1522cba7c858fb1aed",
]
LINUX_TARGET = 'cfg(target_os = "linux")'
PATCHES = {
    "src/variant_iter.rs": [
        (
            b"let p: *mut libc::c_char = std::ptr::null_mut();",
            b"let mut p: *mut libc::c_char = std::ptr::null_mut();",
        ),
        (b"                &p,", b"                &mut p,"),
    ],
    "src/boxed_inline.rs": [
        (
            b"let v_ptr = $crate::ffi::g_malloc(std::mem::size_of::<$ffi_name>()) as *mut $ffi_name;",
            b"let v_ptr = $crate::ffi::g_malloc(std::mem::size_of::<$ffi_name>() * std::cmp::max(t.len(), 1)) as *mut $ffi_name;",
        ),
        (
            b"let v_ptr = $crate::ffi::g_malloc(std::mem::size_of::<$ffi_name>() * std::cmp::max(t.len(), 1)) as *mut $ffi_name;",
            b"let v_ptr = $crate::ffi::g_malloc0(std::mem::size_of::<$ffi_name>() * std::cmp::max(t.len(), 1)) as *mut $ffi_name;",
        ),
    ],
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def verify(root):
    vendor = root / "vendor/glib"
    require(vendor.is_dir() and not vendor.is_symlink(), "vendor/glib must be a real directory")
    provenance_bytes = (root / "vendor/glib-provenance.json").read_bytes()
    require(digest(provenance_bytes) == PROVENANCE_SHA256, "GLib provenance manifest changed; review the source snapshot and verifier together")
    provenance = json.loads(provenance_bytes)
    require(provenance["package"] == "glib" and provenance["version"] == "0.18.5", "GLib must keep its truthful 0.18.5 version")
    require(provenance["archive_sha256"] == ARCHIVE_SHA256 and provenance["upstream_commit"] == UPSTREAM_COMMIT and provenance["fix_commits"] == FIX_COMMITS, "unexpected upstream source or security fixes")
    require(provenance["patched_files"].keys() == PATCHES.keys(), "only the two audited GLib source files may change")
    upstream = provenance["upstream_files"]
    patched = provenance["patched_files"]
    files = {}
    for path in vendor.rglob("*"):
        require(not path.is_symlink(), f"vendored source cannot contain symlinks: {path!s}")
        if path.is_file():
            files[path.relative_to(vendor).as_posix()] = path
        else:
            require(path.is_dir(), f"unexpected vendored source entry: {path!s}")
    require(files.keys() == upstream.keys(), "vendored GLib file set changed; build/cache files must stay outside the source snapshot")
    for name, expected in upstream.items():
        require(digest(files[name].read_bytes()) == patched.get(name, expected), f"vendored GLib content changed: {name!r}")
    for name, replacements in PATCHES.items():
        original = files[name].read_bytes()
        for before, after in reversed(replacements):
            require(original.count(after) == 1, f"GLib source must contain the exact audited fix: {name!r}")
            original = original.replace(after, before, 1)
        require(digest(original) == upstream[name], f"GLib backport must contain only the audited upstream and local line changes: {name!r}")

    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    require(manifest["patch"]["crates-io"]["glib"] == {"path": "vendor/glib"}, "Cargo must use the audited local GLib override")
    require("vendor/glib" in manifest["workspace"].get("exclude", []), "third-party GLib must stay outside the first-party workspace")
    desktop_manifest = tomllib.loads((root / "apps/desktop/src-tauri/Cargo.toml").read_text())
    require(desktop_manifest["target"][LINUX_TARGET]["dev-dependencies"]["glib"] == {"path": "../../../vendor/glib"}, "Linux GLib regressions must depend directly on the audited vendor/glib source")
    crate = tomllib.loads((vendor / "Cargo.toml").read_text())
    require(crate["package"]["name"] == "glib" and crate["package"]["version"] == "0.18.5", "do not relabel the backport as an upstream fixed release")
    locked = tomllib.loads((root / "Cargo.lock").read_text())
    glib = [package for package in locked["package"] if package["name"] == "glib"]
    require(len(glib) == 1 and glib[0]["version"] == "0.18.5" and "source" not in glib[0], "Cargo.lock must contain only the local GLib backport")
    config = tomllib.loads((root / "deny.toml").read_text())
    require(config["advisories"]["unsound"] == "all", "cargo-deny must check transitive soundness advisories")
    require(config["advisories"].get("ignore", []) == [], "GLib remediation must not ignore advisories")

    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--all-features", "--format-version", "1", "--manifest-path", str(root / "Cargo.toml")],
        cwd=root, capture_output=True, text=True, timeout=180, check=False,
    )
    require(result.returncode == 0, f"locked Cargo metadata failed: {result.stderr[-2000:]!r}")
    metadata = json.loads(result.stdout)
    resolved = [package for package in metadata["packages"] if package["name"] == "glib"]
    require(len(resolved) == 1, "Cargo resolved another GLib copy")
    package = resolved[0]
    require(package["version"] == "0.18.5" and package["source"] is None and Path(package["manifest_path"]).resolve() == (vendor / "Cargo.toml").resolve(), "Cargo resolution bypassed the verified GLib source")
    require(package["id"] not in metadata["workspace_members"], "vendored GLib is not first-party workspace code")
    gtk = [package for package in metadata["packages"] if package["name"] == "gtk"]
    require(len(gtk) == 1, "expected the desktop GTK3 dependency")
    gtk_node = next(node for node in metadata["resolve"]["nodes"] if node["id"] == gtk[0]["id"])
    require(any(dep["name"] == "glib" and dep["pkg"] == package["id"] for dep in gtk_node["deps"]), "GTK does not consume the verified GLib package")
    desktop = [candidate for candidate in metadata["packages"] if candidate["name"] == "holdmap-desktop"]
    require(len(desktop) == 1, "expected the native desktop package")
    desktop_node = next(node for node in metadata["resolve"]["nodes"] if node["id"] == desktop[0]["id"])
    desktop_glib = [dep for dep in desktop_node["deps"] if dep["name"] == "glib"]
    require(len(desktop_glib) == 1 and desktop_glib[0]["pkg"] == package["id"], "desktop regressions and GTK must consume the same verified GLib package")
    require(any(kind["kind"] == "dev" and kind["target"] == LINUX_TARGET for kind in desktop_glib[0]["dep_kinds"]), "the verified desktop GLib dependency must be a Linux development dependency")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent, help="repository root (defaults to this script's checkout)")
    args = parser.parse_args()
    try:
        verify(args.root.resolve())
    except (OSError, ValueError, KeyError, StopIteration, subprocess.TimeoutExpired) as error:
        print(f"GLib backport verification failed: {str(error)!r}", file=sys.stderr)
        return 1
    print("GLib 0.18.5 upstream snapshot, both security backports, local initialization fix, lockfile, GTK and Linux regression resolution verified.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Verify the minimal X11 hotkey fixes, published source snapshot and Cargo graph.

Requires Python 3.11+ and Cargo. Locked metadata resolution may fetch dependencies;
the check does not build the desktop or connect to any display.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tomllib


PROVENANCE_SHA256 = "7d57f75676bd440b111e913d5586f00499efd64553561f9c5d284702a984806e"
ARCHIVE_SHA256 = "8c386b0a4a70cb2d39fffd74480f985b6f0bfbcb934b6a6b6b7e630e448f242e"
UPSTREAM_COMMIT = "2a620bf3852008b568f6d36c2baedcc3dd0822f2"
RELATED_PR_COMMIT = "11b352c5b77d6cd03fddc97bb951586f4d8fd927"
PATCH_SHA256 = "bd50e23016ac294f5ec6e0eea92f209239c6a53fe07cbe9305a89a78e37e66d7"
PATCHED_FILE = "src/platform_impl/x11/mod.rs"
LINUX_TARGET = 'cfg(target_os = "linux")'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def reverse_patch(source, patch):
    """Reverse the exact single-file unified diff without external patch utilities."""
    lines = patch.decode().splitlines(keepends=True)
    require(lines[:2] == [f"--- a/{PATCHED_FILE}\n", f"+++ b/{PATCHED_FILE}\n"],
            "hotkey patch must change only the X11 source file")
    source_lines = source.decode().splitlines(keepends=True)
    restored = []
    position = 0
    index = 2
    hunks = 0
    while index < len(lines):
        match = re.fullmatch(r"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@\n", lines[index])
        require(match is not None, "unexpected hotkey patch hunk")
        old_start, old_count, new_start, new_count = match.groups()
        old_start, new_start = int(old_start) - 1, int(new_start) - 1
        old_count = int(old_count) if old_count is not None else 1
        new_count = int(new_count) if new_count is not None else 1
        require(position <= new_start <= len(source_lines), "hotkey patch hunks overlap")
        restored.extend(source_lines[position:new_start])
        require(len(restored) == old_start, "hotkey patch original offset changed")
        before, after = [], []
        index += 1
        while index < len(lines) and not lines[index].startswith("@@ "):
            line = lines[index]
            require(line[:1] in (" ", "+", "-"), "unexpected hotkey patch content")
            if line[0] in " -":
                before.append(line[1:])
            if line[0] in " +":
                after.append(line[1:])
            index += 1
        require(len(before) == old_count and len(after) == new_count, "hotkey patch line count changed")
        require(source_lines[new_start:new_start + new_count] == after,
                "hotkey patch does not match the audited source")
        restored.extend(before)
        position = new_start + new_count
        hunks += 1
    require(hunks > 0, "hotkey patch must contain the reviewed X11 fixes")
    restored.extend(source_lines[position:])
    return "".join(restored).encode()


def verify(root):
    vendor = root / "vendor/global-hotkey"
    require(vendor.is_dir() and not vendor.is_symlink(), "vendor/global-hotkey must be a real directory")
    provenance_bytes = (root / "vendor/global-hotkey-provenance.json").read_bytes()
    require(digest(provenance_bytes) == PROVENANCE_SHA256,
            "hotkey provenance changed; review the snapshot, patch and verifier together")
    provenance = json.loads(provenance_bytes)
    require(provenance["package"] == "global-hotkey" and provenance["version"] == "0.8.0",
            "global-hotkey must retain its truthful published version")
    require(provenance["archive_sha256"] == ARCHIVE_SHA256
            and provenance["upstream_commit"] == UPSTREAM_COMMIT
            and provenance["related_upstream_commit"] == RELATED_PR_COMMIT,
            "unexpected hotkey upstream snapshot or related fix")
    require(provenance["patched_files"].keys() == {PATCHED_FILE},
            "only the reviewed X11 backend may differ from upstream")
    upstream, patched = provenance["upstream_files"], provenance["patched_files"]
    files = {}
    for path in vendor.rglob("*"):
        require(not path.is_symlink(), f"vendored source cannot contain symlinks: {path!s}")
        if path.is_file():
            files[path.relative_to(vendor).as_posix()] = path
        else:
            require(path.is_dir(), f"unexpected vendored source entry: {path!s}")
    require(files.keys() == upstream.keys(), "vendored hotkey file set changed; keep build output outside the snapshot")
    for name, expected in upstream.items():
        require(digest(files[name].read_bytes()) == patched.get(name, expected),
                f"vendored hotkey source changed: {name!r}")
    patch_bytes = (root / "vendor/global-hotkey-x11.patch").read_bytes()
    require(digest(patch_bytes) == PATCH_SHA256 == provenance["patch_sha256"],
            "reviewed hotkey diff changed")
    original = reverse_patch(files[PATCHED_FILE].read_bytes(), patch_bytes)
    require(digest(original) == upstream[PATCHED_FILE],
            "hotkey backport must reverse exactly to the published X11 source")

    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    require(manifest["patch"]["crates-io"]["global-hotkey"] == {"path": "vendor/global-hotkey"},
            "Cargo must use the audited local hotkey override")
    require("vendor/global-hotkey" in manifest["workspace"].get("exclude", []),
            "vendored hotkey code must stay outside the first-party workspace")
    crate = tomllib.loads((vendor / "Cargo.toml").read_text())
    require(crate["package"]["name"] == "global-hotkey" and crate["package"]["version"] == "0.8.0"
            and crate["package"]["license"] == "Apache-2.0 OR MIT",
            "keep the published hotkey package identity and license")
    locked = tomllib.loads((root / "Cargo.lock").read_text())
    packages = [package for package in locked["package"] if package["name"] == "global-hotkey"]
    require(len(packages) == 1 and packages[0]["version"] == "0.8.0" and "source" not in packages[0],
            "Cargo.lock must contain only the local hotkey 0.8.0 backport")

    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--all-features", "--format-version", "1",
         "--manifest-path", str(root / "Cargo.toml")],
        cwd=root, capture_output=True, text=True, timeout=180, check=False,
    )
    require(result.returncode == 0, f"locked Cargo metadata failed: {result.stderr[-2000:]!r}")
    metadata = json.loads(result.stdout)
    resolved = [package for package in metadata["packages"] if package["name"] == "global-hotkey"]
    require(len(resolved) == 1, "Cargo resolved another global-hotkey copy")
    package = resolved[0]
    require(package["version"] == "0.8.0" and package["source"] is None
            and Path(package["manifest_path"]).resolve() == (vendor / "Cargo.toml").resolve(),
            "Cargo resolution bypassed the verified hotkey source")
    require(package["id"] not in metadata["workspace_members"], "vendored hotkey code is not a workspace member")
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    plugin = [candidate for candidate in metadata["packages"] if candidate["name"] == "tauri-plugin-global-shortcut"]
    require(len(plugin) == 1, "expected the genuine Tauri global-shortcut plugin")
    require(any(dep["name"] == "global_hotkey" and dep["pkg"] == package["id"]
                for dep in nodes[plugin[0]["id"]]["deps"]),
            "the Tauri plugin does not consume the verified hotkey source")
    desktop = [candidate for candidate in metadata["packages"] if candidate["name"] == "holdmap-desktop"]
    require(len(desktop) == 1, "expected the native desktop package")
    dependencies = [dep for dep in nodes[desktop[0]["id"]]["deps"] if dep["name"] == "global_hotkey"]
    require(len(dependencies) == 1 and dependencies[0]["pkg"] == package["id"],
            "native regressions and the plugin must consume the same verified hotkey package")
    require(any(kind["kind"] == "dev" and kind["target"] == LINUX_TARGET
                for kind in dependencies[0]["dep_kinds"]),
            "native hotkey regressions must directly test the Linux dependency")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
    try:
        verify(args.root.resolve())
    except (OSError, ValueError, KeyError, StopIteration, subprocess.TimeoutExpired) as error:
        print(f"global-hotkey backport verification failed: {str(error)!r}", file=sys.stderr)
        return 1
    print("global-hotkey 0.8.0 published snapshot, minimal X11 diff, licenses, lockfile, Tauri plugin and Linux regression resolution verified.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env bash
# Every version that release-please bumps must agree; CI fails if one is edited by hand.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 - <<'PY'
import json, re, sys, tomllib
cargo = tomllib.load(open("Cargo.toml", "rb"))
lock = tomllib.load(open("Cargo.lock", "rb"))
pkg_lock = json.load(open("apps/desktop/package-lock.json"))
versions = {
    "Cargo.toml [workspace.package]": cargo["workspace"]["package"]["version"],
    "Cargo.toml holdmap-core dep": cargo["workspace"]["dependencies"]["holdmap-core"]["version"],
    "Cargo.toml holdmap-mcp dep": cargo["workspace"]["dependencies"]["holdmap-mcp"]["version"],
    "apps/desktop/package.json": json.load(open("apps/desktop/package.json"))["version"],
    "apps/desktop/package-lock.json": pkg_lock["version"],
    "apps/desktop/package-lock.json packages['']": pkg_lock["packages"][""]["version"],
    "apps/desktop/src-tauri/tauri.conf.json": json.load(open("apps/desktop/src-tauri/tauri.conf.json"))["version"],
    "version.txt": open("version.txt").read().strip(),
    ".release-please-manifest.json": json.load(open(".release-please-manifest.json"))["."],
}
readme = open("README.md").read()
block = re.search(r"x-release-please-start-version -->\n(.*?)<!-- x-release-please-end", readme, re.S)
if not block:
    sys.exit("README.md lost its x-release-please version block (desktop download links)")
for n, line in enumerate(block.group(1).splitlines()):
    found = re.findall(r"\d+\.\d+\.\d+", line)
    if len(found) > 1:
        sys.exit(f"README.md download block line {n + 1} has more than one version; release-please only bumps the first")
    if found:
        versions[f"README.md download link {n + 1}"] = found[0]
for p in lock["package"]:
    if p["name"] in ("holdmap", "holdmap-core", "holdmap-mcp", "holdmap-desktop"):
        versions[f"Cargo.lock {p['name']}"] = p["version"]
want = versions["Cargo.toml [workspace.package]"]
bad = {k: v for k, v in versions.items() if v != want}
if not re.fullmatch(r"\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?", want):
    sys.exit(f"version {want!r} isn't SemVer")
if bad:
    sys.exit("versions disagree with Cargo.toml ({}):\n".format(want) + "\n".join(f"  {k}: {v}" for k, v in bad.items()))
print(f"all {len(versions)} versions are {want}")
PY

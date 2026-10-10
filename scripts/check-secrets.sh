#!/usr/bin/env bash
# Scan committed history and the current source, including new files, without build caches.
set -euo pipefail
cd "$(dirname "$0")/.."
for tool in git gitleaks python3; do
  command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool (see CONTRIBUTING.md)" >&2; exit 1; }
done
if [[ "$(git rev-parse --is-shallow-repository)" == true ]]; then
  echo "Secret scanning requires full history: run git fetch --unshallow first." >&2
  exit 1
fi

gitleaks git --log-opts=--all --redact=100 --no-banner --timeout 120 .

secret_source_dir=$(mktemp -d "${TMPDIR:-/tmp}/holdmap-secret-scan.XXXXXXXX")
trap 'rm -rf -- "$secret_source_dir"' EXIT
python3 - "$secret_source_dir" <<'PY'
import os
from pathlib import Path
import subprocess
import sys

destination = Path(sys.argv[1])
paths = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"])
for raw in paths.split(b"\0"):
    if not raw:
        continue
    source = Path(os.fsdecode(raw))
    if not source.exists() and not source.is_symlink():
        continue
    target = destination / source
    target.parent.mkdir(parents=True, exist_ok=True)
    # Scan the link text itself; never follow repository symlinks into external files.
    if source.is_symlink():
        target.write_text(os.readlink(source))
    elif source.is_file():
        target.write_bytes(source.read_bytes())
PY
gitleaks dir --redact=100 --no-banner --timeout 120 "$secret_source_dir"

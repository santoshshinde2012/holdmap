#!/usr/bin/env bash
# Validate release context before dist or any downstream publishing job runs.
set -euo pipefail

case "${GITHUB_EVENT_NAME:-}" in
  pull_request)
    exec dist plan --output-format=json
    ;;
  push)
    if [[ "${GITHUB_REF_TYPE:-}" != tag ]] ||
       ! [[ "${GITHUB_REF_NAME:-}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
      echo 'Release requires a vX.Y.Z tag (optionally with a prerelease suffix).' >&2
      exit 1
    fi
    exec dist host --steps=create --tag "$GITHUB_REF_NAME" --output-format=json
    ;;
  *)
    echo 'Unsupported release event.' >&2
    exit 1
    ;;
esac

#!/usr/bin/env bash
set -euo pipefail

count=$(awk '/^## \[Unreleased\]/{count++} END {print count+0}' CHANGELOG.md)
if [[ "$count" -ne 1 ]]; then
  echo "Expected exactly one top-level [Unreleased] heading in CHANGELOG.md, found: $count"
  exit 1
fi

echo "CHANGELOG [Unreleased] heading check passed."

#!/usr/bin/env bash
# Check for breaking public API changes using cargo-public-api.
# Install: cargo install cargo-public-api
# Usage: ./scripts/check-public-api.sh [crate-name]
set -euo pipefail

CRATE=${1:-rskit}
echo "Checking public API for $CRATE..."
if ! output=$(cargo public-api --manifest-path core/Cargo.toml -p "$CRATE" diff 2>&1); then
  if grep -q "Could not find crate \`$CRATE\`" <<<"$output"; then
    echo "No published baseline found for $CRATE; validating current public API generation instead."
    cargo public-api --manifest-path core/Cargo.toml -p "$CRATE" >/dev/null
  else
    printf '%s\n' "$output" >&2
    exit 1
  fi
else
  printf '%s\n' "$output"
fi

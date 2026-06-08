#!/usr/bin/env bash
# Check for breaking public API changes using cargo-public-api.
# Install: rustup toolchain install nightly --profile minimal && cargo install cargo-public-api
# Usage: ./scripts/check-public-api.sh [package-name]
# The facade package is rskit-toolkit, while its Rust crate name remains rskit.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE=${1:-rskit-toolkit}
RUSTDOC_JSON_TOOLCHAIN=${RUSTDOC_JSON_TOOLCHAIN:-nightly}
CARGO_PUBLIC_API=(cargo "+${RUSTDOC_JSON_TOOLCHAIN}" public-api)
MANIFESTS=(
  "core/Cargo.toml"
  "contrib/Cargo.toml"
  "examples/Cargo.toml"
)

if [[ -n "${PYTHON:-}" ]] && command -v "$PYTHON" >/dev/null 2>&1; then
  PYTHON_BIN="$PYTHON"
else
  PYTHON_BIN=""
  for candidate in python3.14 python3.13 python3.12 python3.11 python3; do
    if command -v "$candidate" >/dev/null 2>&1; then
      PYTHON_BIN="$candidate"
      break
    fi
  done
fi

if [ -z "$PYTHON_BIN" ] || ! command -v "$PYTHON_BIN" >/dev/null 2>&1; then
  echo "Python 3 is required" >&2
  exit 1
fi

"$PYTHON_BIN" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3,) else "Python 3 is required")'

workspace_contains_package() {
  local manifest=$1
  local crate=$2

  cargo metadata --manifest-path "$ROOT_DIR/$manifest" --no-deps --format-version 1 2>/dev/null \
    | "$PYTHON_BIN" -c 'import json, sys
crate = sys.argv[1]
data = json.load(sys.stdin)
members = set(data.get("workspace_members", []))
for package in data.get("packages", []):
    if package.get("name") == crate and package.get("id") in members:
        raise SystemExit(0)
raise SystemExit(1)' "$crate"
}

manifest_for_crate() {
  local crate=$1

  for manifest in "${MANIFESTS[@]}"; do
    if workspace_contains_package "$manifest" "$crate"; then
      printf '%s\n' "$manifest"
      return 0
    fi
  done

  return 1
}

if ! MANIFEST=$(manifest_for_crate "$CRATE"); then
  echo "Crate '$CRATE' was not found in core, contrib, or examples workspaces." >&2
  exit 1
fi

echo "Checking public API for $CRATE using $MANIFEST..."
if ! output=$("${CARGO_PUBLIC_API[@]}" --manifest-path "$ROOT_DIR/$MANIFEST" -p "$CRATE" diff 2>&1); then
  if grep -q "Could not find crate \`$CRATE\`" <<<"$output"; then
    echo "No published baseline found for $CRATE; validating current public API generation instead."
    "${CARGO_PUBLIC_API[@]}" --manifest-path "$ROOT_DIR/$MANIFEST" -p "$CRATE" >/dev/null
  else
    printf '%s\n' "$output" >&2
    exit 1
  fi
else
  printf '%s\n' "$output"
fi

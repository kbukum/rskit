#!/usr/bin/env bash
set -Eeuo pipefail

OUT_DIR="${1:-depgraphs}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is not installed or not in PATH" >&2
  exit 1
fi

if ! command -v dot >/dev/null 2>&1; then
  echo "error: dot (Graphviz) is not installed or not in PATH" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

echo "Generating dependency graphs into: $OUT_DIR"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CORE_MANIFEST="$REPO_ROOT/core/Cargo.toml"
CONTRIB_MANIFEST="$REPO_ROOT/contrib/Cargo.toml"

run_graph() {
  local name="$1"
  local manifest="$2"
  shift 2

  local out_file="$OUT_DIR/$name.svg"
  echo "-> generating $out_file"

  cargo depgraph --manifest-path "$manifest" "$@" \
    | dot -Grankdir=LR -Nshape=box -Tsvg \
    > "$out_file"

  echo "   done: $out_file"
}

run_graph "graph-all" "$CORE_MANIFEST" \
  --workspace-only \
  --dedup-transitive-deps

run_graph "graph-contrib" "$CONTRIB_MANIFEST" \
  --workspace-only \
  --dedup-transitive-deps

run_graph "graph-agent" "$CORE_MANIFEST" \
  --workspace-only \
  --focus rskit-agent \
  --depth 2 \
  --dedup-transitive-deps

run_graph "graph-errors" "$CORE_MANIFEST" \
  --workspace-only \
  --focus rskit-errors \
  --depth 2 \
  --dedup-transitive-deps

run_graph "graph-resilience" "$CORE_MANIFEST" \
  --workspace-only \
  --focus rskit-resilience \
  --depth 2 \
  --dedup-transitive-deps

echo "All graphs generated successfully."

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

run_graph() {
  local name="$1"
  shift

  local out_file="$OUT_DIR/$name.svg"
  echo "-> generating $out_file"

  cargo depgraph "$@" \
    | dot -Grankdir=LR -Nshape=box -Tsvg \
    > "$out_file"

  echo "   done: $out_file"
}

run_graph "graph-all" \
  --workspace-only \
  --dedup-transitive-deps \
  --hide workspace-hack

run_graph "graph-agent" \
  --workspace-only \
  --focus rskit-agent \
  --depth 2 \
  --dedup-transitive-deps \
  --hide workspace-hack

run_graph "graph-errors" \
  --workspace-only \
  --focus rskit-errors \
  --depth 2 \
  --dedup-transitive-deps \
  --hide workspace-hack

run_graph "graph-resilience" \
  --workspace-only \
  --focus rskit-resilience \
  --depth 2 \
  --dedup-transitive-deps \
  --hide workspace-hack

echo "All graphs generated successfully."

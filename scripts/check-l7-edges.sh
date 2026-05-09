#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Direct normal-dependency edges that would reintroduce L7 vocabulary coupling.
# rskit-ai is the shared AI/ML vocabulary crate; lower layers must not learn
# about AI/ML crates, and serving/embedding modules must not import LLM
# vocabulary for shared shapes such as Usage or StreamEvent.
disallowed_edges=(
  "rskit-inference:rskit-llm"
  "rskit-inference:rskit-embedding"
  "rskit-embedding:rskit-llm"
  "rskit-embedding:rskit-inference"
  "rskit-tool:rskit-llm"
  "rskit-tool:rskit-mcp"
  "rskit-tool:rskit-agent"
  "rskit-mcp:rskit-llm"
  "rskit-observability:rskit-ai"
  "rskit-observability:rskit-llm"
)

status=0
for edge in "${disallowed_edges[@]}"; do
  parent="${edge%%:*}"
  child="${edge##*:}"
  if cargo tree -e normal --all-features -p "$parent" --depth 1 --prefix none \
    | awk 'NR > 1 { print $1 }' \
    | grep -Fxq "$child"; then
    echo "disallowed dependency edge: $parent -> $child" >&2
    status=1
  fi
done

exit "$status"

#!/usr/bin/env bash
# Structure guard (development principles §4): crate-root `lib.rs` and `mod.rs` files
# declare and re-export only — they must never contain logic or private items. Applied to
# every crate under `core/*/src` and `contrib/*/*/src`. Attribute lines (outer `#[cfg(unix)]`
# or crate-root inner `#![warn(...)]`) and comments are permitted because they annotate a
# following declare/re-export without introducing logic.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fail=0

while IFS= read -r file; do
  invalid_lines="$(awk '
    /^[[:space:]]*$/ { next }
    /^[[:space:]]*\/\/!/ { next }
    /^[[:space:]]*\/\/\// { next }
    /^[[:space:]]*\/\// { next }
    /^[[:space:]]*#\[.*\][[:space:]]*$/ { next }
    /^[[:space:]]*#!\[.*\][[:space:]]*$/ { next }
    /^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?mod[[:space:]]+[A-Za-z_][A-Za-z0-9_]*;[[:space:]]*$/ { next }
    /^[[:space:]]*pub([[:space:]]*\([^)]*\))?[[:space:]]+use[[:space:]].+;[[:space:]]*$/ { next }
    /^[[:space:]]*pub([[:space:]]*\([^)]*\))?[[:space:]]+use[[:space:]].+\{[[:space:]]*$/ { next }
    /^[[:space:]]*[A-Za-z_][A-Za-z0-9_:]*(,[[:space:]]*[A-Za-z_][A-Za-z0-9_:]*)*,?[[:space:]]*$/ { next }
    /^[[:space:]]*\};[[:space:]]*$/ { next }
    { print }
  ' "$file")"
  if [ -n "$invalid_lines" ]; then
    printf 'aggregator contains logic or private items: %s\n%s\n' "${file#"$root"/}" "$invalid_lines" >&2
    fail=1
  fi
done < <(find "$root/core" "$root/contrib" -path '*/src/*' \( -name mod.rs -o -name lib.rs \) \
  -type f 2>/dev/null | sort)

exit "$fail"

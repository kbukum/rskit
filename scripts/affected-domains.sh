#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOMAINS_FILE="${ROOT_DIR}/domains.toml"
PYTHON_BIN=""

for candidate in python3.14 python3.13 python3.12 python3.11 python3; do
  if command -v "$candidate" >/dev/null 2>&1; then
    PYTHON_BIN="$candidate"
    break
  fi
done

if [ -z "$PYTHON_BIN" ]; then
  echo "python3.11+ is required (tomllib)" >&2
  exit 1
fi

if [ ! -t 0 ]; then
  changed_files="$(cat)"
elif [ "$#" -gt 0 ]; then
  changed_files="$(printf '%s\n' "$@")"
else
  changed_files="$(git -C "$ROOT_DIR" diff --name-only origin/main...HEAD)"
fi

DOMAINS_FILE="$DOMAINS_FILE" CHANGED_FILES="$changed_files" "$PYTHON_BIN" - <<'PY'
from __future__ import annotations

import os
import sys
from collections import deque
from pathlib import PurePosixPath
import tomllib

if sys.version_info < (3, 11):
    raise SystemExit("python3.11+ is required (tomllib)")


with open(os.environ["DOMAINS_FILE"], "rb") as fh:
    domains = tomllib.load(fh)["domains"]

all_domains = list(domains.keys())
module_to_domains: dict[str, set[str]] = {}
for name, domain in domains.items():
    for module in domain.get("modules", []):
        module_to_domains.setdefault(module, set()).add(name)


def domains_for_file(path_str: str) -> set[str]:
    path_str = path_str.strip()
    if not path_str:
        return set()

    parts = PurePosixPath(path_str).parts
    if not parts:
        return set()

    if len(parts) == 1 or parts[0] in {".cargo", ".config"}:
        return set(all_domains)

    if len(parts) >= 2 and parts[0] == "crates" and parts[1].startswith("rskit-"):
        module = parts[1][len("rskit-") :]
        return set(module_to_domains.get(module, set()))

    return set()


directly_affected: set[str] = set()
for raw_line in os.environ.get("CHANGED_FILES", "").splitlines():
    directly_affected.update(domains_for_file(raw_line))

inverse: dict[str, list[str]] = {}
for name, domain in domains.items():
    for dep in domain.get("depends_on", []):
        inverse.setdefault(dep, []).append(name)

affected = set(directly_affected)
queue = deque(directly_affected)
while queue:
    current = queue.popleft()
    for dependent in inverse.get(current, []):
        if dependent not in affected:
            affected.add(dependent)
            queue.append(dependent)

for name in sorted(affected):
    print(name)
PY

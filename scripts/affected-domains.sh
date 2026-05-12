#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOMAINS_FILE="${ROOT_DIR}/domains.toml"

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
  echo "Python 3.11+ is required (tomllib)" >&2
  exit 1
fi

if [ ! -t 0 ]; then
  changed_files="$(cat)"
elif [ "$#" -gt 0 ]; then
  changed_files="$(printf '%s\n' "$@")"
else
  changed_files="$(git -C "$ROOT_DIR" diff --name-only origin/main...HEAD)"
fi

DOMAINS_FILE="$DOMAINS_FILE" ROOT_DIR="$ROOT_DIR" CHANGED_FILES="$changed_files" "$PYTHON_BIN" - <<'PY'
from __future__ import annotations

import os
import sys

if sys.version_info < (3, 11):
    raise SystemExit("python3.11+ is required (tomllib)")

import tomllib
from collections import deque
from pathlib import Path, PurePosixPath

with open(os.environ["DOMAINS_FILE"], "rb") as fh:
    domains = tomllib.load(fh)["domains"]

all_domains = list(domains.keys())
module_to_domains: dict[str, set[str]] = {}
for name, domain in domains.items():
    for module in domain.get("modules", []):
        module_to_domains.setdefault(module, set()).add(name)

repo_root = Path(os.environ["ROOT_DIR"])
crate_path_to_domains: dict[str, set[str]] = {}
contrib_dir = repo_root / "contrib"
if contrib_dir.exists():
    for cargo_toml in contrib_dir.glob("*/*/Cargo.toml"):
        with cargo_toml.open("rb") as fh:
            package = tomllib.load(fh).get("package", {})
        crate_name = package.get("name")
        if not crate_name or not crate_name.startswith("rskit-"):
            continue
        module = crate_name[len("rskit-") :]
        crate_path_to_domains[cargo_toml.parent.relative_to(repo_root).as_posix()] = set(
            module_to_domains.get(module, all_domains)
        )


def domains_for_file(path_str: str) -> set[str]:
    path_str = path_str.strip()
    if not path_str:
        return set()

    parts = PurePosixPath(path_str).parts
    if not parts:
        return set()

    global_files = {
        "Cargo.lock",
        "Cargo.toml",
        "Makefile",
        "README.md",
        "domains.toml",
        "core/Cargo.toml",
        "contrib/Cargo.toml",
        "examples/Cargo.toml",
    }
    global_dirs = {".cargo", ".config", ".github", "docs", "scripts"}

    if path_str in global_files or parts[0] in global_dirs:
        return set(all_domains)

    if parts[0] == "core":
        if len(parts) == 1 or parts[1] == "Cargo.toml":
            return set(all_domains)
        if parts[1] == "rskit":
            return set(all_domains)
        if parts[1].startswith("rskit-"):
            module = parts[1][len("rskit-") :]
            return set(module_to_domains.get(module, all_domains))
        return set(all_domains)

    if parts[0] == "contrib":
        if len(parts) < 3:
            return set(all_domains)
        crate_dir = PurePosixPath(*parts[:3]).as_posix()
        return set(crate_path_to_domains.get(crate_dir, all_domains))

    if parts[0] == "examples":
        return set(all_domains)

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

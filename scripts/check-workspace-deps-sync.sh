#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -n "${PYTHON:-}" ]] && command -v "$PYTHON" >/dev/null 2>&1; then
  python_bin="$PYTHON"
else
  python_bin=""
  for candidate in python3.14 python3.13 python3.12 python3.11 python3; do
    if command -v "$candidate" >/dev/null 2>&1; then
      python_bin="$candidate"
      break
    fi
  done
fi

if [ -z "$python_bin" ] || ! command -v "$python_bin" >/dev/null 2>&1; then
  echo "Python 3.11+ is required (tomllib)" >&2
  exit 1
fi

ROOT_DIR="$repo_root" "$python_bin" - <<'PY'
from __future__ import annotations

import os
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path

if sys.version_info < (3, 11):
    raise SystemExit("Python 3.11+ is required (tomllib)")


@dataclass(frozen=True)
class DependencyVersion:
    name: str
    package: str
    version: str


root = Path(os.environ["ROOT_DIR"])


def load(path: Path) -> dict:
    with path.open("rb") as fh:
        return tomllib.load(fh)


def dependency_version(dep_name: str, dep: object) -> DependencyVersion | None:
    if isinstance(dep, str):
        return DependencyVersion(name=dep_name, package=dep_name, version=dep)

    if not isinstance(dep, dict):
        return None

    if "path" in dep or dep.get("workspace") is True:
        return None

    version = dep.get("version")
    if not isinstance(version, str):
        return None

    package = dep.get("package")
    if not isinstance(package, str):
        package = dep_name

    return DependencyVersion(name=dep_name, package=package, version=version)


def workspace_versions(manifest_path: Path) -> dict[str, DependencyVersion]:
    manifest = load(manifest_path)
    dependencies = manifest.get("workspace", {}).get("dependencies", {})
    if not isinstance(dependencies, dict):
        return {}

    versions: dict[str, DependencyVersion] = {}
    for dep_name, dep in dependencies.items():
        version = dependency_version(dep_name, dep)
        if version is None or version.package.startswith("rskit"):
            continue
        versions[version.package] = version
    return versions


core_path = root / "core" / "Cargo.toml"
contrib_path = root / "contrib" / "Cargo.toml"
core_manifest = load(core_path)
contrib_manifest = load(contrib_path)
core = workspace_versions(core_path)
contrib = workspace_versions(contrib_path)

errors: list[str] = []
core_workspace_version = core_manifest.get("workspace", {}).get("package", {}).get("version")
contrib_workspace_version = contrib_manifest.get("workspace", {}).get("package", {}).get("version")
if core_workspace_version != contrib_workspace_version:
    errors.append(
        "workspace.package.version: "
        f"core={core_workspace_version!r}, contrib={contrib_workspace_version!r}"
    )

for package in sorted(set(core) & set(contrib)):
    core_dep = core[package]
    contrib_dep = contrib[package]
    if core_dep.version == contrib_dep.version:
        continue

    errors.append(
        f"{package}: core {core_dep.name}={core_dep.version!r}, "
        f"contrib {contrib_dep.name}={contrib_dep.version!r}"
    )

if errors:
    print("workspace dependency version drift detected:", file=sys.stderr)
    for error in errors:
        print(f"  - {error}", file=sys.stderr)
    print(
        "Keep shared external dependency versions aligned in core/Cargo.toml "
        "and contrib/Cargo.toml, or remove the unused declaration.",
        file=sys.stderr,
    )
    sys.exit(1)

print("workspace dependency versions are synced")
PY

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


workspace_paths = {
    "core": root / "core" / "Cargo.toml",
    "contrib": root / "contrib" / "Cargo.toml",
    "examples": root / "examples" / "Cargo.toml",
}
workspace_manifests = {name: load(path) for name, path in workspace_paths.items()}
workspace_dependency_versions = {
    name: workspace_versions(path) for name, path in workspace_paths.items()
}

errors: list[str] = []
workspace_package_versions = {
    name: manifest.get("workspace", {}).get("package", {}).get("version")
    for name, manifest in workspace_manifests.items()
}
for name, version in sorted(workspace_package_versions.items()):
    if version == workspace_package_versions["core"]:
        continue
    errors.append(
        "workspace.package.version: "
        f"core={workspace_package_versions['core']!r}, {name}={version!r}"
    )

all_packages = sorted(
    {
        package
        for versions in workspace_dependency_versions.values()
        for package in versions
    }
)
for package in all_packages:
    owners = {
        name: versions[package]
        for name, versions in workspace_dependency_versions.items()
        if package in versions
    }
    package_versions = {dep.version for dep in owners.values()}
    if len(package_versions) <= 1:
        continue

    details = ", ".join(
        f"{name} {dep.name}={dep.version!r}" for name, dep in sorted(owners.items())
    )
    errors.append(f"{package}: {details}")

if errors:
    print("workspace dependency version drift detected:", file=sys.stderr)
    for error in errors:
        print(f"  - {error}", file=sys.stderr)
    print(
        "Keep shared external dependency versions aligned in core/Cargo.toml, "
        "contrib/Cargo.toml, and examples/Cargo.toml, or remove the unused declaration.",
        file=sys.stderr,
    )
    sys.exit(1)

print("workspace dependency versions are synced")
PY

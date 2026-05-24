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
from pathlib import Path

if sys.version_info < (3, 11):
    raise SystemExit("Python 3.11+ is required (tomllib)")

root = Path(os.environ["ROOT_DIR"])
errors: list[str] = []


def load(path: Path) -> dict:
    with path.open("rb") as fh:
        return tomllib.load(fh)


def dependency_tables(manifest: dict) -> list[tuple[str, dict]]:
    tables: list[tuple[str, dict]] = []
    for name in ("dependencies", "build-dependencies"):
        table = manifest.get(name, {})
        if isinstance(table, dict):
            tables.append((name, table))
    target = manifest.get("target", {})
    if isinstance(target, dict):
        for cfg_name, cfg in target.items():
            if isinstance(cfg, dict):
                for dep_name in ("dependencies", "build-dependencies"):
                    table = cfg.get(dep_name, {})
                    if isinstance(table, dict):
                        tables.append((f"target.{cfg_name}.{dep_name}", table))
    return tables


def is_optional(dep: object) -> bool:
    return isinstance(dep, dict) and dep.get("optional") is True


def path_value(dep: object) -> str | None:
    if isinstance(dep, dict):
        value = dep.get("path")
        if isinstance(value, str):
            return value
    return None


def dependency(manifest: dict, name: str) -> object | None:
    deps = manifest.get("dependencies", {})
    if isinstance(deps, dict):
        return deps.get(name)
    return None


def features(manifest: dict) -> dict:
    table = manifest.get("features", {})
    return table if isinstance(table, dict) else {}


for cargo_toml in sorted((root / "core").glob("*/Cargo.toml")):
    manifest = load(cargo_toml)
    package = manifest.get("package", {})
    crate = package.get("name", cargo_toml.parent.name)
    rel = cargo_toml.relative_to(root).as_posix()

    for table_name, deps in dependency_tables(manifest):
        for dep_name, dep in deps.items():
            dep_path = path_value(dep)
            if dep_path is None:
                continue
            resolved = (cargo_toml.parent / dep_path).resolve()
            try:
                target = resolved.relative_to(root).as_posix()
            except ValueError:
                continue
            if target.startswith("contrib/") and crate != "rskit":
                errors.append(
                    f"{rel}: {table_name}.{dep_name} points to {target}; only the facade may aggregate contrib adapters"
                )
            if target.startswith("core/rskit-server") and crate == "rskit-grpc":
                errors.append(f"{rel}: rskit-grpc must not depend on rskit-server")

    if crate in {"rskit-http", "rskit-discovery"}:
        bootstrap = dependency(manifest, "rskit-bootstrap")
        if bootstrap is not None and not is_optional(bootstrap):
            errors.append(f"{rel}: rskit-bootstrap must be optional for {crate}")

    if crate == "rskit-server":
        for dep_name in (
            "axum",
            "base64",
            "hyper",
            "hyper-util",
            "rskit-http",
            "rskit-security",
            "rustls",
            "rustls-pemfile",
            "tokio-rustls",
            "tonic",
            "tonic-health",
            "tonic-reflection",
            "tower",
            "tower-http",
            "tower-layer",
            "tower-service",
        ):
            dep = dependency(manifest, dep_name)
            if dep is not None and not is_optional(dep):
                errors.append(f"{rel}: heavy transport dependency {dep_name} must be optional")

    if crate == "rskit":
        if dependency(manifest, "rskit-workload") is not None:
            errors.append(f"{rel}: facade must not depend on removed rskit-workload")
        if "workload" in features(manifest):
            errors.append(f"{rel}: facade must not expose removed workload feature")

for removed in ("core/rskit-workload/Cargo.toml", "core/rskit-integration/Cargo.toml"):
    if (root / removed).exists():
        errors.append(f"{removed}: removed boundary crate still exists")

if errors:
    print("Topology check failed:", file=sys.stderr)
    for error in errors:
        print(f"  - {error}", file=sys.stderr)
    raise SystemExit(1)

print("Topology check passed")
PY

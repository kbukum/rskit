"""Cargo workspace metadata helpers."""

from __future__ import annotations

import dataclasses
from collections.abc import Iterable, Sequence
from pathlib import Path

from .errors import ToolError
from .paths import ROOT, WORKSPACES
from .process import run_json


@dataclasses.dataclass(frozen=True)
class Package:
    """A package discovered from a Cargo workspace."""

    name: str
    workspace: str
    manifest_path: Path
    root: Path
    version: str
    publishable: bool
    umbrella: bool = False


def is_relative_to(path: Path, parent: Path) -> bool:
    """Return true when path is under parent."""

    try:
        path.relative_to(parent)
    except ValueError:
        return False
    return True


def metadata(manifest: Path, *, all_features: bool = False, no_deps: bool = True) -> dict[str, object]:
    """Load cargo metadata for one manifest."""

    command = ["cargo", "metadata", "--format-version=1", "--manifest-path", str(manifest)]
    if no_deps:
        command.append("--no-deps")
    if all_features:
        command.append("--all-features")
    return run_json(command, cwd=ROOT)


def discover_packages(workspace: str | None = None) -> list[Package]:
    """Discover workspace member packages."""

    manifests = {workspace: WORKSPACES[workspace]} if workspace is not None else WORKSPACES
    packages: list[Package] = []
    seen: set[str] = set()
    for workspace_name, manifest in manifests.items():
        data = metadata(manifest)
        members = set(data["workspace_members"])  # type: ignore[index]
        for package_data in data["packages"]:  # type: ignore[index]
            if package_data["id"] not in members:
                continue
            name = package_data["name"]
            if name in seen:
                raise ToolError(f"duplicate package name discovered: {name}")
            manifest_path = Path(package_data["manifest_path"]).resolve()
            package_metadata = package_data.get("metadata") or {}
            release_metadata = package_metadata.get("release") or {}
            packages.append(
                Package(
                    name=name,
                    workspace=workspace_name,
                    manifest_path=manifest_path,
                    root=manifest_path.parent,
                    version=package_data.get("version", ""),
                    publishable=package_data.get("publish") != [],
                    umbrella=bool(release_metadata.get("umbrella", False)),
                )
            )
            seen.add(name)
    return sorted(packages, key=lambda item: (item.workspace, item.name))


def package_by_name(workspace: str | None = None) -> dict[str, Package]:
    """Return discovered packages by name."""

    return {package.name: package for package in discover_packages(workspace)}


def package_manifest(package: str) -> Path:
    """Return the workspace manifest containing a package."""

    for candidate in discover_packages():
        if candidate.name == package:
            return WORKSPACES[candidate.workspace]
    raise ToolError(f"Crate '{package}' was not found in core, contrib, or examples workspaces.")


def packages_for_paths(packages: Sequence[Package], paths: Iterable[Path]) -> set[str]:
    """Map changed paths to package names."""

    workspace_config = {
        Path("Cargo.lock"),
        Path("rust-toolchain.toml"),
        Path("Makefile"),
        Path("clippy.toml"),
        Path("rustfmt.toml"),
        Path("deny.toml"),
        Path("deny.contrib.toml"),
        Path("deny.examples.toml"),
        Path("core/Cargo.toml"),
        Path("contrib/Cargo.toml"),
        Path("examples/Cargo.toml"),
    }
    changed_paths = list(paths)
    global_dirs = {(".cargo",), (".config",), (".github",), ("scripts",)}
    if any(path in workspace_config or path.parts[:1] in global_dirs for path in changed_paths):
        return {package.name for package in packages}

    selected: set[str] = set()
    for changed_path in changed_paths:
        absolute = (ROOT / changed_path).resolve()
        for package in packages:
            if is_relative_to(absolute, package.root):
                selected.add(package.name)
                break
    return selected

"""Coverage package discovery and selection."""

from __future__ import annotations

import argparse
from collections.abc import Sequence

from ..cargo import Package, discover_packages, packages_for_paths
from ..errors import ToolError
from ..git import changed_paths
from .thresholds import split_names


def select_packages(packages: Sequence[Package], args: argparse.Namespace) -> list[Package]:
    """Select packages according to arguments."""

    selected = list(packages)
    requested = set(args.package)
    if args.packages:
        requested.update(split_names(args.packages))
    explicit_requested = bool(requested)
    if args.changed:
        changed = packages_for_paths(packages, changed_paths(args.changed_base))
        requested = requested & changed if requested else changed
    if requested:
        by_name = {package.name: package for package in packages}
        missing = sorted(requested - set(by_name))
        if missing:
            raise ToolError(f"unknown package(s): {', '.join(missing)}")
        selected = [by_name[name] for name in sorted(requested)]
    if not explicit_requested:
        excluded = split_names(args.exclude_packages)
        selected = [package for package in selected if package.name not in excluded]
    return selected


def discover_coverage_packages(args: argparse.Namespace) -> list[Package]:
    """Discover packages in scope for a coverage run."""

    if args.workspace is not None:
        return discover_packages(args.workspace)
    if args.mode == "release":
        return [*discover_packages("core"), *discover_packages("contrib")]
    return discover_packages()

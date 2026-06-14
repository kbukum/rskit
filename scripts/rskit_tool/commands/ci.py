"""CI and local validation commands."""

from __future__ import annotations

import argparse
from collections.abc import Sequence

from ..cargo import Package, discover_packages, package_manifest, packages_for_paths
from ..errors import ToolError
from ..git import changed_paths
from ..paths import WORKSPACES
from ..process import command_exists, run


FEATURE_MODES = ("default", "all", "both")
SCOPES = ("all", "changed")


def add_parser(subparsers: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    """Register CI commands."""

    parser = subparsers.add_parser("ci", help="Shared CI/local validation helpers")
    ci_sub = parser.add_subparsers(dest="ci_command", required=True)

    manifest = ci_sub.add_parser("package-manifest", help="Print the workspace manifest for a package")
    manifest.add_argument("package", help="Cargo package name")
    manifest.set_defaults(func=run_package_manifest)

    test = ci_sub.add_parser("test", help="Run selected package tests with nextest")
    add_selection_args(test)
    test.add_argument("--profile", default="ci", help="nextest profile")
    test.add_argument(
        "--feature-mode",
        choices=FEATURE_MODES,
        default="both",
        help="Feature coverage to run",
    )
    test.set_defaults(func=run_test)

    msrv = ci_sub.add_parser("msrv", help="Run selected MSRV compile checks")
    add_selection_args(msrv)
    msrv.add_argument(
        "--feature-mode",
        choices=FEATURE_MODES,
        default="both",
        help="Feature coverage to check",
    )
    msrv.set_defaults(func=run_msrv)


def add_selection_args(parser: argparse.ArgumentParser) -> None:
    """Add common package-selection arguments."""

    parser.add_argument("--scope", choices=SCOPES, default="changed", help="Packages to validate")
    parser.add_argument("--changed-base", default="origin/main...HEAD", help="git diff range for changed scope")
    parser.add_argument(
        "--workspace",
        action="append",
        choices=sorted(WORKSPACES),
        help="Restrict validation to a workspace; repeat for multiple workspaces",
    )
    parser.add_argument(
        "--package",
        action="append",
        default=[],
        help="Explicit package to include; repeat for multiple packages",
    )


def run_package_manifest(args: argparse.Namespace) -> int:
    """Print the manifest path for a package's workspace."""

    print(package_manifest(args.package))
    return 0


def run_test(args: argparse.Namespace) -> int:
    """Run selected package tests."""

    packages = select_packages(args)
    if not packages:
        print("No packages selected for tests")
        return 0
    if not command_exists("cargo-nextest"):
        raise ToolError("cargo-nextest is required for CI test runs")

    for workspace, workspace_packages in group_by_workspace(packages).items():
        for feature_args in feature_arg_sets(args.feature_mode):
            run(
                [
                    "cargo",
                    "nextest",
                    "run",
                    "--manifest-path",
                    str(WORKSPACES[workspace]),
                    *package_selection_args(workspace, workspace_packages),
                    *feature_args,
                    "--profile",
                    args.profile,
                    "--no-tests",
                    "pass",
                ]
            )
    return 0


def run_msrv(args: argparse.Namespace) -> int:
    """Run selected MSRV compile checks without duplicating the full test suite."""

    packages = select_packages(args)
    if not packages:
        print("No packages selected for MSRV checks")
        return 0

    for workspace, workspace_packages in group_by_workspace(packages).items():
        for feature_args in feature_arg_sets(args.feature_mode):
            run(
                [
                    "cargo",
                    "check",
                    "--manifest-path",
                    str(WORKSPACES[workspace]),
                    *package_selection_args(workspace, workspace_packages),
                    *feature_args,
                    "--tests",
                ]
            )
    return 0


def select_packages(args: argparse.Namespace) -> list[Package]:
    """Select packages for a CI command."""

    packages = discover_packages()
    selected_names: set[str]
    if args.package:
        selected_names = set(args.package)
    elif args.scope == "all":
        selected_names = {package.name for package in packages}
    else:
        selected_names = packages_for_paths(packages, changed_paths(args.changed_base))

    selected = [package for package in packages if package.name in selected_names]
    if args.workspace:
        workspaces = set(args.workspace)
        selected = [package for package in selected if package.workspace in workspaces]

    missing = sorted(set(args.package) - {package.name for package in packages})
    if missing:
        raise ToolError(f"unknown package(s): {', '.join(missing)}")
    return selected


def group_by_workspace(packages: Sequence[Package]) -> dict[str, list[Package]]:
    """Group packages by workspace with deterministic ordering."""

    grouped: dict[str, list[Package]] = {}
    workspace_order = {name: index for index, name in enumerate(WORKSPACES)}
    for package in sorted(packages, key=lambda item: (workspace_order[item.workspace], item.name)):
        grouped.setdefault(package.workspace, []).append(package)
    return grouped


def package_selection_args(workspace: str, packages: Sequence[Package]) -> list[str]:
    """Return Cargo package-selection arguments."""

    workspace_packages = {package.name for package in discover_packages(workspace)}
    selected = {package.name for package in packages}
    if selected == workspace_packages:
        return ["--workspace"]
    args: list[str] = []
    for package in packages:
        args.extend(["-p", package.name])
    return args


def feature_arg_sets(mode: str) -> list[list[str]]:
    """Return Cargo feature arguments for a feature mode."""

    if mode == "default":
        return [[]]
    if mode == "all":
        return [["--all-features"]]
    if mode == "both":
        return [[], ["--all-features"]]
    raise ToolError(f"unknown feature mode: {mode}")

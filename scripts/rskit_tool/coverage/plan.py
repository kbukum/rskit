"""Coverage command planning."""

from __future__ import annotations

import argparse

from ..cargo import Package
from ..paths import COVERAGE_WORKSPACES_DIR, WORKSPACES
from .models import CoverageCommand, WorkspaceCoveragePlan


def workspace_coverage_plans(packages: list[Package], args: argparse.Namespace) -> list[WorkspaceCoveragePlan]:
    """Build cargo llvm-cov command plans grouped by Cargo workspace."""

    by_workspace: dict[str, list[Package]] = {}
    for package in packages:
        by_workspace.setdefault(package.workspace, []).append(package)

    return [
        workspace_coverage_plan(workspace, sorted(workspace_packages, key=lambda package: package.name), args)
        for workspace, workspace_packages in sorted(by_workspace.items())
    ]


def workspace_coverage_plan(
    workspace: str,
    packages: list[Package],
    args: argparse.Namespace,
) -> WorkspaceCoveragePlan:
    """Build cargo llvm-cov commands for one workspace package group."""

    manifest_path = WORKSPACES[workspace]
    manifest = str(manifest_path)
    report_dir = COVERAGE_WORKSPACES_DIR / workspace
    target_dir = manifest_path.parent / "target" / "coverage"
    test_package_args = workspace_test_package_args(workspace, packages)
    report_package_args = workspace_report_package_args(workspace, packages)
    test_command = [
        "cargo",
        "llvm-cov",
        "nextest",
        "--manifest-path",
        manifest,
        *test_package_args,
        "--all-features",
        "--no-report",
        "--no-fail-fast",
    ]
    if args.test_filter:
        test_command.extend(["--", args.test_filter])
    commands = [
        *clean_commands(manifest, args.coverage_clean),
        CoverageCommand("test", test_command),
        CoverageCommand(
            "lcov-report",
            [
                "cargo",
                "llvm-cov",
                "report",
                "--manifest-path",
                manifest,
                *report_package_args,
                "--lcov",
                "--output-path",
                str(report_dir / "lcov.info"),
            ],
        ),
        CoverageCommand(
            "json-summary",
            [
                "cargo",
                "llvm-cov",
                "report",
                "--manifest-path",
                manifest,
                *report_package_args,
                "--json",
                "--summary-only",
                "--output-path",
                str(report_dir / "summary.json"),
            ],
        ),
    ]
    if args.html:
        commands.append(
            CoverageCommand(
                "html-report",
                [
                    "cargo",
                    "llvm-cov",
                    "report",
                    "--manifest-path",
                    manifest,
                    *report_package_args,
                    "--html",
                    "--output-dir",
                    str(report_dir / "html"),
                ],
            )
        )
    return WorkspaceCoveragePlan(
        workspace=workspace,
        packages=tuple(packages),
        manifest_path=manifest_path,
        report_dir=report_dir,
        target_dir=target_dir,
        commands=tuple(commands),
    )


def workspace_test_package_args(workspace: str, packages: list[Package]) -> list[str]:
    """Return test-time cargo package selectors for a workspace package group."""

    package_names = {package.name for package in packages}
    if package_names == set(packages_for_workspace(workspace)):
        return ["--workspace"]
    return package_args(packages)


def workspace_report_package_args(workspace: str, packages: list[Package]) -> list[str]:
    """Return report-time cargo package selectors for a workspace package group."""

    package_names = {package.name for package in packages}
    if package_names == set(packages_for_workspace(workspace)):
        return []
    return package_args(packages)


def package_args(packages: list[Package]) -> list[str]:
    """Return repeated -p selectors for packages."""

    args: list[str] = []
    for package in packages:
        args.extend(["-p", package.name])
    return args


def packages_for_workspace(workspace: str) -> list[str]:
    """Return package names for one workspace."""

    from ..cargo import discover_packages

    return [package.name for package in discover_packages(workspace)]


def coverage_step_count(args: argparse.Namespace) -> int:
    """Return the number of subprocess stages per workspace coverage job."""

    clean_steps = 0 if args.coverage_clean == "none" else 1
    return clean_steps + (4 if args.html else 3)


def clean_commands(manifest: str, mode: str) -> list[CoverageCommand]:
    """Return coverage cleanup commands for one workspace."""

    if mode == "none":
        return []
    command = ["cargo", "llvm-cov", "clean", "--manifest-path", manifest, "--workspace"]
    if mode == "profraw":
        command.append("--profraw-only")
    return [CoverageCommand("clean", command)]

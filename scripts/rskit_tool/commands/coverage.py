"""Coverage command registration."""

from __future__ import annotations

import argparse

from ..coverage.runner import run_coverage
from ..paths import WORKSPACES


def add_parser(subparsers: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    """Register coverage subcommand."""

    parser = subparsers.add_parser("coverage", help="Run workspace-level cargo-llvm-cov and report per-package coverage")
    add_coverage_args(parser)
    parser.set_defaults(func=run_coverage)


def add_coverage_args(parser: argparse.ArgumentParser) -> None:
    """Add coverage arguments to a parser."""

    parser.add_argument("--config", help="Coverage config TOML path")
    parser.add_argument("--mode", default="coverage", choices=("coverage", "release"))
    parser.add_argument("--workspace", choices=tuple(WORKSPACES), help="Limit to one workspace")
    parser.add_argument("--package", action="append", default=[], help="Run one package")
    parser.add_argument("--packages", help="Run a comma or whitespace separated package list")
    parser.add_argument("--changed", action="store_true", help="Run packages touched by git changes")
    parser.add_argument("--changed-base", default="origin/main...HEAD")
    parser.add_argument("--jobs", type=int, help="Concurrent package coverage jobs")
    parser.add_argument(
        "--clean",
        dest="coverage_clean",
        choices=("full", "profraw", "none"),
        help="Coverage cleanup mode before each workspace run",
    )
    parser.add_argument("--exclude-packages", help="Comma or whitespace separated packages excluded by default")
    parser.add_argument("--line-threshold", type=float)
    parser.add_argument("--function-threshold", type=float)
    parser.add_argument("--region-threshold", type=float)
    parser.add_argument("--security-line-threshold", type=float)
    parser.add_argument(
        "--security-packages",
        help="Comma or whitespace separated packages using --security-line-threshold",
    )
    parser.add_argument("--test-filter", help="Pass a test name filter after --")
    parser.add_argument("--html", action=argparse.BooleanOptionalAction, help="Emit per-workspace HTML reports")
    parser.add_argument("--progress-interval", type=float, help="Seconds between long-running subprocess progress updates")
    parser.add_argument("--progress-style", choices=("auto", "line", "bar", "log"), help="Progress rendering style")
    parser.add_argument("--progress-width", type=int, help="Progress bar width")
    parser.add_argument("--list", action="store_true", help="List selected packages without running")

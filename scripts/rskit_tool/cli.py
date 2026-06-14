"""Command-line dispatch for the rskit tooling app."""

from __future__ import annotations

import argparse
import sys
from collections.abc import Sequence

from .commands import checks, ci, coverage, domains, release, selftest
from .errors import ToolError


def build_parser() -> argparse.ArgumentParser:
    """Build the top-level parser."""

    parser = argparse.ArgumentParser(description="rskit repository tooling")
    subparsers = parser.add_subparsers(dest="command", required=True)
    coverage.add_parser(subparsers)
    ci.add_parser(subparsers)
    checks.add_parser(subparsers)
    domains.add_parser(subparsers)
    release.add_parser(subparsers)
    selftest.add_parser(subparsers)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    """Run the CLI."""

    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except ToolError as error:
        print(str(error), file=sys.stderr)
        return 1

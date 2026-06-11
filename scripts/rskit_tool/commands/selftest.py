"""Fast Python tooling test runner."""

from __future__ import annotations

import argparse
import unittest

from ..paths import ROOT


def add_parser(subparsers: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    """Register self-test command."""

    parser = subparsers.add_parser("self-test", help="Run Python tooling tests")
    parser.set_defaults(func=run_self_test)


def run_self_test(_args: argparse.Namespace) -> int:
    """Run the Python tooling unittest suite."""

    suite = unittest.defaultTestLoader.discover(
        start_dir=str(ROOT / "scripts" / "tests"),
        top_level_dir=str(ROOT),
    )
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1

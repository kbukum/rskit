#!/usr/bin/env python3
"""Repository tooling entrypoint for rskit."""

from __future__ import annotations

import sys


def main_entry() -> int:
    """Run the tooling app after validating the Python runtime."""

    if sys.version_info < (3, 11):
        print("error: rskit tooling requires Python 3.11+ (tomllib)", file=sys.stderr)
        return 1

    from rskit_tool.cli import main

    return main(sys.argv[1:])


if __name__ == "__main__":
    raise SystemExit(main_entry())

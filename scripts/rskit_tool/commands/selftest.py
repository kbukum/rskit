"""Fast Python app self-tests."""

from __future__ import annotations

import argparse
import io
import sys
from pathlib import Path

from .domains import affected_domains, resolve_crate_name
from .release import validate_target_subdir
from ..coverage.selftest import run_self_tests as coverage_self_tests
from ..errors import ToolError
from ..process import ParallelTask, run, run_parallel


def add_parser(subparsers: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    """Register self-test command."""

    parser = subparsers.add_parser("self-test", help="Run fast Python tooling self-tests")
    parser.set_defaults(func=run_self_test)


def run_self_test(_args: argparse.Namespace) -> int:
    """Run fast deterministic tests."""

    coverage_self_tests()
    if "core" not in affected_domains([Path("core/rskit-errors/src/lib.rs")]):
        raise ToolError("self-test failed: core domain was not detected for rskit-errors")
    if resolve_crate_name("rskit") != "rskit-suite":
        raise ToolError("self-test failed: rskit alias did not resolve to rskit-suite")
    if run_parallel([ParallelTask("none-result", lambda: None)]) != [None]:
        raise ToolError("self-test failed: run_parallel dropped a None result")
    try:
        run([sys.executable, "--version"], capture=True, stdout=io.StringIO())
    except ToolError:
        pass
    else:
        raise ToolError("self-test failed: run accepted capture=True with stdout")
    for invalid_target in ("../bad", "/tmp/rskit-bad", "target", "target/../bad"):
        try:
            validate_target_subdir(invalid_target)
        except ToolError:
            pass
        else:
            raise ToolError(f"self-test failed: invalid target directory accepted: {invalid_target}")
    if validate_target_subdir("target/release/sbom").name != "sbom":
        raise ToolError("self-test failed: valid target subdirectory was not accepted")
    print("self-tests passed")
    return 0

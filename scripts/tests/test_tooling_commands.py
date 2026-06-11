"""Tests for repository tooling commands and process helpers."""

from __future__ import annotations

import io
import sys
import unittest
from pathlib import Path

from . import support  # noqa: F401
from rskit_tool.cargo import Package
from rskit_tool.commands.domains import affected_domains, resolve_crate_name
from rskit_tool.commands.release import validate_target_subdir
from rskit_tool.errors import ToolError
from rskit_tool.paths import ROOT
from rskit_tool.process import ParallelTask, run, run_parallel


class ToolingCommandTests(unittest.TestCase):
    def test_affected_domains_detects_core_changes(self) -> None:
        self.assertIn("core", affected_domains([Path("core/rskit-errors/src/lib.rs")]))

    def test_resolve_crate_name_supports_facade_alias(self) -> None:
        packages = {
            "rskit-suite": Package(
                name="rskit-suite",
                workspace="core",
                manifest_path=ROOT / "core" / "rskit" / "Cargo.toml",
                root=ROOT / "core" / "rskit",
                version="0.0.0",
                publishable=True,
            )
        }

        self.assertEqual(resolve_crate_name("rskit", packages), "rskit-suite")

    def test_run_parallel_preserves_none_results(self) -> None:
        self.assertEqual(run_parallel([ParallelTask("none-result", lambda: None)]), [None])

    def test_run_rejects_capture_with_explicit_stdout(self) -> None:
        with self.assertRaises(ToolError):
            run([sys.executable, "--version"], capture=True, stdout=io.StringIO())

    def test_release_output_directory_must_stay_under_target_subdirectory(self) -> None:
        for invalid_target in ("../bad", "/tmp/rskit-bad", "target", "target/../bad"):
            with self.subTest(invalid_target=invalid_target):
                with self.assertRaises(ToolError):
                    validate_target_subdir(invalid_target)

        self.assertEqual(validate_target_subdir("target/release/sbom").name, "sbom")


if __name__ == "__main__":
    unittest.main()

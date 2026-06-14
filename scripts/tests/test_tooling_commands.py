"""Tests for repository tooling commands and process helpers."""

from __future__ import annotations

import io
import sys
import unittest
from pathlib import Path

from . import support  # noqa: F401
from rskit_tool.cargo import Package, packages_for_paths
from rskit_tool.commands.ci import feature_arg_sets, group_by_workspace
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

    def test_changed_tooling_paths_select_all_packages(self) -> None:
        packages = [
            Package(
                "rskit-errors",
                "core",
                ROOT / "core/Cargo.toml",
                ROOT / "core/rskit-errors",
                "0.0.0",
                True,
            ),
            Package(
                "rskit-storage-s3",
                "contrib",
                ROOT / "contrib/Cargo.toml",
                ROOT / "contrib/storage/s3",
                "0.0.0",
                True,
            ),
        ]

        for changed_path in (
            Path("Makefile"),
            Path("scripts/rskit_tool.py"),
            Path(".github/workflows/ci.yml"),
        ):
            with self.subTest(changed_path=changed_path):
                self.assertEqual(
                    packages_for_paths(packages, [changed_path]),
                    {"rskit-errors", "rskit-storage-s3"},
                )

    def test_ci_feature_arg_sets_cover_default_and_all_features(self) -> None:
        self.assertEqual(feature_arg_sets("default"), [[]])
        self.assertEqual(feature_arg_sets("all"), [["--all-features"]])
        self.assertEqual(feature_arg_sets("both"), [[], ["--all-features"]])

    def test_ci_group_by_workspace_is_deterministic(self) -> None:
        packages = [
            Package(
                "rskit-storage-s3",
                "contrib",
                ROOT / "contrib/Cargo.toml",
                ROOT / "contrib/storage/s3",
                "0.0.0",
                True,
            ),
            Package(
                "rskit-errors",
                "core",
                ROOT / "core/Cargo.toml",
                ROOT / "core/rskit-errors",
                "0.0.0",
                True,
            ),
            Package(
                "rskit-config",
                "core",
                ROOT / "core/Cargo.toml",
                ROOT / "core/rskit-config",
                "0.0.0",
                True,
            ),
        ]

        grouped = group_by_workspace(packages)

        self.assertEqual(list(grouped), ["core", "contrib"])
        self.assertEqual([package.name for package in grouped["core"]], ["rskit-config", "rskit-errors"])

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

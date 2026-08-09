"""Tests for repository tooling commands and process helpers."""

from __future__ import annotations

import io
import sys
import tempfile
import unittest
from pathlib import Path

from . import support  # noqa: F401
from rskit_tool.cargo import Package, packages_for_paths
from rskit_tool.cli import build_parser
from rskit_tool.commands.checks import find_crowded_modules
from rskit_tool.commands.ci import feature_arg_sets, group_by_workspace, run_lint, run_test
from rskit_tool.commands.domains import affected_domains, resolve_crate_name
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

    def test_ci_test_runs_doctests_by_default(self) -> None:
        parser = build_parser()
        args = parser.parse_args(["ci", "test", "--scope", "all"])
        self.assertIs(args.func, run_test)
        self.assertTrue(args.run_doctests)

    def test_ci_test_no_doc_disables_doctests(self) -> None:
        parser = build_parser()
        args = parser.parse_args(["ci", "test", "--no-doc"])
        self.assertFalse(args.run_doctests)

    def test_ci_lint_defaults_to_changed_scope_all_features(self) -> None:
        parser = build_parser()
        args = parser.parse_args(["ci", "lint"])
        self.assertIs(args.func, run_lint)
        self.assertEqual(args.scope, "changed")
        self.assertEqual(args.feature_mode, "all")

    def test_ci_lint_accepts_changed_base_and_workspace(self) -> None:
        parser = build_parser()
        args = parser.parse_args(
            ["ci", "lint", "--scope", "changed", "--changed-base", "BASE...HEAD", "--workspace", "core"]
        )
        self.assertEqual(args.changed_base, "BASE...HEAD")
        self.assertEqual(args.workspace, ["core"])

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

    def test_crowded_modules_counts_non_aggregator_files_above_threshold(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "core" / "rskit-demo" / "src"
            src.mkdir(parents=True)
            (src / "lib.rs").write_text("")
            for index in range(4):
                (src / f"concern_{index}.rs").write_text("")

            # Aggregators and small modules stay below the threshold.
            self.assertEqual(find_crowded_modules([src], threshold=4), [])
            # A fifth concern file crosses a threshold of 4.
            (src / "concern_4.rs").write_text("")
            findings = find_crowded_modules([src], threshold=4)
            self.assertEqual(len(findings), 1)
            self.assertEqual(findings[0][1], 5)

    def test_crowded_modules_excludes_tests_and_test_support(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            (src / "tests").mkdir(parents=True)
            (src / "test_support").mkdir()
            (src / "mod.rs").write_text("")
            (src / "test_support.rs").write_text("")
            (src / "real.rs").write_text("")
            for index in range(10):
                (src / "tests" / f"case_{index}.rs").write_text("")
                (src / "test_support" / f"fixture_{index}.rs").write_text("")

            # Only `real.rs` counts: mod.rs, test_support.rs, tests/ and test_support/ are excluded.
            self.assertEqual(find_crowded_modules([src], threshold=0), [(src.as_posix(), 1)])

    def test_run_parallel_preserves_none_results(self) -> None:
        self.assertEqual(run_parallel([ParallelTask("none-result", lambda: None)]), [None])

    def test_run_rejects_capture_with_explicit_stdout(self) -> None:
        with self.assertRaises(ToolError):
            run([sys.executable, "--version"], capture=True, stdout=io.StringIO())


if __name__ == "__main__":
    unittest.main()

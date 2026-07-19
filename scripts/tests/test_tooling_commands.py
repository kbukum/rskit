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
from rskit_tool.commands.domains import DOMAIN_ORDER, affected_domains, load_domains, resolve_crate_name
from rskit_tool.commands.release import (
    _domain_reduced_edges,
    build_domain_dot,
    resolve_publish_order,
    validate_target_subdir,
)
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

    def test_release_output_directory_must_stay_under_target_subdirectory(self) -> None:
        for invalid_target in ("../bad", "/tmp/rskit-bad", "target", "target/../bad"):
            with self.subTest(invalid_target=invalid_target):
                with self.assertRaises(ToolError):
                    validate_target_subdir(invalid_target)

        self.assertEqual(validate_target_subdir("target/release/sbom").name, "sbom")

    def test_publish_order_treats_dev_dependencies_as_ordering_constraints(self) -> None:
        # cargo publish must resolve a versioned dev-dependency on crates.io, so a
        # crate that only dev-depends on an internal crate (e.g. rskit-testutil)
        # must still be published after it. Regression for the first-release
        # failure where rskit-process was ordered before rskit-testutil.
        packages = {
            id_: {"name": id_, "manifest": f"{id_}/Cargo.toml", "version": "0.1.0"}
            for id_ in ("testutil", "consumer", "normal-dep")
        }
        documents = [
            {
                "packages": [
                    {"id": "testutil", "name": "testutil", "dependencies": []},
                    {
                        "id": "consumer",
                        "name": "consumer",
                        "dependencies": [{"name": "testutil", "kind": "dev"}],
                    },
                    {
                        "id": "normal-dep",
                        "name": "normal-dep",
                        "dependencies": [{"name": "testutil", "kind": None}],
                    },
                ],
                "resolve": {"nodes": []},
            }
        ]

        order = [package["name"] for package in resolve_publish_order(packages, documents)]

        self.assertLess(order.index("testutil"), order.index("consumer"))
        self.assertLess(order.index("testutil"), order.index("normal-dep"))

    def test_publish_order_emits_facade_last(self) -> None:
        packages = {
            id_: {"name": id_, "manifest": f"{id_}/Cargo.toml", "version": "0.1.0"}
            for id_ in ("rskit-suite", "rskit-errors")
        }
        documents = [
            {
                "packages": [
                    {"id": "rskit-errors", "name": "rskit-errors", "dependencies": []},
                    {
                        "id": "rskit-suite",
                        "name": "rskit-suite",
                        "dependencies": [{"name": "rskit-errors", "kind": None}],
                    },
                ],
                "resolve": {"nodes": []},
            }
        ]

        order = [package["name"] for package in resolve_publish_order(packages, documents)]

        self.assertEqual(order[-1], "rskit-suite")

    def test_publish_order_raises_on_dependency_cycle(self) -> None:
        packages = {
            id_: {"name": id_, "manifest": f"{id_}/Cargo.toml", "version": "0.1.0"}
            for id_ in ("a", "b")
        }
        documents = [
            {
                "packages": [
                    {"id": "a", "name": "a", "dependencies": [{"name": "b", "kind": None}]},
                    {"id": "b", "name": "b", "dependencies": [{"name": "a", "kind": None}]},
                ],
                "resolve": {"nodes": []},
            }
        ]

        with self.assertRaises(ToolError):
            resolve_publish_order(packages, documents)

    def test_domain_reduced_edges_drops_transitively_reachable_edges(self) -> None:
        # a depends on b, c and d directly, but c and d are reachable through b,
        # so only the essential a -> b layer edge should survive the reduction.
        deps = {
            "a": {"b", "c", "d"},
            "b": {"c"},
            "c": {"d"},
            "d": set(),
        }

        reduced = _domain_reduced_edges(deps)

        self.assertEqual(reduced["a"], ["b"])
        self.assertEqual(reduced["b"], ["c"])
        self.assertEqual(reduced["c"], ["d"])
        self.assertEqual(reduced["d"], [])

    def test_domain_reduced_edges_keeps_independent_edges_sorted(self) -> None:
        # When two dependencies are not reachable from one another, both are kept,
        # and the order is deterministic (sorted) regardless of set iteration order.
        deps = {"root": {"z", "a"}, "a": set(), "z": set()}

        self.assertEqual(_domain_reduced_edges(deps)["root"], ["a", "z"])

    def test_build_domain_dot_renders_every_domain_with_reduced_edges(self) -> None:
        domains = load_domains()
        deps = {name: set(domain.depends_on) for name, domain in domains.items()}
        reduced = _domain_reduced_edges(deps)

        dot = build_domain_dot()

        self.assertTrue(dot.startswith("digraph rskit_domains {"))
        self.assertTrue(dot.endswith("}\n"))
        for name in domains:
            self.assertIn(f'"{name}" [label=', dot)
        expected_edges = {
            f'  "{name}" -> "{dependency}";'
            for name, dependencies in reduced.items()
            for dependency in dependencies
        }
        rendered_edges = {line for line in dot.splitlines() if "->" in line}
        self.assertEqual(rendered_edges, expected_edges)

    def test_build_domain_dot_orders_known_domains_first(self) -> None:
        domains = load_domains()
        dot = build_domain_dot()
        node_order = [
            line.split('"')[1]
            for line in dot.splitlines()
            if "[label=" in line
        ]
        known = [name for name in DOMAIN_ORDER if name in domains]
        self.assertEqual(node_order[: len(known)], known)


if __name__ == "__main__":
    unittest.main()

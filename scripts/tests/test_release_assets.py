"""Tests for the restored release-asset commands (SBOM + dependency graphs)."""

from __future__ import annotations

import unittest

from . import support  # noqa: F401
from rskit_tool.commands.domains import _domain_reduced_edges, build_domain_dot
from rskit_tool.commands.sbom import validate_target_subdir
from rskit_tool.errors import ToolError


class SbomTargetDirTests(unittest.TestCase):
    def test_rejects_absolute_path(self) -> None:
        with self.assertRaises(ToolError):
            validate_target_subdir("/etc/sbom")

    def test_rejects_path_escaping_target(self) -> None:
        with self.assertRaises(ToolError):
            validate_target_subdir("docs/sbom")

    def test_rejects_bare_target_root(self) -> None:
        with self.assertRaises(ToolError):
            validate_target_subdir("target")

    def test_accepts_target_subdir(self) -> None:
        resolved = validate_target_subdir("target/sbom")
        self.assertTrue(str(resolved).endswith("target/sbom"))


class DomainGraphTests(unittest.TestCase):
    def test_reduced_edges_drop_transitive_edges(self) -> None:
        deps = {"a": {"b", "c"}, "b": {"c"}, "c": set()}
        reduced = _domain_reduced_edges(deps)
        # a -> c is reachable via a -> b -> c, so the direct edge is dropped.
        self.assertEqual(reduced["a"], ["b"])
        self.assertEqual(reduced["b"], ["c"])

    def test_build_domain_dot_renders_digraph(self) -> None:
        dot = build_domain_dot()
        self.assertIn("digraph rskit_domains", dot)
        self.assertIn("->", dot)
        self.assertTrue(dot.endswith("}\n"))


if __name__ == "__main__":
    unittest.main()

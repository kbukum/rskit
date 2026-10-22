"""Tests for the restored release-asset commands (SBOM + dependency graphs)."""

from __future__ import annotations

import unittest

from . import support  # noqa: F401
from rskit_tool.commands.groups import build_layer_dot, build_module_index, layer_order, parse_groups, reduced_edges
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


GROUPS_JSONL = "\n".join(
    [
        '{"group":"apps","description":"Apps","allow_groups":["base","mid","testing"],"members":[{"module":"rust:app","workspace":"rust:core","path":"app"}]}',
        '{"group":"base","description":"Base","allow_groups":[],"members":[{"module":"rust:rskit-errors","workspace":"rust:core","path":"core/rskit-errors"}]}',
        '{"group":"mid","description":null,"allow_groups":["base","testing"],"members":[]}',
        '{"group":"testing","description":"Fixtures","allow_groups":null,"members":[{"module":"rust:rskit-testutil","workspace":"rust:core","path":"core/rskit-testutil"}]}',
    ]
)


class GroupGraphTests(unittest.TestCase):
    def test_reduced_edges_drop_transitive_edges(self) -> None:
        deps = {"a": {"b", "c"}, "b": {"c"}, "c": set()}
        reduced = reduced_edges(deps)
        # a -> c is reachable via a -> b -> c, so the direct edge is dropped.
        self.assertEqual(reduced["a"], ["b"])
        self.assertEqual(reduced["b"], ["c"])

    def test_parse_groups_reads_toven_records(self) -> None:
        groups = {group.name: group for group in parse_groups(GROUPS_JSONL)}
        self.assertEqual(groups["base"].modules, ("rskit-errors",))
        self.assertEqual(groups["base"].allow_groups, ())
        self.assertIsNone(groups["testing"].allow_groups)
        self.assertEqual(groups["mid"].description, "")

    def test_parse_groups_rejects_malformed_records(self) -> None:
        with self.assertRaises(ToolError):
            parse_groups('{"group":"x"}')

    def test_layer_order_is_bottom_up_with_unrestricted_last(self) -> None:
        names = [group.name for group in layer_order(parse_groups(GROUPS_JSONL))]
        self.assertEqual(names, ["base", "mid", "apps", "testing"])

    def test_layer_dot_keeps_only_layer_edges(self) -> None:
        dot = build_layer_dot(parse_groups(GROUPS_JSONL))
        self.assertIn("digraph rskit_layers", dot)
        self.assertIn('"apps" -> "mid";', dot)
        self.assertIn('"mid" -> "base";', dot)
        self.assertNotIn('"apps" -> "base";', dot)
        self.assertNotIn("testing", dot)
        self.assertTrue(dot.endswith("}\n"))

    def test_module_index_lists_groups_in_layer_order(self) -> None:
        index = build_module_index(parse_groups(GROUPS_JSONL))
        self.assertLess(index.index("## base"), index.index("## apps"))
        self.assertIn("May depend on: no other group.", index)
        self.assertIn("May depend on: any group.", index)
        self.assertIn("`make check-testing`", index)


if __name__ == "__main__":
    unittest.main()

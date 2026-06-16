"""Tests for independent per-crate versioning helpers."""

from __future__ import annotations

import unittest

from . import support  # noqa: F401
from rskit_tool.versioning import (
    SemVer,
    bump,
    compute_bump_plan,
    inherited_workspace_dep_keys,
    package_version_diff_only,
    parse_package_version,
    parse_workspace_dep_floors,
    parse_workspace_package_version,
    set_package_version,
    set_workspace_dep_version,
    transitive_dependents,
    within_caret,
    workspace_dep_floor_changes,
)


class SemVerTests(unittest.TestCase):
    def test_parse_and_roundtrip(self) -> None:
        for text in ("0.1.0", "0.1.0-alpha.1", "1.2.3-rc.2", "0.2.0-alpha.10"):
            self.assertEqual(str(SemVer.parse(text)), text)

    def test_invalid_version_raises(self) -> None:
        for bad in ("", "1", "1.2", "0.1.x", "v1.2.3", "01.2.3"):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                SemVer.parse(bad)

    def test_release_outranks_prerelease(self) -> None:
        self.assertGreater(SemVer.parse("0.1.0"), SemVer.parse("0.1.0-alpha.1"))

    def test_prerelease_ordering(self) -> None:
        order = [
            "0.1.0-alpha.1",
            "0.1.0-alpha.2",
            "0.1.0-alpha.10",
            "0.1.0-beta.1",
            "0.1.0",
            "0.2.0-alpha.1",
            "0.2.0",
        ]
        parsed = [SemVer.parse(text) for text in order]
        self.assertEqual(parsed, sorted(parsed))

    def test_numeric_identifier_ranks_below_alphanumeric(self) -> None:
        self.assertLess(SemVer.parse("1.0.0-1"), SemVer.parse("1.0.0-alpha"))

    def test_build_metadata_ignored_for_precedence(self) -> None:
        self.assertEqual(SemVer.parse("0.1.0+a"), SemVer.parse("0.1.0+b"))


class BumpTests(unittest.TestCase):
    def test_patch_increments_prerelease_counter(self) -> None:
        self.assertEqual(str(bump(SemVer.parse("0.1.0-alpha.1"), "patch")), "0.1.0-alpha.2")

    def test_patch_on_release_increments_patch(self) -> None:
        self.assertEqual(str(bump(SemVer.parse("0.1.5"), "patch")), "0.1.6")

    def test_patch_appends_counter_when_prerelease_not_numeric(self) -> None:
        self.assertEqual(str(bump(SemVer.parse("0.1.0-alpha"), "patch")), "0.1.0-alpha.1")

    def test_minor_reseeds_prerelease_train(self) -> None:
        self.assertEqual(str(bump(SemVer.parse("0.1.0-alpha.3"), "minor")), "0.2.0-alpha.1")

    def test_minor_on_release(self) -> None:
        self.assertEqual(str(bump(SemVer.parse("0.1.5"), "minor")), "0.2.0")

    def test_major_reseeds(self) -> None:
        self.assertEqual(str(bump(SemVer.parse("0.4.2-alpha.7"), "major")), "1.0.0-alpha.1")

    def test_unknown_kind_raises(self) -> None:
        with self.assertRaises(ValueError):
            bump(SemVer.parse("0.1.0"), "nope")


class CaretTests(unittest.TestCase):
    def test_patch_bump_stays_in_caret(self) -> None:
        floor = SemVer.parse("0.1.0-alpha.1")
        self.assertTrue(within_caret(floor, SemVer.parse("0.1.0-alpha.2")))
        self.assertTrue(within_caret(floor, SemVer.parse("0.1.5")))

    def test_minor_bump_leaves_caret(self) -> None:
        floor = SemVer.parse("0.1.0-alpha.1")
        self.assertFalse(within_caret(floor, SemVer.parse("0.2.0-alpha.1")))
        self.assertFalse(within_caret(floor, SemVer.parse("0.2.0")))

    def test_below_floor_excluded(self) -> None:
        self.assertFalse(within_caret(SemVer.parse("0.1.0-alpha.2"), SemVer.parse("0.1.0-alpha.1")))

    def test_one_dot_x_caret_upper_bound(self) -> None:
        floor = SemVer.parse("1.2.0")
        self.assertTrue(within_caret(floor, SemVer.parse("1.9.0")))
        self.assertFalse(within_caret(floor, SemVer.parse("2.0.0")))


class ManifestEditTests(unittest.TestCase):
    def test_set_package_version_only_in_package_table(self) -> None:
        text = (
            "[package]\n"
            'name = "rskit-errors"\n'
            'version = "0.1.0-alpha.1"\n'
            "edition.workspace = true\n\n"
            "[dependencies]\n"
            'serde = { version = "1" }\n'
        )
        updated, changed = set_package_version(text, "0.2.0-alpha.1")
        self.assertTrue(changed)
        self.assertIn('version = "0.2.0-alpha.1"', updated)
        # The dependency version must be untouched.
        self.assertIn('serde = { version = "1" }', updated)

    def test_set_package_version_is_idempotent(self) -> None:
        text = '[package]\nversion = "0.1.0-alpha.2"\n'
        updated, changed = set_package_version(text, "0.1.0-alpha.2")
        self.assertFalse(changed)
        self.assertEqual(updated, text)

    def test_set_workspace_dep_version_rewrites_floor(self) -> None:
        text = (
            "[workspace.dependencies]\n"
            'rskit-errors = { path = "rskit-errors", version = "0.1.0-alpha.1" }\n'
            'rskit-util = { path = "rskit-util", version = "0.1.0-alpha.1" }\n'
        )
        updated, changed = set_workspace_dep_version(text, "rskit-errors", "0.2.0-alpha.1")
        self.assertTrue(changed)
        self.assertIn('rskit-errors = { path = "rskit-errors", version = "0.2.0-alpha.1" }', updated)
        self.assertIn('rskit-util = { path = "rskit-util", version = "0.1.0-alpha.1" }', updated)

    def test_set_workspace_dep_version_resolves_package_alias(self) -> None:
        text = (
            "[workspace.dependencies]\n"
            'rskit = { package = "rskit-suite", path = "rskit", version = "0.1.0-alpha.1" }\n'
        )
        updated, changed = set_workspace_dep_version(text, "rskit-suite", "0.1.0-alpha.2")
        self.assertTrue(changed)
        self.assertIn('version = "0.1.0-alpha.2"', updated)

    def test_set_workspace_dep_version_absent_crate_no_change(self) -> None:
        text = '[workspace.dependencies]\nrskit-util = { path = "rskit-util", version = "0.1.0-alpha.1" }\n'
        updated, changed = set_workspace_dep_version(text, "rskit-missing", "0.2.0")
        self.assertFalse(changed)
        self.assertEqual(updated, text)

    def test_parse_helpers(self) -> None:
        crate = '[package]\nname = "x"\nversion = "0.3.0-alpha.1"\n'
        self.assertEqual(parse_package_version(crate), SemVer.parse("0.3.0-alpha.1"))
        inherited = "[package]\nversion.workspace = true\n"
        self.assertIsNone(parse_package_version(inherited))
        ws = '[workspace.package]\nversion = "0.1.0-alpha.1"\nedition = "2024"\n'
        self.assertEqual(parse_workspace_package_version(ws), SemVer.parse("0.1.0-alpha.1"))
        floors = parse_workspace_dep_floors(
            "[workspace.dependencies]\n"
            'rskit-errors = { path = "rskit-errors", version = "0.1.0-alpha.1" }\n'
            'tokio = { version = "1", features = ["full"] }\n'
            'pathonly = { path = "x" }\n'
        )
        self.assertEqual(floors["rskit-errors"], SemVer.parse("0.1.0-alpha.1"))
        # tokio matches but parses; pathonly has no version field.
        self.assertNotIn("pathonly", floors)


class VersionOnlyDiffTests(unittest.TestCase):
    def test_delockstep_inherited_to_literal_is_version_only(self) -> None:
        old = (
            "[package]\n"
            'name = "rskit-errors"\n'
            "version.workspace = true\n"
            "edition.workspace = true\n"
        )
        new = (
            "[package]\n"
            'name = "rskit-errors"\n'
            'version = "0.1.0-alpha.1"\n'
            "edition.workspace = true\n"
        )
        self.assertTrue(package_version_diff_only(old, new))

    def test_bump_write_is_version_only(self) -> None:
        old = '[package]\nname = "x"\nversion = "0.1.0-alpha.1"\n'
        new = '[package]\nname = "x"\nversion = "0.1.0-alpha.2"\n'
        self.assertTrue(package_version_diff_only(old, new))

    def test_added_dependency_is_not_version_only(self) -> None:
        old = '[package]\nversion = "0.1.0-alpha.1"\n\n[dependencies]\n'
        new = (
            '[package]\nversion = "0.1.0-alpha.2"\n\n'
            '[dependencies]\nserde = { version = "1" }\n'
        )
        self.assertFalse(package_version_diff_only(old, new))

    def test_dependency_version_change_is_not_version_only(self) -> None:
        old = '[package]\nversion = "0.1.0-alpha.1"\n\n[dependencies]\nserde = { version = "1" }\n'
        new = '[package]\nversion = "0.1.0-alpha.1"\n\n[dependencies]\nserde = { version = "2" }\n'
        self.assertFalse(package_version_diff_only(old, new))


class PlanTests(unittest.TestCase):
    def _versions(self, names: list[str]) -> dict[str, SemVer]:
        return {name: SemVer.parse("0.1.0-alpha.1") for name in names}

    def test_changed_crate_defaults_to_patch(self) -> None:
        names = ["a", "b"]
        plan = compute_bump_plan(
            changed=["a"],
            minor=[],
            dependents={},
            current_versions=self._versions(names),
            baselines=self._versions(names),
            current_floors={},
        )
        self.assertEqual([(a.name, str(a.new), a.kind) for a in plan.actions], [("a", "0.1.0-alpha.2", "patch")])
        self.assertEqual(plan.floor_rewrites, ())

    def test_minor_cascades_to_transitive_dependents_and_rewrites_floor(self) -> None:
        names = ["leaf", "mid", "top"]
        dependents = {"leaf": {"mid"}, "mid": {"top"}}
        plan = compute_bump_plan(
            changed=["leaf"],
            minor=["leaf"],
            dependents=dependents,
            current_versions=self._versions(names),
            baselines=self._versions(names),
            current_floors={"leaf": SemVer.parse("0.1.0-alpha.1")},
        )
        actions = {a.name: (str(a.new), a.kind, a.reason) for a in plan.actions}
        self.assertEqual(actions["leaf"], ("0.2.0-alpha.1", "minor", "changed"))
        self.assertEqual(actions["mid"], ("0.1.0-alpha.2", "patch", "cascade"))
        self.assertEqual(actions["top"], ("0.1.0-alpha.2", "patch", "cascade"))
        self.assertEqual(plan.floor_rewrites, (("leaf", SemVer.parse("0.2.0-alpha.1")),))

    def test_patch_does_not_cascade_or_rewrite_floor(self) -> None:
        names = ["leaf", "top"]
        plan = compute_bump_plan(
            changed=["leaf"],
            minor=[],
            dependents={"leaf": {"top"}},
            current_versions=self._versions(names),
            baselines=self._versions(names),
            current_floors={"leaf": SemVer.parse("0.1.0-alpha.1")},
        )
        self.assertEqual([a.name for a in plan.actions], ["leaf"])
        self.assertEqual(plan.floor_rewrites, ())

    def test_idempotent_when_already_bumped(self) -> None:
        current = {"a": SemVer.parse("0.1.0-alpha.2")}
        plan = compute_bump_plan(
            changed=["a"],
            minor=[],
            dependents={},
            current_versions=current,
            baselines={"a": SemVer.parse("0.1.0-alpha.1")},
            current_floors={},
        )
        self.assertEqual(plan.actions, ())

    def test_unreleased_crate_without_baseline_is_skipped(self) -> None:
        plan = compute_bump_plan(
            changed=["new"],
            minor=[],
            dependents={},
            current_versions={"new": SemVer.parse("0.1.0-alpha.1")},
            baselines={},
            current_floors={},
        )
        self.assertEqual(plan.actions, ())

    def test_transitive_dependents_closure(self) -> None:
        dependents = {"a": {"b"}, "b": {"c"}, "c": {"d"}}
        self.assertEqual(transitive_dependents(["a"], dependents), {"b", "c", "d"})


class WorkspaceFloorInheritanceTests(unittest.TestCase):
    def _ws(self, errors_floor: str, util_floor: str) -> str:
        return (
            "[workspace.dependencies]\n"
            f'rskit-errors = {{ path = "../core/rskit-errors", version = "{errors_floor}" }}\n'
            f'rskit-util = {{ path = "../core/rskit-util", version = "{util_floor}" }}\n'
            'tokio = { version = "1", features = ["full"] }\n'
        )

    def test_floor_change_detects_changed_key(self) -> None:
        old = self._ws("0.1.0-alpha.1", "0.1.0-alpha.1")
        new = self._ws("0.2.0-alpha.1", "0.1.0-alpha.1")
        self.assertEqual(workspace_dep_floor_changes(old, new), {"rskit-errors"})

    def test_no_floor_change_is_empty(self) -> None:
        text = self._ws("0.1.0-alpha.1", "0.1.0-alpha.1")
        self.assertEqual(workspace_dep_floor_changes(text, text), set())

    def test_added_and_removed_floors_count_as_changed(self) -> None:
        old = '[workspace.dependencies]\nrskit-errors = { version = "0.1.0-alpha.1" }\n'
        new = (
            "[workspace.dependencies]\n"
            'rskit-errors = { version = "0.1.0-alpha.1" }\n'
            'rskit-fs = { version = "0.1.0-alpha.1" }\n'
        )
        self.assertEqual(workspace_dep_floor_changes(old, new), {"rskit-fs"})
        self.assertEqual(workspace_dep_floor_changes(new, old), {"rskit-fs"})

    def test_inherited_keys_cover_dotted_and_inline_forms(self) -> None:
        manifest = (
            "[package]\nname = \"rskit-s3\"\nversion = \"0.1.0-alpha.1\"\n\n"
            "[dependencies]\n"
            "rskit-errors.workspace = true\n"
            "aws-sdk-s3 = { workspace = true }\n"
            'serde = { version = "1" }\n\n'
            "[dev-dependencies]\n"
            "rskit-testutil.workspace = true\n\n"
            "[build-dependencies]\n"
            "prost-build = { workspace = true }\n"
        )
        self.assertEqual(
            inherited_workspace_dep_keys(manifest),
            {"rskit-errors", "aws-sdk-s3", "rskit-testutil", "prost-build"},
        )

    def test_inherited_keys_ignore_non_dependency_tables(self) -> None:
        manifest = (
            "[package]\nname = \"x\"\n\n"
            "[features]\n"
            "default = []\n\n"
            "[dependencies]\n"
            "rskit-errors.workspace = true\n"
        )
        self.assertEqual(inherited_workspace_dep_keys(manifest), {"rskit-errors"})

    def test_inherited_keys_cover_target_specific_tables(self) -> None:
        manifest = (
            "[package]\nname = \"x\"\n\n"
            "[target.'cfg(unix)'.dependencies]\n"
            "nix = { workspace = true }\n"
        )
        self.assertEqual(inherited_workspace_dep_keys(manifest), {"nix"})


if __name__ == "__main__":
    unittest.main()

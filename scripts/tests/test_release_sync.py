"""Tests for the release README version-pin sync (bump on_resolved hook)."""

from __future__ import annotations

import json
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

from . import support  # noqa: F401
from rskit_tool.commands import release
from rskit_tool.errors import ToolError


class SetReadmeDependencyVersionsTests(unittest.TestCase):
    def test_rewrites_simple_pin(self) -> None:
        text = 'rskit-errors = "0.2.0-alpha.5"\n'
        out, changed = release.set_readme_dependency_versions(
            text, {"rskit-errors": "0.3.0"}
        )
        self.assertTrue(changed)
        self.assertEqual(out, 'rskit-errors = "0.3.0"\n')

    def test_rewrites_table_attribute_pin(self) -> None:
        text = 'rskit-suite = { version = "0.2.0-alpha.7", features = ["server"] }\n'
        out, changed = release.set_readme_dependency_versions(
            text, {"rskit-suite": "0.3.0"}
        )
        self.assertTrue(changed)
        self.assertEqual(
            out, 'rskit-suite = { version = "0.3.0", features = ["server"] }\n'
        )

    def test_rewrites_column_aligned_pin(self) -> None:
        text = 'rskit-errors     = "0.2.0-alpha.5"\n'
        out, changed = release.set_readme_dependency_versions(
            text, {"rskit-errors": "0.3.0"}
        )
        self.assertTrue(changed)
        self.assertEqual(out, 'rskit-errors     = "0.3.0"\n')

    def test_leaves_unmapped_crate_untouched(self) -> None:
        text = 'rskit-worker = "0.2.0-alpha.4"\n'
        out, changed = release.set_readme_dependency_versions(
            text, {"rskit-errors": "0.3.0"}
        )
        self.assertFalse(changed)
        self.assertEqual(out, text)

    def test_is_idempotent_at_resolved_version(self) -> None:
        text = 'rskit-errors = "0.3.0"\n'
        out, changed = release.set_readme_dependency_versions(
            text, {"rskit-errors": "0.3.0"}
        )
        self.assertFalse(changed)
        self.assertEqual(out, text)

    def test_leaves_prose_untouched(self) -> None:
        text = "We shipped rskit-errors 0.2.0 last week.\n"
        out, changed = release.set_readme_dependency_versions(
            text, {"rskit-errors": "0.3.0"}
        )
        self.assertFalse(changed)
        self.assertEqual(out, text)


class LoadVersionMapTests(unittest.TestCase):
    def test_loads_string_map(self) -> None:
        with TemporaryDirectory() as tmp:
            path = Path(tmp) / "versions.json"
            path.write_text(json.dumps({"rskit-errors": "0.3.0"}), encoding="utf-8")
            self.assertEqual(release.load_version_map(path), {"rskit-errors": "0.3.0"})

    def test_missing_file_raises(self) -> None:
        with self.assertRaises(ToolError):
            release.load_version_map(Path("/nonexistent/versions.json"))

    def test_non_object_raises(self) -> None:
        with TemporaryDirectory() as tmp:
            path = Path(tmp) / "versions.json"
            path.write_text(json.dumps(["0.3.0"]), encoding="utf-8")
            with self.assertRaises(ToolError):
                release.load_version_map(path)


class SyncReadmeVersionsTests(unittest.TestCase):
    def test_writes_only_changed_readmes(self) -> None:
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            readme = root / "README.md"
            readme.write_text('rskit-errors = "0.1.0"\n', encoding="utf-8")
            unchanged = root / "core" / "rskit-worker"
            unchanged.mkdir(parents=True)
            (unchanged / "README.md").write_text(
                'rskit-worker = "0.9.0"\n', encoding="utf-8"
            )
            with mock.patch.object(release, "source_readmes", return_value=[readme]):
                changed = release.sync_readme_versions({"rskit-errors": "0.2.0"})
            self.assertEqual(changed, [readme])
            self.assertEqual(readme.read_text(encoding="utf-8"), 'rskit-errors = "0.2.0"\n')


if __name__ == "__main__":
    unittest.main()

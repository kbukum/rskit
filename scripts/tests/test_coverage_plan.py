"""Tests for coverage command planning."""

from __future__ import annotations

import unittest

from . import support  # noqa: F401
from rskit_tool.cargo import Package, discover_packages
from rskit_tool.coverage.plan import workspace_report_package_args, workspace_test_package_args
from rskit_tool.paths import ROOT


class CoveragePlanTests(unittest.TestCase):
    def test_selected_package_args_are_package_scoped(self) -> None:
        package = Package(
            name="rskit-util",
            workspace="core",
            manifest_path=ROOT / "core" / "rskit-util" / "Cargo.toml",
            root=ROOT / "core" / "rskit-util",
            version="0.0.0",
            publishable=True,
        )

        self.assertEqual(workspace_test_package_args("core", [package]), ["-p", "rskit-util"])
        self.assertEqual(workspace_report_package_args("core", [package]), ["-p", "rskit-util"])

    def test_full_workspace_args_use_workspace_test_and_unscoped_report(self) -> None:
        packages = discover_packages("core")

        self.assertEqual(workspace_test_package_args("core", packages), ["--workspace"])
        self.assertEqual(workspace_report_package_args("core", packages), [])


if __name__ == "__main__":
    unittest.main()

"""Tests for coverage summary parsing."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from . import support  # noqa: F401
from rskit_tool.cargo import Package
from rskit_tool.coverage.summary import parse_package_summaries, parse_summary_json
from rskit_tool.errors import ToolError
from rskit_tool.paths import ROOT


class CoverageSummaryTests(unittest.TestCase):
    def test_package_summaries_are_aggregated_from_owned_files(self) -> None:
        with tempfile.TemporaryDirectory(prefix="rskit-coverage-test-") as temp_dir:
            temp_path = Path(temp_dir)
            summary_path = temp_path / "summary.json"
            package_root = temp_path / "demo"
            (package_root / "src").mkdir(parents=True)
            package = Package(
                name="aggregate-demo",
                workspace="core",
                manifest_path=package_root / "Cargo.toml",
                root=package_root,
                version="0.0.0",
                publishable=False,
            )
            summary_path.write_text(
                json.dumps(
                    {
                        "data": [
                            {
                                "files": [
                                    {
                                        "filename": str(package_root / "src" / "lib.rs"),
                                        "summary": {
                                            "lines": {"covered": 3, "count": 4, "percent": 75.0},
                                            "functions": {"covered": 1, "count": 2, "percent": 50.0},
                                            "regions": {"covered": 5, "count": 10, "percent": 50.0},
                                        },
                                    },
                                    {
                                        "filename": str(package_root / "src" / "extra.rs"),
                                        "summary": {
                                            "lines": {"covered": 1, "count": 1, "percent": 100.0},
                                            "functions": {"covered": 1, "count": 1, "percent": 100.0},
                                            "regions": {"covered": 2, "count": 2, "percent": 100.0},
                                        },
                                    },
                                    {
                                        "filename": str(ROOT / "outside.rs"),
                                        "summary": {"lines": {"covered": 100, "count": 100, "percent": 100.0}},
                                    },
                                ]
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )

            summaries = parse_package_summaries(summary_path, [package])

            self.assertEqual(summaries["aggregate-demo"].lines.percent, 80.0)

    def test_malformed_summary_json_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory(prefix="rskit-coverage-test-") as temp_dir:
            summary_path = Path(temp_dir) / "summary.json"
            summary_path.write_text("{", encoding="utf-8")

            with self.assertRaises(ToolError):
                parse_summary_json(summary_path)


if __name__ == "__main__":
    unittest.main()

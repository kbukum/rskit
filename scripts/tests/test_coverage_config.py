"""Tests for coverage configuration defaults."""

from __future__ import annotations

import unittest
from argparse import Namespace

from . import support  # noqa: F401
from rskit_tool.coverage.config import apply_config_defaults, load_coverage_config
from rskit_tool.paths import COVERAGE_CONFIG


def coverage_args(**overrides: object) -> Namespace:
    defaults: dict[str, object] = {
        "line_threshold": None,
        "function_threshold": None,
        "region_threshold": None,
        "security_line_threshold": None,
        "security_packages": None,
        "jobs": None,
        "coverage_clean": None,
        "exclude_packages": None,
        "html": None,
        "progress_interval": None,
        "progress_style": None,
        "progress_width": None,
    }
    defaults.update(overrides)
    return Namespace(**defaults)


class CoverageConfigTests(unittest.TestCase):
    def test_loads_repository_coverage_config(self) -> None:
        config = load_coverage_config(COVERAGE_CONFIG)

        self.assertIn("rskit-security", config.security.packages)
        self.assertEqual(config.packages["rskit-suite"].line, 80.0)
        self.assertEqual(config.runner.clean, "profraw")
        self.assertIn("rskit-suite", config.runner.exclude_packages)

    def test_explicit_line_threshold_overrides_security_and_package_defaults(self) -> None:
        args = apply_config_defaults(coverage_args(line_threshold=0.0), load_coverage_config(COVERAGE_CONFIG))

        self.assertEqual(args.security_line_threshold, 0.0)
        self.assertEqual(args.package_thresholds["rskit-suite"].line, 0.0)

    def test_explicit_security_threshold_is_preserved(self) -> None:
        args = apply_config_defaults(
            coverage_args(line_threshold=0.0, security_line_threshold=95.0),
            load_coverage_config(COVERAGE_CONFIG),
        )

        self.assertEqual(args.security_line_threshold, 95.0)


if __name__ == "__main__":
    unittest.main()

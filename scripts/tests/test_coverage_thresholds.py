"""Tests for coverage threshold evaluation."""

from __future__ import annotations

import unittest

from . import support  # noqa: F401
from rskit_tool.coverage.models import CoverageTotals, Metric, ThresholdOverride, Thresholds
from rskit_tool.coverage.thresholds import evaluate_thresholds, split_names


class CoverageThresholdTests(unittest.TestCase):
    def test_passing_and_failing_totals_are_classified_by_metric(self) -> None:
        thresholds = Thresholds(90.0, 90.0, 90.0, None, None, None, frozenset(), {})
        good = CoverageTotals(
            lines=Metric(covered=9, count=10, percent=90.0),
            functions=Metric(covered=1, count=1, percent=100.0),
            regions=Metric(covered=2, count=2, percent=100.0),
        )
        bad = CoverageTotals(
            lines=Metric(covered=8, count=10, percent=80.0),
            functions=Metric(covered=0, count=1, percent=0.0),
            regions=Metric(covered=1, count=2, percent=50.0),
        )

        self.assertEqual(evaluate_thresholds("demo", good, thresholds), [])
        self.assertEqual(len(evaluate_thresholds("demo", bad, thresholds)), 3)

    def test_package_and_security_threshold_overrides_take_precedence(self) -> None:
        thresholds = Thresholds(
            90.0,
            None,
            None,
            95.0,
            None,
            None,
            frozenset({"secure-demo"}),
            {"demo": ThresholdOverride(lines=80.0)},
        )

        self.assertEqual(thresholds.line_threshold_for("demo"), 80.0)
        self.assertEqual(thresholds.line_threshold_for("secure-demo"), 95.0)

    def test_split_names_accepts_commas_and_whitespace(self) -> None:
        self.assertEqual(split_names("a,b c\n d"), {"a", "b", "c", "d"})


if __name__ == "__main__":
    unittest.main()

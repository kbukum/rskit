"""Tests for coverage progress and formatting helpers."""

from __future__ import annotations

import contextlib
import io
import tempfile
import unittest

from . import support  # noqa: F401
from rskit_tool.cargo import Package
from rskit_tool.coverage.events import CoverageEvent, CoverageEventBus, CoverageProgressReporter
from rskit_tool.coverage.formatting import format_bar, format_package_result, format_percent, format_stage_progress
from rskit_tool.formatting import format_duration
from rskit_tool.coverage.models import CoverageTotals, Metric, ModuleResult
from rskit_tool.coverage.runner import print_command_log_tail
from rskit_tool.paths import ROOT


class CoverageFormattingTests(unittest.TestCase):
    def test_duration_renders_subsecond_as_under_one_second(self) -> None:
        # A positive sub-second value must not truncate to "0s" — it should read
        # as time still remaining in the live wait UX.
        self.assertEqual(format_duration(0.8), "<1s")
        self.assertEqual(format_duration(0.0), "0s")
        self.assertEqual(format_duration(1.0), "1s")
        self.assertEqual(format_duration(90.0), "1m30s")

    def test_package_result_and_progress_formatting_include_counters(self) -> None:
        package = Package(
            name="demo",
            workspace="core",
            manifest_path=ROOT / "core" / "demo" / "Cargo.toml",
            root=ROOT / "core" / "demo",
            version="0.0.0",
            publishable=False,
        )
        totals = CoverageTotals(
            lines=Metric(covered=9, count=10, percent=90.0),
            functions=Metric(covered=1, count=1, percent=100.0),
            regions=Metric(covered=2, count=2, percent=100.0),
        )
        result = ModuleResult(package, "passed", totals, 90.0, 90.0, 90.0, ())

        self.assertTrue(format_package_result(result, 1, 1).startswith("✓ [packages 1/1, 100.0%] demo"))
        self.assertEqual(format_percent(2, 4), "50.0%")
        self.assertEqual(format_bar(2, 4, width=8), "[====----]")
        self.assertEqual(
            format_stage_progress("demo", "core", "finished test", 2, 4, 10, 40, "jobs 1/2 done"),
            "→ [overall 10/40, 25.0%; job 2/4, 50.0%; jobs 1/2 done] demo (core): finished test",
        )

    def test_event_bus_and_reporter_track_progress(self) -> None:
        package = Package(
            name="demo",
            workspace="core",
            manifest_path=ROOT / "core" / "demo" / "Cargo.toml",
            root=ROOT / "core" / "demo",
            version="0.0.0",
            publishable=False,
        )
        events: list[str] = []
        bus = CoverageEventBus()
        bus.subscribe(lambda event: events.append(event.kind))

        bus.emit(CoverageEvent("package_started", package=package))

        self.assertEqual(events, ["package_started"])

    def test_progress_reporter_renders_heartbeat_and_dashboard(self) -> None:
        package = Package(
            name="demo",
            workspace="core",
            manifest_path=ROOT / "core" / "demo" / "Cargo.toml",
            root=ROOT / "core" / "demo",
            version="0.0.0",
            publishable=False,
        )
        reporter = CoverageProgressReporter(total_packages=2, steps_per_package=4)
        with contextlib.redirect_stdout(io.StringIO()):
            reporter.handle(CoverageEvent("step_completed", package=package, step="clean", package_completed_steps=1))
        self.assertEqual(reporter.completed_steps, 1)

        with contextlib.redirect_stdout(io.StringIO()) as output:
            reporter.handle(
                CoverageEvent(
                    "step_heartbeat",
                    package=package,
                    step="test",
                    package_completed_steps=1,
                    elapsed_seconds=65.0,
                    last_output="Compiling rskit-util v0.1.0",
                )
            )
        self.assertIn("still running test for 1m05s", output.getvalue())

        dashboard = CoverageProgressReporter(total_packages=2, steps_per_package=4, style="bar", bar_width=10)
        with contextlib.redirect_stdout(io.StringIO()):
            dashboard.handle(CoverageEvent("package_started", package=package))
            dashboard.handle(CoverageEvent("step_completed", package=package, step="clean", package_completed_steps=1))
        self.assertTrue(dashboard.dashboard_lines()[0].startswith("coverage [=---------] 12.5%"))

    def test_command_log_tail_prints_bounded_failure_context(self) -> None:
        target = ROOT / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=target) as directory:
            log_path = ROOT / directory / "command.log"
            log_path.write_text("line 1\nline 2\nline 3\n", encoding="utf-8")

            with contextlib.redirect_stderr(io.StringIO()) as output:
                print_command_log_tail(log_path, max_lines=2)

        rendered = output.getvalue()
        self.assertIn("last 2 line(s)", rendered)
        self.assertNotIn("line 1", rendered)
        self.assertIn("line 2", rendered)
        self.assertIn("line 3", rendered)


if __name__ == "__main__":
    unittest.main()

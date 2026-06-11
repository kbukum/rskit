"""Coverage tooling self-tests."""

from __future__ import annotations

import contextlib
import io
import json
import tempfile
from argparse import Namespace
from pathlib import Path

from ..cargo import Package, discover_packages
from ..errors import ToolError
from ..paths import COVERAGE_CONFIG, ROOT
from .config import apply_config_defaults, load_coverage_config
from .events import CoverageEvent, CoverageEventBus, CoverageProgressReporter
from .formatting import format_bar, format_package_result, format_percent, format_stage_progress
from .models import CoverageTotals, Metric, ModuleResult, ThresholdOverride, Thresholds
from .plan import workspace_report_package_args, workspace_test_package_args
from .summary import parse_package_summaries, parse_summary_json
from .thresholds import evaluate_thresholds


def run_self_tests() -> None:
    """Run coverage-specific fast self-tests."""

    good = CoverageTotals(
        lines=Metric(covered=9, count=10, percent=90.0),
        functions=Metric(covered=1, count=1, percent=100.0),
        regions=Metric(covered=2, count=2, percent=100.0),
    )
    thresholds = Thresholds(90.0, 90.0, 90.0, None, None, None, frozenset(), {})
    if evaluate_thresholds("demo", good, thresholds) != []:
        raise ToolError("coverage self-test failed: passing totals reported threshold failures")
    bad = CoverageTotals(
        lines=Metric(covered=8, count=10, percent=80.0),
        functions=Metric(covered=0, count=1, percent=0.0),
        regions=Metric(covered=1, count=2, percent=50.0),
    )
    if len(evaluate_thresholds("demo", bad, thresholds)) != 3:
        raise ToolError("coverage self-test failed: failing totals did not report all threshold failures")
    overridden = Thresholds(
        90.0,
        None,
        None,
        95.0,
        None,
        None,
        frozenset({"secure-demo"}),
        {"demo": ThresholdOverride(lines=80.0)},
    )
    if overridden.line_threshold_for("demo") != 80.0:
        raise ToolError("coverage self-test failed: package threshold did not override defaults")
    if overridden.line_threshold_for("secure-demo") != 95.0:
        raise ToolError("coverage self-test failed: security threshold did not override defaults")
    package = Package(
        name="demo",
        workspace="core",
        manifest_path=ROOT / "core" / "demo" / "Cargo.toml",
        root=ROOT / "core" / "demo",
        version="0.0.0",
        publishable=False,
    )
    result = ModuleResult(
        package=package,
        status="passed",
        totals=good,
        line_threshold=90.0,
        function_threshold=90.0,
        region_threshold=90.0,
        failures=(),
    )
    if not format_package_result(result, 1, 1).startswith("✓ [packages 1/1, 100.0%] demo: 90.00% lines"):
        raise ToolError("coverage self-test failed: package result line was not formatted with counters")
    if format_percent(2, 4) != "50.0%":
        raise ToolError("coverage self-test failed: completion percent was not formatted correctly")
    if format_bar(2, 4, width=8) != "[====----]":
        raise ToolError("coverage self-test failed: progress bar was not formatted correctly")
    if format_stage_progress("demo", "core", "finished test", 2, 4, 10, 40, "jobs 1/2 done") != (
        "→ [overall 10/40, 25.0%; job 2/4, 50.0%; jobs 1/2 done] demo (core): finished test"
    ):
        raise ToolError("coverage self-test failed: stage progress line was not formatted correctly")
    events: list[str] = []
    event_bus = CoverageEventBus()
    event_bus.subscribe(lambda event: events.append(event.kind))
    event_bus.emit(CoverageEvent("package_started", package=package))
    if events != ["package_started"]:
        raise ToolError("coverage self-test failed: event bus did not deliver events")
    reporter = CoverageProgressReporter(total_packages=2, steps_per_package=4)
    with contextlib.redirect_stdout(io.StringIO()):
        reporter.handle(CoverageEvent("step_completed", package=package, step="clean", package_completed_steps=1))
    if reporter.completed_steps != 1:
        raise ToolError("coverage self-test failed: reporter did not track completed steps")
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
    if "still running test for 1m05s" not in output.getvalue():
        raise ToolError("coverage self-test failed: heartbeat progress was not rendered")
    dashboard = CoverageProgressReporter(total_packages=2, steps_per_package=4, style="bar", bar_width=10)
    with contextlib.redirect_stdout(io.StringIO()):
        dashboard.handle(CoverageEvent("package_started", package=package))
        dashboard.handle(CoverageEvent("step_completed", package=package, step="clean", package_completed_steps=1))
    if not dashboard.dashboard_lines()[0].startswith("coverage [=---------] 12.5%"):
        raise ToolError("coverage self-test failed: dashboard progress bar did not advance")
    config = load_coverage_config(COVERAGE_CONFIG)
    if "rskit-security" not in config.security.packages:
        raise ToolError("coverage self-test failed: coverage config security packages were not loaded")
    if config.packages["rskit-suite"].line != 80.0:
        raise ToolError("coverage self-test failed: package threshold overrides were not loaded")
    if config.runner.clean != "profraw":
        raise ToolError("coverage self-test failed: coverage clean mode was not loaded")
    if "rskit-suite" not in config.runner.exclude_packages:
        raise ToolError("coverage self-test failed: excluded coverage packages were not loaded")
    threshold_args = coverage_args(line_threshold=0.0)
    apply_config_defaults(threshold_args, config)
    if threshold_args.security_line_threshold != 0.0:
        raise ToolError("coverage self-test failed: explicit line threshold did not override security threshold")
    if threshold_args.package_thresholds["rskit-suite"].line != 0.0:
        raise ToolError("coverage self-test failed: explicit line threshold did not override package threshold")
    security_args = coverage_args(line_threshold=0.0, security_line_threshold=95.0)
    apply_config_defaults(security_args, config)
    if security_args.security_line_threshold != 95.0:
        raise ToolError("coverage self-test failed: explicit security threshold was not preserved")
    util_package = Package(
        name="rskit-util",
        workspace="core",
        manifest_path=ROOT / "core" / "rskit-util" / "Cargo.toml",
        root=ROOT / "core" / "rskit-util",
        version="0.0.0",
        publishable=True,
    )
    if workspace_test_package_args("core", [util_package]) != ["-p", "rskit-util"]:
        raise ToolError("coverage self-test failed: selected package test args were not package-scoped")
    if workspace_report_package_args("core", [util_package]) != ["-p", "rskit-util"]:
        raise ToolError("coverage self-test failed: selected package report args were not package-scoped")
    all_core_packages = discover_packages("core")
    if workspace_test_package_args("core", all_core_packages) != ["--workspace"]:
        raise ToolError("coverage self-test failed: full workspace test args did not use --workspace")
    if workspace_report_package_args("core", all_core_packages) != []:
        raise ToolError("coverage self-test failed: full workspace report args used unsupported --workspace")
    with tempfile.TemporaryDirectory(prefix="rskit-coverage-selftest-") as temp_dir:
        temp_path = Path(temp_dir)
        summary_path = temp_path / "summary.json"
        package_root = temp_path / "demo"
        package_root.mkdir()
        aggregate_package = Package(
            name="aggregate-demo",
            workspace="core",
            manifest_path=package_root / "Cargo.toml",
            root=package_root,
            version="0.0.0",
            publishable=False,
        )
        lib_path = package_root / "src" / "lib.rs"
        extra_path = package_root / "src" / "extra.rs"
        summary_path.write_text(
            json.dumps(
                {
                    "data": [
                        {
                            "files": [
                                {
                                    "filename": str(lib_path),
                                    "summary": {
                                        "lines": {"covered": 3, "count": 4, "percent": 75.0},
                                        "functions": {"covered": 1, "count": 2, "percent": 50.0},
                                        "regions": {"covered": 5, "count": 10, "percent": 50.0},
                                    },
                                },
                                {
                                    "filename": str(extra_path),
                                    "summary": {
                                        "lines": {"covered": 1, "count": 1, "percent": 100.0},
                                        "functions": {"covered": 1, "count": 1, "percent": 100.0},
                                        "regions": {"covered": 2, "count": 2, "percent": 100.0},
                                    },
                                },
                            ],
                            "totals": {
                                "lines": {"covered": 4, "count": 5, "percent": 80.0},
                                "functions": {"covered": 2, "count": 3, "percent": 66.6666666667},
                                "regions": {"covered": 7, "count": 12, "percent": 58.3333333333},
                            },
                        }
                    ]
                }
            ),
            encoding="utf-8",
        )
        package_summaries = parse_package_summaries(summary_path, [aggregate_package])
        if package_summaries["aggregate-demo"].lines.percent != 80.0:
            raise ToolError("coverage self-test failed: package summaries were not aggregated from files")
        summary_path.write_text("{", encoding="utf-8")
        try:
            parse_summary_json(summary_path)
        except ToolError:
            pass
        else:
            raise ToolError("coverage self-test failed: malformed summary JSON was accepted")


def coverage_args(**overrides: object) -> Namespace:
    """Build coverage args for config self-tests."""

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

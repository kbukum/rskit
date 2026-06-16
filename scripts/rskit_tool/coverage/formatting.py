"""Coverage output formatting."""

from __future__ import annotations

from ..formatting import format_bar, format_duration, format_percent
from .models import Metric, ModuleResult

__all__ = [
    "format_bar",
    "format_duration",
    "format_percent",
    "format_package_result",
    "format_stage_progress",
    "format_metric",
    "format_percent_metric",
    "format_percent_result",
    "format_threshold",
]



def format_package_result(result: ModuleResult, completed: int, total: int) -> str:
    """Format a one-line package completion message."""

    prefix = f"[packages {completed}/{total}, {format_percent(completed, total)}]"
    if result.status == "failed":
        return f"✗ {prefix} {result.package.name}: {result.error}"
    if result.status == "below-threshold":
        return f"✗ {prefix} {result.package.name}: {', '.join(result.failures)}"
    return f"✓ {prefix} {result.package.name}: {format_percent_result(result.totals.lines if result.totals else None)} lines"


def format_stage_progress(
    package_name: str,
    workspace: str,
    status: str,
    job_completed_steps: int,
    job_total_steps: int,
    completed_steps: int,
    total_steps: int,
    run_status: str,
) -> str:
    """Format a one-line package stage progress message."""

    return (
        f"→ [overall {completed_steps}/{total_steps}, {format_percent(completed_steps, total_steps)}; "
        f"job {job_completed_steps}/{job_total_steps}, "
        f"{format_percent(job_completed_steps, job_total_steps)}; {run_status}] "
        f"{package_name} ({workspace}): {status}"
    )


def format_metric(metric: Metric) -> str:
    """Format a metric row."""

    return f"{metric.covered:7d}  {metric.missed:6d}  {metric.count:5d}  {format_percent_metric(metric):>7}"


def format_percent_metric(metric: Metric) -> str:
    """Format a metric percent."""

    return "N/A" if metric.percent is None else f"{metric.percent:.2f}%"


def format_percent_result(metric: Metric | None) -> str:
    """Format percent for summary tables."""

    return "N/A" if metric is None or metric.percent is None else f"{metric.percent:.2f}%"


def format_threshold(value: float | None) -> str:
    """Format an optional threshold."""

    return "disabled" if value is None else f"{value:.2f}"

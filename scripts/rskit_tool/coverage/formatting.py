"""Coverage output formatting."""

from __future__ import annotations

from .models import Metric, ModuleResult


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


def format_percent(completed: int, total: int) -> str:
    """Format completion percentage."""

    if total <= 0:
        return "100.0%"
    return f"{(completed / total) * 100:.1f}%"


def format_bar(completed: int, total: int, *, width: int) -> str:
    """Format a fixed-width ASCII progress bar."""

    if width < 1:
        return ""
    ratio = 1.0 if total <= 0 else min(1.0, max(0.0, completed / total))
    filled = int(round(ratio * width))
    return "[" + ("=" * filled) + ("-" * (width - filled)) + "]"


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


def format_duration(seconds: float) -> str:
    """Format elapsed time compactly."""

    total_seconds = max(0, int(seconds))
    minutes, seconds = divmod(total_seconds, 60)
    hours, minutes = divmod(minutes, 60)
    if hours:
        return f"{hours}h{minutes:02d}m{seconds:02d}s"
    if minutes:
        return f"{minutes}m{seconds:02d}s"
    return f"{seconds}s"

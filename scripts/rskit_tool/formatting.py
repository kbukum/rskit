"""Generic, dependency-free output formatting helpers.

These primitives are shared by any tooling command that renders progress, so
they live in a leaf module both ``coverage`` and ``publish`` can depend on
without coupling those areas to each other.
"""

from __future__ import annotations


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

"""Coverage threshold evaluation."""

from __future__ import annotations

import argparse

from .models import CoverageTotals, ThresholdOverride, Thresholds


def thresholds_from_args(args: argparse.Namespace) -> Thresholds:
    """Build threshold config."""

    return Thresholds(
        lines=args.line_threshold,
        functions=args.function_threshold,
        regions=args.region_threshold,
        security_lines=args.security_line_threshold,
        security_functions=args.security_function_threshold,
        security_regions=args.security_region_threshold,
        security_packages=frozenset(split_names(args.security_packages)),
        package_overrides=package_overrides_from_args(args),
    )


def evaluate_thresholds(package_name: str, totals: CoverageTotals, thresholds: Thresholds) -> list[str]:
    """Return threshold failures for measured totals."""

    if not totals.measured:
        return []
    failures: list[str] = []
    append_threshold_failure(failures, "line", totals.lines.percent, thresholds.line_threshold_for(package_name))
    function_threshold = thresholds.function_threshold_for(package_name)
    if function_threshold is not None:
        append_threshold_failure(failures, "function", totals.functions.percent, function_threshold)
    region_threshold = thresholds.region_threshold_for(package_name)
    if region_threshold is not None:
        append_threshold_failure(failures, "region", totals.regions.percent, region_threshold)
    return failures


def package_overrides_from_args(args: argparse.Namespace) -> dict[str, ThresholdOverride]:
    """Build package-specific threshold overrides from merged config."""

    overrides: dict[str, ThresholdOverride] = {}
    for package, override in getattr(args, "package_thresholds", {}).items():
        overrides[package] = ThresholdOverride(
            lines=override.line,
            functions=override.function,
            regions=override.region,
        )
    return overrides


def append_threshold_failure(failures: list[str], label: str, percent: float | None, threshold: float) -> None:
    """Append a threshold failure when coverage is below the configured minimum."""

    if percent is None:
        failures.append(f"{label} coverage is N/A; required >= {threshold:.2f}%")
    elif percent < threshold:
        failures.append(f"{label} coverage {percent:.2f}% is below {threshold:.2f}%")


def split_names(value: str) -> set[str]:
    """Split a comma or whitespace separated package list."""

    return {item.strip() for chunk in value.split(",") for item in chunk.split() if item.strip()}

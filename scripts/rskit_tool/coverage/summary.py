"""Coverage summary parsing and rendering."""

from __future__ import annotations

import json
from collections.abc import Sequence
from pathlib import Path

from ..cargo import Package, is_relative_to
from ..errors import ToolError
from ..paths import COVERAGE_ROOT, ROOT
from .formatting import format_metric, format_percent_result, format_threshold
from .models import CoverageTotals, Metric, ModuleResult, Thresholds


def parse_summary_json(path: Path) -> CoverageTotals:
    """Parse llvm-cov summary JSON."""

    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        totals = data["data"][0]["totals"]
    except FileNotFoundError as error:
        raise ToolError(f"coverage summary not found: {display_path(path)}") from error
    except json.JSONDecodeError as error:
        raise ToolError(f"coverage summary is not valid JSON: {display_path(path)}: {error}") from error
    except (IndexError, KeyError, TypeError) as error:
        raise ToolError(f"coverage summary has unexpected format: {display_path(path)}") from error
    return CoverageTotals(
        lines=parse_metric(totals.get("lines", {})),
        functions=parse_metric(totals.get("functions", {})),
        regions=parse_metric(totals.get("regions", {})),
    )


def parse_package_summaries(path: Path, packages: Sequence[Package]) -> dict[str, CoverageTotals]:
    """Parse one workspace llvm-cov JSON summary into per-package totals."""

    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        files = data["data"][0].get("files", [])
    except FileNotFoundError as error:
        raise ToolError(f"coverage summary not found: {display_path(path)}") from error
    except json.JSONDecodeError as error:
        raise ToolError(f"coverage summary is not valid JSON: {display_path(path)}: {error}") from error
    except (IndexError, KeyError, TypeError) as error:
        raise ToolError(f"coverage summary has unexpected format: {display_path(path)}") from error

    if not isinstance(files, list):
        raise ToolError(f"coverage summary has unexpected files format: {display_path(path)}")

    accumulators = {package.name: CoverageAccumulator() for package in packages}
    package_roots = sorted(((package.root.resolve(), package.name) for package in packages), key=lambda item: len(item[0].parts), reverse=True)

    for file_entry in files:
        if not isinstance(file_entry, dict):
            raise ToolError(f"coverage summary has unexpected file entry format: {display_path(path)}")
        filename = file_entry.get("filename")
        summary = file_entry.get("summary")
        if not isinstance(filename, str) or not isinstance(summary, dict):
            raise ToolError(f"coverage summary has unexpected file entry format: {display_path(path)}")
        source_path = normalize_source_path(filename)
        package_name = package_for_source(source_path, package_roots)
        if package_name is None:
            continue
        accumulators[package_name].add(summary)

    return {package.name: accumulators[package.name].totals() for package in packages}


class CoverageAccumulator:
    """Mutable coverage counter used while aggregating file summaries."""

    def __init__(self) -> None:
        self.lines_covered = 0
        self.lines_count = 0
        self.functions_covered = 0
        self.functions_count = 0
        self.regions_covered = 0
        self.regions_count = 0

    def add(self, summary: dict[str, object]) -> None:
        """Add one llvm-cov file summary."""

        lines = parse_metric(summary.get("lines", {}))
        functions = parse_metric(summary.get("functions", {}))
        regions = parse_metric(summary.get("regions", {}))
        self.lines_covered += lines.covered
        self.lines_count += lines.count
        self.functions_covered += functions.covered
        self.functions_count += functions.count
        self.regions_covered += regions.covered
        self.regions_count += regions.count

    def totals(self) -> CoverageTotals:
        """Return immutable coverage totals."""

        return CoverageTotals(
            lines=aggregate_metric(self.lines_covered, self.lines_count),
            functions=aggregate_metric(self.functions_covered, self.functions_count),
            regions=aggregate_metric(self.regions_covered, self.regions_count),
        )


def aggregate_metric(covered: int, count: int) -> Metric:
    """Build one aggregate coverage metric."""

    percent = None if count == 0 else covered / count * 100.0
    return Metric(covered=covered, count=count, percent=percent)


def normalize_source_path(filename: str) -> Path:
    """Normalize a report filename to an absolute path."""

    path = Path(filename)
    if path.is_absolute():
        return path.resolve()
    return (ROOT / path).resolve()


def package_for_source(source_path: Path, package_roots: Sequence[tuple[Path, str]]) -> str | None:
    """Return the package owning a covered source file."""

    for package_root, package_name in package_roots:
        if is_relative_to(source_path, package_root):
            return package_name
    return None


def parse_metric(data: object) -> Metric:
    """Parse one llvm-cov metric."""

    if not isinstance(data, dict):
        raise ToolError("coverage summary metric has unexpected format")
    percent_value = data.get("percent")
    return Metric(
        covered=int(data.get("covered", 0)),
        count=int(data.get("count", 0)),
        percent=None if percent_value is None else float(percent_value),
    )


def write_module_summary(path: Path, package: Package, totals: CoverageTotals, failures: Sequence[str], thresholds: Thresholds) -> None:
    """Write a human-readable per-module summary."""

    lines = [
        f"package: {package.name}",
        f"workspace: {package.workspace}",
        f"status: {'n/a' if not totals.measured else ('below-threshold' if failures else 'passed')}",
        f"line-threshold: {thresholds.line_threshold_for(package.name):.2f}",
        f"function-threshold: {format_threshold(thresholds.function_threshold_for(package.name))}",
        f"region-threshold: {format_threshold(thresholds.region_threshold_for(package.name))}",
        "",
        "metric      covered  missed  total  percent",
        f"lines       {format_metric(totals.lines)}",
        f"functions   {format_metric(totals.functions)}",
        f"regions     {format_metric(totals.regions)}",
    ]
    if failures:
        lines.extend(["", "failures:", *[f"- {failure}" for failure in failures]])
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_summaries(results: Sequence[ModuleResult], thresholds: Thresholds, mode: str) -> None:
    """Write machine-readable and Markdown coverage summaries."""

    COVERAGE_ROOT.mkdir(parents=True, exist_ok=True)
    payload = {
        "mode": mode,
        "thresholds": {
            "lines": thresholds.lines,
            "functions": thresholds.functions,
            "regions": thresholds.regions,
            "security_lines": thresholds.security_lines,
            "security_functions": thresholds.security_functions,
            "security_regions": thresholds.security_regions,
            "security_packages": sorted(thresholds.security_packages),
            "packages": {
                package: {
                    "lines": override.lines,
                    "functions": override.functions,
                    "regions": override.regions,
                }
                for package, override in sorted(thresholds.package_overrides.items())
            },
        },
        "modules": [result_to_json(result) for result in results],
    }
    (COVERAGE_ROOT / "summary.json").write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (COVERAGE_ROOT / "summary.md").write_text(render_summary_markdown(results), encoding="utf-8")


def result_to_json(result: ModuleResult) -> dict[str, object]:
    """Serialize one module result."""

    totals = result.totals
    return {
        "package": result.package.name,
        "workspace": result.package.workspace,
        "path": str(result.package.root.relative_to(ROOT)),
        "status": result.status,
        "thresholds": {
            "lines": result.line_threshold,
            "functions": result.function_threshold,
            "regions": result.region_threshold,
        },
        "coverage": None
        if totals is None
        else {
            "lines": metric_to_json(totals.lines),
            "functions": metric_to_json(totals.functions),
            "regions": metric_to_json(totals.regions),
        },
        "failures": list(result.failures),
        "error": result.error,
        "log": None if result.log_path is None else str(result.log_path.relative_to(ROOT)),
    }


def metric_to_json(metric: Metric) -> dict[str, object]:
    """Serialize one metric."""

    return {"covered": metric.covered, "missed": metric.missed, "count": metric.count, "percent": metric.percent}


def render_summary_markdown(results: Sequence[ModuleResult]) -> str:
    """Render the top-level Markdown coverage summary."""

    rows = [
        "# Coverage summary",
        "",
        "| Workspace | Package | Status | Lines | Functions | Regions | Notes |",
        "|-----------|---------|--------|------:|----------:|--------:|-------|",
    ]
    for result in results:
        rows.append(
            "| {workspace} | `{package}` | {status} | {lines} | {functions} | {regions} | {notes} |".format(
                workspace=result.package.workspace,
                package=result.package.name,
                status=result.status,
                lines=format_percent_result(result.totals.lines if result.totals else None),
                functions=format_percent_result(result.totals.functions if result.totals else None),
                regions=format_percent_result(result.totals.regions if result.totals else None),
                notes="<br>".join(result.failures) if result.failures else (result.error or ""),
            )
        )
    return "\n".join(rows) + "\n"


def display_path(path: Path) -> str:
    """Return a compact display path when possible."""

    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)

"""Coverage data models."""

from __future__ import annotations

import dataclasses
from pathlib import Path

from ..cargo import Package


@dataclasses.dataclass(frozen=True)
class ThresholdOverride:
    """Optional coverage threshold override."""

    lines: float | None = None
    functions: float | None = None
    regions: float | None = None


@dataclasses.dataclass(frozen=True)
class Metric:
    """Coverage counts for one llvm-cov metric."""

    covered: int
    count: int
    percent: float | None

    @property
    def missed(self) -> int:
        """Return uncovered count."""

        return self.count - self.covered


@dataclasses.dataclass(frozen=True)
class CoverageTotals:
    """Coverage totals reported by llvm-cov."""

    lines: Metric
    functions: Metric
    regions: Metric

    @property
    def measured(self) -> bool:
        """Return true when llvm-cov found instrumented lines."""

        return self.lines.count > 0


@dataclasses.dataclass(frozen=True)
class Thresholds:
    """Coverage thresholds."""

    lines: float
    functions: float | None
    regions: float | None
    security_lines: float | None
    security_functions: float | None
    security_regions: float | None
    security_packages: frozenset[str]
    package_overrides: dict[str, ThresholdOverride]

    def line_threshold_for(self, package: str) -> float:
        """Return the line threshold for a package."""

        if package in self.package_overrides and self.package_overrides[package].lines is not None:
            return self.package_overrides[package].lines
        if self.security_lines is not None and package in self.security_packages:
            return self.security_lines
        return self.lines

    def function_threshold_for(self, package: str) -> float | None:
        """Return the function threshold for a package."""

        if package in self.package_overrides and self.package_overrides[package].functions is not None:
            return self.package_overrides[package].functions
        if self.security_functions is not None and package in self.security_packages:
            return self.security_functions
        return self.functions

    def region_threshold_for(self, package: str) -> float | None:
        """Return the region threshold for a package."""

        if package in self.package_overrides and self.package_overrides[package].regions is not None:
            return self.package_overrides[package].regions
        if self.security_regions is not None and package in self.security_packages:
            return self.security_regions
        return self.regions


@dataclasses.dataclass(frozen=True)
class ModuleResult:
    """Coverage result for one package."""

    package: Package
    status: str
    totals: CoverageTotals | None
    line_threshold: float
    function_threshold: float | None
    region_threshold: float | None
    failures: tuple[str, ...]
    error: str | None = None
    log_path: Path | None = None


@dataclasses.dataclass(frozen=True)
class CoverageCommand:
    """One named command in a package coverage job."""

    step: str
    command: list[str]


@dataclasses.dataclass(frozen=True)
class WorkspaceCoveragePlan:
    """Coverage commands for one selected workspace package group."""

    workspace: str
    packages: tuple[Package, ...]
    manifest_path: Path
    report_dir: Path
    target_dir: Path
    commands: tuple[CoverageCommand, ...]

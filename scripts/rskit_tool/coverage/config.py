"""Coverage configuration loading."""

from __future__ import annotations

import argparse
import dataclasses
import tomllib
from pathlib import Path

from ..errors import ToolError
from ..paths import COVERAGE_CONFIG


@dataclasses.dataclass(frozen=True)
class CoverageThresholdConfig:
    """Coverage threshold defaults loaded from repository config."""

    line: float
    function: float | None = None
    region: float | None = None


@dataclasses.dataclass(frozen=True)
class CoverageSecurityConfig:
    """Security-sensitive package coverage defaults."""

    packages: tuple[str, ...]
    line: float | None = None
    function: float | None = None
    region: float | None = None


@dataclasses.dataclass(frozen=True)
class CoveragePackageConfig:
    """Per-package coverage threshold overrides."""

    line: float | None = None
    function: float | None = None
    region: float | None = None


@dataclasses.dataclass(frozen=True)
class CoverageRunnerConfig:
    """Coverage runner defaults."""

    jobs: int | None = None
    clean: str = "profraw"
    exclude_packages: tuple[str, ...] = ()
    html: bool = False
    progress_interval_seconds: float = 10.0
    progress_style: str = "auto"
    progress_width: int = 32


@dataclasses.dataclass(frozen=True)
class CoverageConfig:
    """Repository coverage configuration."""

    thresholds: CoverageThresholdConfig
    security: CoverageSecurityConfig
    packages: dict[str, CoveragePackageConfig]
    runner: CoverageRunnerConfig


def config_path_from_args(args: argparse.Namespace) -> Path:
    """Return the coverage config path requested by CLI args."""

    raw_path = getattr(args, "config", None)
    return COVERAGE_CONFIG if raw_path is None else Path(raw_path)


def load_coverage_config(path: Path) -> CoverageConfig:
    """Load repository coverage configuration from TOML."""

    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise ToolError(f"coverage config not found: {path}") from error
    except tomllib.TOMLDecodeError as error:
        raise ToolError(f"coverage config is not valid TOML: {path}: {error}") from error
    if not isinstance(data, dict):
        raise ToolError(f"coverage config must be a TOML table: {path}")
    return CoverageConfig(
        thresholds=parse_thresholds(data.get("thresholds"), path),
        security=parse_security(data.get("security"), path),
        packages=parse_packages(data.get("packages"), path),
        runner=parse_runner(data.get("runner"), path),
    )


def apply_config_defaults(args: argparse.Namespace, config: CoverageConfig) -> argparse.Namespace:
    """Merge repository config defaults with explicit CLI arguments."""

    explicit_line_threshold = args.line_threshold is not None
    explicit_security_line_threshold = args.security_line_threshold is not None
    args.line_threshold = config.thresholds.line if args.line_threshold is None else args.line_threshold
    args.function_threshold = config.thresholds.function if args.function_threshold is None else args.function_threshold
    args.region_threshold = config.thresholds.region if args.region_threshold is None else args.region_threshold
    if args.security_line_threshold is None:
        args.security_line_threshold = args.line_threshold if explicit_line_threshold else security_line_threshold(config)
    args.security_function_threshold = config.security.function
    args.security_region_threshold = config.security.region
    args.security_packages = ",".join(config.security.packages) if args.security_packages is None else args.security_packages
    args.package_thresholds = package_thresholds(config, args.line_threshold if explicit_line_threshold else None)
    args.explicit_line_threshold = explicit_line_threshold
    args.explicit_security_line_threshold = explicit_security_line_threshold
    args.jobs = config.runner.jobs if args.jobs is None else args.jobs
    args.coverage_clean = config.runner.clean if args.coverage_clean is None else args.coverage_clean
    args.exclude_packages = (
        ",".join(config.runner.exclude_packages) if args.exclude_packages is None else args.exclude_packages
    )
    args.html = config.runner.html if args.html is None else args.html
    args.progress_interval = (
        config.runner.progress_interval_seconds if args.progress_interval is None else args.progress_interval
    )
    if args.progress_interval <= 0:
        raise ToolError("--progress-interval must be > 0")
    args.progress_style = config.runner.progress_style if args.progress_style is None else args.progress_style
    args.progress_width = config.runner.progress_width if args.progress_width is None else args.progress_width
    if args.progress_style not in {"auto", "line", "bar", "log"}:
        raise ToolError("--progress-style must be one of: auto, line, bar, log")
    if args.progress_width < 10:
        raise ToolError("--progress-width must be >= 10")
    return args


def security_line_threshold(config: CoverageConfig) -> float | None:
    """Return the configured security line threshold."""

    return config.security.line


def package_thresholds(
    config: CoverageConfig,
    explicit_line_threshold: float | None,
) -> dict[str, CoveragePackageConfig]:
    """Return package thresholds after applying an explicit global line override.

    An explicit global line threshold only ever *relaxes* a per-package override — it lowers a package's line gate but never raises it above the documented-achievable level the override declares. So a package pinned below the workspace default (e.g. live-broker code that cannot be unit-tested) stays at its override on the strict main-branch run, while the relaxed PR gate still lowers every package uniformly.
    """

    if explicit_line_threshold is None:
        return config.packages
    return {
        package: dataclasses.replace(
            override,
            line=explicit_line_threshold
            if override.line is None
            else min(override.line, explicit_line_threshold),
        )
        for package, override in config.packages.items()
    }


def parse_thresholds(value: object, path: Path) -> CoverageThresholdConfig:
    """Parse the [thresholds] coverage config table."""

    table = require_table(value, "thresholds", path)
    line = require_number(table, "line", path)
    return CoverageThresholdConfig(
        line=line,
        function=optional_number(table, "function", path, "thresholds"),
        region=optional_number(table, "region", path, "thresholds"),
    )


def parse_security(value: object, path: Path) -> CoverageSecurityConfig:
    """Parse the [security] coverage config table."""

    table = require_table(value, "security", path)
    packages = table.get("packages")
    if not isinstance(packages, list) or not all(isinstance(package, str) and package for package in packages):
        raise ToolError(f"coverage config {path}: security.packages must be a non-empty string array")
    return CoverageSecurityConfig(
        packages=tuple(sorted(set(packages))),
        line=optional_number(table, "line", path, "security"),
        function=optional_number(table, "function", path, "security"),
        region=optional_number(table, "region", path, "security"),
    )


def parse_packages(value: object, path: Path) -> dict[str, CoveragePackageConfig]:
    """Parse the [packages.<name>] coverage config tables."""

    if value is None:
        return {}
    table = require_table(value, "packages", path)
    package_configs: dict[str, CoveragePackageConfig] = {}
    for package, raw_config in table.items():
        if not isinstance(package, str) or not package:
            raise ToolError(f"coverage config {path}: package threshold names must be non-empty strings")
        package_table = require_table(raw_config, f"packages.{package}", path)
        package_config = CoveragePackageConfig(
            line=optional_number(package_table, "line", path, f"packages.{package}"),
            function=optional_number(package_table, "function", path, f"packages.{package}"),
            region=optional_number(package_table, "region", path, f"packages.{package}"),
        )
        if package_config.line is None and package_config.function is None and package_config.region is None:
            raise ToolError(f"coverage config {path}: packages.{package} must set at least one threshold")
        package_configs[package] = package_config
    return package_configs


def parse_runner(value: object, path: Path) -> CoverageRunnerConfig:
    """Parse the [runner] coverage config table."""

    table = {} if value is None else require_table(value, "runner", path)
    jobs = optional_int(table, "jobs", path)
    clean = table.get("clean", "profraw")
    if clean not in {"full", "profraw", "none"}:
        raise ToolError(f"coverage config {path}: runner.clean must be one of: full, profraw, none")
    exclude_packages = optional_string_list(table, "exclude_packages", path, "runner")
    html = table.get("html", False)
    if not isinstance(html, bool):
        raise ToolError(f"coverage config {path}: runner.html must be a boolean")
    progress_interval = optional_positive_number(table, "progress_interval_seconds", path, "runner")
    progress_style = table.get("progress_style", "auto")
    if progress_style not in {"auto", "line", "bar", "log"}:
        raise ToolError(f"coverage config {path}: runner.progress_style must be one of: auto, line, bar, log")
    progress_width = optional_int(table, "progress_width", path)
    if progress_width is not None and progress_width < 10:
        raise ToolError(f"coverage config {path}: runner.progress_width must be >= 10")
    return CoverageRunnerConfig(
        jobs=jobs,
        clean=clean,
        exclude_packages=exclude_packages,
        html=html,
        progress_interval_seconds=10.0 if progress_interval is None else progress_interval,
        progress_style=progress_style,
        progress_width=32 if progress_width is None else progress_width,
    )


def require_table(value: object, key: str, path: Path) -> dict[str, object]:
    """Return a required TOML table."""

    if not isinstance(value, dict):
        raise ToolError(f"coverage config {path}: [{key}] table is required")
    return value


def require_number(table: dict[str, object], key: str, path: Path) -> float:
    """Read a required numeric config field."""

    value = table.get(key)
    if isinstance(value, bool) or not isinstance(value, int | float):
        raise ToolError(f"coverage config {path}: thresholds.{key} must be a number")
    return float(value)


def optional_number(table: dict[str, object], key: str, path: Path, table_name: str) -> float | None:
    """Read an optional numeric config field."""

    value = table.get(key)
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, int | float):
        raise ToolError(f"coverage config {path}: {table_name}.{key} must be a number")
    return float(value)


def optional_int(table: dict[str, object], key: str, path: Path) -> int | None:
    """Read an optional positive integer config field."""

    value = table.get(key)
    if value is None:
        return None
    if not isinstance(value, int) or isinstance(value, bool) or value < 1:
        raise ToolError(f"coverage config {path}: runner.{key} must be an integer >= 1")
    return value


def optional_string_list(table: dict[str, object], key: str, path: Path, table_name: str) -> tuple[str, ...]:
    """Read an optional string list config field."""

    value = table.get(key)
    if value is None:
        return ()
    if not isinstance(value, list) or not all(isinstance(item, str) and item for item in value):
        raise ToolError(f"coverage config {path}: {table_name}.{key} must be a string array")
    return tuple(sorted(set(value)))


def optional_positive_number(table: dict[str, object], key: str, path: Path, table_name: str) -> float | None:
    """Read an optional positive numeric config field."""

    value = table.get(key)
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, int | float) or value <= 0:
        raise ToolError(f"coverage config {path}: {table_name}.{key} must be a number > 0")
    return float(value)

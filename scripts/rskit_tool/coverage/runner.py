"""Coverage execution orchestration."""

from __future__ import annotations

import argparse
import concurrent.futures
import os
import selectors
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path
from typing import TextIO

from ..cargo import Package, discover_packages
from ..errors import ToolError
from ..paths import COVERAGE_ROOT, COVERAGE_WORKSPACES_DIR, MODULES_DIR, ROOT
from ..process import normalize_jobs, printable
from .config import CoverageConfig, apply_config_defaults, config_path_from_args, load_coverage_config
from .events import CoverageEvent, CoverageEventBus, CoverageProgressReporter
from .models import ModuleResult, Thresholds, WorkspaceCoveragePlan
from .plan import coverage_step_count, workspace_coverage_plans
from .selection import discover_coverage_packages, select_packages
from .summary import parse_package_summaries, write_module_summary, write_summaries
from .thresholds import evaluate_thresholds, thresholds_from_args


def run_coverage(args: argparse.Namespace) -> int:
    """Run coverage according to parsed args."""

    config = load_coverage_config(config_path_from_args(args))
    validate_config_package_names(config)
    args = apply_config_defaults(args, config)
    packages = discover_coverage_packages(args)
    selected = select_packages(packages, args)
    if args.list:
        for package in selected:
            print(f"{package.name}\t{package.workspace}\t{package.root.relative_to(ROOT)}")
        return 0

    COVERAGE_ROOT.mkdir(parents=True, exist_ok=True)
    reset_output_dir(MODULES_DIR)
    reset_output_dir(COVERAGE_WORKSPACES_DIR)
    MODULES_DIR.mkdir(parents=True, exist_ok=True)
    COVERAGE_WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)

    thresholds = thresholds_from_args(args)
    if not selected:
        write_summaries([], thresholds, mode=args.mode)
        print("No packages selected for coverage.")
        return 0

    plans = workspace_coverage_plans(selected, args)
    jobs = 1 if args.jobs is None else normalize_jobs(args.jobs, len(plans))
    print(
        f"==> Running coverage for {len(selected)} package(s) across {len(plans)} workspace group(s) with {jobs} job(s)...",
        flush=True,
    )
    results: list[ModuleResult] = []
    event_bus = CoverageEventBus()
    process_registry = CoverageProcessRegistry()
    reporter = CoverageProgressReporter(
        total_packages=len(plans),
        steps_per_package=coverage_step_count(args),
        style=args.progress_style,
        bar_width=args.progress_width,
    )
    event_bus.subscribe(reporter.handle)
    executor = concurrent.futures.ThreadPoolExecutor(max_workers=jobs)
    interrupted = False
    try:
        futures = {
            executor.submit(run_workspace_coverage, plan, args, thresholds, event_bus, process_registry): plan
            for plan in plans
        }
        try:
            for future in concurrent.futures.as_completed(futures):
                plan = futures[future]
                try:
                    plan_results = future.result()
                except Exception as error:  # noqa: BLE001 - keep one package failure from aborting the report.
                    plan_results = [
                        failed_module_result(
                            package,
                            thresholds,
                            f"unexpected coverage error: {type(error).__name__}: {error}",
                            plan.report_dir / "command.log",
                        )
                        for package in plan.packages
                    ]
                    event_bus.emit(CoverageEvent("package_completed", package=workspace_event_package(plan)))
                results.extend(plan_results)
        except KeyboardInterrupt:
            interrupted = True
            process_registry.terminate_all()
            executor.shutdown(wait=False, cancel_futures=True)
            print("\ncoverage interrupted; active cargo subprocesses were terminated", file=sys.stderr)
            return 130
    finally:
        if not interrupted:
            executor.shutdown(wait=True, cancel_futures=True)

    results.sort(key=lambda item: (item.package.workspace, item.package.name))
    reporter.finish()
    write_summaries(results, thresholds, mode=args.mode)

    failed = [result for result in results if result.status in {"failed", "below-threshold"}]
    if failed:
        print(f"error: coverage failed for {len(failed)} package(s); see target/coverage/summary.md", file=sys.stderr)
        for result in failed:
            detail = result.error or ", ".join(result.failures)
            print(f"error: {result.package.name}: {detail}", file=sys.stderr)
        return 1

    print("✓ Coverage summary: target/coverage/summary.md")
    return 0


def run_workspace_coverage(
    plan: WorkspaceCoveragePlan,
    args: argparse.Namespace,
    thresholds: Thresholds,
    event_bus: CoverageEventBus,
    process_registry: "CoverageProcessRegistry",
) -> list[ModuleResult]:
    """Run llvm-cov once for one workspace package group."""

    event_package = workspace_event_package(plan)
    event_bus.emit(CoverageEvent("package_started", package=event_package))
    plan.report_dir.mkdir(parents=True, exist_ok=True)
    plan.target_dir.mkdir(parents=True, exist_ok=True)

    log_path = plan.report_dir / "command.log"
    env = {"CARGO_TARGET_DIR": str(plan.target_dir)}
    completed_steps = 0
    with log_path.open("w", encoding="utf-8") as log:
        for coverage_command in plan.commands:
            event_bus.emit(
                CoverageEvent(
                    "step_started",
                    package=event_package,
                    step=coverage_command.step,
                    package_completed_steps=completed_steps,
                )
            )
            log.write(f"$ {printable(coverage_command.command)}\n")
            log.flush()
            returncode = run_coverage_subprocess(
                coverage_command.command,
                env=env,
                log=log,
                package=event_package,
                step=coverage_command.step,
                package_completed_steps=completed_steps,
                progress_interval=args.progress_interval,
                event_bus=event_bus,
                process_registry=process_registry,
            )
            if returncode != 0:
                print_command_log_tail(log_path)
                event_bus.emit(
                    CoverageEvent(
                        "step_failed",
                        package=event_package,
                        step=coverage_command.step,
                        package_completed_steps=completed_steps,
                    )
                )
                event_bus.emit(CoverageEvent("package_completed", package=event_package))
                return [
                    failed_module_result(
                        package=package,
                        thresholds=thresholds,
                        error=f"command failed: {printable(coverage_command.command)} (log: {log_path.relative_to(ROOT)})",
                        log_path=log_path,
                    )
                    for package in plan.packages
                ]
            completed_steps += 1
            event_bus.emit(
                CoverageEvent(
                    "step_completed",
                    package=event_package,
                    step=coverage_command.step,
                    package_completed_steps=completed_steps,
                )
            )

    try:
        package_totals = parse_package_summaries(plan.report_dir / "summary.json", plan.packages)
    except Exception as error:
        event_bus.emit(CoverageEvent("package_completed", package=event_package))
        return [
            failed_module_result(
                package=package,
                thresholds=thresholds,
                error=f"{error} (log: {log_path.relative_to(ROOT)})",
                log_path=log_path,
            )
            for package in plan.packages
        ]

    results: list[ModuleResult] = []
    for package in plan.packages:
        totals = package_totals[package.name]
        failures = evaluate_thresholds(package.name, totals, thresholds)
        status = "n/a" if not totals.measured else ("below-threshold" if failures else "passed")
        module_dir = MODULES_DIR / safe_name(package.name)
        module_dir.mkdir(parents=True, exist_ok=True)
        write_module_summary(module_dir / "summary.txt", package, totals, failures, thresholds)
        results.append(
            ModuleResult(
                package=package,
                status=status,
                totals=totals,
                line_threshold=thresholds.line_threshold_for(package.name),
                function_threshold=thresholds.function_threshold_for(package.name),
                region_threshold=thresholds.region_threshold_for(package.name),
                failures=tuple(failures),
                log_path=log_path,
            )
        )
    event_bus.emit(CoverageEvent("package_completed", package=event_package))
    return results


def print_command_log_tail(log_path: Path, max_lines: int = 120) -> None:
    """Print a bounded command log tail so CI exposes the root failure."""

    try:
        lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError as error:
        print(
            f"error: failed to read coverage command log {log_path.relative_to(ROOT)}: {error}",
            file=sys.stderr,
        )
        return

    print(
        "error: coverage command failed; "
        f"last {min(max_lines, len(lines))} line(s) from {log_path.relative_to(ROOT)}:",
        file=sys.stderr,
    )
    for line in lines[-max_lines:]:
        print(line, file=sys.stderr)


def workspace_event_package(plan: WorkspaceCoveragePlan) -> Package:
    """Build a progress subject for one workspace coverage job."""

    return Package(
        name=f"{plan.workspace} workspace",
        workspace=plan.workspace,
        manifest_path=plan.manifest_path,
        root=plan.manifest_path.parent,
        version="",
        publishable=False,
    )


def failed_module_result(
    package: Package,
    thresholds: Thresholds,
    error: str,
    log_path: Path | None = None,
) -> ModuleResult:
    """Build a failed package coverage result."""

    return ModuleResult(
        package=package,
        status="failed",
        totals=None,
        line_threshold=thresholds.line_threshold_for(package.name),
        function_threshold=thresholds.function_threshold_for(package.name),
        region_threshold=thresholds.region_threshold_for(package.name),
        failures=(),
        error=error,
        log_path=log_path if log_path is not None and log_path.exists() else None,
    )


def safe_name(value: str) -> str:
    """Return filesystem-safe module name."""

    return "".join(character if character.isalnum() or character in "-_." else "_" for character in value)


def reset_output_dir(path: Path) -> None:
    """Remove stale generated coverage outputs."""

    if path.exists():
        shutil.rmtree(path)


def validate_config_package_names(config: CoverageConfig) -> None:
    """Reject coverage config package entries that do not match workspace packages."""

    known = {package.name for package in discover_packages()}
    configured = {*config.security.packages, *config.packages, *config.runner.exclude_packages}
    unknown = sorted(configured - known)
    if unknown:
        names = ", ".join(unknown)
        raise ToolError(f"coverage config references unknown package(s): {names}")


def run_coverage_subprocess(
    command: list[str],
    *,
    env: dict[str, str],
    log: TextIO,
    package: Package,
    step: str,
    package_completed_steps: int,
    progress_interval: float,
    event_bus: CoverageEventBus,
    process_registry: "CoverageProcessRegistry",
) -> int:
    """Run one coverage subprocess while emitting periodic heartbeat events."""

    try:
        process = subprocess.Popen(
            command,
            cwd=ROOT,
            env={**os.environ, **env},
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
    except OSError as error:
        raise ToolError(f"failed to execute command: {printable(command)}: {error}") from error

    if process.stdout is None:
        raise ToolError(f"failed to capture command output: {printable(command)}")

    process_registry.register(process)
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    started_at = time.monotonic()
    next_heartbeat = started_at + progress_interval
    last_output: str | None = None

    try:
        while True:
            timeout = max(0.0, next_heartbeat - time.monotonic())
            for key, _mask in selector.select(timeout):
                line = key.fileobj.readline()
                if line:
                    log.write(line)
                    log.flush()
                    stripped = line.strip()
                    if stripped:
                        last_output = stripped
                else:
                    selector.unregister(key.fileobj)

            now = time.monotonic()
            if process.poll() is not None:
                for line in process.stdout:
                    log.write(line)
                    log.flush()
                    stripped = line.strip()
                    if stripped:
                        last_output = stripped
                return process.wait()

            if now >= next_heartbeat:
                event_bus.emit(
                    CoverageEvent(
                        "step_heartbeat",
                        package=package,
                        step=step,
                        package_completed_steps=package_completed_steps,
                        elapsed_seconds=now - started_at,
                        last_output=last_output,
                    )
                )
                next_heartbeat = now + progress_interval
    finally:
        process_registry.unregister(process)
        selector.close()


class CoverageProcessRegistry:
    """Track active coverage subprocesses for cooperative interruption."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._processes: set[subprocess.Popen[str]] = set()

    def register(self, process: subprocess.Popen[str]) -> None:
        """Track an active subprocess."""

        with self._lock:
            self._processes.add(process)

    def unregister(self, process: subprocess.Popen[str]) -> None:
        """Stop tracking a subprocess."""

        with self._lock:
            self._processes.discard(process)

    def terminate_all(self) -> None:
        """Terminate all active subprocesses."""

        with self._lock:
            processes = tuple(self._processes)
        for process in processes:
            if process.poll() is None:
                process.terminate()

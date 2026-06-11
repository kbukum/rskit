"""Coverage progress events and reporting."""

from __future__ import annotations

import dataclasses
import shutil
import sys
import threading
import time
from collections.abc import Callable
from typing import Literal

from ..cargo import Package
from .formatting import format_bar, format_duration, format_package_result, format_percent, format_stage_progress
from .models import ModuleResult

CoverageEventKind = Literal[
    "package_started",
    "step_started",
    "step_completed",
    "step_failed",
    "step_heartbeat",
    "package_completed",
]


@dataclasses.dataclass(frozen=True)
class CoverageEvent:
    """Progress event emitted by package coverage workers."""

    kind: CoverageEventKind
    package: Package
    step: str | None = None
    package_completed_steps: int = 0
    elapsed_seconds: float = 0.0
    last_output: str | None = None
    result: ModuleResult | None = None


class CoverageEventBus:
    """Synchronous event bus shared by coverage workers and the reporter."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._subscribers: list[Callable[[CoverageEvent], None]] = []

    def subscribe(self, subscriber: Callable[[CoverageEvent], None]) -> None:
        """Register an event subscriber."""

        with self._lock:
            self._subscribers.append(subscriber)

    def emit(self, event: CoverageEvent) -> None:
        """Publish one event to all subscribers."""

        with self._lock:
            subscribers = tuple(self._subscribers)
        for subscriber in subscribers:
            subscriber(event)


@dataclasses.dataclass
class CoverageProgressReporter:
    """Render coverage worker events as progressive CLI output."""

    total_packages: int
    steps_per_package: int
    style: str = "auto"
    bar_width: int = 32
    completed_steps: int = 0
    completed_packages: int = 0
    active_packages: dict[str, str] = dataclasses.field(default_factory=dict)
    package_started_at: dict[str, float] = dataclasses.field(default_factory=dict)
    package_steps: dict[str, int] = dataclasses.field(default_factory=dict)
    package_last_output: dict[str, str] = dataclasses.field(default_factory=dict)
    package_step_elapsed: dict[str, float] = dataclasses.field(default_factory=dict)
    completed_package_seconds: float = 0.0
    started_at: float = dataclasses.field(default_factory=time.monotonic)
    rendered_lines: int = 0
    last_rendered: tuple[str, ...] = ()
    lock: threading.Lock = dataclasses.field(default_factory=threading.Lock)

    @property
    def total_steps(self) -> int:
        """Return the total number of package coverage stages in this run."""

        return self.total_packages * self.steps_per_package

    def handle(self, event: CoverageEvent) -> None:
        """Render one coverage progress event."""

        with self.lock:
            if event.kind == "package_completed":
                now = time.monotonic()
                self.completed_packages += 1
                self.active_packages.pop(event.package.name, None)
                started_at = self.package_started_at.pop(event.package.name, None)
                self.package_steps.pop(event.package.name, None)
                self.package_last_output.pop(event.package.name, None)
                self.package_step_elapsed.pop(event.package.name, None)
                if started_at is not None:
                    self.completed_package_seconds += now - started_at
                if event.result is not None and not self.interactive:
                    print(format_package_result(event.result, self.completed_packages, self.total_packages), flush=True)
                self.render()
                return

            if event.kind == "package_started":
                self.active_packages[event.package.name] = "starting"
                self.package_started_at[event.package.name] = time.monotonic()
                self.package_steps[event.package.name] = 0
            if event.kind in {"step_started", "step_completed", "step_failed", "step_heartbeat"} and event.step is not None:
                self.active_packages[event.package.name] = event.step
                self.package_steps[event.package.name] = event.package_completed_steps
                self.package_step_elapsed[event.package.name] = event.elapsed_seconds
                if event.last_output:
                    self.package_last_output[event.package.name] = event.last_output
            if event.kind == "step_completed":
                self.completed_steps += 1
                self.package_steps[event.package.name] = event.package_completed_steps

            if self.interactive:
                self.render()
            else:
                self.render_log_event(event)

    @property
    def interactive(self) -> bool:
        """Return true when progress should redraw in place."""

        return self.render_mode in {"line", "bar"}

    def finish(self) -> None:
        """Finish interactive rendering before ordinary output resumes."""

        with self.lock:
            if self.render_mode == "line" and self.last_rendered:
                sys.stdout.write("\n")
                sys.stdout.flush()

    def render(self) -> None:
        """Render interactive progress."""

        if not self.interactive:
            return
        if self.render_mode == "line":
            self.render_single_line()
            return
        self.render_dashboard()

    @property
    def render_mode(self) -> str:
        """Return the effective progress rendering mode."""

        if self.style == "auto":
            return "line" if sys.stdout.isatty() else "log"
        return self.style

    def render_single_line(self) -> None:
        """Render a portable single-line progress bar."""

        width, _height = terminal_size()
        line = fit_line(self.header_line(max(10, min(self.bar_width, width - 90))), max(40, width - 1))
        if line == (self.last_rendered[0] if self.last_rendered else ""):
            return
        sys.stdout.write("\r\x1b[2K" + line)
        self.rendered_lines = 0
        self.last_rendered = (line,)
        sys.stdout.flush()

    def render_dashboard(self) -> None:
        """Render the multi-line dashboard."""

        lines = self.dashboard_lines()
        if tuple(lines) == self.last_rendered:
            return
        if self.rendered_lines:
            sys.stdout.write(f"\x1b[{self.rendered_lines}A")
        for line in lines:
            sys.stdout.write("\r\x1b[2K" + line + "\n")
        if self.rendered_lines > len(lines):
            for _ in range(self.rendered_lines - len(lines)):
                sys.stdout.write("\r\x1b[2K\n")
        self.rendered_lines = len(lines)
        self.last_rendered = tuple(lines)
        sys.stdout.flush()

    def dashboard_lines(self) -> list[str]:
        """Return dashboard lines for the current state."""

        width, height = terminal_size()
        line_width = max(40, width - 1)
        bar_width = min(self.bar_width, max(10, line_width - 90))
        header = self.header_line(bar_width)
        lines = [fit_line(header, line_width)]
        if self.active_packages:
            lines.append("running:")
        max_active_rows = max(1, min(len(self.active_packages), max(1, height - 4), 12))
        active_items = list(self.active_packages.items())
        for package_name, step in active_items[:max_active_rows]:
            completed = self.package_steps.get(package_name, 0)
            elapsed = self.package_step_elapsed.get(package_name, 0.0)
            last_output = self.package_last_output.get(package_name)
            status = f"{step} {format_bar(completed, self.steps_per_package, width=12)} {format_percent(completed, self.steps_per_package)}"
            if elapsed:
                status = f"{status} for {format_duration(elapsed)}"
            if last_output:
                status = f"{status} | {last_output}"
            lines.append(fit_line(f"  {package_name:<32} {status}", line_width))
        hidden = len(active_items) - max_active_rows
        if hidden > 0:
            lines.append(fit_line(f"  ... {hidden} more running module(s)", line_width))
        return lines

    def header_line(self, bar_width: int) -> str:
        """Return the shared progress header line."""

        pending = max(0, self.total_packages - self.completed_packages - len(self.active_packages))
        return (
            f"coverage {format_bar(self.completed_steps, self.total_steps, width=bar_width)} "
            f"{format_percent(self.completed_steps, self.total_steps)} "
            f"steps {self.completed_steps}/{self.total_steps} | "
            f"jobs {self.completed_packages}/{self.total_packages} done, "
            f"{len(self.active_packages)} running, {pending} pending | "
            f"elapsed {format_duration(time.monotonic() - self.started_at)} | eta {self.eta()}"
        )

    def render_log_event(self, event: CoverageEvent) -> None:
        """Render a compact append-only event for non-interactive logs."""

        if event.kind not in {"step_completed", "step_failed", "step_heartbeat"}:
            return
        status = self._event_status(event)
        print(
            format_stage_progress(
                event.package.name,
                event.package.workspace,
                status,
                event.package_completed_steps,
                self.steps_per_package,
                self.completed_steps,
                self.total_steps,
                self.run_status(),
            ),
            flush=True,
        )

    def run_status(self) -> str:
        """Return package-level run status with elapsed time and ETA."""

        running = len(self.active_packages)
        pending = max(0, self.total_packages - self.completed_packages - running)
        elapsed = format_duration(time.monotonic() - self.started_at)
        eta = self.eta()
        return (
            f"jobs {self.completed_packages}/{self.total_packages} done, "
            f"{running} running, {pending} pending; elapsed {elapsed}; eta {eta}"
        )

    def eta(self) -> str:
        """Estimate remaining package time from completed package durations."""

        remaining = self.total_packages - self.completed_packages
        if self.completed_packages == 0 or remaining <= 0:
            return "calculating"
        average = self.completed_package_seconds / self.completed_packages
        return format_duration(average * remaining)

    def _event_status(self, event: CoverageEvent) -> str:
        if event.kind == "package_started":
            return "started"
        if event.kind == "step_started":
            return f"running {event.step}"
        if event.kind == "step_completed":
            return f"finished {event.step}"
        if event.kind == "step_failed":
            return f"failed {event.step}"
        if event.kind == "step_heartbeat":
            detail = f"still running {event.step} for {format_duration(event.elapsed_seconds)}"
            if event.last_output:
                detail = f"{detail}; last: {truncate_output(event.last_output)}"
            return detail
        return "completed"


def truncate_output(value: str, limit: int = 120) -> str:
    """Return a single-line, bounded output excerpt."""

    normalized = " ".join(value.split())
    if len(normalized) <= limit:
        return normalized
    return normalized[: limit - 1] + "…"


def terminal_size() -> tuple[int, int]:
    """Return terminal width and height with a safe fallback."""

    size = shutil.get_terminal_size(fallback=(120, 24))
    return size.columns, size.lines


def fit_line(value: str, width: int) -> str:
    """Fit one logical line into the terminal width to avoid wrapping."""

    normalized = " ".join(value.split())
    if len(normalized) <= width:
        return normalized
    if width <= 1:
        return ""
    return normalized[: width - 1] + "…"

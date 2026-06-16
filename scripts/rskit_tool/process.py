"""Subprocess helpers with explicit failures."""

from __future__ import annotations

import json
import os
import shlex
import shutil
import subprocess
import sys
from collections.abc import Callable, Mapping, Sequence
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path
from typing import Generic, TypeVar, cast

from .errors import ToolError
from .paths import ROOT

T = TypeVar("T")
_UNSET = object()


@dataclass(frozen=True)
class ParallelTask(Generic[T]):
    """A named unit of independent work for bounded parallel execution."""

    name: str
    action: Callable[[], T]


def printable(command: Sequence[str]) -> str:
    """Return a display form for a subprocess command."""

    return shlex.join(command)


def run(
    command: Sequence[str],
    *,
    cwd: Path = ROOT,
    env: Mapping[str, str] | None = None,
    capture: bool = False,
    stdin: str | None = None,
    check: bool = True,
    stdout=None,
    stderr=None,
) -> subprocess.CompletedProcess[str]:
    """Run a command and raise ToolError with command context on failure."""

    if capture and (stdout is not None or stderr is not None):
        raise ToolError("capture=True cannot be combined with stdout or stderr")

    try:
        completed = subprocess.run(
            list(command),
            cwd=cwd,
            env=None if env is None else {**os.environ, **dict(env)},
            input=stdin,
            text=True,
            capture_output=capture,
            stdout=stdout,
            stderr=stderr,
            check=False,
        )
    except OSError as error:
        raise ToolError(f"failed to execute command: {printable(command)}: {error}") from error
    if check and completed.returncode != 0:
        detail = completed.stderr or completed.stdout or ""
        suffix = f"\n{detail.strip()}" if detail.strip() else ""
        raise ToolError(f"command failed ({completed.returncode}): {printable(command)}{suffix}")
    return completed


def run_streamed(
    command: Sequence[str],
    *,
    cwd: Path = ROOT,
    env: Mapping[str, str] | None = None,
    sink: Callable[[str], None] | None = None,
) -> subprocess.CompletedProcess[str]:
    """Run a command, teeing combined output to ``sink`` while capturing it.

    Unlike ``run(capture=True)`` — which buffers everything and reveals it only
    after the process exits — this streams each line as the child emits it, so a
    long step (e.g. ``cargo publish`` compiling and uploading) shows live
    progress. stderr is merged into stdout to preserve ordering, and the full
    combined text is still returned for callers that need to inspect it.
    """

    emit = sink if sink is not None else _default_sink
    chunks: list[str] = []
    try:
        with subprocess.Popen(
            list(command),
            cwd=cwd,
            env=None if env is None else {**os.environ, **dict(env)},
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            bufsize=1,
        ) as process:
            assert process.stdout is not None  # PIPE is configured above.
            for line in process.stdout:
                chunks.append(line)
                emit(line)
            returncode = process.wait()
    except OSError as error:
        raise ToolError(f"failed to execute command: {printable(command)}: {error}") from error
    return subprocess.CompletedProcess(list(command), returncode, "".join(chunks), "")


def _default_sink(text: str) -> None:
    """Write streamed output straight through to stdout without extra newlines."""

    sys.stdout.write(text)
    sys.stdout.flush()


def run_json(command: Sequence[str], *, cwd: Path = ROOT) -> dict[str, object]:
    """Run a JSON-emitting command."""

    completed = run(command, cwd=cwd, capture=True)
    try:
        data = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise ToolError(f"command did not emit valid JSON: {printable(command)}: {error}") from error
    if not isinstance(data, dict):
        raise ToolError(f"command emitted non-object JSON: {printable(command)}")
    return data


def normalize_jobs(requested: int | None, task_count: int, *, cap: int = 8) -> int:
    """Normalize bounded parallelism for independent tasks."""

    if task_count <= 0:
        return 0
    jobs = requested if requested is not None else min(os.cpu_count() or 1, cap)
    if jobs < 1:
        raise ToolError("--jobs must be >= 1")
    return min(jobs, task_count)


def run_parallel(tasks: Sequence[ParallelTask[T]], *, jobs: int | None = None) -> list[T]:
    """Run independent tasks with bounded parallelism and deterministic results."""

    if not tasks:
        return []
    worker_count = normalize_jobs(jobs, len(tasks))
    results: list[object] = [_UNSET] * len(tasks)
    failures: list[str] = []

    with ThreadPoolExecutor(max_workers=worker_count) as executor:
        futures = {executor.submit(task.action): (index, task) for index, task in enumerate(tasks)}
        for future in as_completed(futures):
            index, task = futures[future]
            try:
                results[index] = future.result()
            except Exception as error:  # noqa: BLE001 - preserve task name while aggregating all failures.
                failures.append(f"{task.name}: {error}")

    if failures:
        raise ToolError("parallel task failure(s):\n" + "\n".join(f"  - {failure}" for failure in failures))

    return [cast(T, result) for result in results]


def command_exists(name: str) -> bool:
    """Return true when an executable exists on PATH."""

    return shutil.which(name) is not None


def notice(message: str) -> None:
    """Emit a GitHub Actions notice when available, otherwise plain text."""

    if os.environ.get("GITHUB_ACTIONS") == "true":
        print(f"::notice title=rskit tooling::{message}")
    else:
        print(f"notice: {message}")


def fail(message: str, code: int = 1) -> int:
    """Print an error and return an exit code."""

    print(message, file=sys.stderr)
    return code

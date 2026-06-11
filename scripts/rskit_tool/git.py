"""Git helpers."""

from __future__ import annotations

from pathlib import Path

from .errors import ToolError
from .paths import ROOT
from .process import run


def changed_paths(base: str = "origin/main...HEAD") -> list[Path]:
    """Return paths from git diff against base plus untracked paths."""

    changed: set[Path] = set()
    completed = run(["git", "diff", "--name-only", base], cwd=ROOT, capture=True, check=False)
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        suffix = f": {detail}" if detail else ""
        raise ToolError(f"failed to detect changed paths for base '{base}'{suffix}")
    changed.update(Path(line) for line in completed.stdout.splitlines() if line)

    untracked = run(
        ["git", "ls-files", "--others", "--exclude-standard"],
        cwd=ROOT,
        capture=True,
        check=False,
    )
    if untracked.returncode == 0:
        changed.update(Path(line) for line in untracked.stdout.splitlines() if line)
    return sorted(changed)

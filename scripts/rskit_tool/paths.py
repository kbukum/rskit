"""Shared repository paths."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKSPACES = {
    "core": ROOT / "core" / "Cargo.toml",
    "contrib": ROOT / "contrib" / "Cargo.toml",
    "examples": ROOT / "examples" / "Cargo.toml",
}
CORE_AND_CONTRIB = {
    "core": WORKSPACES["core"],
    "contrib": WORKSPACES["contrib"],
}

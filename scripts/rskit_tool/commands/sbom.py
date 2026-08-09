"""CycloneDX SBOM generation.

rskit validates and ships the ``--all-features`` surface, so its SBOMs must
enumerate feature-gated optional dependencies. Toven's default SBOM step invokes
``cargo-cyclonedx`` without ``--all-features``, so this all-features-capable
wrapper stays owned by rskit tooling; Toven orchestrates it as a ``command``
task.
"""

from __future__ import annotations

import argparse
import shutil
from pathlib import Path

from ..cargo import metadata
from ..errors import ToolError
from ..paths import CORE_AND_CONTRIB, ROOT
from ..process import run


def add_parser(subparsers: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    """Register the SBOM command."""

    parser = subparsers.add_parser("sbom", help="Generate CycloneDX SBOMs (all features)")
    parser.add_argument("--out-dir", default="target/sbom")
    parser.set_defaults(func=run_sbom)


def validate_target_subdir(value: str) -> Path:
    """Validate a repo-relative output directory under ``target/``."""

    path = Path(value)
    target_root = (ROOT / "target").resolve()
    if value == "" or path.is_absolute():
        raise ToolError(
            f"error: output directory must be a repo-relative target subdirectory: {value}"
        )
    resolved = (ROOT / path).resolve()
    try:
        resolved.relative_to(target_root)
    except ValueError as error:
        raise ToolError(f"error: output directory must resolve under target/: {value}") from error
    if resolved == target_root:
        raise ToolError(f"error: output directory must be a non-empty target subdirectory: {value}")
    return resolved


def run_sbom(args: argparse.Namespace) -> int:
    """Generate per-crate CycloneDX SBOMs across the all-features surface."""

    out_dir = validate_target_subdir(args.out_dir)
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)
    print("==> Generating CycloneDX SBOMs (all features)...")
    run(["cargo", "cyclonedx", "--manifest-path", "core/Cargo.toml", "--format", "json", "--all-features"])
    run(["cargo", "cyclonedx", "--manifest-path", "contrib/Cargo.toml", "--format", "json", "--all-features"])

    package_by_dir: dict[Path, str] = {}
    for manifest in CORE_AND_CONTRIB.values():
        data = metadata(manifest)
        members = set(data["workspace_members"])  # type: ignore[index]
        for package in data["packages"]:  # type: ignore[index]
            if package["id"] in members:
                package_by_dir[Path(package["manifest_path"]).resolve().parent] = package["name"]

    moved = 0
    for file in sorted(ROOT.glob("core/**/*.cdx.json")) + sorted(ROOT.glob("contrib/**/*.cdx.json")):
        crate = package_by_dir.get(file.parent)
        if crate is not None:
            shutil.move(str(file), out_dir / f"{crate}.cdx.json")
            moved += 1
    if moved == 0:
        raise ToolError("error: cargo cyclonedx did not produce any workspace SBOM files")
    print(f"✓ SBOMs written to {out_dir}")
    return 0

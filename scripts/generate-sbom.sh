#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT_DIR_INPUT="${1:-target/sbom}"
case "$OUT_DIR_INPUT" in
    ""|"/"|.|/*|*"/../"*|../*|*/..)
        echo "error: output directory must be a non-empty repo-relative path without '..': ${OUT_DIR_INPUT}" >&2
        exit 2
        ;;
esac
OUT_DIR="$ROOT/$OUT_DIR_INPUT"
rm -rf -- "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "==> Generating CycloneDX SBOMs..."
cargo cyclonedx --manifest-path core/Cargo.toml --format json --all-features
cargo cyclonedx --manifest-path contrib/Cargo.toml --format json --all-features

python3 - "$ROOT" "$OUT_DIR" <<'PY'
import json
import pathlib
import shutil
import subprocess
import sys

root = pathlib.Path(sys.argv[1]).resolve()
out_dir = pathlib.Path(sys.argv[2]).resolve()
manifests = [root / "core" / "Cargo.toml", root / "contrib" / "Cargo.toml"]

package_by_dir: dict[pathlib.Path, str] = {}
for manifest in manifests:
    data = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--manifest-path", str(manifest), "--no-deps", "--format-version=1"],
            text=True,
        )
    )
    members = set(data["workspace_members"])
    for package in data["packages"]:
        if package["id"] in members:
            package_by_dir[pathlib.Path(package["manifest_path"]).resolve().parent] = package["name"]

sbom_files = sorted(root.glob("core/**/*.cdx.json")) + sorted(root.glob("contrib/**/*.cdx.json"))
moved = 0
for file in sbom_files:
    crate = package_by_dir.get(file.parent)
    if crate is None:
        continue
    shutil.move(str(file), out_dir / f"{crate}.cdx.json")
    moved += 1

if moved == 0:
    print("error: cargo cyclonedx did not produce any workspace SBOM files", file=sys.stderr)
    sys.exit(1)
PY

echo "✓ SBOMs written to ${OUT_DIR}"

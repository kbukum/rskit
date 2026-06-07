#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT_DIR="${1:-target/sbom}"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "==> Generating CycloneDX SBOMs..."
cargo cyclonedx --manifest-path core/Cargo.toml --format json --all-features
cargo cyclonedx --manifest-path contrib/Cargo.toml --format json --all-features

find core contrib -mindepth 2 -maxdepth 4 -name '*.cdx.json' -print0 |
    while IFS= read -r -d '' file; do
        crate="$(basename "$(dirname "$file")")"
        mv "$file" "$OUT_DIR/${crate}.cdx.json"
    done

echo "✓ SBOMs written to ${OUT_DIR}"

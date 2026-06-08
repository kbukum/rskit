#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="${1:-workspace}"
OVERALL_THRESHOLD="${OVERALL_COVERAGE_THRESHOLD:-85}"
PACKAGE_THRESHOLD="${PACKAGE_COVERAGE_THRESHOLD:-80}"
SECURITY_THRESHOLD="${SECURITY_COVERAGE_THRESHOLD:-85}"
SECURITY_PACKAGES=" rskit-errors rskit-auth rskit-authz rskit-security rskit-resilience rskit-encryption "

mkdir -p target/coverage

echo "==> Checking workspace coverage (>=${OVERALL_THRESHOLD}%)..."
cargo llvm-cov --manifest-path core/Cargo.toml --workspace \
    --all-features \
    --fail-under-lines "$OVERALL_THRESHOLD" \
    --lcov --output-path target/coverage/core.lcov
cargo llvm-cov --manifest-path contrib/Cargo.toml --workspace \
    --all-features \
    --fail-under-lines "$OVERALL_THRESHOLD" \
    --lcov --output-path target/coverage/contrib.lcov

if [ "$MODE" != "release" ]; then
    echo "✓ Workspace coverage thresholds passed"
    exit 0
fi

echo "==> Checking per-crate coverage..."
while IFS= read -r package; do
    [ -n "$package" ] || continue
    threshold="$PACKAGE_THRESHOLD"
    case "$SECURITY_PACKAGES" in
        *" $package "*) threshold="$SECURITY_THRESHOLD" ;;
    esac

    echo "==> ${package} coverage (>=${threshold}%)..."
    core_stderr="$(mktemp "${TMPDIR:-/tmp}/rskit-core-coverage.XXXXXX")"
    if cargo llvm-cov --manifest-path core/Cargo.toml -p "$package" \
        --all-features \
        --fail-under-lines "$threshold" \
        --lcov --output-path "target/coverage/${package}.lcov" 2>"$core_stderr"; then
        rm -f "$core_stderr"
        continue
    fi
    contrib_stderr="$(mktemp "${TMPDIR:-/tmp}/rskit-contrib-coverage.XXXXXX")"
    if cargo llvm-cov --manifest-path contrib/Cargo.toml -p "$package" \
        --all-features \
        --fail-under-lines "$threshold" \
        --lcov --output-path "target/coverage/${package}.lcov" 2>"$contrib_stderr"; then
        rm -f "$core_stderr" "$contrib_stderr"
        continue
    fi
    echo "error: coverage failed for ${package}; core attempt stderr:" >&2
    cat "$core_stderr" >&2
    echo "error: coverage failed for ${package}; contrib attempt stderr:" >&2
    cat "$contrib_stderr" >&2
    rm -f "$core_stderr" "$contrib_stderr"
    exit 1
done < <(
    python3 - "$ROOT" <<'PY'
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1]).resolve()
seen: set[str] = set()
for manifest in (root / "core" / "Cargo.toml", root / "contrib" / "Cargo.toml"):
    data = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--format-version=1", "--manifest-path", str(manifest)],
            text=True,
        )
    )
    for package in data["packages"]:
        manifest_path = pathlib.Path(package["manifest_path"]).resolve()
        try:
            manifest_path.relative_to(root / "core")
        except ValueError:
            try:
                manifest_path.relative_to(root / "contrib")
            except ValueError:
                continue
        name = package["name"]
        if name not in seen:
            seen.add(name)
            print(name)
PY
)

echo "✓ Release coverage thresholds passed"

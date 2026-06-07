#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> Checking release guardrails..."
./scripts/check-topology.sh
./scripts/check-l7-edges.sh
./scripts/check-public-api.sh

echo "==> Checking GitHub Actions are SHA-pinned..."
if git grep -n -E 'uses: [^[:space:]#@]+([[:space:]]*$|@(v[0-9]|main|master|stable)([[:space:]#]|$))' -- .github/workflows; then
    echo "error: unpinned GitHub Actions references found" >&2
    exit 1
fi

echo "==> Sweeping library runtime panic and dynamic public API hazards..."
python3 <<'PY'
import pathlib
import re
import sys

root = pathlib.Path.cwd()
hazard_re = re.compile(r"\b(?:unwrap|expect)\s*\(|panic!\s*\(")
findings: list[str] = []

for source_root in (root / "core", root / "contrib"):
    for path in sorted(source_root.glob("**/src/**/*.rs")):
        relative = path.relative_to(root)
        if (
            relative.parts[:2] == ("core", "rskit-testutil")
            or path.name in {"tests.rs", "fixture_tests.rs"}
        ):
            continue
        brace_depth = 0
        pending_cfg_test = False
        pending_test_attr = False
        pending_helper_fn = False
        test_block_depth: int | None = None
        test_fn_depth: int | None = None
        helper_fn_depth: int | None = None

        for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            stripped = line.strip()
            in_test_scope = (
                (test_block_depth is not None and brace_depth >= test_block_depth)
                or (test_fn_depth is not None and brace_depth >= test_fn_depth)
                or (helper_fn_depth is not None and brace_depth >= helper_fn_depth)
            )

            if stripped.startswith("#[cfg(test)]"):
                pending_cfg_test = True
            elif pending_cfg_test and re.search(r"\bmod\b", stripped):
                in_test_scope = True
                pending_cfg_test = False
                if "{" in stripped:
                    test_block_depth = brace_depth + stripped.count("{")
            elif stripped.startswith("#[test") or stripped.startswith("#[tokio::test"):
                pending_test_attr = True
            elif pending_test_attr and re.search(r"\bfn\b", stripped):
                in_test_scope = True
                pending_test_attr = False
                if "{" in stripped:
                    test_fn_depth = brace_depth + stripped.count("{")
            elif pending_helper_fn:
                in_test_scope = True
                if "{" in stripped:
                    helper_fn_depth = brace_depth + stripped.count("{")
                    pending_helper_fn = False
            elif re.search(r"\bpub\s+(async\s+)?fn\s+(assert_|wait_for_message\b)", stripped):
                in_test_scope = True
                if "{" in stripped:
                    helper_fn_depth = brace_depth + stripped.count("{")
                else:
                    pending_helper_fn = True
            elif stripped and not stripped.startswith("#"):
                pending_cfg_test = False

            if (
                not in_test_scope
                and not stripped.startswith("#")
                and not stripped.startswith("///")
                and not stripped.startswith("//!")
                and hazard_re.search(line)
            ):
                findings.append(f"{relative}:{line_no}:{line}")

            brace_depth += line.count("{") - line.count("}")
            if test_block_depth is not None and brace_depth < test_block_depth:
                test_block_depth = None
            if test_fn_depth is not None and brace_depth < test_fn_depth:
                test_fn_depth = None
            if helper_fn_depth is not None and brace_depth < helper_fn_depth:
                helper_fn_depth = None

if findings:
    print("\n".join(findings), file=sys.stderr)
    print("error: release-blocking runtime panic pattern found", file=sys.stderr)
    sys.exit(1)
PY

echo "==> Checking unsafe policy..."
python3 <<'PY'
import pathlib
import re
import sys

root = pathlib.Path.cwd()
unsafe_re = re.compile(r"(^|[^A-Za-z0-9_])unsafe\s*(\{|fn\b|impl\b|trait\b)")
findings: list[str] = []

for source_root in (root / "core", root / "contrib"):
    for path in sorted(source_root.glob("**/src/**/*.rs")):
        relative = path.relative_to(root)
        if path.name in {"tests.rs", "fixture_tests.rs"}:
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        for index, line in enumerate(lines):
            stripped = line.strip()
            if stripped.startswith("///") or stripped.startswith("//!") or stripped.startswith("#"):
                continue
            if not unsafe_re.search(line):
                continue
            window = lines[max(0, index - 4) : index + 1]
            if not any("SAFETY:" in candidate for candidate in window):
                findings.append(f"{relative}:{index + 1}:{line}")

if findings:
    print("\n".join(findings), file=sys.stderr)
    print("error: unsafe block/function missing nearby // SAFETY: justification", file=sys.stderr)
    sys.exit(1)
PY

echo "==> Running cargo-deny..."
cargo deny --manifest-path core/Cargo.toml check licenses advisories sources bans
cargo deny --manifest-path contrib/Cargo.toml check licenses advisories sources bans

echo "==> Running cargo-audit..."
audit_ignore=(
    --ignore RUSTSEC-2023-0071
    --ignore RUSTSEC-2024-0436
    --ignore RUSTSEC-2025-0134
    --ignore RUSTSEC-2026-0097
    --ignore RUSTSEC-2026-0098
    --ignore RUSTSEC-2026-0099
    --ignore RUSTSEC-2026-0104
    --ignore RUSTSEC-2026-0173
)
cargo audit --file core/Cargo.lock "${audit_ignore[@]}"
cargo audit --file contrib/Cargo.lock "${audit_ignore[@]}"

echo "==> Checking release fuzz targets exist..."
for target in \
    auth_jwt_decode \
    encryption_envelope \
    errors_problem_detail \
    http_request_parse \
    jwt_parser \
    schema_validation \
    util_parsers \
    validation_inputs
do
    if [ ! -f "fuzz/fuzz_targets/${target}.rs" ]; then
        echo "error: missing fuzz target fuzz/fuzz_targets/${target}.rs" >&2
        exit 1
    fi
done

echo "✓ Release readiness sweep passed"

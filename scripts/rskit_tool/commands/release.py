"""Release orchestration commands."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import sys
import urllib.error
import urllib.request
from collections import defaultdict
from pathlib import Path
from types import SimpleNamespace

from ..cargo import is_relative_to, metadata
from ..errors import ToolError
from ..paths import CORE_AND_CONTRIB, ROOT, WORKSPACES
from ..process import command_exists, notice, run
from .checks import run_l7_edges, run_public_api, run_topology, run_workspace_deps_sync


def add_parser(subparsers: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    """Register release commands."""

    parser = subparsers.add_parser("release", help="Release orchestration")
    release_sub = parser.add_subparsers(dest="release_command", required=True)

    release_sub.add_parser("readiness", help="Run release-readiness guardrails").set_defaults(func=run_readiness)

    depgraphs = release_sub.add_parser("depgraphs", help="Generate dependency graph SVGs")
    depgraphs.add_argument("out_dir", nargs="?", default="depgraphs")
    depgraphs.set_defaults(func=run_depgraphs)

    sbom = release_sub.add_parser("sbom", help="Generate CycloneDX SBOMs")
    sbom.add_argument("out_dir", nargs="?", default="target/sbom")
    sbom.set_defaults(func=run_sbom)

    publish = release_sub.add_parser("publish-dry-run", help="Publish dry-run orchestration")
    publish.add_argument("--list", dest="mode", action="store_const", const="--list")
    publish.add_argument("--dry-run", dest="mode", action="store_const", const="--dry-run")
    publish.add_argument("--publish", dest="mode", action="store_const", const="--publish")
    publish.set_defaults(mode="--dry-run")
    publish.set_defaults(func=run_publish)


def run_readiness(args: argparse.Namespace) -> int:
    """Run release-readiness guardrails."""

    print("==> Checking release guardrails...")
    for check_args, check in (
        (args, run_topology),
        (args, run_l7_edges),
        (SimpleNamespace(package="rskit-suite"), run_public_api),
    ):
        status = check(check_args)
        if status != 0:
            return status

    print("==> Checking GitHub Actions are SHA-pinned...")
    completed = run(
        [
            "git",
            "grep",
            "-n",
            "-E",
            r"uses: [^[:space:]#@]+([[:space:]]*$|@((v[0-9][^[:space:]#]*)|main|master|stable)([[:space:]#]|$))",
            "--",
            ".github/workflows",
        ],
        capture=True,
        check=False,
    )
    if completed.returncode == 0:
        print(completed.stdout, end="")
        print("error: unpinned GitHub Actions references found", file=sys.stderr)
        return 1
    if completed.returncode not in (0, 1):
        print(completed.stderr, file=sys.stderr, end="")
        return completed.returncode

    for label, check in (
        ("Sweeping library runtime panic and dynamic public API hazards", check_runtime_panic_hazards),
        ("Checking unsafe policy", check_unsafe_policy),
    ):
        print(f"==> {label}...")
        findings = check()
        if findings:
            print("\n".join(findings), file=sys.stderr)
            print(f"error: release-blocking {label.lower()} found", file=sys.stderr)
            return 1

    print("==> Running cargo-deny...")
    if run_workspace_deps_sync(args) != 0:
        return 1
    run(["cargo", "deny", "--manifest-path", "core/Cargo.toml", "check", "--config", "deny.toml", "licenses", "advisories", "sources", "bans"])
    run(["cargo", "deny", "--manifest-path", "contrib/Cargo.toml", "check", "--config", "deny.contrib.toml", "licenses", "advisories", "sources", "bans"])
    run(["cargo", "deny", "--manifest-path", "examples/Cargo.toml", "check", "--config", "deny.examples.toml", "licenses", "advisories", "sources", "bans"])

    print("==> Running cargo-audit...")
    run(["cargo", "audit", "--deny", "warnings", "--file", "core/Cargo.lock"])
    run(["cargo", "audit", "--deny", "warnings", "--file", "contrib/Cargo.lock"])
    run(["cargo", "audit", "--deny", "warnings", "--file", "examples/Cargo.lock"])

    print("==> Checking release fuzz targets exist...")
    missing = [
        target
        for target in (
            "auth_jwt_decode",
            "encryption_envelope",
            "errors_problem_detail",
            "http_request_parse",
            "jwt_parser",
            "schema_validation",
            "util_parsers",
            "validation_inputs",
        )
        if not (ROOT / "fuzz" / "fuzz_targets" / f"{target}.rs").exists()
    ]
    if missing:
        for target in missing:
            print(f"error: missing fuzz target fuzz/fuzz_targets/{target}.rs", file=sys.stderr)
        return 1
    print("✓ Release readiness sweep passed")
    return 0


def check_runtime_panic_hazards() -> list[str]:
    """Find runtime unwrap/expect/panic hazards outside test scopes."""

    hazard_re = re.compile(r"\b(?:unwrap|expect)\s*\(|panic!\s*\(")
    findings: list[str] = []
    for source_root in (ROOT / "core", ROOT / "contrib"):
        for path in sorted(source_root.glob("**/src/**/*.rs")):
            relative = path.relative_to(ROOT)
            if relative.parts[:2] == ("core", "rskit-testutil") or path.name in {"tests.rs", "fixture_tests.rs"}:
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
                if re.match(r"#\[cfg\((?:.*\b)?test\b.*\)\]", stripped):
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

                if not in_test_scope and not stripped.startswith("#") and not stripped.startswith("//") and hazard_re.search(line):
                    findings.append(f"{relative}:{line_no}:{line}")
                brace_depth += line.count("{") - line.count("}")
                if test_block_depth is not None and brace_depth < test_block_depth:
                    test_block_depth = None
                if test_fn_depth is not None and brace_depth < test_fn_depth:
                    test_fn_depth = None
                if helper_fn_depth is not None and brace_depth < helper_fn_depth:
                    helper_fn_depth = None
    return findings


def check_unsafe_policy() -> list[str]:
    """Find unsafe blocks/functions without nearby SAFETY comment."""

    unsafe_re = re.compile(r"(^|[^A-Za-z0-9_])unsafe\s*(\{|fn\b|impl\b|trait\b)")
    findings: list[str] = []
    for source_root in (ROOT / "core", ROOT / "contrib"):
        for path in sorted(source_root.glob("**/src/**/*.rs")):
            relative = path.relative_to(ROOT)
            if path.name in {"tests.rs", "fixture_tests.rs"}:
                continue
            lines = path.read_text(encoding="utf-8").splitlines()
            for index, line in enumerate(lines):
                stripped = line.strip()
                if stripped.startswith("//") or stripped.startswith("#") or not unsafe_re.search(line):
                    continue
                window = lines[max(0, index - 4) : index + 1]
                if not any("SAFETY:" in candidate for candidate in window):
                    findings.append(f"{relative}:{index + 1}:{line}")
    return findings


def run_depgraphs(args: argparse.Namespace) -> int:
    """Generate dependency graph SVGs."""

    if not command_exists("cargo"):
        raise ToolError("error: cargo is not installed or not in PATH")
    if not command_exists("dot"):
        raise ToolError("error: dot (Graphviz) is not installed or not in PATH")
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    print(f"Generating dependency graphs into: {out_dir}")
    graph_specs = [
        ("graph-all", WORKSPACES["core"], ["--workspace-only", "--dedup-transitive-deps"]),
        ("graph-contrib", WORKSPACES["contrib"], ["--workspace-only", "--dedup-transitive-deps"]),
        ("graph-agent", WORKSPACES["core"], ["--workspace-only", "--focus", "rskit-agent", "--depth", "2", "--dedup-transitive-deps"]),
        ("graph-errors", WORKSPACES["core"], ["--workspace-only", "--focus", "rskit-errors", "--depth", "2", "--dedup-transitive-deps"]),
        ("graph-resilience", WORKSPACES["core"], ["--workspace-only", "--focus", "rskit-resilience", "--depth", "2", "--dedup-transitive-deps"]),
    ]
    for name, manifest, extra in graph_specs:
        out_file = out_dir / f"{name}.svg"
        print(f"-> generating {out_file}")
        depgraph = run(["cargo", "depgraph", "--manifest-path", str(manifest), *extra], capture=True)
        with out_file.open("w", encoding="utf-8") as output:
            run(["dot", "-Grankdir=LR", "-Nshape=box", "-Tsvg"], stdin=depgraph.stdout, stdout=output)
        print(f"   done: {out_file}")
    print("All graphs generated successfully.")
    return 0


def validate_target_subdir(value: str) -> Path:
    """Validate target-relative output directory."""

    path = Path(value)
    target_root = (ROOT / "target").resolve()
    if value == "" or path.is_absolute():
        raise ToolError(f"error: output directory must be a repo-relative target subdirectory: {value}")
    resolved = (ROOT / path).resolve()
    try:
        resolved.relative_to(target_root)
    except ValueError as error:
        raise ToolError(f"error: output directory must resolve under target/: {value}") from error
    if resolved == target_root:
        raise ToolError(f"error: output directory must be a non-empty target subdirectory: {value}")
    return resolved


def run_sbom(args: argparse.Namespace) -> int:
    """Generate CycloneDX SBOMs."""

    out_dir = validate_target_subdir(args.out_dir)
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)
    print("==> Generating CycloneDX SBOMs...")
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


def run_publish(args: argparse.Namespace) -> int:
    """Run publish order/list/dry-run/publish."""

    mode = args.mode
    dirty_args = [os.environ["CARGO_PACKAGE_DIRTY_FLAG"]] if os.environ.get("CARGO_PACKAGE_DIRTY_FLAG") else []
    if mode == "--list":
        cargo_args: list[str] = []
    elif mode == "--dry-run":
        cargo_args = ["publish", "--dry-run", "--locked", *dirty_args]
    else:
        cargo_args = ["publish", "--locked"]

    print("==> Resolving publish order...")
    skipped = 0
    published_cache: dict[tuple[str, str], bool] = {}
    for package in publish_order():
        if mode == "--list":
            print(f"{package['name']}\t{package['manifest']}")
            continue
        if mode == "--dry-run" and package["internal_deps"]:
            blocked: list[str] = []
            for dep_entry in package["internal_deps"].split(","):
                if not dep_entry:
                    continue
                dep_name, dep_version = dep_entry.rsplit("@", 1)
                status = cached_crate_version_published(published_cache, dep_name, dep_version)
                if status is False:
                    blocked.append(dep_entry)
            if blocked:
                skipped += 1
                joined = ",".join(blocked)
                notice(
                    f"{package['name']} {package['version']} depends on unpublished internal crate(s): {joined}. "
                    "cargo publish --dry-run cannot fully validate this crate until those same-version dependencies exist on crates.io; "
                    "running package-list sanity check instead."
                )
                run(["cargo", "package", "--locked", "--list", *dirty_args, "--manifest-path", package["manifest"]])
                continue
        print(f"==> cargo {' '.join(cargo_args)} {package['name']}")
        run(["cargo", *cargo_args, "--manifest-path", package["manifest"]])
    if mode == "--dry-run" and skipped > 0:
        print(
            f"warning: {skipped} crate(s) were package-listed but not cargo publish --dry-run validated because their same-version internal dependencies are not on crates.io yet."
        )
    print(f"✓ Cargo publish {mode.removeprefix('--')} completed")
    return 0


def cached_crate_version_published(cache: dict[tuple[str, str], bool], crate: str, version: str) -> bool:
    """Return cached crates.io publication status for a crate version."""

    key = (crate, version)
    if key not in cache:
        cache[key] = crate_version_published(crate, version)
    return cache[key]


def publish_order() -> list[dict[str, str]]:
    """Resolve publishable packages in dependency order."""

    packages: dict[str, dict[str, str]] = {}
    edges: dict[str, set[str]] = defaultdict(set)
    documents: list[dict] = []
    for workspace_manifest in CORE_AND_CONTRIB.values():
        data = metadata(workspace_manifest, all_features=True, no_deps=False)
        documents.append(data)
        workspace_ids = set(data["workspace_members"])  # type: ignore[index]
        for package in data["packages"]:  # type: ignore[index]
            manifest_path = Path(package["manifest_path"]).resolve()
            if package["id"] not in workspace_ids or package.get("publish") == []:
                continue
            if not (is_relative_to(manifest_path, ROOT / "core") or is_relative_to(manifest_path, ROOT / "contrib")):
                continue
            packages[package["id"]] = {
                "name": package["name"],
                "manifest": str(manifest_path),
                "version": package["version"],
            }

    for data in documents:
        package_by_id = {package["id"]: package for package in data["packages"]}  # type: ignore[index]
        name_to_id = {package["name"]: package["id"] for package in data["packages"] if package["id"] in packages}  # type: ignore[index]
        for package_id, package in package_by_id.items():
            if package_id not in packages:
                continue
            for dep in package["dependencies"]:
                if dep["kind"] not in (None, "build"):
                    continue
                dep_id = name_to_id.get(dep["name"])
                if dep_id in packages:
                    edges[package_id].add(dep_id)
        for node in data["resolve"]["nodes"]:  # type: ignore[index]
            if node["id"] not in packages:
                continue
            for dep in node["deps"]:
                if not any(kind["kind"] in (None, "build") for kind in dep.get("dep_kinds", [])):
                    continue
                if dep["pkg"] in packages:
                    edges[node["id"]].add(dep["pkg"])

    visited: set[str] = set()
    visiting: set[str] = set()
    ordered: list[str] = []

    def visit(package_id: str) -> None:
        if package_id in visited:
            return
        if package_id in visiting:
            raise ToolError(f"dependency cycle involving {packages[package_id]['name']}")
        visiting.add(package_id)
        for dep_id in sorted(edges[package_id], key=lambda item: packages[item]["name"]):
            visit(dep_id)
        visiting.remove(package_id)
        visited.add(package_id)
        ordered.append(package_id)

    for package_id in sorted(packages, key=lambda item: packages[item]["name"]):
        visit(package_id)
    ordered.sort(key=lambda package_id: packages[package_id]["name"] == "rskit-suite")

    result: list[dict[str, str]] = []
    for package_id in ordered:
        package = packages[package_id]
        internal_deps = ",".join(
            f"{packages[dep_id]['name']}@{packages[dep_id]['version']}"
            for dep_id in sorted(edges[package_id], key=lambda item: packages[item]["name"])
        )
        result.append({**package, "internal_deps": internal_deps})
    return result


def crate_version_published(crate: str, version: str) -> bool:
    """Return true when crates.io has a crate version, false for 404."""

    url = f"https://crates.io/api/v1/crates/{crate}/{version}"
    request = urllib.request.Request(url, headers={"User-Agent": "rskit-release-rehearsal"})
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            data = json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return False
        raise ToolError(f"crates.io lookup for {crate} {version} failed: HTTP {error.code}") from error
    except Exception as error:
        raise ToolError(f"crates.io lookup for {crate} {version} failed: {error}") from error
    return data.get("version", {}).get("num") == version

"""Guardrail check commands."""

from __future__ import annotations

import argparse
import os
import re
import tomllib
from dataclasses import dataclass
from pathlib import Path

from ..cargo import Package, discover_packages, package_manifest
from ..errors import ToolError
from ..paths import ROOT, WORKSPACES
from ..process import run
from .domains import list_domain_names, run_domain_checks


DISALLOWED_L7_EDGES = (
    ("rskit-inference", "rskit-llm"),
    ("rskit-inference", "rskit-embedding"),
    ("rskit-embedding", "rskit-llm"),
    ("rskit-embedding", "rskit-inference"),
    ("rskit-tool", "rskit-llm"),
    ("rskit-tool", "rskit-mcp"),
    ("rskit-tool", "rskit-agent"),
    ("rskit-mcp", "rskit-llm"),
    ("rskit-observability", "rskit-ai"),
    ("rskit-observability", "rskit-llm"),
)


@dataclass(frozen=True)
class DependencyVersion:
    """External workspace dependency version declaration."""

    name: str
    package: str
    version: str


def add_parser(subparsers: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    """Register check commands."""

    parser = subparsers.add_parser("check", help="Repository guardrail checks")
    check_sub = parser.add_subparsers(dest="check_command", required=True)

    domain = check_sub.add_parser("domain", help="Run clippy/tests for a domain")
    domain.add_argument("domain", nargs="?", help="Domain name")
    domain.add_argument("--list", action="store_true", help="List available domains")
    domain.add_argument("--all", action="store_true", help="Run all domains")
    domain.add_argument("--jobs", type=int, help="Concurrent independent workspace checks")
    domain.set_defaults(func=run_domain)

    check_sub.add_parser("l7-edges", help="Check disallowed L7 dependency edges").set_defaults(func=run_l7_edges)
    check_sub.add_parser("workspace-deps-sync", help="Check workspace dependency version sync").set_defaults(func=run_workspace_deps_sync)
    check_sub.add_parser("topology", help="Check module topology guardrails").set_defaults(func=run_topology)

    public_api = check_sub.add_parser("public-api", help="Check public API diff/generation")
    public_api.add_argument("package", nargs="?", default="rskit-suite")
    public_api.set_defaults(func=run_public_api)


def run_domain(args: argparse.Namespace) -> int:
    """Run domain command."""

    if args.list:
        for name in list_domain_names():
            print(name)
        return 0
    domains = list_domain_names() if args.all else [args.domain]
    if not domains or domains == [None]:
        raise ToolError("Usage: check domain <domain|--list|--all>")
    for domain in domains:
        run_domain_checks(
            domain,
            test_runner=os.environ.get("CHECK_DOMAIN_TEST_RUNNER"),
            test_threads=os.environ.get("CHECK_DOMAIN_TEST_THREADS", "1"),
            jobs=args.jobs,
        )
    return 0


def run_l7_edges(_args: argparse.Namespace) -> int:
    """Check direct normal-dependency edges that would reintroduce L7 coupling."""

    packages = {package.name: package for package in discover_packages()}
    errors: list[str] = []
    for parent, child in DISALLOWED_L7_EDGES:
        package = packages.get(parent)
        if package is None:
            errors.append(f"unable to locate workspace for crate: {parent}")
            continue
        manifest = WORKSPACES[package.workspace]
        completed = run(
            [
                "cargo",
                "tree",
                "--manifest-path",
                str(manifest),
                "-e",
                "normal",
                "--all-features",
                "-p",
                parent,
                "--depth",
                "1",
                "--prefix",
                "none",
            ],
            capture=True,
            check=False,
        )
        if completed.returncode != 0:
            errors.append(f"unable to inspect dependencies for crate: {parent}")
            continue
        deps = {line.split()[0] for line in completed.stdout.splitlines()[1:] if line.strip()}
        if child in deps:
            errors.append(f"disallowed dependency edge: {parent} -> {child}")
    if errors:
        for error in errors:
            print(error, file=os.sys.stderr)
        return 1
    return 0


def run_workspace_deps_sync(_args: argparse.Namespace) -> int:
    """Check workspace dependency version drift."""

    workspace_manifests = {name: load_toml(path) for name, path in WORKSPACES.items()}
    workspace_dependency_versions = {name: workspace_versions(path) for name, path in WORKSPACES.items()}
    errors: list[str] = []
    workspace_package_versions = {
        name: manifest.get("workspace", {}).get("package", {}).get("version")
        for name, manifest in workspace_manifests.items()
    }
    for name, version in sorted(workspace_package_versions.items()):
        if version != workspace_package_versions["core"]:
            errors.append(f"workspace.package.version: core={workspace_package_versions['core']!r}, {name}={version!r}")

    all_packages = sorted({package for versions in workspace_dependency_versions.values() for package in versions})
    for package in all_packages:
        owners = {name: versions[package] for name, versions in workspace_dependency_versions.items() if package in versions}
        package_versions = {dep.version for dep in owners.values()}
        if len(package_versions) > 1:
            details = ", ".join(f"{name} {dep.name}={dep.version!r}" for name, dep in sorted(owners.items()))
            errors.append(f"{package}: {details}")

    if errors:
        print("workspace dependency version drift detected:", file=os.sys.stderr)
        for error in errors:
            print(f"  - {error}", file=os.sys.stderr)
        print(
            "Keep shared external dependency versions aligned in core/Cargo.toml, "
            "contrib/Cargo.toml, and examples/Cargo.toml, or remove the unused declaration.",
            file=os.sys.stderr,
        )
        return 1
    print("workspace dependency versions are synced")
    return 0


def run_topology(_args: argparse.Namespace) -> int:
    """Run topology guardrails."""

    errors: list[str] = []
    core_workspace = load_toml(ROOT / "core" / "Cargo.toml")
    core_workspace_deps = core_workspace.get("workspace", {}).get("dependencies", {})
    if not isinstance(core_workspace_deps, dict):
        core_workspace_deps = {}

    for cargo_toml in sorted((ROOT / "core").glob("*/Cargo.toml")):
        manifest = load_toml(cargo_toml)
        package = manifest.get("package", {})
        crate = package.get("name", cargo_toml.parent.name)
        rel = cargo_toml.relative_to(ROOT).as_posix()

        for table_name, deps in dependency_tables(manifest):
            for dep_name, dep in deps.items():
                dep_path = path_value(dep)
                if dep_path is None:
                    continue
                resolved = (cargo_toml.parent / dep_path).resolve()
                try:
                    target = resolved.relative_to(ROOT).as_posix()
                except ValueError:
                    continue
                if target.startswith("contrib/") and crate != "rskit":
                    errors.append(f"{rel}: {table_name}.{dep_name} points to {target}; only the facade may aggregate contrib adapters")
                if target.startswith("core/rskit-server") and crate == "rskit-grpc":
                    errors.append(f"{rel}: rskit-grpc must not depend on rskit-server")

        if crate == "rskit-util":
            for table_name, deps in dependency_tables(manifest):
                for dep_name, dep in deps.items():
                    effective_name = package_name(dep_name, dep)
                    internal_target = internal_dependency_target(cargo_toml.parent, core_workspace_deps, dep_name, dep)
                    if effective_name.startswith("rskit-") and internal_target is not None:
                        errors.append(
                            f"{rel}: L0 utility crate must not depend on internal {table_name}.{dep_name} ({effective_name}) pointing to {internal_target}"
                        )

        if crate in {"rskit-http", "rskit-discovery"}:
            for table_name, bootstrap in dependency_entries(manifest, "rskit-bootstrap"):
                if not is_optional(bootstrap):
                    errors.append(f"{rel}: {table_name}.rskit-bootstrap must be optional for {crate}")

        if crate == "rskit-server":
            heavy = (
                "axum",
                "base64",
                "hyper",
                "hyper-util",
                "rskit-http",
                "rskit-security",
                "rustls",
                "rustls-pemfile",
                "tokio-rustls",
                "tonic",
                "tonic-health",
                "tonic-reflection",
                "tower",
                "tower-http",
                "tower-layer",
                "tower-service",
            )
            for dep_name in heavy:
                for table_name, dep in dependency_entries(manifest, dep_name):
                    if not is_optional(dep):
                        errors.append(f"{rel}: heavy transport dependency {table_name}.{dep_name} must be optional")

    for removed in ("core/rskit-integration/Cargo.toml",):
        if (ROOT / removed).exists():
            errors.append(f"{removed}: removed boundary crate still exists")

    if errors:
        print("Topology check failed:", file=os.sys.stderr)
        for error in errors:
            print(f"  - {error}", file=os.sys.stderr)
        return 1
    print("Topology check passed")
    return 0


def run_public_api(args: argparse.Namespace) -> int:
    """Check public API diff or generation."""

    crate = args.package
    manifest = package_manifest(crate)
    toolchain = os.environ.get("RUSTDOC_JSON_TOOLCHAIN", "nightly")
    command = ["cargo", f"+{toolchain}", "public-api"]
    diff_command = [*command, "diff", "--manifest-path", str(manifest), "-p", crate]
    list_command = [*command, "--manifest-path", str(manifest), "-p", crate]
    print(f"Checking public API for {crate} using {manifest.relative_to(ROOT)}...")
    completed = run(diff_command, capture=True, check=False)
    if completed.returncode != 0:
        output = f"{completed.stdout}{completed.stderr}"
        if f"Could not find crate `{crate}`" in output:
            print(f"No published baseline found for {crate}; validating current public API generation instead.")
            run(list_command)
            return 0
        print(output, file=os.sys.stderr, end="")
        return 1
    print(completed.stdout, end="")
    return 0


def load_toml(path: Path) -> dict:
    """Load TOML."""

    with path.open("rb") as handle:
        return tomllib.load(handle)


def dependency_version(dep_name: str, dep: object) -> DependencyVersion | None:
    """Extract a dependency version declaration."""

    if isinstance(dep, str):
        return DependencyVersion(dep_name, dep_name, dep)
    if not isinstance(dep, dict):
        return None
    if "path" in dep or dep.get("workspace") is True:
        return None
    version = dep.get("version")
    if not isinstance(version, str):
        return None
    package = dep.get("package")
    return DependencyVersion(dep_name, package if isinstance(package, str) else dep_name, version)


def workspace_versions(manifest_path: Path) -> dict[str, DependencyVersion]:
    """Return external workspace dependency versions."""

    dependencies = load_toml(manifest_path).get("workspace", {}).get("dependencies", {})
    if not isinstance(dependencies, dict):
        return {}
    versions: dict[str, DependencyVersion] = {}
    for dep_name, dep in dependencies.items():
        version = dependency_version(dep_name, dep)
        if version is not None and not version.package.startswith("rskit"):
            versions[version.package] = version
    return versions


def dependency_tables(manifest: dict) -> list[tuple[str, dict]]:
    """Return dependency tables from a manifest."""

    tables: list[tuple[str, dict]] = []
    for name in ("dependencies", "dev-dependencies", "build-dependencies"):
        table = manifest.get(name, {})
        if isinstance(table, dict):
            tables.append((name, table))
    target = manifest.get("target", {})
    if isinstance(target, dict):
        for cfg_name, cfg in target.items():
            if isinstance(cfg, dict):
                for dep_name in ("dependencies", "dev-dependencies", "build-dependencies"):
                    table = cfg.get(dep_name, {})
                    if isinstance(table, dict):
                        tables.append((f"target.{cfg_name}.{dep_name}", table))
    return tables


def is_optional(dep: object) -> bool:
    """Return true when a dependency declaration is optional."""

    return isinstance(dep, dict) and dep.get("optional") is True


def path_value(dep: object) -> str | None:
    """Return dependency path value."""

    if isinstance(dep, dict):
        value = dep.get("path")
        if isinstance(value, str):
            return value
    return None


def package_name(dep_name: str, dep: object) -> str:
    """Return effective package name for a dependency."""

    if isinstance(dep, dict):
        value = dep.get("package")
        if isinstance(value, str):
            return value
    return dep_name


def is_workspace_dependency(dep: object) -> bool:
    """Return true when dependency uses workspace inheritance."""

    return isinstance(dep, dict) and dep.get("workspace") is True


def internal_dependency_target(manifest_dir: Path, workspace_deps: dict, dep_name: str, dep: object) -> str | None:
    """Resolve an internal dependency target path."""

    dep_path = path_value(dep)
    if dep_path is None and is_workspace_dependency(dep):
        workspace_dep = workspace_deps.get(dep_name) or workspace_deps.get(package_name(dep_name, dep))
        dep_path = path_value(workspace_dep)
        manifest_dir = ROOT / "core"
    if dep_path is None:
        return None
    resolved = (manifest_dir / dep_path).resolve()
    try:
        target = resolved.relative_to(ROOT).as_posix()
    except ValueError:
        return None
    if target.startswith("core/") or target.startswith("contrib/"):
        return target
    return None


def dependency_entries(manifest: dict, name: str) -> list[tuple[str, object]]:
    """Return dependency entries with a given name."""

    return [(table_name, deps[name]) for table_name, deps in dependency_tables(manifest) if name in deps]


def features(manifest: dict) -> dict:
    """Return features table."""

    table = manifest.get("features", {})
    return table if isinstance(table, dict) else {}

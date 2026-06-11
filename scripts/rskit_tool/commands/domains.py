"""Domain tooling commands."""

from __future__ import annotations

import argparse
import dataclasses
import sys
import tomllib
from collections import deque
from pathlib import Path, PurePosixPath

from ..cargo import Package, discover_packages, package_by_name
from ..errors import ToolError
from ..git import changed_paths
from ..paths import ROOT
from ..process import ParallelTask, command_exists, run, run_parallel

DOMAIN_EMOJI = {
    "core": "🧱",
    "patterns": "🔧",
    "crosscutting": "🔄",
    "composition": "🏗️",
    "transport": "🌐",
    "auth": "🔑",
    "data": "💾",
    "ai": "🧠",
    "media": "🎬",
    "infra": "⚙️",
}
DOMAIN_ORDER = [
    "core",
    "patterns",
    "crosscutting",
    "composition",
    "transport",
    "auth",
    "data",
    "ai",
    "media",
    "infra",
]
DOMAIN_TITLES = {
    "core": "Core",
    "patterns": "Patterns",
    "crosscutting": "Cross-cutting",
    "composition": "Composition",
    "transport": "Transport",
    "auth": "Auth",
    "data": "Data",
    "ai": "AI",
    "media": "Media",
    "infra": "Infra",
}


@dataclasses.dataclass(frozen=True)
class Domain:
    """A domain from domains.toml."""

    name: str
    modules: tuple[str, ...]
    depends_on: tuple[str, ...]


def add_parser(subparsers: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    """Register domain commands."""

    parser = subparsers.add_parser("domains", help="Domain detection and documentation")
    domain_sub = parser.add_subparsers(dest="domain_command", required=True)

    affected = domain_sub.add_parser("affected", help="Print domains affected by changed files")
    affected.add_argument("paths", nargs="*", help="Changed paths; stdin or git diff is used when omitted")
    affected.add_argument("--changed-base", default="origin/main...HEAD")
    affected.set_defaults(func=run_affected)

    index = domain_sub.add_parser("module-index", help="Generate docs/MODULE-INDEX.md")
    index.set_defaults(func=run_module_index)


def load_domains() -> dict[str, Domain]:
    """Load domains.toml."""

    with (ROOT / "domains.toml").open("rb") as handle:
        raw = tomllib.load(handle).get("domains", {})
    domains: dict[str, Domain] = {}
    for name, value in raw.items():
        domains[name] = Domain(
            name=name,
            modules=tuple(value.get("modules", [])),
            depends_on=tuple(value.get("depends_on", [])),
        )
    return domains


def run_affected(args: argparse.Namespace) -> int:
    """Print affected domains."""

    if args.paths:
        files = [Path(path) for path in args.paths]
    elif not sys.stdin.isatty():
        files = [Path(line.strip()) for line in sys.stdin if line.strip()]
    else:
        files = changed_paths(args.changed_base)

    for name in affected_domains(files):
        print(name)
    return 0


def affected_domains(files: list[Path]) -> list[str]:
    """Return affected domains including downstream dependent domains."""

    domains = load_domains()
    all_domains = list(domains)
    module_to_domains: dict[str, set[str]] = {}
    for name, domain in domains.items():
        for module in domain.modules:
            module_to_domains.setdefault(module, set()).add(name)

    crate_path_to_domains = contrib_crate_path_domains(module_to_domains, all_domains)
    directly_affected: set[str] = set()
    for path in files:
        directly_affected.update(domains_for_file(path.as_posix(), all_domains, module_to_domains, crate_path_to_domains))

    inverse: dict[str, list[str]] = {}
    for name, domain in domains.items():
        for dep in domain.depends_on:
            inverse.setdefault(dep, []).append(name)

    affected = set(directly_affected)
    queue = deque(directly_affected)
    while queue:
        current = queue.popleft()
        for dependent in inverse.get(current, []):
            if dependent not in affected:
                affected.add(dependent)
                queue.append(dependent)
    return sorted(affected)


def contrib_crate_path_domains(module_to_domains: dict[str, set[str]], all_domains: list[str]) -> dict[str, set[str]]:
    """Map contrib crate paths to domains."""

    if command_exists("cargo"):
        try:
            return contrib_crate_path_domains_from_metadata(module_to_domains, all_domains)
        except ToolError:
            pass
    return contrib_crate_path_domains_from_manifests(module_to_domains, all_domains)


def contrib_crate_path_domains_from_metadata(
    module_to_domains: dict[str, set[str]],
    all_domains: list[str],
) -> dict[str, set[str]]:
    """Map contrib crate paths to domains using Cargo metadata."""

    result: dict[str, set[str]] = {}
    for package in discover_packages("contrib"):
        if not package.name.startswith("rskit-"):
            continue
        module = package.name.removeprefix("rskit-")
        result[package.root.relative_to(ROOT).as_posix()] = set(module_to_domains.get(module, all_domains))
    return result


def contrib_crate_path_domains_from_manifests(
    module_to_domains: dict[str, set[str]],
    all_domains: list[str],
) -> dict[str, set[str]]:
    """Map contrib crate paths to domains by scanning crate manifests."""

    result: dict[str, set[str]] = {}
    for manifest in sorted((ROOT / "contrib").glob("*/*/Cargo.toml")):
        with manifest.open("rb") as handle:
            name = tomllib.load(handle).get("package", {}).get("name")
        if not isinstance(name, str) or not name.startswith("rskit-"):
            continue
        module = name.removeprefix("rskit-")
        result[manifest.parent.relative_to(ROOT).as_posix()] = set(module_to_domains.get(module, all_domains))
    return result


def domains_for_file(
    path_str: str,
    all_domains: list[str],
    module_to_domains: dict[str, set[str]],
    crate_path_to_domains: dict[str, set[str]],
) -> set[str]:
    """Return domains directly affected by a file."""

    path_str = path_str.strip()
    if not path_str:
        return set()
    parts = PurePosixPath(path_str).parts
    if not parts:
        return set()

    global_files = {
        "Cargo.lock",
        "Cargo.toml",
        "Makefile",
        "README.md",
        "domains.toml",
        "core/Cargo.toml",
        "contrib/Cargo.toml",
        "examples/Cargo.toml",
    }
    global_dirs = {".cargo", ".config", ".github", "docs", "scripts"}
    if path_str in global_files or parts[0] in global_dirs:
        return set(all_domains)
    if parts[0] == "core":
        if len(parts) == 1 or parts[1] in {"Cargo.toml", "rskit"}:
            return set(all_domains)
        if parts[1].startswith("rskit-"):
            return set(module_to_domains.get(parts[1].removeprefix("rskit-"), all_domains))
        return set(all_domains)
    if parts[0] == "contrib":
        if len(parts) < 3:
            return set(all_domains)
        return set(crate_path_to_domains.get(PurePosixPath(*parts[:3]).as_posix(), all_domains))
    if parts[0] == "examples":
        return set(all_domains)
    return set()


def run_module_index(_args: argparse.Namespace) -> int:
    """Generate docs/MODULE-INDEX.md."""

    domains = load_domains()
    docs_dir = ROOT / "docs"
    docs_dir.mkdir(parents=True, exist_ok=True)
    output_path = docs_dir / "MODULE-INDEX.md"
    lines = [
        "# Module Index by Domain",
        "",
        "<!-- Auto-generated by scripts/rskit_tool.py domains module-index - do not edit manually -->",
        "",
    ]
    ordered = [domain for domain in DOMAIN_ORDER if domain in domains]
    ordered.extend(domain for domain in domains if domain not in DOMAIN_ORDER)
    for domain in ordered:
        modules = domains[domain].modules
        emoji = DOMAIN_EMOJI.get(domain, "📦")
        title = DOMAIN_TITLES.get(domain, domain.replace("-", " ").title())
        lines.append(f"## {emoji} {title}  (`make check-{domain}`)")
        lines.append(" · ".join(modules))
        lines.append("")
    output_path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print(f"Generated {output_path}")
    return 0


def list_domain_names() -> list[str]:
    """List domain names in deterministic order."""

    return sorted(load_domains())


def modules_for_domain(domain: str) -> tuple[str, ...]:
    """Return modules for a domain."""

    domains = load_domains()
    if domain not in domains:
        available = "\n".join(f"  {name}" for name in sorted(domains))
        raise ToolError(f"Unknown domain: {domain}\nAvailable domains:\n{available}")
    return domains[domain].modules


def resolve_crate_name(module: str, packages: dict[str, Package] | None = None) -> str:
    """Resolve a domain module name to a crate package name."""

    packages = packages or package_by_name()
    aliases = {"rskit": "rskit-suite", "logger": "rskit-logging", "logging": "rskit-logging"}
    if module in aliases and aliases[module] in packages:
        return aliases[module]
    candidate = f"rskit-{module}"
    if candidate in packages:
        return candidate
    if module in packages:
        return module
    return candidate


def run_domain_checks(
    domain: str,
    *,
    test_runner: str | None = None,
    test_threads: str = "1",
    jobs: int | None = None,
) -> None:
    """Run clippy and tests for a domain with batched workspace invocations."""

    packages = package_by_name()
    print(f"==> Domain: {domain}")
    selected: list[Package] = []
    for module in modules_for_domain(domain):
        crate = resolve_crate_name(module, packages)
        package = packages.get(crate)
        if package is None or package.workspace == "examples":
            raise ToolError(f"Unable to resolve crate for '{module}' (expected in core/ or contrib/)")
        selected.append(package)

    groups = group_packages_by_workspace(selected)
    nextest_available = command_exists("cargo-nextest")
    tasks = [
        ParallelTask(
            name=f"{domain}/{workspace}",
            action=lambda workspace=workspace, workspace_packages=workspace_packages: run_workspace_domain_checks(
                domain,
                workspace,
                workspace_packages,
                test_runner=test_runner,
                test_threads=test_threads,
                nextest_available=nextest_available,
            ),
        )
        for workspace, workspace_packages in groups.items()
    ]
    for output in run_parallel(tasks, jobs=jobs):
        if output:
            print(output, end="" if output.endswith("\n") else "\n")


def run_workspace_domain_checks(
    domain: str,
    workspace: str,
    packages: list[Package],
    *,
    test_runner: str | None,
    test_threads: str,
    nextest_available: bool,
) -> str:
    """Run clippy and tests for one workspace/package batch."""

    manifest = ROOT / workspace / "Cargo.toml"
    package_args = package_selection_args(packages)
    names = ", ".join(package.name for package in packages)
    lines = [f"==> Checking {domain} {workspace} packages: {names}"]
    commands = [["cargo", "clippy", "--manifest-path", str(manifest), *package_args, "--", "-D", "warnings"]]
    if test_runner == "cargo-test":
        commands.append(
            ["cargo", "test", "--manifest-path", str(manifest), *package_args, "--", "--test-threads", test_threads]
        )
    elif nextest_available:
        commands.append(
            [
                "cargo",
                "nextest",
                "run",
                "--manifest-path",
                str(manifest),
                *package_args,
                "--no-tests",
                "pass",
            ]
        )
    else:
        commands.append(["cargo", "test", "--manifest-path", str(manifest), *package_args])

    for command in commands:
        completed = run(command, capture=True)
        if completed.stdout:
            lines.append(completed.stdout.rstrip())
        if completed.stderr:
            lines.append(completed.stderr.rstrip())
    return "\n".join(lines) + "\n"


def group_packages_by_workspace(packages: list[Package]) -> dict[str, list[Package]]:
    """Group packages by workspace with deterministic ordering."""

    grouped: dict[str, list[Package]] = {}
    for package in sorted(packages, key=lambda item: (item.workspace, item.name)):
        grouped.setdefault(package.workspace, []).append(package)
    return grouped


def package_selection_args(packages: list[Package]) -> list[str]:
    """Build Cargo package selection arguments."""

    args: list[str] = []
    for package in packages:
        args.extend(["-p", package.name])
    return args

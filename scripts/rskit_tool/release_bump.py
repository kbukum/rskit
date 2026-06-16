"""``release bump`` orchestration.

Wires the pure helpers in :mod:`rskit_tool.versioning` to the working tree:
detect crates changed since the last release tag, classify their bump, cascade
breaking minors to in-workspace dependents, and rewrite manifests (crate
versions plus caret floors) idempotently. The bump performs **no network
writes** — it only edits local ``Cargo.toml`` files.
"""

from __future__ import annotations

import argparse
from collections import defaultdict
from pathlib import Path

from .cargo import Package, discover_packages, is_relative_to, metadata
from .errors import ToolError
from .git import changed_paths, file_at_ref, latest_tag
from .paths import ROOT, WORKSPACES
from .publish import CratesIoRegistry
from .versioning import (
    BumpPlan,
    SemVer,
    compute_bump_plan,
    package_version_diff_only,
    parse_package_version,
    parse_workspace_dep_floors,
    parse_workspace_package_version,
    set_package_version,
    set_workspace_dep_version,
)

# Only publishable workspaces are bump targets; examples are publish=false and
# never reach crates.io, so they keep their seed version (de-lockstepped for
# manifest consistency, but nothing to bump for a release).
_BUMP_WORKSPACES = ("core", "contrib")


def add_bump_parser(release_sub: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    """Register the ``release bump`` subcommand."""

    bump = release_sub.add_parser(
        "bump", help="Compute and apply independent per-crate version bumps"
    )
    bump.add_argument(
        "--workspace",
        choices=_BUMP_WORKSPACES,
        required=True,
        help="Workspace whose changed crates are bumped (operates per workspace).",
    )
    bump.add_argument(
        "--minor",
        action="append",
        default=[],
        metavar="CRATE",
        help="Mark a crate's change as breaking (minor bump). Repeatable.",
    )
    bump.add_argument(
        "--base",
        default=None,
        help="Git ref to diff against for change detection (default: latest tag).",
    )
    bump.add_argument(
        "--offline",
        action="store_true",
        help="Skip crates.io lookups; anchor idempotency on the release tag only.",
    )
    bump.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the planned changes without writing any manifests.",
    )
    bump.set_defaults(func=run_bump)


def _workspace_graph(workspace: str) -> tuple[dict[str, Package], dict[str, set[str]]]:
    """Return ``(members_by_name, dependents)`` for a single workspace.

    ``dependents`` maps a crate to the set of in-workspace crates that depend on
    it (the reverse of the internal dependency edges), used to cascade a breaking
    minor upward.
    """

    members = {pkg.name: pkg for pkg in discover_packages(workspace) if pkg.publishable}
    data = metadata(WORKSPACES[workspace])
    member_names = set(members)
    dependents: dict[str, set[str]] = defaultdict(set)
    for package in data["packages"]:  # type: ignore[index]
        name = package["name"]
        if name not in member_names:
            continue
        for dependency in package["dependencies"]:
            dep_name = dependency["name"]
            if dep_name in member_names and dep_name != name:
                dependents[dep_name].add(name)
    return members, dependents


def _detect_changed(members: dict[str, Package], base_ref: str) -> set[str]:
    """Map paths changed since ``base_ref`` to owning crates in this workspace.

    Only paths under a crate root count, so workspace-global and tooling changes
    (``Cargo.toml``, ``scripts/``, ``.github/`` ...) do not over-bump the world.
    A crate whose *only* change is a version-field-only edit to its own manifest
    is ignored: the version field is an output of this tool, not a source change
    (this guards the lock-step de-lockstep and the tool's own prior writes).
    """

    changed_by_crate: dict[str, list[Path]] = defaultdict(list)
    for changed in changed_paths(f"{base_ref}..HEAD"):
        absolute = (ROOT / changed).resolve()
        for package in members.values():
            if is_relative_to(absolute, package.root):
                changed_by_crate[package.name].append(absolute)
                break

    selected: set[str] = set()
    for name, paths in changed_by_crate.items():
        manifest = members[name].manifest_path.resolve()
        if any(path != manifest for path in paths):
            selected.add(name)
            continue
        relative = members[name].manifest_path.relative_to(ROOT).as_posix()
        base_text = file_at_ref(base_ref, relative)
        current_text = members[name].manifest_path.read_text(encoding="utf-8")
        if base_text is None or not package_version_diff_only(base_text, current_text):
            selected.add(name)
    return selected


def _all_workspace_floors() -> dict[str, SemVer]:
    """Collect every internal caret floor across all workspace manifests."""

    floors: dict[str, SemVer] = {}
    for manifest in WORKSPACES.values():
        floors.update(parse_workspace_dep_floors(manifest.read_text(encoding="utf-8")))
    return floors


def _tag_seed(workspace: str, base_ref: str) -> SemVer | None:
    """Return the ``[workspace.package].version`` of ``workspace`` at ``base_ref``."""

    text = file_at_ref(base_ref, f"{workspace}/Cargo.toml")
    if text is None:
        return None
    return parse_workspace_package_version(text)


def _released_baselines(
    members: dict[str, Package],
    base_ref: str,
    *,
    registry: CratesIoRegistry | None,
) -> dict[str, SemVer]:
    """Resolve each crate's released baseline = max(crates.io max, version-at-tag).

    Crates with neither anchor are omitted (treated as unreleased by the planner).
    """

    seeds: dict[str, SemVer | None] = {}
    baselines: dict[str, SemVer] = {}
    for name, package in members.items():
        candidates: list[SemVer] = []

        relative = package.manifest_path.relative_to(ROOT).as_posix()
        manifest_at_tag = file_at_ref(base_ref, relative)
        if manifest_at_tag is not None:
            tag_version = parse_package_version(manifest_at_tag)
            if tag_version is None:
                # The crate inherited its version at the tag; fall back to the
                # workspace seed captured once per workspace.
                if package.workspace not in seeds:
                    seeds[package.workspace] = _tag_seed(package.workspace, base_ref)
                tag_version = seeds[package.workspace]
            if tag_version is not None:
                candidates.append(tag_version)

        if registry is not None:
            published = registry.max_published_version(name)
            if published is not None:
                candidates.append(SemVer.parse(published))

        if candidates:
            baselines[name] = max(candidates)
    return baselines


def _apply_plan(plan: BumpPlan, members: dict[str, Package]) -> None:
    """Write crate version bumps and caret-floor rewrites to disk."""

    for action in plan.actions:
        manifest = members[action.name].manifest_path
        text = manifest.read_text(encoding="utf-8")
        updated, changed = set_package_version(text, str(action.new))
        if changed:
            manifest.write_text(updated, encoding="utf-8")

    # A breaking crate's floor lives wherever it is referenced (a contrib adapter
    # is pinned from core, for example), so rewrite every workspace manifest.
    for name, new_floor in plan.floor_rewrites:
        for manifest in WORKSPACES.values():
            text = manifest.read_text(encoding="utf-8")
            updated, changed = set_workspace_dep_version(text, name, str(new_floor))
            if changed:
                manifest.write_text(updated, encoding="utf-8")


def _print_plan(plan: BumpPlan, *, dry_run: bool) -> None:
    """Report the planned or applied changes."""

    if not plan.actions and not plan.floor_rewrites:
        print("✓ No version bumps needed (already up to date)")
        return
    verb = "Would bump" if dry_run else "Bumped"
    for action in plan.actions:
        tag = "breaking" if action.kind == "minor" else action.reason
        print(f"  {verb} {action.name}: {action.old} -> {action.new} ({action.kind}, {tag})")
    for name, new_floor in plan.floor_rewrites:
        rewrite = "Would rewrite" if dry_run else "Rewrote"
        print(f"  {rewrite} caret floor {name} -> {new_floor}")
    summary = "planned" if dry_run else "applied"
    print(
        f"✓ Bump {summary}: {len(plan.actions)} version change(s), "
        f"{len(plan.floor_rewrites)} floor rewrite(s)"
    )


def run_bump(args: argparse.Namespace) -> int:
    """Run ``release bump`` for one workspace."""

    base_ref = args.base or latest_tag()
    if base_ref is None:
        raise ToolError("no release tag found to diff against; pass --base <ref>")

    members, dependents = _workspace_graph(args.workspace)
    if not members:
        raise ToolError(f"workspace '{args.workspace}' has no publishable crates")

    unknown = [crate for crate in args.minor if crate not in members]
    if unknown:
        raise ToolError(
            f"--minor crate(s) not in workspace '{args.workspace}': {', '.join(sorted(unknown))}"
        )

    changed = _detect_changed(members, base_ref)
    registry = None if args.offline else CratesIoRegistry()
    baselines = _released_baselines(members, base_ref, registry=registry)
    current_versions = {name: SemVer.parse(package.version) for name, package in members.items()}

    plan = compute_bump_plan(
        changed=changed,
        minor=args.minor,
        dependents=dependents,
        current_versions=current_versions,
        baselines=baselines,
        current_floors=_all_workspace_floors(),
    )

    print(f"==> Planning bump for workspace '{args.workspace}' (base {base_ref})...")
    if not changed:
        print("  No crates changed since base.")
    if not args.dry_run:
        _apply_plan(plan, members)
    _print_plan(plan, dry_run=args.dry_run)
    return 0

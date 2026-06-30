"""``release bump`` orchestration.

Wires the pure helpers in :mod:`rskit_tool.versioning` to the working tree:
detect crates changed since the last release tag, classify their bump, cascade
breaking minors to in-workspace dependents, and rewrite manifests (crate
versions plus caret floors) plus the version pins in install-snippet READMEs,
idempotently. The bump performs **no network writes** — it only edits local
``Cargo.toml`` and ``README.md`` files.
"""

from __future__ import annotations

import argparse
from collections import defaultdict
from collections.abc import Mapping
from pathlib import Path

from .cargo import Package, discover_packages, is_relative_to, metadata
from .errors import ToolError
from .git import changed_paths, file_at_ref, latest_tag, merge_base
from .paths import ROOT, WORKSPACES
from .publish import CratesIoRegistry
from .versioning import (
    BumpPlan,
    SemVer,
    compute_bump_plan,
    inherited_workspace_dep_keys,
    package_version_diff_only,
    parse_package_version,
    parse_workspace_dep_floors,
    parse_workspace_package_version,
    readme_version_diff_only,
    set_package_version,
    set_readme_dependency_versions,
    set_workspace_dep_version,
    workspace_dep_floor_changes,
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
        "--all-minor",
        action="store_true",
        help=(
            "Coordinated workspace-wide MINOR bump: re-seed EVERY crate to the "
            "next minor (e.g. 0.1.x -> 0.2.0[-pre]), regardless of what changed. "
            "Realigns the whole workspace onto one version; rewrites internal "
            "caret floors and README pins to match. Cannot be combined with "
            "--minor or --all-major."
        ),
    )
    bump.add_argument(
        "--all-major",
        action="store_true",
        help=(
            "Coordinated workspace-wide MAJOR bump: re-seed EVERY crate to the "
            "next major. Cannot be combined with --minor or --all-minor."
        ),
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


def _detect_changed(members: dict[str, Package], base_ref: str, workspace: str) -> set[str]:
    """Map paths changed since ``base_ref`` to owning crates in this workspace.

    Change detection diffs from the *merge base* of ``base_ref`` and ``HEAD`` so
    a release branch or backport whose tag is not a strict ancestor of ``HEAD``
    still resolves the correct set of changes (mirroring ``changed_paths``).

    Only paths under a crate root count, so workspace-global and tooling changes
    (``Cargo.toml``, ``scripts/``, ``.github/`` ...) do not over-bump the world.
    A crate whose *only* change is a tool-generated version edit is ignored — a
    version-field-only edit to its own manifest, or a dependency-pin-only edit to
    its ``README.md`` — because those are outputs of this tool, not source changes
    (this guards the lock-step de-lockstep and the tool's own prior writes,
    including the README pin sync). See :func:`_is_tool_generated_change`.

    Workspace-global ``[workspace.dependencies]`` floor changes are the one
    exception: a crate that inherits a bumped floor via ``<dep>.workspace = true``
    has a different *published* manifest even though nothing under its crate root
    changed, so those inheritors are selected too (the cross-workspace cascade,
    e.g. a ``core`` breaking-minor rewriting ``contrib/Cargo.toml`` floors).
    """

    diff_base = merge_base(base_ref, "HEAD") or base_ref
    changed_by_crate: dict[str, list[Path]] = defaultdict(list)
    for changed in changed_paths(f"{diff_base}..HEAD"):
        absolute = (ROOT / changed).resolve()
        for package in members.values():
            if is_relative_to(absolute, package.root):
                changed_by_crate[package.name].append(absolute)
                break

    selected: set[str] = set()
    for name, paths in changed_by_crate.items():
        package = members[name]
        if any(not _is_tool_generated_change(package, path, diff_base) for path in paths):
            selected.add(name)

    selected |= _floor_inheritors(members, workspace, diff_base)
    return selected


def _is_tool_generated_change(package: Package, path: Path, diff_base: str) -> bool:
    """Whether ``path`` is a release-tooling output, not a release-worthy change.

    Two kinds of edits are outputs of this tool rather than source changes, so a
    crate whose *only* change is one of them must not be bumped:

    * its own ``Cargo.toml`` changed only in the ``[package]`` version line; or
    * its ``README.md`` changed only in tool-managed dependency-pin versions (the
      install-snippet floors this command keeps in sync — see
      :func:`_sync_readme_versions`).

    A path that is neither manifest nor README, or whose diff touches more than the
    tool-managed version tokens, is a real change. A path with no base revision
    (added since the tag) is likewise treated as a real change.
    """

    manifest = package.manifest_path.resolve()
    readme = (package.root / "README.md").resolve()
    if path == manifest:
        return _diff_only(manifest, diff_base, package_version_diff_only)
    if path == readme:
        return _diff_only(readme, diff_base, readme_version_diff_only)
    return False


def _diff_only(path: Path, diff_base: str, predicate) -> bool:
    """Apply a ``(base, current) -> bool`` version-only predicate to ``path``."""

    relative = path.relative_to(ROOT).as_posix()
    base_text = file_at_ref(diff_base, relative)
    if base_text is None:
        return False
    return predicate(base_text, path.read_text(encoding="utf-8"))


def _floor_inheritors(
    members: dict[str, Package], workspace: str, diff_base: str
) -> set[str]:
    """Select crates whose inherited ``[workspace.dependencies]`` floor changed.

    Compares the workspace manifest at ``diff_base`` against the working tree, so
    an uncommitted floor rewrite from an earlier workspace's bump is still seen.
    """

    base_text = file_at_ref(diff_base, f"{workspace}/Cargo.toml")
    if base_text is None:
        return set()
    current_text = WORKSPACES[workspace].read_text(encoding="utf-8")
    changed_keys = workspace_dep_floor_changes(base_text, current_text)
    if not changed_keys:
        return set()

    affected: set[str] = set()
    for name, package in members.items():
        inherited = inherited_workspace_dep_keys(
            package.manifest_path.read_text(encoding="utf-8")
        )
        if inherited & changed_keys:
            affected.add(name)
    return affected


def _umbrella_selection(members: dict[str, Package], changed: set[str]) -> set[str]:
    """Add umbrella crates to ``changed`` when a real release is happening.

    An umbrella crate (marked ``[package.metadata.release] umbrella = true``) is a
    facade that re-exports its workspace, so it should carry the headline release
    version even when its own source did not change. It is force-selected whenever
    any *other* crate in its workspace is bumped, but never on its own — an empty
    release stays empty. Idempotency is preserved by the planner, which anchors the
    target on the released baseline, so re-running never bumps it twice.
    """

    umbrella = {name for name, package in members.items() if package.umbrella}
    if not umbrella:
        return changed
    if changed - umbrella:
        return changed | umbrella
    return changed


def _all_workspace_floors() -> dict[str, SemVer]:
    """Collect each internal crate's caret floor across all workspace manifests.

    A crate can be pinned in more than one workspace manifest. When floors
    diverge, the **minimum** (most conservative) floor is kept so floor-rewrite
    detection still fires for every manifest that needs it — picking the last or
    highest floor could mask a manifest whose floor no longer contains the bumped
    version.
    """

    floors: dict[str, SemVer] = {}
    for manifest in WORKSPACES.values():
        manifest_floors = parse_workspace_dep_floors(manifest.read_text(encoding="utf-8"))
        for name, floor in manifest_floors.items():
            existing = floors.get(name)
            if existing is None or floor < existing:
                floors[name] = floor
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
    A crates.io lookup is only an idempotency optimization, so a transient
    registry failure (HTTP 429/5xx, network error) is downgraded to a warning and
    planning falls back to tag-only baselines instead of aborting the bump.
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
            try:
                published = registry.max_published_version(name)
            except ToolError as error:
                print(
                    f"  ! crates.io lookup failed ({error}); "
                    "falling back to tag-only baselines"
                )
                registry = None
            else:
                if published is not None:
                    candidates.append(SemVer.parse(published))

        if candidates:
            baselines[name] = max(candidates)
    return baselines


def _authoritative_versions(plan: BumpPlan) -> dict[str, str]:
    """Map every publishable crate to its post-bump version.

    Starts from the current on-disk version of each publishable crate across *all*
    workspaces — README install snippets cross workspace boundaries (a ``contrib``
    adapter README pins a ``core`` crate; the root README pins the facade) — then
    overlays this plan's bumps so the map reflects the versions a reader should
    copy after the release.
    """

    versions = {pkg.name: pkg.version for pkg in discover_packages() if pkg.publishable}
    for action in plan.actions:
        versions[action.name] = str(action.new)
    return versions


def _source_readmes() -> list[Path]:
    """Every published install-snippet README in the source tree (no build output).

    The root README plus every README under the ``core`` and ``contrib`` workspace
    trees (core crates sit at ``core/<crate>``; contrib adapters nest a domain
    level at ``contrib/<domain>/<crate>``). Generated ``target/`` package artifacts
    are excluded so only source-of-truth files are rewritten.
    """

    readmes = [ROOT / "README.md"]
    for workspace_dir in ("core", "contrib"):
        readmes.extend(
            path
            for path in sorted((ROOT / workspace_dir).rglob("README.md"))
            if "target" not in path.parts
        )
    return [path for path in readmes if path.is_file()]


def _sync_readme_versions(plan: BumpPlan, *, dry_run: bool) -> list[Path]:
    """Rewrite stale dependency-pin versions in install-snippet READMEs.

    Resolves the post-bump version of every publishable crate (see
    :func:`_authoritative_versions`) and rewrites each source README's
    dependency pins to match. Returns the READMEs that needed a change and writes
    them unless ``dry_run``. This keeps the published install snippets in
    lock-step with the versions just bumped; the change-detection guard
    (:func:`_is_tool_generated_change`) ensures these rewrites never feed back
    into a future bump.
    """

    versions = _authoritative_versions(plan)
    changed: list[Path] = []
    for readme in _source_readmes():
        text = readme.read_text(encoding="utf-8")
        updated, did_change = set_readme_dependency_versions(text, versions)
        if did_change:
            changed.append(readme)
            if not dry_run:
                readme.write_text(updated, encoding="utf-8")
    return changed


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


def _print_plan(plan: BumpPlan, readmes: list[Path], *, dry_run: bool) -> None:
    """Report the planned or applied changes."""

    if not plan.actions and not plan.floor_rewrites:
        print("✓ No version bumps needed (already up to date)")
        return
    verb = "Would bump" if dry_run else "Bumped"
    for action in plan.actions:
        tag = "breaking" if action.kind in ("minor", "major") else action.reason
        print(f"  {verb} {action.name}: {action.old} -> {action.new} ({action.kind}, {tag})")
    for name, new_floor in plan.floor_rewrites:
        rewrite = "Would rewrite" if dry_run else "Rewrote"
        print(f"  {rewrite} caret floor {name} -> {new_floor}")
    for readme in readmes:
        sync = "Would sync" if dry_run else "Synced"
        print(f"  {sync} README pins {readme.relative_to(ROOT).as_posix()}")
    summary = "planned" if dry_run else "applied"
    print(
        f"✓ Bump {summary}: {len(plan.actions)} version change(s), "
        f"{len(plan.floor_rewrites)} floor rewrite(s), "
        f"{len(readmes)} README sync(s)"
    )


def _release_anchor(base_ref: str, baselines: Mapping[str, SemVer]) -> SemVer | None:
    """Resolve a single released anchor for a coordinated ``--all-*`` bump.

    A coordinated bump must compute its uniform target from a fixed released
    version (so re-running is idempotent rather than advancing every run). Prefer
    the ``base_ref`` when it is a ``vX.Y.Z`` release tag; otherwise fall back to
    the highest per-crate baseline (max of crates.io and the tag manifests). When
    nothing has ever been released there is no anchor to advance from.
    """

    candidate = base_ref[1:] if base_ref.startswith("v") else base_ref
    try:
        return SemVer.parse(candidate)
    except ValueError:
        pass
    if baselines:
        return max(baselines.values())
    return None


def run_bump(args: argparse.Namespace) -> int:
    """Run ``release bump`` for one workspace."""

    if getattr(args, "all_minor", False) and getattr(args, "all_major", False):
        raise ToolError("--all-minor and --all-major are mutually exclusive")
    all_kind = (
        "major"
        if getattr(args, "all_major", False)
        else "minor"
        if getattr(args, "all_minor", False)
        else None
    )
    if all_kind and args.minor:
        raise ToolError("--minor cannot be combined with --all-minor/--all-major")

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

    registry = None if args.offline else CratesIoRegistry()
    baselines = _released_baselines(members, base_ref, registry=registry)
    current_versions = {name: SemVer.parse(package.version) for name, package in members.items()}

    if all_kind:
        # Coordinated workspace-wide bump: select every crate and apply the same
        # breaking kind. Crates never released (no tag/crates.io anchor, e.g. a
        # crate added since the last tag) get the shared release anchor so they
        # land on the same uniform target instead of being skipped.
        changed: set[str] = set(members)
        anchor = _release_anchor(base_ref, baselines)
        if anchor is None:
            raise ToolError(
                "coordinated --all-minor/--all-major needs a released anchor; "
                "pass --base <vX.Y.Z tag>"
            )
        for name in members:
            baselines.setdefault(name, anchor)
        minor_arg = list(members) if all_kind == "minor" else []
        major_arg = list(members) if all_kind == "major" else []
    else:
        changed = _detect_changed(members, base_ref, args.workspace)
        changed = _umbrella_selection(members, changed)
        minor_arg = args.minor
        major_arg = []

    plan = compute_bump_plan(
        changed=changed,
        minor=minor_arg,
        major=major_arg,
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
    # README pins follow package versions, which only move via plan actions; a
    # pure floor-rewrite release leaves install snippets untouched.
    synced = _sync_readme_versions(plan, dry_run=args.dry_run) if plan.actions else []
    _print_plan(plan, synced, dry_run=args.dry_run)
    return 0

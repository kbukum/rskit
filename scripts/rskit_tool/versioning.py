"""Independent per-crate semantic versioning building blocks.

Pure, dependency-free helpers for the ``release bump`` command. The repository
runs independent per-crate semver with caret pins and 0.x breaking semantics
(see ``docs/VERSIONING.md`` and ``docs/VERSIONING-ROADMAP.md``):

* a **patch** bump is absorbed by a dependent's caret pin (no cascade);
* a **minor** bump (the breaking position in 0.x) leaves the caret range, so
  the dependency floor must move and in-workspace dependents republish.

Everything here operates on plain data so it stays trivially testable: semver
parsing/ordering/bumping, caret-range checks, ``Cargo.toml`` text edits that
preserve formatting, and bump-plan computation over a crate dependency graph.
"""

from __future__ import annotations

import dataclasses
import functools
import re
from collections.abc import Iterable, Mapping

# Semantic-version grammar (semver.org 2.0.0), anchored to a full version.
# Pre-release identifiers follow the spec's strict form: a numeric identifier is
# ``0`` or ``[1-9]\d*`` (no leading zeros), while an alphanumeric identifier must
# contain at least one non-digit. Build metadata stays permissive (leading zeros
# are allowed there per §10).
_PRERELEASE_ID = r"(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)"
_SEMVER_RE = re.compile(
    r"^(?P<major>0|[1-9]\d*)\.(?P<minor>0|[1-9]\d*)\.(?P<patch>0|[1-9]\d*)"
    rf"(?:-(?P<prerelease>{_PRERELEASE_ID}(?:\.{_PRERELEASE_ID})*))?"
    r"(?:\+(?P<build>[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)

_BUMP_KINDS = ("patch", "minor", "major")


def _split_identifiers(text: str) -> tuple[int | str, ...]:
    """Split a dotted pre-release/build string into typed identifiers."""

    parts: list[int | str] = []
    for part in text.split("."):
        # A purely-numeric identifier is compared numerically; anything else is
        # an alphanumeric identifier compared lexically (semver §11).
        if part.isdigit() and (part == "0" or not part.startswith("0")):
            parts.append(int(part))
        else:
            parts.append(part)
    return tuple(parts)


@functools.total_ordering
@dataclasses.dataclass(frozen=True)
class SemVer:
    """A parsed semantic version with precedence-correct ordering."""

    major: int
    minor: int
    patch: int
    prerelease: tuple[int | str, ...] = ()
    build: tuple[str, ...] = ()

    @classmethod
    def parse(cls, text: str) -> SemVer:
        """Parse ``text`` into a :class:`SemVer` or raise ``ValueError``."""

        match = _SEMVER_RE.match(text.strip())
        if match is None:
            raise ValueError(f"invalid semantic version: {text!r}")
        prerelease = _split_identifiers(match["prerelease"]) if match["prerelease"] else ()
        build = tuple(match["build"].split(".")) if match["build"] else ()
        return cls(int(match["major"]), int(match["minor"]), int(match["patch"]), prerelease, build)

    @property
    def core(self) -> tuple[int, int, int]:
        """The ``(major, minor, patch)`` precedence triple."""

        return (self.major, self.minor, self.patch)

    def __str__(self) -> str:
        text = f"{self.major}.{self.minor}.{self.patch}"
        if self.prerelease:
            text += "-" + ".".join(str(part) for part in self.prerelease)
        if self.build:
            text += "+" + ".".join(self.build)
        return text

    def __eq__(self, other: object) -> bool:
        # Build metadata is ignored for precedence (semver §10).
        if not isinstance(other, SemVer):
            return NotImplemented
        return self.core == other.core and self.prerelease == other.prerelease

    def __hash__(self) -> int:
        return hash((self.core, self.prerelease))

    def __lt__(self, other: object) -> bool:
        if not isinstance(other, SemVer):
            return NotImplemented
        if self.core != other.core:
            return self.core < other.core
        return _prerelease_lt(self.prerelease, other.prerelease)


def _prerelease_lt(left: tuple[int | str, ...], right: tuple[int | str, ...]) -> bool:
    """Order pre-release identifier tuples (a release outranks any pre-release)."""

    if left == right:
        return False
    if not left:
        return False  # release > pre-release
    if not right:
        return True
    for lhs, rhs in zip(left, right):
        if lhs == rhs:
            continue
        lhs_num, rhs_num = isinstance(lhs, int), isinstance(rhs, int)
        if lhs_num and rhs_num:
            return lhs < rhs  # type: ignore[operator]
        if lhs_num != rhs_num:
            return lhs_num  # numeric identifiers rank below alphanumeric ones
        return str(lhs) < str(rhs)
    return len(left) < len(right)


def _reset_prerelease(prerelease: tuple[int | str, ...]) -> tuple[int | str, ...]:
    """Reset the trailing numeric counter of a pre-release seed to 1.

    Keeps the alpha/beta train when starting a fresh minor/major line, e.g.
    ``-alpha.3`` becomes ``-alpha.1``; a release version stays a release.
    """

    if not prerelease:
        return ()
    reset = list(prerelease)
    if isinstance(reset[-1], int):
        reset[-1] = 1
    return tuple(reset)


def bump(version: SemVer, kind: str) -> SemVer:
    """Return ``version`` bumped by ``kind`` (``patch``/``minor``/``major``).

    Pre-release lines are kept on a ``patch`` (``-alpha.1`` -> ``-alpha.2``) and
    re-seeded on a ``minor``/``major`` (``0.1.0-alpha.3`` -> ``0.2.0-alpha.1``),
    so a crate keeps publishing within its current alpha/beta train.
    """

    if kind not in _BUMP_KINDS:
        raise ValueError(f"unknown bump kind: {kind!r}")
    if kind == "patch":
        if version.prerelease:
            tail = list(version.prerelease)
            if isinstance(tail[-1], int):
                tail[-1] += 1
            else:
                tail.append(1)
            return dataclasses.replace(version, prerelease=tuple(tail), build=())
        return dataclasses.replace(version, patch=version.patch + 1, build=())
    if kind == "minor":
        return SemVer(version.major, version.minor + 1, 0, _reset_prerelease(version.prerelease))
    return SemVer(version.major + 1, 0, 0, _reset_prerelease(version.prerelease))


def within_caret(floor: SemVer, candidate: SemVer) -> bool:
    """Return ``True`` when ``candidate`` satisfies a bare ``floor`` (caret ``^floor``) pin.

    Cargo treats a bare version string as a caret requirement, so ``floor`` here
    means ``^floor`` (not the exact ``=floor`` requirement). Mirrors cargo's
    default, including its pre-release rule: a pre-release candidate only matches
    when it shares the floor's ``major.minor.patch`` and the floor itself carries
    a pre-release.
    """

    if candidate < floor:
        return False
    if floor.major > 0:
        upper = (floor.major + 1, 0, 0)
    elif floor.minor > 0:
        upper = (0, floor.minor + 1, 0)
    else:
        upper = (0, 0, floor.patch + 1)
    if candidate.core >= upper:
        return False
    if candidate.prerelease and not (floor.prerelease and candidate.core == floor.core):
        return False
    return True


# --------------------------------------------------------------------------- #
# Cargo.toml text edits (formatting-preserving, idempotent)
# --------------------------------------------------------------------------- #


def _table_header(line: str) -> str | None:
    """Return the table name for a ``[table]``/``[[table]]`` header line.

    Tolerates a trailing TOML comment (``[table] # note``): the comment is
    stripped before the closing bracket is checked, but only when doing so still
    leaves a valid header, so a ``#`` inside a quoted key is left untouched.
    """

    stripped = line.strip()
    if not (stripped.startswith("[") and "]" in stripped):
        return None
    if "#" in stripped:
        without_comment = stripped[: stripped.index("#")].rstrip()
        if without_comment.endswith("]"):
            stripped = without_comment
    if stripped.endswith("]"):
        return stripped.strip("[]").strip()
    return None


def set_package_version(text: str, new_version: str) -> tuple[str, bool]:
    """Set ``[package].version`` to ``new_version``; return ``(text, changed)``."""

    return _set_table_string(text, "package", "version", new_version)


def _get_table_string(text: str, table: str, key: str) -> str | None:
    """Return the literal ``key = "..."`` string value inside ``[table]``."""

    pattern = re.compile(rf'^\s*{re.escape(key)}\s*=\s*"([^"]*)"')
    current: str | None = None
    for line in text.split("\n"):
        header = _table_header(line)
        if header is not None:
            current = header
            continue
        if current != table:
            continue
        match = pattern.match(line)
        if match is not None:
            return match[1]
    return None


def _parse_optional(value: str | None) -> SemVer | None:
    """Parse ``value`` into a :class:`SemVer`, returning None on absence/invalid."""

    if value is None:
        return None
    try:
        return SemVer.parse(value)
    except ValueError:
        return None


def parse_package_version(text: str) -> SemVer | None:
    """Return the literal ``[package].version`` (None when inherited/absent)."""

    return _parse_optional(_get_table_string(text, "package", "version"))


def parse_workspace_package_version(text: str) -> SemVer | None:
    """Return the literal ``[workspace.package].version`` (None when absent)."""

    return _parse_optional(_get_table_string(text, "workspace.package", "version"))


_PACKAGE_VERSION_LINE_RE = re.compile(r"^\s*version(\.workspace)?\s*=")


def _strip_package_version_line(text: str) -> str:
    """Drop the ``[package]`` version line (literal or inherited form) from ``text``."""

    out: list[str] = []
    current: str | None = None
    for line in text.split("\n"):
        header = _table_header(line)
        if header is not None:
            current = header
            out.append(line)
            continue
        if current == "package" and _PACKAGE_VERSION_LINE_RE.match(line):
            continue
        out.append(line)
    return "\n".join(out)


def package_version_diff_only(old_text: str, new_text: str) -> bool:
    """Return True when two manifests differ *only* in the ``[package]`` version line.

    The version field is an output of the release tooling, not a source change, so
    a manifest edit that touches nothing else (e.g. the lock-step de-lockstep, or
    a prior bump's own write) must not be counted as a release-worthy change.
    """

    return _strip_package_version_line(old_text) == _strip_package_version_line(new_text)


# A README dependency pin in tool-managed install snippets, in either form:
#   rskit-foo = "0.1.0-alpha.2"
#   rskit-foo = { version = "0.1.0-alpha.2", features = ["x"] }
# Only the version token is captured; the surrounding crate name and any table
# attributes are preserved verbatim on rewrite. The version is anchored to a
# semver-shaped token so prose and illustrative example strings (which never take
# the ``rskit-<name> = "..."`` assignment form) are left untouched.
_README_PIN_RE = re.compile(
    r"(?P<crate>rskit-[a-z0-9-]+)"
    r"(?P<lead>\s*=\s*(?:\{[^}\n]*?\bversion\s*=\s*)?\")"
    r"(?P<version>\d+\.\d+\.\d+[0-9A-Za-z.\-+]*)\""
)


def set_readme_dependency_versions(text: str, versions: Mapping[str, str]) -> tuple[str, bool]:
    """Rewrite tool-managed README dependency pins to authoritative versions.

    Every ``rskit-<name> = "..."`` (or ``{ version = "..." }``) pin whose crate is
    in ``versions`` is set to that crate's current version; pins for crates absent
    from the map are left untouched. Returns ``(text, changed)``.

    The version pin shown in a README install snippet is derived from the crate's
    real version — an output of the release tooling, not hand-authored — so the
    bump keeps these in sync on every release instead of letting them drift.
    """

    def replace(match: re.Match[str]) -> str:
        target = versions.get(match["crate"])
        if target is None:
            return match[0]
        return f"{match['crate']}{match['lead']}{target}\""

    rewritten = _README_PIN_RE.sub(replace, text)
    return rewritten, rewritten != text


def _strip_readme_pin_versions(text: str) -> str:
    """Neutralize every tool-managed README pin version to a fixed placeholder."""

    return _README_PIN_RE.sub(
        lambda match: f"{match['crate']}{match['lead']}<version>\"", text
    )


def readme_version_diff_only(old_text: str, new_text: str) -> bool:
    """Return True when two READMEs differ *only* in tool-managed pin versions.

    The pin version mirrors the crate's released version (a tooling output), so a
    README whose sole change is a pin bump — e.g. a prior release's own sync write
    — must not be counted as a release-worthy source change. Any prose, example,
    or added/removed dependency line makes the normalized texts differ and the
    README is treated as a real change.
    """

    return _strip_readme_pin_versions(old_text) == _strip_readme_pin_versions(new_text)


def parse_workspace_dep_floors(text: str) -> dict[str, SemVer]:
    """Map crate package-name to its caret floor in ``[workspace.dependencies]``.

    Resolves ``package = "..."`` aliases and ignores entries without a version
    field (e.g. pure ``{ path = ... }`` or plain string requirements).
    """

    floors: dict[str, SemVer] = {}
    current: str | None = None
    for line in text.split("\n"):
        header = _table_header(line)
        if header is not None:
            current = header
            continue
        if current != "workspace.dependencies":
            continue
        match = _DEP_LINE_RE.match(line)
        if match is None:
            continue
        body = match["body"]
        version_match = _VERSION_FIELD_RE.search(body)
        if version_match is None:
            continue
        package_match = _PACKAGE_FIELD_RE.search(body)
        package = package_match[1] if package_match else match["key"]
        parsed = _parse_optional(version_match[2])
        if parsed is not None:
            floors[package] = parsed
    return floors


def _workspace_dep_floors_by_key(text: str) -> dict[str, SemVer]:
    """Map ``[workspace.dependencies]`` *table key* to its caret floor.

    Unlike :func:`parse_workspace_dep_floors` (keyed by resolved package name),
    this keys by the manifest key on the left-hand side, which is what a member
    crate references via ``<key>.workspace = true`` / ``<key> = { workspace = true }``.
    """

    floors: dict[str, SemVer] = {}
    current: str | None = None
    for line in text.split("\n"):
        header = _table_header(line)
        if header is not None:
            current = header
            continue
        if current != "workspace.dependencies":
            continue
        match = _DEP_LINE_RE.match(line)
        if match is None:
            continue
        version_match = _VERSION_FIELD_RE.search(match["body"])
        if version_match is None:
            continue
        parsed = _parse_optional(version_match[2])
        if parsed is not None:
            floors[match["key"]] = parsed
    return floors


def workspace_dep_floor_changes(old_text: str, new_text: str) -> set[str]:
    """Return ``[workspace.dependencies]`` keys whose caret floor changed.

    A key counts as changed when its floor differs between the two manifests or
    when it is present in exactly one of them. Member crates that inherit such a
    key (``<key>.workspace = true``) have a different *published* manifest even
    when no file under their own crate root changed, so they must republish.
    """

    old = _workspace_dep_floors_by_key(old_text)
    new = _workspace_dep_floors_by_key(new_text)
    return {key for key in old.keys() | new.keys() if old.get(key) != new.get(key)}


_DEP_TABLE_LEAVES = frozenset({"dependencies", "dev-dependencies", "build-dependencies"})
_INLINE_WORKSPACE_RE = re.compile(r"\bworkspace\s*=\s*true\b")
_DOTTED_WORKSPACE_RE = re.compile(r"^\s*(?P<key>[A-Za-z0-9_.-]+)\.workspace\s*=\s*true\b")


def _is_dependency_table(header: str) -> bool:
    """Return True for ``[dependencies]``/``[*-dependencies]`` and target variants."""

    return header.split(".")[-1] in _DEP_TABLE_LEAVES


def inherited_workspace_dep_keys(manifest_text: str) -> set[str]:
    """Return dependency keys a crate inherits via ``workspace = true``.

    Covers both the dotted (``serde.workspace = true``) and inline-table
    (``serde = { workspace = true }``) forms across ``[dependencies]``,
    ``[dev-dependencies]``, ``[build-dependencies]``, and their
    ``[target.<cfg>.*dependencies]`` variants.
    """

    keys: set[str] = set()
    current: str | None = None
    for line in manifest_text.split("\n"):
        header = _table_header(line)
        if header is not None:
            current = header
            continue
        if current is None or not _is_dependency_table(current):
            continue
        dotted = _DOTTED_WORKSPACE_RE.match(line)
        if dotted is not None:
            keys.add(dotted["key"])
            continue
        inline = _DEP_LINE_RE.match(line)
        if inline is not None and _INLINE_WORKSPACE_RE.search(inline["body"]):
            keys.add(inline["key"])
    return keys


def _set_table_string(text: str, table: str, key: str, new_value: str) -> tuple[str, bool]:
    """Replace the ``key = "..."`` string inside ``[table]`` (first occurrence)."""

    had_final_newline = text.endswith("\n")
    lines = text.split("\n")
    pattern = re.compile(rf'^(\s*{re.escape(key)}\s*=\s*")([^"]*)(".*)$')
    current: str | None = None
    changed = False
    for index, line in enumerate(lines):
        header = _table_header(line)
        if header is not None:
            current = header
            continue
        if current != table:
            continue
        match = pattern.match(line)
        if match is not None:
            if match[2] != new_value:
                lines[index] = f"{match[1]}{new_value}{match[3]}"
                changed = True
            break
    result = "\n".join(lines)
    if had_final_newline and not result.endswith("\n"):
        result += "\n"
    return result, changed


_DEP_LINE_RE = re.compile(r"^(?P<indent>\s*)(?P<key>[A-Za-z0-9_.-]+)\s*=\s*\{(?P<body>.*)\}(?P<rest>\s*(#.*)?)$")
_VERSION_FIELD_RE = re.compile(r'(version\s*=\s*")([^"]*)(")')
_PACKAGE_FIELD_RE = re.compile(r'package\s*=\s*"([^"]+)"')


def set_workspace_dep_version(text: str, crate_name: str, new_version: str) -> tuple[str, bool]:
    """Rewrite the caret floor of ``crate_name`` in ``[workspace.dependencies]``.

    Matches the dependency by package name, resolving ``package = "..."`` aliases
    (e.g. the ``rskit`` key that renames ``rskit-suite``). Inline-table formatting
    and trailing comments are preserved; only the ``version`` field changes.
    """

    had_final_newline = text.endswith("\n")
    lines = text.split("\n")
    current: str | None = None
    changed = False
    for index, line in enumerate(lines):
        header = _table_header(line)
        if header is not None:
            current = header
            continue
        if current != "workspace.dependencies":
            continue
        match = _DEP_LINE_RE.match(line)
        if match is None:
            continue
        body = match["body"]
        package_match = _PACKAGE_FIELD_RE.search(body)
        package = package_match[1] if package_match else match["key"]
        if package != crate_name:
            continue
        new_body, count = _VERSION_FIELD_RE.subn(rf"\g<1>{new_version}\g<3>", body, count=1)
        if count == 0 or new_body == body:
            break
        lines[index] = f'{match["indent"]}{match["key"]} = {{{new_body}}}{match["rest"]}'
        changed = True
        break
    result = "\n".join(lines)
    if had_final_newline and not result.endswith("\n"):
        result += "\n"
    return result, changed


# --------------------------------------------------------------------------- #
# Bump-plan computation
# --------------------------------------------------------------------------- #


@dataclasses.dataclass(frozen=True)
class BumpAction:
    """A single crate version change produced by :func:`compute_bump_plan`."""

    name: str
    old: SemVer
    new: SemVer
    kind: str
    reason: str  # "changed" (direct edit) or "cascade" (breaking dependency)


@dataclasses.dataclass(frozen=True)
class BumpPlan:
    """The result of planning a bump: version actions and caret-floor rewrites."""

    actions: tuple[BumpAction, ...]
    floor_rewrites: tuple[tuple[str, SemVer], ...]


def transitive_dependents(roots: Iterable[str], dependents: Mapping[str, set[str]]) -> set[str]:
    """Collect every crate that transitively depends on any crate in ``roots``."""

    found: set[str] = set()
    stack = list(roots)
    while stack:
        node = stack.pop()
        for parent in dependents.get(node, set()):
            if parent not in found:
                found.add(parent)
                stack.append(parent)
    return found


def compute_bump_plan(
    *,
    changed: Iterable[str],
    minor: Iterable[str],
    dependents: Mapping[str, set[str]],
    current_versions: Mapping[str, SemVer],
    baselines: Mapping[str, SemVer],
    current_floors: Mapping[str, SemVer],
) -> BumpPlan:
    """Plan version bumps and caret-floor rewrites for one workspace.

    ``changed`` crates default to a **patch** bump; those also listed in
    ``minor`` take a breaking **minor** bump and cascade a patch to their
    in-workspace transitive dependents. Each target is computed against the
    released ``baselines`` (max of crates.io and the last tag) so re-running is a
    no-op once the local manifest already carries the bumped version.

    Floor rewrites are emitted for any dependency whose final version no longer
    satisfies its current caret floor — exactly the breaking-minor case — which
    keeps path-based resolution valid and is itself idempotent.
    """

    minor_set = set(minor)
    decided: dict[str, tuple[str, str]] = {}  # name -> (kind, reason)
    for name in changed:
        kind = "minor" if name in minor_set else "patch"
        decided[name] = (kind, "changed")

    breaking = [name for name, (kind, _) in decided.items() if kind == "minor"]
    for dependent in transitive_dependents(breaking, dependents):
        decided.setdefault(dependent, ("patch", "cascade"))

    actions: list[BumpAction] = []
    final_versions: dict[str, SemVer] = dict(current_versions)
    for name, (kind, reason) in sorted(decided.items()):
        baseline = baselines.get(name)
        if baseline is None:
            # Never released (no tag/crates.io anchor): the crate ships at its
            # current seed version, so there is nothing to supersede yet.
            continue
        target = bump(baseline, kind)
        current = current_versions[name]
        # Idempotent: only move forward, and never past an already-bumped manifest.
        if current >= target:
            continue
        actions.append(BumpAction(name=name, old=current, new=target, kind=kind, reason=reason))
        final_versions[name] = target

    floor_rewrites: list[tuple[str, SemVer]] = []
    for name, floor in sorted(current_floors.items()):
        final = final_versions.get(name)
        if final is None:
            continue
        if not within_caret(floor, final):
            floor_rewrites.append((name, final))

    return BumpPlan(actions=tuple(actions), floor_rewrites=tuple(floor_rewrites))

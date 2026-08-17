"""Release-time working-tree sync driven by Toven's bump ``on_resolved`` hook.

Toven owns the version bump, tagging, and publication (see ``docs/RELEASING.md``).
Its native version-reference sync keeps simple ``crate = "x.y.z"`` pins in
lock-step, but it is line-anchored and cannot rewrite rskit's install-snippet
pins that carry table attributes (``crate = { version = "x.y.z", features = [..] }``)
or column-aligned whitespace. During ``toven release bump`` the ``on_resolved``
hook runs :func:`sync_readme_versions`, handed the authoritative
``key -> version`` map Toven materializes, so every README install-snippet pin
is rewritten to the just-resolved versions and staged alongside the manifests.
"""

from __future__ import annotations

import argparse
import json
import re
from collections.abc import Mapping
from pathlib import Path

from ..errors import ToolError
from ..paths import ROOT

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


def add_parser(subparsers: argparse._SubParsersAction[argparse.ArgumentParser]) -> None:
    """Register the ``sync-readme-versions`` command."""

    parser = subparsers.add_parser(
        "sync-readme-versions",
        help="Rewrite README install-snippet version pins from Toven's resolved-version map",
    )
    parser.add_argument(
        "version_map",
        help="Path to the JSON key->version map Toven hands the bump on_resolved hook (argv-first)",
    )
    parser.set_defaults(func=run_sync_readme_versions)


def load_version_map(path: Path) -> dict[str, str]:
    """Load Toven's resolved ``key -> version`` JSON map.

    Toven keys each module by its canonical ``ecosystem:name`` identifier (e.g.
    ``rust:rskit-suite``). The raw keys are returned verbatim here;
    :func:`normalize_version_map` re-indexes them by the bare ``rskit-<name>``
    the README pins reference.
    """

    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise ToolError(f"error: version map not found: {path}") from error
    except json.JSONDecodeError as error:
        raise ToolError(f"error: version map is not valid JSON: {path}") from error
    if not isinstance(raw, dict) or not all(
        isinstance(key, str) and isinstance(value, str) for key, value in raw.items()
    ):
        raise ToolError(f"error: version map must be a JSON object of string versions: {path}")
    return raw


def normalize_version_map(raw: Mapping[str, str]) -> dict[str, str]:
    """Re-index Toven's resolved-version map by bare crate name.

    Toven keys each module by its canonical ``<ecosystem>:<name>`` identifier
    (e.g. ``rust:rskit-suite``) and may additionally emit bare aliases. README
    install-snippet pins reference the bare crate name (``rskit-suite``), so any
    leading ``<ecosystem>:`` segment is stripped and only ``rskit-`` crates are
    kept. rskit crate names are globally unique, so a bare key and a prefixed
    alias for the same crate must agree; a conflicting version for one bare name
    is a tooling error and aborts the sync rather than picking a version at
    random.
    """

    normalized: dict[str, str] = {}
    for key, version in raw.items():
        bare = key.rsplit(":", 1)[-1]
        if not bare.startswith("rskit-"):
            continue
        existing = normalized.get(bare)
        if existing is not None and existing != version:
            raise ToolError(
                f"error: conflicting versions for {bare} in version map: {existing} vs {version}"
            )
        normalized[bare] = version
    return normalized


def set_readme_dependency_versions(text: str, versions: Mapping[str, str]) -> tuple[str, bool]:
    """Rewrite tool-managed README dependency pins to authoritative versions.

    Every ``rskit-<name> = "..."`` (or ``{ version = "..." }``) pin whose crate is
    in ``versions`` is set to that crate's current version; pins for crates absent
    from the map are left untouched. Returns ``(text, changed)``.

    The version pin shown in a README install snippet is derived from the crate's
    real version — an output of the release, not hand-authored — so the bump keeps
    these in sync on every release instead of letting them drift.
    """

    def replace(match: re.Match[str]) -> str:
        target = versions.get(match["crate"])
        if target is None:
            return match[0]
        return f"{match['crate']}{match['lead']}{target}\""

    rewritten = _README_PIN_RE.sub(replace, text)
    return rewritten, rewritten != text


def source_readmes() -> list[Path]:
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


def sync_readme_versions(versions: Mapping[str, str]) -> list[Path]:
    """Rewrite stale dependency-pin versions in install-snippet READMEs.

    Rewrites each source README's pins to the versions in ``versions`` and writes
    only the files that changed. Returns the rewritten paths. Idempotent: a README
    already at the resolved versions is left byte-for-byte unchanged.
    """

    changed: list[Path] = []
    for readme in source_readmes():
        text = readme.read_text(encoding="utf-8")
        rewritten, was_changed = set_readme_dependency_versions(text, versions)
        if was_changed:
            readme.write_text(rewritten, encoding="utf-8")
            changed.append(readme)
    return changed


def run_sync_readme_versions(args: argparse.Namespace) -> int:
    """Sync README install-snippet pins from Toven's resolved-version map."""

    versions = normalize_version_map(load_version_map(Path(args.version_map)))
    changed = sync_readme_versions(versions)
    if changed:
        print(f"==> Synced README version pins in {len(changed)} file(s):")
        for path in changed:
            print(f"  {path.relative_to(ROOT)}")
    else:
        print("==> README version pins already in sync")
    return 0

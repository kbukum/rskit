"""Command-level tests for ``release bump`` orchestration.

Exercises ``run_bump`` / ``_detect_changed`` / ``_released_baselines`` against
fakes for git and crates.io, so the release-critical wiring (change detection,
``--minor`` validation, ``--offline`` behavior, floor-inheritor selection, and
soft-failing crates.io lookups) is covered without a network or a real tag.
"""

from __future__ import annotations

import argparse
import contextlib
import io
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from . import support  # noqa: F401
from rskit_tool import release_bump as rb
from rskit_tool.cargo import Package
from rskit_tool.errors import ToolError
from rskit_tool.versioning import SemVer


class FakeRegistry:
    """In-memory crates.io stand-in for baseline resolution."""

    def __init__(self, versions: dict[str, str] | None = None, *, fail: bool = False) -> None:
        self.versions = versions or {}
        self.fail = fail
        self.calls = 0

    def max_published_version(self, name: str) -> str | None:
        self.calls += 1
        if self.fail:
            raise ToolError(f"crates.io versions for {name} failed: HTTP 503")
        return self.versions.get(name)


def _pkg(root: Path, name: str, workspace: str, version: str = "0.1.0-alpha.1") -> Package:
    return Package(
        name=name,
        workspace=workspace,
        manifest_path=root / "Cargo.toml",
        root=root,
        version=version,
        publishable=True,
    )


def _args(**overrides: object) -> argparse.Namespace:
    base = {
        "workspace": "contrib",
        "minor": [],
        "base": None,
        "offline": False,
        "dry_run": True,
    }
    base.update(overrides)
    return argparse.Namespace(**base)


class DetectChangedTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name).resolve()
        (self.root / "contrib" / "storage" / "s3").mkdir(parents=True)
        self.ws_manifest = self.root / "contrib" / "Cargo.toml"
        self.crate_manifest = self.root / "contrib" / "storage" / "s3" / "Cargo.toml"
        self.crate_manifest.write_text(
            '[package]\nname = "rskit-storage-s3"\nversion = "0.1.0-alpha.1"\n\n'
            "[dependencies]\nrskit-errors.workspace = true\n",
            encoding="utf-8",
        )
        self.members = {"rskit-storage-s3": _pkg(self.crate_manifest.parent, "rskit-storage-s3", "contrib")}

    def tearDown(self) -> None:
        self._tmp.cleanup()

    @contextlib.contextmanager
    def _patched(self, *, changed: list[Path], at_ref: dict[str, str]):
        def fake_file_at_ref(_ref: str, relative: str) -> str | None:
            return at_ref.get(relative)

        with (
            mock.patch.object(rb, "ROOT", self.root),
            mock.patch.object(rb, "WORKSPACES", {"contrib": self.ws_manifest}),
            mock.patch.object(rb, "merge_base", return_value="MB"),
            mock.patch.object(rb, "changed_paths", return_value=changed),
            mock.patch.object(rb, "file_at_ref", side_effect=fake_file_at_ref),
        ):
            yield

    def test_inherited_floor_change_selects_crate(self) -> None:
        # Working-tree workspace manifest carries a bumped rskit-errors floor;
        # the base (tag) manifest still has the old floor.
        self.ws_manifest.write_text(
            "[workspace.dependencies]\n"
            'rskit-errors = { path = "../core/rskit-errors", version = "0.2.0-alpha.1" }\n',
            encoding="utf-8",
        )
        base_ws = (
            "[workspace.dependencies]\n"
            'rskit-errors = { path = "../core/rskit-errors", version = "0.1.0-alpha.1" }\n'
        )
        with self._patched(changed=[], at_ref={"contrib/Cargo.toml": base_ws}):
            selected = rb._detect_changed(self.members, "TAG", "contrib")
        self.assertEqual(selected, {"rskit-storage-s3"})

    def test_version_only_manifest_change_is_skipped(self) -> None:
        self.ws_manifest.write_text("[workspace.dependencies]\n", encoding="utf-8")
        base_crate = (
            '[package]\nname = "rskit-storage-s3"\nversion = "0.1.0-alpha.0"\n\n'
            "[dependencies]\nrskit-errors.workspace = true\n"
        )
        changed = [Path("contrib/storage/s3/Cargo.toml")]
        at_ref = {
            "contrib/Cargo.toml": "[workspace.dependencies]\n",
            "contrib/storage/s3/Cargo.toml": base_crate,
        }
        with self._patched(changed=changed, at_ref=at_ref):
            selected = rb._detect_changed(self.members, "TAG", "contrib")
        self.assertEqual(selected, set())

    def test_source_change_selects_crate(self) -> None:
        self.ws_manifest.write_text("[workspace.dependencies]\n", encoding="utf-8")
        changed = [Path("contrib/storage/s3/src/lib.rs")]
        with self._patched(changed=changed, at_ref={"contrib/Cargo.toml": "[workspace.dependencies]\n"}):
            selected = rb._detect_changed(self.members, "TAG", "contrib")
        self.assertEqual(selected, {"rskit-storage-s3"})


class ReleasedBaselinesTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name).resolve()
        crate_dir = self.root / "core" / "rskit-errors"
        crate_dir.mkdir(parents=True)
        self.manifest = crate_dir / "Cargo.toml"
        self.manifest.write_text(
            '[package]\nname = "rskit-errors"\nversion = "0.1.0-alpha.1"\n', encoding="utf-8"
        )
        self.members = {"rskit-errors": _pkg(crate_dir, "rskit-errors", "core")}

    def tearDown(self) -> None:
        self._tmp.cleanup()

    @contextlib.contextmanager
    def _patched(self) -> None:
        tag_manifest = '[package]\nname = "rskit-errors"\nversion = "0.1.0-alpha.1"\n'
        with (
            mock.patch.object(rb, "ROOT", self.root),
            mock.patch.object(rb, "file_at_ref", return_value=tag_manifest),
        ):
            yield

    def test_registry_failure_falls_back_to_tag_baseline(self) -> None:
        registry = FakeRegistry(fail=True)
        buffer = io.StringIO()
        with self._patched(), contextlib.redirect_stdout(buffer):
            baselines = rb._released_baselines(self.members, "TAG", registry=registry)
        self.assertEqual(baselines, {"rskit-errors": SemVer.parse("0.1.0-alpha.1")})
        self.assertEqual(registry.calls, 1)  # stops querying after the first failure
        self.assertIn("crates.io lookup failed", buffer.getvalue())

    def test_registry_max_supersedes_tag(self) -> None:
        registry = FakeRegistry({"rskit-errors": "0.3.0-alpha.1"})
        with self._patched():
            baselines = rb._released_baselines(self.members, "TAG", registry=registry)
        self.assertEqual(baselines, {"rskit-errors": SemVer.parse("0.3.0-alpha.1")})


class RunBumpTests(unittest.TestCase):
    def _graph(self) -> tuple[dict[str, Package], dict[str, set[str]]]:
        root = Path("/nonexistent/contrib/storage/s3")
        return {"rskit-storage-s3": _pkg(root, "rskit-storage-s3", "contrib")}, {}

    def test_unknown_minor_crate_raises(self) -> None:
        with (
            mock.patch.object(rb, "latest_tag", return_value="TAG"),
            mock.patch.object(rb, "_workspace_graph", return_value=self._graph()),
        ):
            with self.assertRaises(ToolError) as ctx:
                rb.run_bump(_args(minor=["rskit-missing"]))
        self.assertIn("rskit-missing", str(ctx.exception))

    def test_missing_tag_without_base_raises(self) -> None:
        with mock.patch.object(rb, "latest_tag", return_value=None):
            with self.assertRaises(ToolError):
                rb.run_bump(_args(base=None))

    def test_offline_does_not_construct_registry(self) -> None:
        with (
            mock.patch.object(rb, "latest_tag", return_value="TAG"),
            mock.patch.object(rb, "_workspace_graph", return_value=self._graph()),
            mock.patch.object(rb, "_detect_changed", return_value=set()),
            mock.patch.object(rb, "_released_baselines", return_value={}) as baselines,
            mock.patch.object(rb, "CratesIoRegistry") as registry_cls,
        ):
            with contextlib.redirect_stdout(io.StringIO()):
                rc = rb.run_bump(_args(offline=True))
        self.assertEqual(rc, 0)
        registry_cls.assert_not_called()
        self.assertIsNone(baselines.call_args.kwargs["registry"])

    def test_online_constructs_registry(self) -> None:
        with (
            mock.patch.object(rb, "latest_tag", return_value="TAG"),
            mock.patch.object(rb, "_workspace_graph", return_value=self._graph()),
            mock.patch.object(rb, "_detect_changed", return_value=set()),
            mock.patch.object(rb, "_released_baselines", return_value={}),
            mock.patch.object(rb, "CratesIoRegistry") as registry_cls,
        ):
            with contextlib.redirect_stdout(io.StringIO()):
                rb.run_bump(_args(offline=False))
        registry_cls.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()

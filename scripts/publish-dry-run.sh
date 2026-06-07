#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="${1:---dry-run}"
dirty_args=()
if [ -n "${CARGO_PACKAGE_DIRTY_FLAG:-}" ]; then
    dirty_args=("${CARGO_PACKAGE_DIRTY_FLAG}")
fi
case "$MODE" in
    --list) cargo_args=() ;;
    --dry-run) cargo_args=(publish --dry-run --locked "${dirty_args[@]}") ;;
    --publish) cargo_args=(publish --locked) ;;
    *)
        echo "usage: $0 [--list|--dry-run|--publish]" >&2
        exit 2
        ;;
esac

echo "==> Resolving publish order..."
order_file="$(mktemp)"
trap 'rm -f "$order_file"' EXIT
python3 - "$ROOT" > "$order_file" <<'PY'
import json
import pathlib
import subprocess
import sys
from collections import defaultdict

root = pathlib.Path(sys.argv[1]).resolve()
workspace_manifests = [root / "core" / "Cargo.toml", root / "contrib" / "Cargo.toml"]
packages: dict[str, dict[str, str]] = {}
edges: dict[str, set[str]] = defaultdict(set)
metadata_documents: list[dict] = []

for workspace_manifest in workspace_manifests:
    data = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--format-version=1",
                "--all-features",
                "--manifest-path",
                str(workspace_manifest),
            ],
            text=True,
        )
    )
    metadata_documents.append(data)
    workspace_ids = set(data["workspace_members"])
    for package in data["packages"]:
        manifest_path = pathlib.Path(package["manifest_path"]).resolve()
        if package["id"] not in workspace_ids:
            continue
        if package.get("publish") == []:
            continue
        if not (manifest_path.is_relative_to(root / "core") or manifest_path.is_relative_to(root / "contrib")):
            continue
        packages[package["id"]] = {
            "name": package["name"],
            "manifest": str(manifest_path),
        }

for data in metadata_documents:
    package_by_id = {package["id"]: package for package in data["packages"]}
    name_to_id = {package["name"]: package["id"] for package in data["packages"] if package["id"] in packages}
    for package_id, package in package_by_id.items():
        if package_id not in packages:
            continue
        for dep in package["dependencies"]:
            if dep["kind"] not in (None, "build"):
                continue
            dep_id = name_to_id.get(dep["name"])
            if dep_id in packages:
                edges[package_id].add(dep_id)
    for node in data["resolve"]["nodes"]:
        if node["id"] not in packages:
            continue
        for dep in node["deps"]:
            dep_id = dep["pkg"]
            if not any(kind["kind"] in (None, "build") for kind in dep.get("dep_kinds", [])):
                continue
            if dep_id in packages:
                edges[node["id"]].add(dep_id)

visited: set[str] = set()
visiting: set[str] = set()
ordered: list[str] = []

def visit(package_id: str) -> None:
    if package_id in visited:
        return
    if package_id in visiting:
        raise SystemExit(f"dependency cycle involving {packages[package_id]['name']}")
    visiting.add(package_id)
    for dep_id in sorted(edges[package_id], key=lambda item: packages[item]["name"]):
        visit(dep_id)
    visiting.remove(package_id)
    visited.add(package_id)
    ordered.append(package_id)

for package_id in sorted(packages, key=lambda item: packages[item]["name"]):
    visit(package_id)

ordered.sort(key=lambda package_id: packages[package_id]["name"] == "rskit")

for package_id in ordered:
    package = packages[package_id]
    print(f"{package['name']}\t{package['manifest']}")
PY

while IFS=$'\t' read -r name manifest; do
    [ -n "$name" ] || continue
    if [ "$MODE" = "--list" ]; then
        printf '%s\t%s\n' "$name" "$manifest"
        continue
    fi
    echo "==> cargo ${cargo_args[*]} ${name}"
    cargo "${cargo_args[@]}" --manifest-path "$manifest"
done < "$order_file"

echo "✓ Cargo publish ${MODE#--} completed"

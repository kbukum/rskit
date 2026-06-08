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
order_file="$(mktemp "${TMPDIR:-/tmp}/rskit-publish-order.XXXXXX")"
trap 'rm -f "$order_file"' EXIT

crate_version_published() {
    local crate=$1
    local version=$2

    python3 - "$crate" "$version" <<'PY'
import json
import sys
import urllib.error
import urllib.request

crate = sys.argv[1]
version = sys.argv[2]
url = f"https://crates.io/api/v1/crates/{crate}/{version}"
request = urllib.request.Request(url, headers={"User-Agent": "rskit-release-rehearsal"})
try:
    with urllib.request.urlopen(request, timeout=10) as response:
        data = json.load(response)
except urllib.error.HTTPError as error:
    if error.code == 404:
        raise SystemExit(1)
    print(f"warning: crates.io lookup for {crate} {version} failed: HTTP {error.code}", file=sys.stderr)
    raise SystemExit(2)
except Exception as error:
    print(f"warning: crates.io lookup for {crate} {version} failed: {error}", file=sys.stderr)
    raise SystemExit(2)

published = data.get("version", {}).get("num") == version
raise SystemExit(0 if published else 1)
PY
}

notice() {
    local message=$1
    if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
        echo "::notice title=Publish dry-run skipped::${message}"
    else
        echo "notice: ${message}"
    fi
}

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

def is_relative_to(path: pathlib.Path, parent: pathlib.Path) -> bool:
    try:
        path.relative_to(parent)
    except ValueError:
        return False
    return True

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
        if not (
            is_relative_to(manifest_path, root / "core")
            or is_relative_to(manifest_path, root / "contrib")
        ):
            continue
        packages[package["id"]] = {
            "name": package["name"],
            "manifest": str(manifest_path),
            "version": package["version"],
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
    internal_deps = ",".join(
        f"{packages[dep_id]['name']}@{packages[dep_id]['version']}"
        for dep_id in sorted(edges[package_id], key=lambda item: packages[item]["name"])
    )
    print(f"{package['name']}\t{package['manifest']}\t{package['version']}\t{internal_deps}")
PY

skipped=0
while IFS=$'\t' read -r name manifest version internal_deps; do
    [ -n "$name" ] || continue
    if [ "$MODE" = "--list" ]; then
        printf '%s\t%s\n' "$name" "$manifest"
        continue
    fi
    if [ "$MODE" = "--dry-run" ] && [ -n "$internal_deps" ]; then
        blocked=()
        IFS=',' read -r -a deps <<< "$internal_deps"
        for dep_entry in "${deps[@]}"; do
            [ -n "$dep_entry" ] || continue
            dep_name=${dep_entry%@*}
            dep_version=${dep_entry##*@}
            if [ -z "$dep_name" ] || [ -z "$dep_version" ] || [ "$dep_name" = "$dep_version" ]; then
                echo "error: invalid internal dependency entry: ${dep_entry}" >&2
                exit 1
            fi
            set +e
            crate_version_published "$dep_name" "$dep_version"
            status=$?
            set -e
            case "$status" in
                0) ;;
                1) blocked+=("$dep_entry") ;;
                *)
                    echo "error: failed to verify crates.io publication status for ${dep_entry}" >&2
                    exit "$status"
                    ;;
            esac
        done
        if [ "${#blocked[@]}" -gt 0 ]; then
            skipped=$((skipped + 1))
            joined=$(IFS=','; echo "${blocked[*]}")
            notice "${name} ${version} depends on unpublished internal crate(s): ${joined}. cargo publish --dry-run cannot fully validate this crate until those same-version dependencies exist on crates.io; running package-list sanity check instead."
            cargo package --locked --list "${dirty_args[@]}" --manifest-path "$manifest" >/dev/null
            continue
        fi
    fi
    echo "==> cargo ${cargo_args[*]} ${name}"
    cargo "${cargo_args[@]}" --manifest-path "$manifest"
done < "$order_file"

if [ "$MODE" = "--dry-run" ] && [ "$skipped" -gt 0 ]; then
    echo "warning: ${skipped} crate(s) were package-listed but not cargo publish --dry-run validated because their same-version internal dependencies are not on crates.io yet."
fi
echo "✓ Cargo publish ${MODE#--} completed"

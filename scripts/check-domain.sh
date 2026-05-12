#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ -n "${PYTHON:-}" ]]; then
  python_bin="$PYTHON"
elif command -v python3.14 >/dev/null 2>&1; then
  python_bin="python3.14"
elif command -v python3.13 >/dev/null 2>&1; then
  python_bin="python3.13"
elif command -v python3.12 >/dev/null 2>&1; then
  python_bin="python3.12"
elif command -v python3.11 >/dev/null 2>&1; then
  python_bin="python3.11"
else
  python_bin="python3"
fi

if ! command -v "$python_bin" >/dev/null 2>&1; then
  echo "Python 3.11+ is required but '$python_bin' was not found" >&2
  exit 1
fi

require_python() {
  "$python_bin" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 11) else "Python 3.11+ is required")'
}

list_domains() {
  "$python_bin" -c 'import tomllib
with open("domains.toml", "rb") as f:
    data = tomllib.load(f)
for name in sorted(data.get("domains", {})):
    print(name)'
}

read_modules() {
  local domain="$1"
  "$python_bin" -c 'import sys, tomllib
with open("domains.toml", "rb") as f:
    data = tomllib.load(f)
domains = data.get("domains", {})
name = sys.argv[1]
entry = domains.get(name)
if entry is None:
    print(f"Unknown domain: {name}", file=sys.stderr)
    print("Available domains:", file=sys.stderr)
    for item in sorted(domains):
        print(f"  {item}", file=sys.stderr)
    raise SystemExit(1)
for module in entry["modules"]:
    print(module)' "$domain"
}

resolve_crate_name() {
  local module="$1"
  case "$module" in
    rskit)
      if [[ -d core/rskit ]]; then
        printf '%s\n' "rskit"
        return 0
      fi
      ;;
    logger|logging)
      if [[ -d core/rskit-logging ]]; then
        printf '%s\n' "rskit-logging"
        return 0
      fi
      ;;
  esac

  if [[ -d "core/rskit-$module" ]]; then
    printf '%s\n' "rskit-$module"
    return 0
  fi

  if find contrib -mindepth 2 -maxdepth 3 -name Cargo.toml -exec grep -lE "^name\s*=\s*\"rskit-${module}\"" {} + 2>/dev/null | grep -q .; then
    printf '%s\n' "rskit-$module"
    return 0
  fi

  printf '%s\n' "rskit-$module"
}

resolve_manifest_path() {
  local crate="$1"
  if cargo metadata --manifest-path core/Cargo.toml --no-deps --format-version 1 2>/dev/null | grep -q "\"name\":\"$crate\""; then
    printf '%s\n' "core/Cargo.toml"
    return 0
  fi

  if cargo metadata --manifest-path contrib/Cargo.toml --no-deps --format-version 1 2>/dev/null | grep -q "\"name\":\"$crate\""; then
    printf '%s\n' "contrib/Cargo.toml"
    return 0
  fi

  return 1
}

run_module_checks() {
  local module="$1"
  local crate manifest
  crate="$(resolve_crate_name "$module")"

  if ! manifest="$(resolve_manifest_path "$crate")"; then
    echo "Unable to resolve crate for '$module' (expected in core/ or contrib/)" >&2
    return 1
  fi

  echo "==> Checking $module ($crate)"
  cargo clippy --manifest-path "$manifest" -p "$crate" -- -D warnings
  if command -v cargo-nextest >/dev/null 2>&1; then
    cargo nextest run --manifest-path "$manifest" -p "$crate"
  else
    cargo test --manifest-path "$manifest" -p "$crate"
  fi
}

run_domain() {
  local domain="$1"
  local modules_output
  local -a modules=()
  local module

  modules_output="$(read_modules "$domain")"
  if [[ -n "$modules_output" ]]; then
    while IFS= read -r module; do
      [[ -n "$module" ]] || continue
      modules+=("$module")
    done <<< "$modules_output"
  fi

  echo "==> Domain: $domain"
  for module in "${modules[@]}"; do
    run_module_checks "$module"
  done
}

main() {
  require_python

  if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <domain|--list|--all>" >&2
    exit 1
  fi

  case "$1" in
    --list)
      list_domains
      ;;
    --all)
      local domains_output
      local -a domains=()
      local domain
      domains_output="$(list_domains)"
      if [[ -n "$domains_output" ]]; then
        while IFS= read -r domain; do
          [[ -n "$domain" ]] || continue
          domains+=("$domain")
        done <<< "$domains_output"
      fi
      for domain in "${domains[@]}"; do
        run_domain "$domain"
      done
      ;;
    *)
      run_domain "$1"
      ;;
  esac
}

main "$@"

#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYTHON_BIN="${PYTHON:-python3}"
TOVEN_BIN="${TOVEN:-toven}"
INSTALL_SYSTEM_TOOLS="${INSTALL_SYSTEM_TOOLS:-0}"
INSTALL_RELEASE_TOOLS="${INSTALL_RELEASE_TOOLS:-0}"
CHECK_ONLY=0
RUST_TOOLCHAIN=""

usage() {
  cat <<'EOF'
Usage: scripts/setup.sh [--check-only] [--system] [--release]

Installs or verifies local rskit development tooling:
  - Python 3.11+ for scripts/rskit_tool.py
  - pinned Rust toolchain, clippy, rustfmt, llvm-tools-preview
  - nightly rustdoc JSON toolchain for cargo-public-api
  - Cargo tools used by Make/release checks
  - the pinned Toven binary that drives Make guardrail/structure and release tasks

Options:
  --check-only  Verify tools without installing missing Cargo/Rust tools.
  --system      Also try to install system tools via apt-get or Homebrew.
  --release     Include release-only tool checks such as cosign and gh.

Environment:
  PYTHON=<bin>                 Python binary to validate (default: python3)
  TOVEN=<bin>                  Toven binary to verify (default: toven)
  INSTALL_SYSTEM_TOOLS=1       Same as --system
  INSTALL_RELEASE_TOOLS=1      Same as --release
  CARGO_PUBLIC_API_VERSION=... cargo-public-api version (default: 0.52.0)
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --check-only) CHECK_ONLY=1 ;;
    --system) INSTALL_SYSTEM_TOOLS=1 ;;
    --release) INSTALL_RELEASE_TOOLS=1 ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown setup option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

need_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "error: required command not found: $name" >&2
    return 1
  fi
}

ensure_python() {
  echo "==> Checking Python runtime..."
  need_command "$PYTHON_BIN"
  "$PYTHON_BIN" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 11) else "Python 3.11+ is required")'
}

load_rust_toolchain() {
  RUST_TOOLCHAIN="$("$PYTHON_BIN" - "$ROOT/rust-toolchain.toml" <<'PY'
import sys
import tomllib

with open(sys.argv[1], "rb") as handle:
    channel = tomllib.load(handle).get("toolchain", {}).get("channel")
if not isinstance(channel, str) or not channel:
    raise SystemExit("rust-toolchain.toml must define toolchain.channel")
print(channel)
PY
)"
}

ensure_rust_toolchains() {
  echo "==> Checking Rust toolchains..."
  need_command rustup
  need_command cargo
  if [ "$CHECK_ONLY" -eq 1 ]; then
    rustup toolchain list | awk '{print $1}' | grep -F -q "$RUST_TOOLCHAIN" || {
      echo "error: Rust toolchain $RUST_TOOLCHAIN is not installed" >&2
      return 1
    }
    rustup toolchain list | grep -q '^nightly' || {
      echo "error: nightly toolchain is not installed" >&2
      return 1
    }
    return 0
  fi
  rustup toolchain install "$RUST_TOOLCHAIN" --profile minimal --component rustfmt --component clippy --component llvm-tools-preview
  rustup toolchain install nightly --profile minimal
}

ensure_cargo_tool() {
  local binary="$1"
  local crate="$2"
  shift 2

  if command -v "$binary" >/dev/null 2>&1; then
    echo "✓ $binary"
    return 0
  fi

  if [ "$CHECK_ONLY" -eq 1 ]; then
    echo "error: missing Cargo tool: $binary" >&2
    return 1
  fi

  echo "==> Installing $crate..."
  (cd "$ROOT" && cargo install "$crate" "$@")
}

ensure_cargo_tools() {
  echo "==> Checking Cargo tools..."
  ensure_cargo_tool cargo-nextest cargo-nextest --locked
  ensure_cargo_tool cargo-deny cargo-deny --locked
  ensure_cargo_tool cargo-audit cargo-audit --locked
  ensure_cargo_tool cargo-llvm-cov cargo-llvm-cov --locked
  ensure_cargo_tool cargo-cyclonedx cargo-cyclonedx --locked
  ensure_cargo_tool cargo-depgraph cargo-depgraph --locked
  ensure_cargo_tool cargo-public-api cargo-public-api --version "${CARGO_PUBLIC_API_VERSION:-0.52.0}" --locked
}

install_system_tools() {
  if [ "$INSTALL_SYSTEM_TOOLS" != "1" ]; then
    return 0
  fi

  echo "==> Installing system tools..."
  case "$(uname -s)" in
    Linux)
      if command -v apt-get >/dev/null 2>&1; then
        need_command sudo
        sudo apt-get update
        sudo apt-get install -y clang mold graphviz
      else
        echo "warning: unsupported Linux package manager; install clang, mold, and graphviz manually" >&2
      fi
      ;;
    Darwin)
      if command -v brew >/dev/null 2>&1; then
        brew install graphviz
      else
        echo "warning: Homebrew not found; install graphviz manually if you need depgraphs" >&2
      fi
      ;;
    *)
      echo "warning: unsupported OS for automatic system tool installation" >&2
      ;;
  esac
}

check_system_tools() {
  echo "==> Checking system tools..."
  case "$(uname -s)" in
    Linux)
      command -v clang >/dev/null 2>&1 || echo "warning: clang not found; Linux mold linker setup may fail" >&2
      command -v mold >/dev/null 2>&1 || echo "warning: mold not found; install with --system or your package manager" >&2
      ;;
  esac
  command -v dot >/dev/null 2>&1 || echo "warning: dot (Graphviz) not found; release depgraphs need it" >&2
}

ensure_toven() {
  echo "==> Checking Toven..."
  if command -v "$TOVEN_BIN" >/dev/null 2>&1; then
    echo "✓ $TOVEN_BIN ($("$TOVEN_BIN" --version 2>/dev/null || echo 'version unknown'))"
    return 0
  fi
  echo "error: Toven not found on PATH (looked for '$TOVEN_BIN')." >&2
  echo "  Toven drives 'make check' guardrail/structure tasks and the release." >&2
  echo "  Install the pinned binary from https://github.com/kbukum/toven" >&2
  echo "  (curl … scripts/install.sh | sh) or set TOVEN=<path> for the Make targets." >&2
  return 1
}

check_release_tools() {

  echo "==> Checking release tools..."
  command -v gh >/dev/null 2>&1 || echo "warning: gh not found; install GitHub CLI for release asset operations" >&2
  if ! command -v cosign >/dev/null 2>&1; then
    if [ "$CHECK_ONLY" -eq 1 ]; then
      echo "error: cosign not found" >&2
      return 1
    fi
    if command -v go >/dev/null 2>&1; then
      go install github.com/sigstore/cosign/v2/cmd/cosign@v2.6.1
      if ! command -v cosign >/dev/null 2>&1; then
        local go_bin
        go_bin="$(go env GOBIN)"
        if [ -z "$go_bin" ]; then
          go_bin="$(go env GOPATH)/bin"
        fi
        echo "error: cosign installed but is not on PATH; add $go_bin to PATH" >&2
        return 1
      fi
    else
      echo "error: cosign not found and Go is unavailable; install cosign manually" >&2
      return 1
    fi
  fi
}

ensure_python
load_rust_toolchain
ensure_rust_toolchains
ensure_cargo_tools
ensure_toven
"$PYTHON_BIN" "$ROOT/scripts/rskit_tool.py" self-test
install_system_tools
check_system_tools
check_release_tools

echo "✓ rskit local tooling setup complete"

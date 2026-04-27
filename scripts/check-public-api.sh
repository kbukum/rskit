#!/usr/bin/env bash
# Check for breaking public API changes using cargo-public-api.
# Install: cargo install cargo-public-api
# Usage: ./scripts/check-public-api.sh [crate-name]
set -euo pipefail

CRATE=${1:-rskit}
echo "Checking public API for $CRATE..."
cargo public-api -p "$CRATE" diff

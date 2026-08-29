#!/usr/bin/env bash
# Retry a command with exponential backoff (local dev + CI).
#
# Wraps a command that reaches a flaky external service — such as the Toven
# canary's crates.io registry queries — so a transient network fault (SSL reset,
# connection drop, 5xx) is retried with delay instead of failing the job on the
# first miss. It changes no behaviour on success and preserves the command's
# final exit code when every attempt is exhausted.
#
# Usage:
#   scripts/retry.sh <command> [args...]
#
# Tunables (environment):
#   RETRY_ATTEMPTS   total attempts before giving up          (default 5)
#   RETRY_DELAY      initial backoff in seconds               (default 5)
#   RETRY_MAX_DELAY  cap on the per-attempt backoff in seconds (default 60)
set -euo pipefail

attempts="${RETRY_ATTEMPTS:-5}"
delay="${RETRY_DELAY:-5}"
max_delay="${RETRY_MAX_DELAY:-60}"

if [ "$#" -eq 0 ]; then
  echo "retry: no command given" >&2
  exit 2
fi

attempt=1
while true; do
  if "$@"; then
    exit 0
  else
    status=$?
  fi

  if [ "${attempt}" -ge "${attempts}" ]; then
    echo "retry: '$*' failed after ${attempt} attempt(s) (exit ${status})" >&2
    exit "${status}"
  fi

  echo "retry: '$*' failed (exit ${status}); attempt ${attempt}/${attempts}, retrying in ${delay}s..." >&2
  sleep "${delay}"
  attempt=$((attempt + 1))
  delay=$((delay * 2))
  if [ "${delay}" -gt "${max_delay}" ]; then
    delay="${max_delay}"
  fi
done

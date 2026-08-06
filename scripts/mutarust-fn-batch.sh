#!/usr/bin/env bash
# Memory-safe single-function mutarust batch for 16 GB hosts.
set -euo pipefail

MIN_FREE_MB="${MIN_FREE_MB:-1800}"
MATCH="${1:?usage: mutarust-fn-batch.sh <function_name> [extra mutarust args...]}"
shift || true

free_mb() {
  local pages
  pages=$(vm_stat | awk '/Pages free/ {gsub("\\.","",$3); print $3}')
  echo $((pages * 16384 / 1024 / 1024))
}

cleanup() {
  pkill -9 -x mutarust 2>/dev/null || true
  pkill -9 -x rustc 2>/dev/null || true
  # Do not pkill all cargo — may kill unrelated jobs; only children die with mutarust.
  rm -rf "${TMPDIR:-/tmp}"/mutarust-* 2>/dev/null || true
}

free=$(free_mb)
if (( free < MIN_FREE_MB )); then
  echo "ABORT: free_mb=${free} < MIN_FREE_MB=${MIN_FREE_MB}" >&2
  exit 2
fi

export TMPDIR="${TMPDIR:-$HOME/tmp/mutarust-run}"
mkdir -p "$TMPDIR"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

LOG="${LOG:-/tmp/mutarust-fn-${MATCH}.log}"
rm -f "$LOG"

echo "START match=${MATCH} free_mb=${free} TMPDIR=${TMPDIR}"

# Watchdog: kill mutarust if free RAM collapses mid-run.
(
  while pgrep -x mutarust >/dev/null 2>&1; do
    f=$(free_mb)
    if (( f < 400 )); then
      echo "WATCHDOG free_mb=${f} — killing mutarust" >&2
      pkill -9 -x mutarust 2>/dev/null || true
      pkill -9 -x rustc 2>/dev/null || true
      exit 3
    fi
    sleep 5
  done
) &
WD_PID=$!

set +e
script -q "$LOG" env TMPDIR="$TMPDIR" CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS" mutarust \
  --config mutarust.yml \
  --workers 1 \
  --exec-timeout 90 \
  --min-msi 0 \
  --min-covered-msi 0 \
  --test-flags "--test cli --lib" \
  --match "$MATCH" \
  "$@" \
  src/lib.rs src/main.rs
rc=$?
set -e

kill "$WD_PID" 2>/dev/null || true
wait "$WD_PID" 2>/dev/null || true

cleanup

echo "==== summary match=${MATCH} rc=${rc} free_after=$(free_mb) ===="
tr '\r' '\n' < "$LOG" | rg 'Killed:|Escaped:|Errored:|Not covered:|Skipped:|Total:|Mutation score|Covered-code|ABORT|WATCHDOG' | tail -20
exit "$rc"

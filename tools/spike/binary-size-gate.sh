#!/usr/bin/env bash
# spike-binary-size-gate — measure Rust release binary size against a C baseline
# and warn/fail if the ratio exceeds a threshold.
#
# Usage:
#   tools/spike/binary-size-gate.sh <rust-bin-path> <c-bin-path> [max-ratio]
#
# Example:
#   tools/spike/binary-size-gate.sh \
#       noricum-spike-miniz/target/release-min/rbench \
#       /tmp/cbench_min \
#       3.5
#
# Defaults:
#   max-ratio = 3.5 (Rust binary can be up to 3.5x the C baseline)
#
# Exits:
#   0 — within budget
#   1 — Rust binary doesn't exist
#   2 — C binary doesn't exist
#   3 — ratio exceeds threshold (gate failure)
#
# CI integration: add this as a step in the spike branch's CI workflow.
# Any time the Rust binary grows past the ratio, the build fails and the
# PR gets a review gate. Forces the dependency budget conversation to
# happen explicitly in every PR that touches the spike.

set -euo pipefail

RUST_BIN="${1:-}"
C_BIN="${2:-}"
MAX_RATIO="${3:-3.5}"

if [ -z "$RUST_BIN" ] || [ -z "$C_BIN" ]; then
    echo "Usage: $0 <rust-bin-path> <c-bin-path> [max-ratio]" >&2
    exit 1
fi

if [ ! -f "$RUST_BIN" ]; then
    echo "ERROR: Rust binary not found: $RUST_BIN" >&2
    echo "Run 'cargo build --profile release-min' first." >&2
    exit 1
fi

if [ ! -f "$C_BIN" ]; then
    echo "ERROR: C baseline not found: $C_BIN" >&2
    echo "Compile a C equivalent first (see tools/spike/scaffold.sh wrapper_smoke.c)." >&2
    exit 2
fi

RUST_SIZE=$(stat -c %s "$RUST_BIN")
C_SIZE=$(stat -c %s "$C_BIN")
RATIO=$(awk "BEGIN {printf \"%.2f\", $RUST_SIZE/$C_SIZE}")
RATIO_INT=$(awk "BEGIN {printf \"%d\", ($RUST_SIZE*100)/$C_SIZE}")
MAX_RATIO_INT=$(awk "BEGIN {printf \"%d\", $MAX_RATIO*100}")

printf "%-35s %12d bytes\n" "C baseline:" "$C_SIZE"
printf "%-35s %12d bytes\n" "Rust release-min:" "$RUST_SIZE"
printf "%-35s %12s\n" "Ratio:" "${RATIO}x"
printf "%-35s %12s\n" "Max allowed:" "${MAX_RATIO}x"

if [ "$RATIO_INT" -gt "$MAX_RATIO_INT" ]; then
    echo ""
    echo "*** GATE FAILURE ***"
    echo "Rust binary is ${RATIO}x the C baseline (max allowed ${MAX_RATIO}x)."
    echo ""
    echo "Options:"
    echo "  1. Accept the ratio — bump the threshold in CI config"
    echo "     (valid if the spike is Mode-1 FREE and size is not a constraint)"
    echo "  2. Switch to Mode-2 MATCHING or Mode-3 ZERO"
    echo "     (vendor primitives instead of depending on crates)"
    echo "  3. Feature-gate large optional subsystems (AES, compression variants)"
    echo "  4. Enable 'release-min' profile if not already"
    echo ""
    echo "See docs/methodology/dependency-budget.md for the three modes."
    exit 3
fi

echo ""
echo "PASS: Rust binary is within ${MAX_RATIO}x of the C baseline."

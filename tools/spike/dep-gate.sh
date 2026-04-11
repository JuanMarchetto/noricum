#!/usr/bin/env bash
# spike-dep-gate — enforce the chosen dependency budget mode for a spike.
#
# Reads the dep budget mode from a file in the spike directory
# (SPIKE.md's "Dependency budget" section, parsed loosely) and fails if
# the current Cargo.toml deps violate the mode.
#
# Usage:
#   tools/spike/dep-gate.sh <spike-dir>
#
# Modes:
#   FREE     — any deps allowed; just warn on >10 transitive deps
#   MATCHING — max N runtime deps where N = dep count of the C original
#              (user must set MATCHING_MAX in the spike dir or as env var)
#   ZERO     — cargo tree must show only the spike crate itself; any runtime
#              dep is a failure
#
# Exit:
#   0 — within budget
#   1 — mode detection failed
#   2 — budget violation

set -euo pipefail

SPIKE_DIR="${1:-}"
if [ -z "$SPIKE_DIR" ] || [ ! -d "$SPIKE_DIR" ]; then
    echo "Usage: $0 <spike-dir>" >&2
    exit 1
fi

cd "$SPIKE_DIR"

# Detect mode from SPIKE.md — look for a checked [x] line.
MODE="unknown"
if [ -f SPIKE.md ]; then
    if grep -qE '^- \[x\] \*\*Mode 1 — FREE' SPIKE.md; then
        MODE="FREE"
    elif grep -qE '^- \[x\] \*\*Mode 2 — MATCHING' SPIKE.md; then
        MODE="MATCHING"
    elif grep -qE '^- \[x\] \*\*Mode 3 — ZERO' SPIKE.md; then
        MODE="ZERO"
    fi
fi
# Env var override
MODE="${SPIKE_DEP_MODE:-$MODE}"

if [ "$MODE" = "unknown" ]; then
    echo "ERROR: dependency budget mode not set." >&2
    echo "Either check a box in SPIKE.md under 'Dependency budget' or set SPIKE_DEP_MODE=FREE|MATCHING|ZERO" >&2
    exit 1
fi

echo "Dep budget mode: $MODE"

# Count runtime deps from Cargo.toml (lines matching <name> = ... outside [dev-dependencies]).
# Not perfect but good enough for a gate.
RUNTIME_DEPS=$(awk '
    /^\[dependencies\]/ { in_dep=1; next }
    /^\[.*\]/ { in_dep=0 }
    in_dep && /^[a-zA-Z0-9_-]+ ?=/ && !/^#/ { count++ }
    END { print count + 0 }
' Cargo.toml)
echo "Runtime deps in Cargo.toml: $RUNTIME_DEPS"

# Count transitive via cargo tree (if cargo is available).
TRANSITIVE_COUNT=""
if command -v cargo >/dev/null 2>&1; then
    TRANSITIVE_COUNT=$(cargo tree --edges=normal 2>/dev/null | grep -cE '^[├└│ ]+[^ ]' || echo "?")
    echo "Transitive deps from cargo tree: $TRANSITIVE_COUNT"
fi

case "$MODE" in
    FREE)
        # Just warn on >10 transitive.
        if [ -n "$TRANSITIVE_COUNT" ] && [ "$TRANSITIVE_COUNT" != "?" ] && [ "$TRANSITIVE_COUNT" -gt 10 ]; then
            echo "WARN: $TRANSITIVE_COUNT transitive deps — consider whether each is justified."
        fi
        echo "PASS (FREE mode, no hard limits)"
        ;;
    MATCHING)
        MAX="${MATCHING_MAX:-2}"
        if [ "$RUNTIME_DEPS" -gt "$MAX" ]; then
            echo ""
            echo "*** GATE FAILURE ***"
            echo "MATCHING mode allows max $MAX runtime deps (matching the C original's dep count)."
            echo "Current: $RUNTIME_DEPS"
            echo ""
            echo "Options:"
            echo "  1. Remove a dep and vendor its functionality as source"
            echo "  2. Argue that the C lib actually has more deps than $MAX — update MATCHING_MAX"
            echo "  3. Switch to FREE mode if dep count is not a constraint"
            echo ""
            echo "See docs/methodology/dependency-budget.md"
            exit 2
        fi
        echo "PASS (MATCHING mode, $RUNTIME_DEPS <= $MAX runtime deps)"
        ;;
    ZERO)
        if [ "$RUNTIME_DEPS" -gt 0 ]; then
            echo ""
            echo "*** GATE FAILURE ***"
            echo "ZERO mode requires exactly 0 runtime deps. Current: $RUNTIME_DEPS."
            echo ""
            echo "Every algorithm must be implemented in-tree or vendored as source."
            echo "Remove all entries from [dependencies] in Cargo.toml."
            echo ""
            echo "See docs/methodology/dependency-budget.md"
            exit 2
        fi
        echo "PASS (ZERO mode, no runtime deps)"
        ;;
esac

#!/usr/bin/env bash
# Batch behavioral equivalence review for all 14 C↔Rust migration pairs.
# Usage: bash reviews/run-behavioral-review.sh
set -euo pipefail
cd "$(dirname "$0")/.."

# Export API key from .env
if [ -f .env ]; then
    export "$(grep '^ANTHROPIC_API_KEY=' .env | head -1)"
fi

if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo "ERROR: ANTHROPIC_API_KEY not set. Add it to .env or export it."
    exit 1
fi

OUTDIR="reviews/behavioral"
mkdir -p "$OUTDIR"

# Define the 14 pairs: "name|c_source|rust_source"
PAIRS=(
    "add|tests/fixtures/simple/add.c|output/simple/add.rs"
    "buffer|tests/fixtures/simple/buffer.c|output/simple/buffer.rs"
    "error_codes|tests/fixtures/simple/error_codes.c|output/simple/error_codes.rs"
    "factorial|tests/fixtures/simple/factorial.c|output/simple/factorial.rs"
    "fibonacci|tests/fixtures/simple/fibonacci.c|output/simple/fibonacci.rs"
    "gcd|tests/fixtures/simple/gcd.c|output/simple/gcd.rs"
    "linked_list|tests/fixtures/simple/linked_list.c|output/simple/linked_list.rs"
    "max_min|tests/fixtures/simple/max_min.c|output/simple/max_min.rs"
    "power|tests/fixtures/simple/power.c|output/simple/power.rs"
    "strlen|tests/fixtures/simple/strlen.c|output/simple/strlen.rs"
    "hash_table|tests/fixtures/medium/hash_table.c|output/medium/hash_table.rs"
    "miniz_test|tests/fixtures/miniz/miniz_test.c|output/miniz/miniz_test.rs"
    "cjson_combined|tests/fixtures/cjson/cjson_combined.c|output/cjson/cjson_combined.rs"
    "expr_eval|tests/fixtures/large/expr_eval.c|output/large/expr_eval.rs"
)

TOTAL=${#PAIRS[@]}
PASSED=0
FAILED=0
SKIPPED=0

echo "=== Noricum Behavioral Equivalence Review ==="
echo "    Pairs: $TOTAL"
echo "    Output: $OUTDIR/"
echo ""

for i in "${!PAIRS[@]}"; do
    IFS='|' read -r NAME C_SRC RS_SRC <<< "${PAIRS[$i]}"
    NUM=$((i + 1))

    echo -n "[$NUM/$TOTAL] $NAME ... "

    # Check files exist
    if [ ! -f "$C_SRC" ]; then
        echo "SKIP (C source not found: $C_SRC)"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi
    if [ ! -f "$RS_SRC" ]; then
        echo "SKIP (Rust source not found: $RS_SRC)"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi

    OUTFILE="$OUTDIR/${NAME}.json"

    if cargo run -p noricum-cli --quiet -- review \
        --c-source "$C_SRC" \
        --rust-source "$RS_SRC" \
        --diff-test --json \
        > "$OUTFILE" 2>"$OUTDIR/${NAME}-stderr.log"; then

        VERDICT=$(python3 -c "import json,sys; d=json.load(open('$OUTFILE')); print(d.get('verdict','?'))" 2>/dev/null || echo "?")
        CONF=$(python3 -c "import json,sys; d=json.load(open('$OUTFILE')); print(d.get('confidence','?'))" 2>/dev/null || echo "?")
        echo "OK  verdict=$VERDICT  confidence=${CONF}%"
        PASSED=$((PASSED + 1))
    else
        echo "FAIL (see $OUTDIR/${NAME}-stderr.log)"
        FAILED=$((FAILED + 1))
    fi
done

echo ""
echo "=== Summary ==="
echo "  Completed: $PASSED / $TOTAL"
echo "  Failed:    $FAILED"
echo "  Skipped:   $SKIPPED"

if [ "$FAILED" -gt 0 ]; then
    exit 1
fi

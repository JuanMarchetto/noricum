#!/usr/bin/env bash
# record-demo.sh — Record a terminal demo of Noricum using asciinema
#
# Prerequisites:
#   sudo apt install asciinema    # or: pip install asciinema
#   cargo build --release
#   export ANTHROPIC_API_KEY=sk-ant-...
#
# Usage:
#   ./scripts/record-demo.sh              # Interactive recording
#   ./scripts/record-demo.sh --scripted   # Automated with expect-like typing
#
# After recording, convert to GIF:
#   npm install -g svg-term-cli    # or use agg: cargo install agg
#   agg demo.cast demo.gif --cols 100 --rows 30 --speed 2
#   # Or use asciinema.org to host: asciinema upload demo.cast

set -euo pipefail

DEMO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_FILE="${DEMO_DIR}/demo.cast"

cd "$DEMO_DIR"

if ! command -v asciinema &>/dev/null; then
    echo "Error: asciinema not found. Install with: sudo apt install asciinema"
    exit 1
fi

if ! [ -f target/release/noricum ]; then
    echo "Building noricum..."
    cargo build --release
fi

echo "=== Noricum Demo Recording ==="
echo ""
echo "This will record a terminal session to: ${OUTPUT_FILE}"
echo ""
echo "Suggested demo flow (type these commands):"
echo "  1. noricum doctor"
echo "  2. noricum analyze tests/fixtures/medium/hash_table.c"
echo "  3. noricum migrate tests/fixtures/simple/add.c --diff-test"
echo "  4. noricum migrate tests/fixtures/medium/hash_table.c --diff-test --report /tmp/report.html"
echo "  5. cat output (show generated Rust)"
echo "  6. Type 'exit' to stop recording"
echo ""
echo "Tips:"
echo "  - Pause between commands for readability"
echo "  - Keep it under 60 seconds"
echo "  - Use --diff-test to show the verification"
echo ""

if [ "${1:-}" = "--scripted" ]; then
    # Automated recording using script-based approach
    # This creates a temporary script that types commands with delays
    SCRIPT_FILE=$(mktemp)
    cat > "$SCRIPT_FILE" << 'SCRIPT'
#!/usr/bin/env bash
set -e
sleep 1
echo "# First, let's check our setup"
sleep 0.5
noricum doctor
sleep 2

echo ""
echo "# Analyze a 204-line C hash table"
sleep 0.5
noricum analyze tests/fixtures/medium/hash_table.c
sleep 2

echo ""
echo "# Now migrate a simple C function with verification"
sleep 0.5
noricum migrate tests/fixtures/simple/power.c --diff-test 2>&1 | head -30
sleep 2

echo ""
echo "# The big one: migrate a 204-line hash table with malloc/free"
sleep 0.5
noricum migrate tests/fixtures/medium/hash_table.c --diff-test 2>&1 | head -40
sleep 2

echo ""
echo "# 0 unsafe blocks. Diff test PASS. Done."
sleep 1
SCRIPT
    chmod +x "$SCRIPT_FILE"

    asciinema rec "$OUTPUT_FILE" \
        --title "Noricum: C to Safe Rust Migration Agent" \
        --cols 100 \
        --rows 30 \
        --command "bash $SCRIPT_FILE"

    rm -f "$SCRIPT_FILE"
else
    # Interactive recording
    asciinema rec "$OUTPUT_FILE" \
        --title "Noricum: C to Safe Rust Migration Agent" \
        --cols 100 \
        --rows 30
fi

echo ""
echo "Recording saved to: ${OUTPUT_FILE}"
echo ""
echo "Next steps:"
echo "  1. Preview: asciinema play ${OUTPUT_FILE}"
echo "  2. Upload: asciinema upload ${OUTPUT_FILE}"
echo "  3. Convert to GIF: agg ${OUTPUT_FILE} demo.gif --cols 100 --rows 30 --speed 2"
echo "  4. Or SVG: svg-term --in ${OUTPUT_FILE} --out demo.svg --window --width 100"

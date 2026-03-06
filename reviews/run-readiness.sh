#!/usr/bin/env bash
# Launch Readiness Assessment for Noricum
# Usage: bash reviews/run-readiness.sh [--update-marketing]
#   --update-marketing  Also update marketing docs after assessment
# Cron:  0 7 * * * cd /home/marche/noricum && bash reviews/run-readiness.sh --update-marketing >> reviews/reports/readiness-cron.log 2>&1

set -uo pipefail

export PATH="/usr/local/bin:/usr/bin:/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
if [ -z "${TERM:-}" ]; then
    [ -f "$HOME/.profile" ] && source "$HOME/.profile" || true
fi

UPDATE_MARKETING=false
if [[ "${1:-}" == "--update-marketing" ]]; then
    UPDATE_MARKETING=true
fi

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

DATE=$(date +%Y-%m-%d)
REPORT_DIR="$PROJECT_ROOT/reviews/reports"
READINESS_FILE="$REPORT_DIR/readiness-${DATE}.md"
METRICS_FILE=$(mktemp /tmp/noricum-readiness-XXXXXX.txt)

mkdir -p "$REPORT_DIR"

echo "=== Noricum Launch Readiness Assessment ==="
echo "Date: $DATE"
echo ""

# --- Collect Metrics ---

cat > "$METRICS_FILE" <<HEADER
# Readiness Metrics — $DATE
HEADER

# 1. Compilation
echo "## Compilation" >> "$METRICS_FILE"
if cargo check --workspace 2>&1 | tail -3 >> "$METRICS_FILE"; then
    echo "STATUS: PASS" >> "$METRICS_FILE"
else
    echo "STATUS: FAIL" >> "$METRICS_FILE"
fi

# 2. Tests
echo "" >> "$METRICS_FILE"
echo "## Tests" >> "$METRICS_FILE"
TOTAL_PASS=0
while IFS= read -r line; do
    if [[ "$line" =~ ^"test result:".*"ok."[[:space:]]*([0-9]+) ]]; then
        TOTAL_PASS=$((TOTAL_PASS + ${BASH_REMATCH[1]}))
    fi
    echo "$line" >> "$METRICS_FILE"
done < <(cargo test --workspace 2>&1 | grep -E "^test result|^running")
echo "TOTAL TESTS PASSING: $TOTAL_PASS" >> "$METRICS_FILE"

# 3. Clippy
echo "" >> "$METRICS_FILE"
echo "## Clippy" >> "$METRICS_FILE"
if cargo clippy --workspace -- -D warnings 2>&1 | tail -3 >> "$METRICS_FILE"; then
    echo "STATUS: 0 warnings" >> "$METRICS_FILE"
else
    echo "STATUS: WARNINGS FOUND" >> "$METRICS_FILE"
fi

# 4. LOC
echo "" >> "$METRICS_FILE"
echo "## Stats" >> "$METRICS_FILE"
RS_FILES=$(find crates -name "*.rs" | wc -l)
RS_LOC=$(find crates -name "*.rs" -exec wc -l {} + 2>/dev/null | tail -1 | awk '{print $1}')
TEST_COUNT=$(grep -r "#\[test\]" crates/ tests/ 2>/dev/null | wc -l)
echo "Rust files: $RS_FILES" >> "$METRICS_FILE"
echo "Rust LOC: $RS_LOC" >> "$METRICS_FILE"
echo "Test annotations: $TEST_COUNT" >> "$METRICS_FILE"

# 5. C fixtures
echo "" >> "$METRICS_FILE"
echo "## C Fixtures" >> "$METRICS_FILE"
for dir in tests/fixtures/*/; do
    count=$(find "$dir" -name "*.c" | wc -l)
    total_loc=$(find "$dir" -name "*.c" -exec wc -l {} + 2>/dev/null | tail -1 | awk '{print $1}')
    echo "$(basename "$dir"): $count files, ${total_loc:-0} LOC" >> "$METRICS_FILE"
done

# 6. Largest fixture
echo "" >> "$METRICS_FILE"
echo "## Largest C Fixture" >> "$METRICS_FILE"
find tests/fixtures -name "*.c" -exec wc -l {} + 2>/dev/null | sort -rn | head -5 >> "$METRICS_FILE"

# 7. Marketing materials status
echo "" >> "$METRICS_FILE"
echo "## Marketing Materials" >> "$METRICS_FILE"
echo "Blog post exists: $(test -f blog/2026-03-06-c-to-rust-llm-agent.md && echo 'YES' || echo 'NO')" >> "$METRICS_FILE"
echo "Demo GIF exists: $(test -f demo.gif && echo 'YES' || echo 'NO')" >> "$METRICS_FILE"
echo "Demo cast exists: $(test -f demo.cast && echo 'YES' || echo 'NO')" >> "$METRICS_FILE"
echo "Social posts exist: $(test -f docs/marketing/social-posts.md && echo 'YES' || echo 'NO')" >> "$METRICS_FILE"
echo "Anthropic app exists: $(test -f docs/marketing/anthropic-build-application.md && echo 'YES' || echo 'NO')" >> "$METRICS_FILE"

# 8. GitHub stars (if gh available)
echo "" >> "$METRICS_FILE"
echo "## GitHub" >> "$METRICS_FILE"
if command -v gh &>/dev/null; then
    STARS=$(gh api repos/JuanMarchetto/noricum --jq '.stargazers_count' 2>/dev/null || echo "N/A")
    FORKS=$(gh api repos/JuanMarchetto/noricum --jq '.forks_count' 2>/dev/null || echo "N/A")
    echo "Stars: $STARS" >> "$METRICS_FILE"
    echo "Forks: $FORKS" >> "$METRICS_FILE"
else
    echo "gh CLI not available" >> "$METRICS_FILE"
fi

# 9. Git stats
echo "" >> "$METRICS_FILE"
echo "## Git" >> "$METRICS_FILE"
echo "Commits: $(git rev-list --count HEAD)" >> "$METRICS_FILE"
echo "Branch: $(git branch --show-current)" >> "$METRICS_FILE"
echo "Last commit: $(git log --oneline -1)" >> "$METRICS_FILE"

echo "" >> "$METRICS_FILE"
echo "Metrics collected."
echo ""

# --- Run Assessment ---

READINESS_PROMPT="You are performing a daily launch readiness assessment for the Noricum project.

Read the assessment framework at reviews/prompts/launch-readiness.md.
Read the README.md for current project state.
Read the blog post at blog/2026-03-06-c-to-rust-llm-agent.md.

Use the metrics below to evaluate each criterion. Be honest and specific.

IMPORTANT: Output the entire assessment report as text to stdout. Do NOT write files. Just print the assessment directly.

## Metrics

$(cat "$METRICS_FILE")"

echo "Running readiness assessment with Claude Code..."

# Check for claude CLI
if ! command -v claude &>/dev/null; then
    echo "ERROR: 'claude' CLI not found."
    echo "Manual metrics collected at: $METRICS_FILE"
    cat "$METRICS_FILE"
    rm "$METRICS_FILE"
    exit 1
fi

STDERR_LOG="${REPORT_DIR}/readiness-${DATE}-stderr.log"

if ! env -u CLAUDECODE claude -p "$READINESS_PROMPT" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)' \
    --output-format text \
    > "$READINESS_FILE" 2>"$STDERR_LOG"; then
    echo "ERROR: Assessment failed."
    [ -s "$STDERR_LOG" ] && head -10 "$STDERR_LOG"
    rm "$METRICS_FILE"
    exit 1
fi

rm "$METRICS_FILE"

if [ -s "$READINESS_FILE" ]; then
    echo "=== Assessment Complete ==="
    echo "Report: $READINESS_FILE"
    echo ""
    # Show verdict
    grep -E "VERDICT:|WEIGHTED SCORE:|TOP 3 BLOCKERS:" "$READINESS_FILE" 2>/dev/null || true
    echo ""
fi

# --- Update Marketing Materials ---

if $UPDATE_MARKETING && [ -s "$READINESS_FILE" ]; then
    echo "=== Updating Marketing Materials ==="

    UPDATE_PROMPT="You are updating the marketing materials for Noricum based on the latest readiness assessment.

Read the update guidelines at reviews/prompts/update-marketing.md.
Read the readiness assessment at $READINESS_FILE.
Read and update these files with current, accurate numbers:
- blog/2026-03-06-c-to-rust-llm-agent.md (benchmark table, LOC counts, test counts)
- README.md (badge numbers, benchmark table)
- docs/marketing/social-posts.md (numbers in all posts)
- docs/marketing/anthropic-build-application.md (results section, technical details)
- CHANGELOG.md (ensure unreleased section is current)

Rules:
- Only update numbers and facts, not structure or tone
- All claims must match actual test results
- Do not exaggerate"

    FIX_STDERR_LOG="${REPORT_DIR}/readiness-${DATE}-marketing-stderr.log"

    if env -u CLAUDECODE claude -p "$UPDATE_PROMPT" \
        --dangerously-skip-permissions \
        --output-format text \
        > "${REPORT_DIR}/readiness-${DATE}-marketing.md" 2>"$FIX_STDERR_LOG"; then
        echo "Marketing materials updated."
        echo "Log: ${REPORT_DIR}/readiness-${DATE}-marketing.md"
    else
        echo "WARNING: Marketing update failed."
        [ -s "$FIX_STDERR_LOG" ] && head -10 "$FIX_STDERR_LOG"
    fi
fi

echo ""
echo "Done."

#!/usr/bin/env bash
# Economic Evaluation Runner for Noricum
# Usage: bash reviews/run-economic-eval.sh [--fix]
#   --fix  After generating evaluation, automatically execute high-priority economic recommendations
# Cron:  0 */6 * * * cd /home/marche/noricum && bash reviews/run-economic-eval.sh >> reviews/reports/econ-cron.log 2>&1

set -euo pipefail

export PATH="/usr/local/bin:/usr/bin:/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
if [ -z "${TERM:-}" ]; then
    [ -f "$HOME/.profile" ] && source "$HOME/.profile" || true
fi

FIX_MODE=false
if [[ "${1:-}" == "--fix" ]]; then
    FIX_MODE=true
fi

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

REPORT_DIR="$PROJECT_ROOT/reviews/reports"
LAST_COMMIT_FILE="$REPORT_DIR/last-econ-eval-commit.txt"
DATE=$(date +%Y-%m-%d-%H)

mkdir -p "$REPORT_DIR"

# --- Check if enough changes since last eval ---
LAST_COMMIT=$(cat "$LAST_COMMIT_FILE" 2>/dev/null || echo "HEAD~50")
CHANGED_FILES=$(git diff --name-only "$LAST_COMMIT" HEAD 2>/dev/null | wc -l || echo "0")

if [ "$CHANGED_FILES" -lt 10 ]; then
    echo "[$DATE] No significant changes since last eval ($CHANGED_FILES files changed, need 10+). Skipping."
    exit 0
fi

echo "=== Noricum Economic Evaluation ==="
echo "Date: $DATE"
echo "Changed files since last eval: $CHANGED_FILES"

REPORT_FILE="$REPORT_DIR/econ-${DATE}.md"
METRICS_FILE=$(mktemp /tmp/noricum-econ-metrics-XXXXXX.txt)

# --- Collect Economic Metrics ---

cat > "$METRICS_FILE" <<'HEADER'
# Economic Metrics (Auto-Collected)

HEADER

# Migration success rate
echo "## Migration Success Rate" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
FIXTURE_COUNT=$(find tests/fixtures -name "*.c" | wc -l)
echo "Total C fixtures: $FIXTURE_COUNT" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# LOC coverage
echo "## Lines of Code" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
RUST_LOC=$(find crates -name "*.rs" | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
echo "Rust source: ${RUST_LOC:-0} lines" >> "$METRICS_FILE"
C_LOC=$(find tests/fixtures -name "*.c" | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
echo "C fixtures: ${C_LOC:-0} lines" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# Test count
echo "## Test Coverage" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
TESTS=$(grep -r "#\[test\]" crates/ tests/ 2>/dev/null | wc -l)
echo "Total tests: $TESTS" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# Community metrics
echo "## Community" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
COMMITS=$(git rev-list --count HEAD 2>/dev/null || echo "0")
CONTRIBUTORS=$(git log --format='%ae' | sort -u | wc -l)
echo "Total commits: $COMMITS" >> "$METRICS_FILE"
echo "Contributors: $CONTRIBUTORS" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# Git activity (last 30 days)
echo "## Recent Activity (30 days)" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
RECENT_COMMITS=$(git log --since="30 days ago" --oneline 2>/dev/null | wc -l)
echo "Commits in last 30 days: $RECENT_COMMITS" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# Benchmark results from README
echo "## Benchmark Summary" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
grep -A 20 "## Benchmark Results" README.md 2>/dev/null | head -25 >> "$METRICS_FILE" || echo "No benchmark section found" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

echo "Economic metrics collected to $METRICS_FILE"

# --- Build the Evaluation Prompt ---

EVAL_PROMPT=$(cat <<'PROMPT_END'
You are performing an economic viability evaluation of the Noricum project, a C-to-Rust migration agent.

## Your Task

1. Read the economic evaluation framework at `reviews/ECONOMIC_EVALUATION.md`
2. Read `README.md`, `EVALUATION.md`, and `CHANGELOG.md` for context
3. Review the automated metrics provided below
4. For each of the 6 economic perspectives (E1-E6):
   a. Go through every checklist item with evidence
   b. Assign a score 1-10 with justification
   c. List top recommendations
5. Calculate the weighted overall grade
6. Write an executive summary with 30/60/90-day economic roadmap

## Output Format

Produce a complete markdown report with all scores filled in. Be specific and actionable. Be honest — do not inflate scores.

## Automated Metrics

PROMPT_END
)

FULL_PROMPT="${EVAL_PROMPT}

$(cat "$METRICS_FILE")"

# --- Execute Evaluation ---

echo "Running economic evaluation with Claude Code..."

if ! command -v claude &> /dev/null; then
    echo "ERROR: 'claude' CLI not found in PATH."
    rm "$METRICS_FILE"
    exit 1
fi

claude -p "$FULL_PROMPT" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)' \
    --output-format text \
    > "$REPORT_FILE" 2>/dev/null

rm "$METRICS_FILE"

# Save current commit hash
git rev-parse HEAD > "$LAST_COMMIT_FILE"

if [ -s "$REPORT_FILE" ]; then
    LINES=$(wc -l < "$REPORT_FILE")
    echo "=== Economic Evaluation Complete ==="
    echo "Report: $REPORT_FILE"
    echo "Lines: $LINES"
    grep -E "^### Grade:|^## Overall" "$REPORT_FILE" 2>/dev/null || echo "(Grade not found)"
else
    echo "ERROR: Evaluation produced empty output."
    exit 1
fi

# --- Auto-Fix Pass ---
if $FIX_MODE && [ -s "$REPORT_FILE" ]; then
    echo ""
    echo "=== Running Economic Auto-Fix Pass ==="
    FIX_PROMPT="Read the economic evaluation at $REPORT_FILE. For every high-priority economic recommendation, implement what can be done in code (documentation improvements, API readiness, benchmark additions, etc.). Run cargo check and cargo test after changes."

    claude -p "$FIX_PROMPT" \
        --allowedTools 'Read,Write,Edit,Grep,Glob,Bash' \
        --output-format text \
        > "${REPORT_DIR}/econ-${DATE}-fixes.md" 2>/dev/null

    echo "Fix log: ${REPORT_DIR}/econ-${DATE}-fixes.md"
fi

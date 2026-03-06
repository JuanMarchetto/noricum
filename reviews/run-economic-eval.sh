#!/usr/bin/env bash
# Economic Evaluation Runner for Noricum
# Usage: bash reviews/run-economic-eval.sh [--no-fix]
#   --no-fix  Skip the automatic fix pass (default: always fix)
# Cron:  0 */6 * * * cd /home/marche/noricum && bash reviews/run-economic-eval.sh >> reviews/reports/econ-cron.log 2>&1

set -euo pipefail

export PATH="/usr/local/bin:/usr/bin:/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
if [ -z "${TERM:-}" ]; then
    [ -f "$HOME/.profile" ] && source "$HOME/.profile" || true
fi

FIX_MODE=true
if [[ "${1:-}" == "--no-fix" ]]; then
    FIX_MODE=false
fi

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

REPORT_DIR="$PROJECT_ROOT/reviews/reports"
LAST_COMMIT_FILE="$REPORT_DIR/last-econ-eval-commit.txt"
DATE=$(date +%Y-%m-%d-%H)

mkdir -p "$REPORT_DIR"

# --- Check if enough changes since last eval ---
if [ -f "$LAST_COMMIT_FILE" ] && git cat-file -t "$(cat "$LAST_COMMIT_FILE")" &>/dev/null; then
    LAST_COMMIT=$(cat "$LAST_COMMIT_FILE")
else
    # First run or invalid commit — use initial commit as baseline
    LAST_COMMIT=$(git rev-list --max-parents=0 HEAD 2>/dev/null | head -1)
fi
CHANGED_FILES=$(git diff --name-only "$LAST_COMMIT" HEAD 2>/dev/null | wc -l | tr -d ' ')

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

IMPORTANT: Output the entire report as text to stdout. Do NOT attempt to write files. Just print the full markdown report.

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

STDERR_LOG="${REPORT_DIR}/econ-${DATE}-stderr.log"

if ! env -u CLAUDECODE claude -p "$FULL_PROMPT" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)' \
    --output-format text \
    > "$REPORT_FILE" 2>"$STDERR_LOG"; then
    echo "ERROR: Claude CLI failed (exit code $?)."
    [ -s "$STDERR_LOG" ] && echo "stderr: $(head -10 "$STDERR_LOG")"
    rm "$METRICS_FILE"
    exit 1
fi

rm "$METRICS_FILE"

if [ -s "$STDERR_LOG" ]; then
    echo "WARNING: Claude produced stderr output. Check $STDERR_LOG"
fi

if [ -s "$REPORT_FILE" ]; then
    # Only save commit hash AFTER successful report generation
    git rev-parse HEAD > "$LAST_COMMIT_FILE"

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

    # Safety: create a git checkpoint before autonomous changes
    git stash push -m "pre-econ-fix-${DATE}" --include-untracked 2>/dev/null || true

    FIX_PROMPT="You are a Rust software engineer working on the Noricum project (a C-to-Rust migration tool).

Read the economic evaluation report at $REPORT_FILE.

Extract all high-priority recommendations from each economic perspective (E1-E6). For each recommendation that can be addressed in code:
1. Implement the fix (documentation improvements, README updates, benchmark additions, API readiness, test coverage, etc.)
2. After each change, run: cargo check --workspace && cargo test --workspace
3. If a fix breaks compilation or tests, revert it and move to the next item

Skip recommendations that require external actions (marketing, community outreach, funding applications). Focus only on code and documentation improvements.

Output a summary of what you implemented and what you skipped, with file paths."

    FIX_STDERR_LOG="${REPORT_DIR}/econ-${DATE}-fix-stderr.log"

    if env -u CLAUDECODE claude -p "$FIX_PROMPT" \
        --dangerously-skip-permissions \
        --output-format text \
        > "${REPORT_DIR}/econ-${DATE}-fixes.md" 2>"$FIX_STDERR_LOG"; then

        # Verify compilation still passes after fix pass
        if ! cargo check --workspace 2>/dev/null; then
            echo "WARNING: Fix pass broke compilation. Restoring from stash."
            git checkout -- . 2>/dev/null
            git stash pop 2>/dev/null || true
        else
            echo "Fix pass completed. Changes verified with cargo check."
            git stash drop 2>/dev/null || true
        fi
    else
        echo "WARNING: Fix pass Claude CLI failed. Restoring from stash."
        [ -s "$FIX_STDERR_LOG" ] && echo "stderr: $(head -10 "$FIX_STDERR_LOG")"
        git checkout -- . 2>/dev/null
        git stash pop 2>/dev/null || true
    fi

    echo "Fix log: ${REPORT_DIR}/econ-${DATE}-fixes.md"
fi

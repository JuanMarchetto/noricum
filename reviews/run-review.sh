#!/usr/bin/env bash
# Multi-Stakeholder Review Runner for Noricum
# Usage: bash reviews/run-review.sh [--fix]
#   --fix  After generating report, automatically fix blocking/high-priority issues
# Cron:  0 */2 * * * cd /home/marche/noricum && bash reviews/run-review.sh >> reviews/reports/cron.log 2>&1

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

DATE=$(date +%Y-%m-%d-%H)
REPORT_DIR="$PROJECT_ROOT/reviews/reports"
REPORT_FILE="$REPORT_DIR/${DATE}.md"
METRICS_FILE=$(mktemp /tmp/noricum-metrics-XXXXXX.txt)

mkdir -p "$REPORT_DIR"

echo "=== Noricum Multi-Stakeholder Review ==="
echo "Date: $DATE"
echo "Collecting project metrics..."

# --- Metric Collection ---

cat > "$METRICS_FILE" <<'HEADER'
# Project Metrics (Auto-Collected)

HEADER

# 1. Compilation check
echo "## Compilation" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
if cargo check --workspace 2>&1 | tail -5 >> "$METRICS_FILE"; then
    echo "RESULT: PASS" >> "$METRICS_FILE"
else
    echo "RESULT: FAIL" >> "$METRICS_FILE"
fi
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 2. Test results
echo "## Tests" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
cargo test --workspace 2>&1 | grep -E "^test result|^running|failures" >> "$METRICS_FILE" || echo "No test output captured" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 3. Clippy
echo "## Clippy" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
if cargo clippy --workspace -- -D warnings 2>&1 | tail -5 >> "$METRICS_FILE"; then
    echo "RESULT: PASS (0 warnings)" >> "$METRICS_FILE"
else
    echo "RESULT: WARNINGS/ERRORS FOUND" >> "$METRICS_FILE"
fi
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 4. Format check
echo "## Format" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
if cargo fmt --all -- --check 2>&1 | head -20 >> "$METRICS_FILE"; then
    echo "RESULT: PASS" >> "$METRICS_FILE"
else
    echo "RESULT: FORMAT ISSUES FOUND" >> "$METRICS_FILE"
fi
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 5. LOC count
echo "## Lines of Code" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
find crates -name "*.rs" | xargs wc -l 2>/dev/null | tail -1 >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 6. Test count
echo "## Test Count" >> "$METRICS_FILE"
TESTS=$(grep -r "#\[test\]" crates/ tests/ 2>/dev/null | wc -l)
echo "Total #[test] annotations: $TESTS" >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 7. Dependency count
echo "## Dependencies" >> "$METRICS_FILE"
DEPS=$(grep "^name = " Cargo.lock 2>/dev/null | wc -l)
echo "Total crates in Cargo.lock: $DEPS" >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 8. unwrap() in production code
echo "## unwrap() in Production Code" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
grep -rn "\.unwrap()" crates/*/src/ --include="*.rs" 2>/dev/null | grep -v "#\[cfg(test)\]" | grep -v "mod tests" | grep -v "// test" | head -30 >> "$METRICS_FILE" || echo "None found" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 9. unsafe in production code
echo "## unsafe Usage" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
grep -rn "unsafe" crates/*/src/ --include="*.rs" 2>/dev/null | grep -v "unsafe_count" | grep -v "// " | grep -v "test" | grep -v "doc" | head -20 >> "$METRICS_FILE" || echo "None found" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 10. Recent git history
echo "## Recent Commits" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
git log --oneline -20 >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 11. File structure
echo "## Crate Structure" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
for crate_dir in crates/*/; do
    crate_name=$(basename "$crate_dir")
    rs_count=$(find "$crate_dir" -name "*.rs" | wc -l)
    loc=$(find "$crate_dir" -name "*.rs" -exec wc -l {} + 2>/dev/null | tail -1 | awk '{print $1}')
    echo "$crate_name: $rs_count files, ${loc:-0} lines" >> "$METRICS_FILE"
done
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 12. Dependency versions (outdated check)
echo "## Key Dependency Versions" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
grep -E "^(rig-core|tokio|clap|thiserror|serde|tracing)" crates/*/Cargo.toml 2>/dev/null | head -20 >> "$METRICS_FILE" || echo "Use 'cargo outdated' for full check" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

# 13. Security-relevant files
echo "## Security-Relevant Files" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo ".env in .gitignore: $(grep -c '\.env' .gitignore 2>/dev/null || echo 'NOT FOUND')" >> "$METRICS_FILE"
echo "SECURITY.md exists: $(test -f SECURITY.md && echo 'YES' || echo 'NO')" >> "$METRICS_FILE"
echo "Cargo.lock committed: $(git ls-files Cargo.lock | wc -l | xargs)" >> "$METRICS_FILE"
echo '```' >> "$METRICS_FILE"
echo "" >> "$METRICS_FILE"

echo "Metrics collected to $METRICS_FILE"
echo ""

# --- Build the Review Prompt ---

REVIEW_PROMPT=$(cat <<'PROMPT_END'
You are performing a comprehensive multi-stakeholder review of the Noricum project, a C-to-Rust migration agent. You have access to the project's source code and automated metrics.

## Your Task

1. Read the review framework at `reviews/STAKEHOLDER_REVIEW.md`
2. Read ALL files listed in the "Data Sources > Files to Read" section
3. Review the automated metrics provided below
4. For each of the 8 stakeholder perspectives (P1-P8):
   a. Go through every checklist item and mark it pass/fail with evidence
   b. Write concrete findings with file:line references
   c. Assign a score 1-10 with justification
   d. List blocking, high-priority, and nice-to-have items
5. Calculate the weighted overall grade
6. Write an executive summary

## Output Format

Produce a complete markdown report following the structure in STAKEHOLDER_REVIEW.md, with all scores filled in and findings documented. Be specific — cite file paths and line numbers. Be honest — do not inflate scores.

## Automated Metrics

PROMPT_END
)

# Append metrics to prompt
FULL_PROMPT="${REVIEW_PROMPT}

$(cat "$METRICS_FILE")"

# --- Execute Review ---

echo "Running review with Claude Code..."
echo "(This may take several minutes)"
echo ""

# Check if claude CLI is available
if ! command -v claude &> /dev/null; then
    echo "ERROR: 'claude' CLI not found in PATH."
    echo "Install Claude Code or run the review manually:"
    echo "  cat $METRICS_FILE"
    echo "  # Then paste metrics + reviews/STAKEHOLDER_REVIEW.md into Claude"
    rm "$METRICS_FILE"
    exit 1
fi

# Run Claude in non-interactive mode
claude -p "$FULL_PROMPT" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)' \
    --output-format text \
    > "$REPORT_FILE" 2>/dev/null

# Cleanup
rm "$METRICS_FILE"

# Verify output
if [ -s "$REPORT_FILE" ]; then
    LINES=$(wc -l < "$REPORT_FILE")
    echo "=== Review Complete ==="
    echo "Report: $REPORT_FILE"
    echo "Lines: $LINES"
    echo ""
    # Extract grade if present
    grep -E "^### Grade:" "$REPORT_FILE" 2>/dev/null || echo "(Grade not found in report — check format)"
    echo ""
    echo "To view: cat $REPORT_FILE"
else
    echo "ERROR: Review produced empty output."
    echo "Try running manually: claude -p \"$(head -5 "$METRICS_FILE")...\""
    exit 1
fi

# --- Auto-Fix Pass ---
if $FIX_MODE && [ -s "$REPORT_FILE" ]; then
    echo ""
    echo "=== Running Auto-Fix Pass ==="
    FIX_PROMPT="Read the stakeholder review at $REPORT_FILE. For every Blocking and High-priority issue listed, implement the fix directly. Run cargo check, cargo test, and cargo clippy after each change to verify. Do NOT fix Nice-to-have items unless trivial."

    claude -p "$FIX_PROMPT" \
        --allowedTools 'Read,Write,Edit,Grep,Glob,Bash' \
        --output-format text \
        > "${REPORT_DIR}/${DATE}-fixes.md" 2>/dev/null

    echo "Fix log: ${REPORT_DIR}/${DATE}-fixes.md"
fi

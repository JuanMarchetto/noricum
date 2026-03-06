#!/usr/bin/env bash
# Multi-Stakeholder Review Runner for Noricum
# Usage: bash reviews/run-review.sh [--no-fix]
#   --no-fix  Skip the automatic fix pass (default: always fix)
# Cron:  0 */2 * * * cd /home/marche/noricum && bash reviews/run-review.sh >> reviews/reports/cron.log 2>&1

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

IMPORTANT: Output the entire report as text to stdout. Do NOT attempt to write files or ask questions. Just print the full markdown report directly.

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

# Run Claude in non-interactive mode (unset CLAUDECODE to allow nested invocation)
STDERR_LOG="${REPORT_DIR}/${DATE}-stderr.log"

if ! env -u CLAUDECODE claude -p "$FULL_PROMPT" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)' \
    --output-format text \
    > "$REPORT_FILE" 2>"$STDERR_LOG"; then
    echo "ERROR: Claude CLI failed (exit code $?)."
    [ -s "$STDERR_LOG" ] && echo "stderr: $(head -10 "$STDERR_LOG")"
    rm "$METRICS_FILE"
    exit 1
fi

# Cleanup
rm "$METRICS_FILE"

if [ -s "$STDERR_LOG" ]; then
    echo "WARNING: Claude produced stderr output. Check $STDERR_LOG"
fi

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
    echo "Check stderr log: $STDERR_LOG"
    exit 1
fi

# --- Auto-Fix Pass ---
if $FIX_MODE && [ -s "$REPORT_FILE" ]; then
    echo ""
    echo "=== Running Auto-Fix Pass ==="

    # Safety: create a git checkpoint before autonomous changes
    git stash push -m "pre-review-fix-${DATE}" --include-untracked 2>/dev/null || true

    FIX_PROMPT="You are a Rust software engineer working on the Noricum project (a C-to-Rust migration tool).

Read the stakeholder review report at $REPORT_FILE.

Extract all Blocking and High-priority issues from each stakeholder perspective (P1-P8). For each issue:
1. Identify the file and line referenced
2. Implement the code fix (edit Rust source files, Cargo.toml, CI configs, docs, etc.)
3. After each change, run: cargo check --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings
4. If a fix breaks compilation or tests, revert it and move to the next issue

Skip Nice-to-have items. Focus only on code-level fixes (no external actions like publishing crates or setting up services).

Output a summary of what you fixed and what you skipped, with file paths."

    FIX_STDERR_LOG="${REPORT_DIR}/${DATE}-fix-stderr.log"

    if env -u CLAUDECODE claude -p "$FIX_PROMPT" \
        --dangerously-skip-permissions \
        --output-format text \
        > "${REPORT_DIR}/${DATE}-fixes.md" 2>"$FIX_STDERR_LOG"; then

        # Verify compilation still passes after fix pass
        if ! cargo check --workspace 2>/dev/null; then
            echo "WARNING: Fix pass broke compilation. Restoring from stash."
            git checkout -- . 2>/dev/null
            git stash pop 2>/dev/null || true
        else
            echo "Fix pass completed. Changes verified with cargo check."
            # Pop stash (no conflict expected since fix pass replaced changes)
            git stash drop 2>/dev/null || true
        fi
    else
        echo "WARNING: Fix pass Claude CLI failed. Restoring from stash."
        [ -s "$FIX_STDERR_LOG" ] && echo "stderr: $(head -10 "$FIX_STDERR_LOG")"
        git checkout -- . 2>/dev/null
        git stash pop 2>/dev/null || true
    fi

    echo "Fix log: ${REPORT_DIR}/${DATE}-fixes.md"
fi

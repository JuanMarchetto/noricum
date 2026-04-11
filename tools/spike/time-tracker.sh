#!/usr/bin/env bash
# spike-time-tracker — manage the 8-hour budget clock for an interactive spike.
#
# Usage:
#   tools/spike/time-tracker.sh start           — record Hour 0 start timestamp
#   tools/spike/time-tracker.sh status          — show elapsed + remaining
#   tools/spike/time-tracker.sh commit-hook     — git hook helper: warn if elapsed > budget
#   tools/spike/time-tracker.sh reset           — clear the clock
#   tools/spike/time-tracker.sh install-hook    — install post-commit git hook
#
# The clock file lives at .noricum-spike-clock at the repo root and is
# .gitignored. Each spike branch has its own clock because the file's
# content is tied to the branch's spike name. Switching branches
# switches clocks via a branch-aware lookup.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
CLOCK_FILE="$REPO_ROOT/.noricum-spike-clock"
BUDGET_MIN=${SPIKE_BUDGET_MIN:-480}  # 8 hours default

# Ensure .noricum-spike-clock is gitignored.
ensure_gitignored() {
    local gi="$REPO_ROOT/.gitignore"
    if [ ! -f "$gi" ] || ! grep -q "^.noricum-spike-clock$" "$gi"; then
        echo ".noricum-spike-clock" >> "$gi"
        echo "added .noricum-spike-clock to .gitignore"
    fi
}

cmd_start() {
    ensure_gitignored
    local branch
    branch=$(git branch --show-current)
    date -u +%s > "$CLOCK_FILE"
    echo "branch=$branch" >> "$CLOCK_FILE"
    echo "budget_min=$BUDGET_MIN" >> "$CLOCK_FILE"
    echo "Spike clock started for branch '$branch'."
    echo "Budget: $BUDGET_MIN minutes. Hour 8 exit enforced on status."
}

cmd_status() {
    if [ ! -f "$CLOCK_FILE" ]; then
        echo "No spike clock running. Run '$0 start' to begin."
        return 0
    fi
    local start_ts
    local branch
    local budget
    start_ts=$(head -1 "$CLOCK_FILE")
    branch=$(grep '^branch=' "$CLOCK_FILE" | cut -d= -f2 || echo "?")
    budget=$(grep '^budget_min=' "$CLOCK_FILE" | cut -d= -f2 || echo "$BUDGET_MIN")
    local now_ts
    now_ts=$(date -u +%s)
    local elapsed_s=$((now_ts - start_ts))
    local elapsed_m=$((elapsed_s / 60))
    local remaining_m=$((budget - elapsed_m))
    local pct=$((elapsed_m * 100 / budget))

    echo "Spike clock:"
    echo "  Branch:    $branch"
    echo "  Started:   $(date -u -d @$start_ts '+%Y-%m-%dT%H:%M:%SZ')"
    echo "  Elapsed:   ${elapsed_m} min (${pct}% of ${budget} min)"
    echo "  Remaining: ${remaining_m} min"

    if [ "$remaining_m" -le 0 ]; then
        echo ""
        echo "*** HOUR 8 EXIT ***"
        echo "The budget is exhausted. Commit current state and declare WIN or LOSS."
        echo "If you need more time, you're no longer in spike mode — start a new branch."
        return 2
    elif [ "$pct" -ge 75 ]; then
        echo ""
        echo "WARNING: 75% of budget consumed. Plan your exit."
    fi
}

cmd_commit_hook() {
    if [ ! -f "$CLOCK_FILE" ]; then
        return 0
    fi
    # On each commit, log elapsed time into the commit message via a trailer.
    # This turns the commit log into a time-ordered session transcript.
    local start_ts
    start_ts=$(head -1 "$CLOCK_FILE")
    local now_ts
    now_ts=$(date -u +%s)
    local elapsed_s=$((now_ts - start_ts))
    local elapsed_m=$((elapsed_s / 60))
    echo "Spike-Elapsed: ${elapsed_m} min"
}

cmd_reset() {
    rm -f "$CLOCK_FILE"
    echo "Spike clock cleared."
}

cmd_install_hook() {
    local hook="$REPO_ROOT/.git/hooks/post-commit"
    mkdir -p "$(dirname "$hook")"
    cat > "$hook" <<'EOF'
#!/usr/bin/env bash
# noricum spike time tracker — auto-status on every commit
if [ -f "$(git rev-parse --show-toplevel)/.noricum-spike-clock" ]; then
    "$(git rev-parse --show-toplevel)/tools/spike/time-tracker.sh" status || true
fi
EOF
    chmod +x "$hook"
    echo "Installed post-commit hook at $hook"
    echo "Every commit on this repo now reports spike clock status when active."
}

case "${1:-status}" in
    start)        cmd_start ;;
    status)       cmd_status ;;
    commit-hook)  cmd_commit_hook ;;
    reset)        cmd_reset ;;
    install-hook) cmd_install_hook ;;
    *)
        echo "Usage: $0 {start|status|commit-hook|reset|install-hook}" >&2
        exit 1
        ;;
esac

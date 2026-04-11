#!/usr/bin/env bash
# Sync "public" agent-memory files from ~/.claude/projects/{slug}/memory/
# into docs/methodology/ inside this repo. This is how private per-machine
# memories become part of the published product.
#
# Only syncs files that are on the WHITELIST below. Everything else stays
# private (session state, personal preferences, ephemeral project notes).
#
# Run this after any session that produced new methodology-worthy learnings.
# The script will:
#   1. Read each whitelisted memory file
#   2. Strip the "type: feedback|project|reference|user" frontmatter
#   3. Add a provenance header pointing at the agent memory source
#   4. Write the result to docs/methodology/{name}.md
#   5. Report what was synced
#
# Manual review of the diff is expected before committing. This script
# does not auto-commit.

set -euo pipefail

MEMORY_DIR="${MEMORY_DIR:-$HOME/.claude/projects/-home-marche-noricum/memory}"
REPO_ROOT="$(git rev-parse --show-toplevel)"
DOCS_DIR="$REPO_ROOT/docs/methodology"

# Whitelist: memory file name → docs/methodology/ target filename.
# Memory files NOT in this list are considered private and are not synced.
declare -A SYNC_MAP=(
    [feedback_interactive_spike_methodology.md]=interactive-spike-mode-memory.md
    [feedback_architectural_elimination.md]=architectural-elimination-memory.md
    [reference_mcp_tool_limits.md]=mcp-tool-limits-memory.md
    [project_interactive_spike_branch.md]=interactive-spike-case-study-memory.md
    [feedback_trait_object_limitation.md]=trait-object-limitation-memory.md
    [feedback_contract_minimalist.md]=contract-minimalist-memory.md
    [feedback_budget_exceeded.md]=budget-exceeded-memory.md
    [feedback_scope_reduction.md]=scope-reduction-memory.md
    [feedback_manual_fix_patterns.md]=manual-fix-patterns-memory.md
    [project_p33_learnings.md]=p33-type-contract-memory.md
    [project_tractor_landscape.md]=tractor-landscape-memory.md
)

if [ ! -d "$MEMORY_DIR" ]; then
    echo "ERROR: memory dir not found at $MEMORY_DIR" >&2
    exit 1
fi
mkdir -p "$DOCS_DIR"

SYNCED=0
SKIPPED_MISSING=0
for src_name in "${!SYNC_MAP[@]}"; do
    src="$MEMORY_DIR/$src_name"
    dst="$DOCS_DIR/${SYNC_MAP[$src_name]}"

    if [ ! -f "$src" ]; then
        echo "SKIP (missing): $src_name"
        SKIPPED_MISSING=$((SKIPPED_MISSING + 1))
        continue
    fi

    {
        echo "<!--"
        echo "  Synced from agent memory: $src_name"
        echo "  Source: $MEMORY_DIR/$src_name"
        echo "  Sync timestamp: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo "  Do not edit directly — update the memory file and re-run tools/sync-memories-to-docs.sh"
        echo "-->"
        echo ""
        # Copy everything except the YAML frontmatter (--- ... ---).
        awk '
            /^---$/ { in_fm = !in_fm; next }
            !in_fm { print }
        ' "$src"
    } > "$dst"

    SYNCED=$((SYNCED + 1))
    echo "SYNCED: $src_name -> $(realpath --relative-to="$REPO_ROOT" "$dst")"
done

echo ""
echo "Summary: $SYNCED synced, $SKIPPED_MISSING missing (not yet written)"
echo ""
echo "Review the diff with 'git diff docs/methodology/' and commit manually."
